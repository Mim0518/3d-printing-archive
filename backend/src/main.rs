//! Comandos:
//!   printing-archive [servir]          arranca el servidor
//!   printing-archive usuario <nombre>  crea el usuario o le cambia la contraseña (la pide en la terminal,
//!                                      o la lee de la entrada estándar si no es una terminal)
//!   printing-archive migrar            crea la base o la pone al día y sale

use std::io::{BufRead, IsTerminal};

use anyhow::{Context, bail};
use printing_archive::{AppState, app, auth, config::Config, db};
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
        Some(otro) => {
            bail!("comando desconocido: {otro} (usa: servir | usuario <nombre> | migrar)")
        }
    }
}

async fn servir(config: Config) -> anyhow::Result<()> {
    let pool = db::abrir(&config.base).await?;
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
