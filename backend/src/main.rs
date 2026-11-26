//! Comandos:
//!   printing-archive [servir]          arranca el servidor
//!   printing-archive usuario <nombre>  crea el usuario o le cambia la contraseña (la pide en la terminal,
//!                                      o la lee de la entrada estándar si no es una terminal)
//!   printing-archive migrar            crea la base o la pone al día y sale
//!   printing-archive escanear          escanea la biblioteca principal (ARCHIVO_BIBLIOTECA) y liga los archivos
//!   printing-archive importar catalogo <hoja.csv>
//!                                      importa la pestaña Catálogo de la hoja de Google (exportada a CSV)
//!   printing-archive importar frio <frio.csv> [contenido.csv]
//!                                      importa la lista de lo que está en almacenamiento frío

use std::{
    io::{BufRead, IsTerminal},
    path::Path,
    sync::{Arc, atomic::AtomicU64},
};

use anyhow::{Context, bail};
use printing_archive::{
    AppState, app, auth, catalogo, config::Config, db, frio, inventario, trabajos,
};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info")),
        )
        .init();
    let config = Config::desde_entorno()?;
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None | Some("servir") => servir(config).await,
        Some("usuario") => {
            let nombre = args
                .get(1)
                .context("uso: printing-archive usuario <nombre>")?;
            usuario(config, nombre).await
        }
        Some("migrar") => {
            db::abrir(&config.base).await?;
            println!("Base al día: {}", config.base.display());
            Ok(())
        }
        Some("escanear") => escanear(config).await,
        Some("importar") => match (args.get(1).map(String::as_str), args.get(2)) {
            (Some("catalogo"), Some(csv)) => {
                let pool = db::abrir(&config.base).await?;
                biblioteca(&pool, &config).await?;
                let r = catalogo::importar_catalogo(&pool, Path::new(csv)).await?;
                println!(
                    "Catálogo: {} filas ({} nuevas, {} actualizadas), {} etiquetas puestas",
                    r.filas, r.nuevos, r.actualizados, r.etiquetas
                );
                if !r.no_vienen.is_empty() {
                    println!(
                        "Modelos que ya estaban y no vienen en el CSV (no se borraron): {}",
                        r.no_vienen.len()
                    );
                    for c in r.no_vienen.iter().take(20) {
                        println!("  {c}");
                    }
                }
                Ok(())
            }
            (Some("frio"), Some(csv)) => {
                let pool = db::abrir(&config.base).await?;
                let r = frio::importar(&pool, Path::new(csv), args.get(3).map(Path::new)).await?;
                println!(
                    "Frío: {} copias ({} ligadas a un modelo), {} entradas de comprimidos; ubicaciones: {}",
                    r.copias,
                    r.ligadas,
                    r.entradas,
                    r.ubicaciones.join(", ")
                );
                Ok(())
            }
            _ => bail!(
                "uso: importar catalogo <hoja.csv> | importar frio <frio.csv> [contenido.csv]"
            ),
        },
        Some(otro) => {
            bail!(
                "comando desconocido: {otro} (usa: servir | usuario <nombre> | migrar | escanear | importar …)"
            )
        }
    }
}

/// Registra la biblioteca principal si ARCHIVO_BIBLIOTECA está configurada.
async fn biblioteca(pool: &sqlx::SqlitePool, config: &Config) -> anyhow::Result<Option<i64>> {
    match &config.biblioteca {
        Some(r) => Ok(Some(db::asegurar_biblioteca_principal(pool, r).await?)),
        None => Ok(None),
    }
}

async fn escanear(config: Config) -> anyhow::Result<()> {
    let pool = db::abrir(&config.base).await?;
    let id = biblioteca(&pool, &config)
        .await?
        .context("falta ARCHIVO_BIBLIOTECA (la carpeta de la colección)")?;
    let r = inventario::escanear(&pool, id, Arc::new(AtomicU64::new(0))).await?;
    println!(
        "Escaneo: {} archivos ({} nuevos, {} cambiados, {} iguales), {} faltan, {} errores",
        r.archivos,
        r.nuevos,
        r.cambiados,
        r.iguales,
        r.faltan,
        r.errores.len()
    );
    for e in r.errores.iter().take(20) {
        println!("  {e}");
    }
    Ok(())
}

async fn servir(config: Config) -> anyhow::Result<()> {
    let pool = db::abrir(&config.base).await?;
    biblioteca(&pool, &config).await?;
    trabajos::marcar_interrumpidos(&pool).await?;
    // ruta nueva, pendientes y avisos al día (por si cambiaron las reglas entre versiones)
    printing_archive::edicion::recalcular(&pool, None).await?;
    let direccion = config.direccion;
    match &config.estaticos {
        Some(d) => tracing::info!("frontend: {}", d.display()),
        None => tracing::warn!("sin frontend compilado: solo se sirve la API"),
    }
    let listener = tokio::net::TcpListener::bind(direccion)
        .await
        .with_context(|| format!("no se pudo escuchar en {direccion}"))?;
    tracing::info!("escuchando en http://{direccion}");
    axum::serve(listener, app(AppState::new(pool, config)))
        .with_graceful_shutdown(senal_de_salida())
        .await?;
    Ok(())
}

async fn usuario(config: Config, nombre: &str) -> anyhow::Result<()> {
    let contrasena = if std::io::stdin().is_terminal() {
        let a = rpassword::prompt_password(format!("Contraseña para {nombre}: "))?;
        let b = rpassword::prompt_password("Repítela: ")?;
        if a != b {
            bail!("las contraseñas no coinciden");
        }
        a
    } else {
        let mut linea = String::new();
        std::io::stdin().lock().read_line(&mut linea)?;
        linea.trim_end_matches(['\r', '\n']).to_string()
    };
    if contrasena.chars().count() < 8 {
        bail!("la contraseña debe tener al menos 8 caracteres");
    }
    let pool = db::abrir(&config.base).await?;
    auth::guardar_usuario(&pool, nombre, &contrasena).await?;
    println!("Usuario «{nombre}» guardado en {}", config.base.display());
    Ok(())
}

/// Ctrl+C o SIGTERM (systemd) cierran el servidor con calma.
async fn senal_de_salida() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = term => {},
    }
    tracing::info!("cerrando");
}
