//! Miniaturas: una imagen por malla (render de f3d sin pantalla), por imagen (reducida) y por escena o laminado
//! que traiga vista previa adentro (ver `lectores`), guardadas como WebP de 512 y 128 px en una caché en disco.
//! Un 3MF que trae la imagen de su placa (proyectos de Orca, Bambu, Prusa) la usa en lugar del render.
//!
//! - La clave de cada miniatura es inode + tamaño + fecha del archivo: los nombres de un mismo archivo (enlaces
//!   duros) comparten miniatura y solo se renderiza una vez; si el archivo cambia, cambia la clave.
//! - Solo se genera lo que falta. Un archivo que falló no se vuelve a intentar salvo que se pida (`forzar`).
//! - Lee la colección, nunca escribe en ella.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use anyhow::Context;
use serde::Serialize;
use sqlx::SqlitePool;
use tokio::{sync::Semaphore, task::JoinSet};

use crate::db::ahora;

pub const EXT_MALLA: [&str; 4] = ["stl", "obj", "3mf", "ply"];
pub const EXT_IMAGEN: [&str; 6] = ["jpg", "jpeg", "png", "webp", "gif", "bmp"];
pub const TAMANOS: [u32; 2] = [512, 128];
const LIMITE_RENDER: Duration = Duration::from_secs(180);

/// Lo que se repite en `WHERE` para elegir los archivos que llevan miniatura. Las escenas y los laminados, solo si
/// la detección de placas vio que traen vista previa.
pub const SQL_RENDERIZABLE: &str = "((a.tipo = 'malla' AND a.ext IN ('stl', 'obj', '3mf', 'ply'))
     OR (a.tipo = 'imagen' AND a.ext IN ('jpg', 'jpeg', 'png', 'webp', 'gif', 'bmp'))
     OR (a.tipo IN ('escena', 'laminado') AND EXISTS (
         SELECT 1 FROM datos_placa dp WHERE dp.archivo_id = a.id AND dp.vista_previa = 1)))";

#[derive(Debug, Clone)]
pub struct Opciones {
    /// Carpeta de la caché.
    pub dir: PathBuf,
    /// Programa f3d (o uno que acepte sus mismas opciones).
    pub f3d: String,
    /// Cuántos renders a la vez.
    pub paralelo: usize,
}

#[derive(Debug, Default, Serialize)]
pub struct Resultado {
    pub candidatos: u64,
    pub ya_estaban: u64,
    pub generadas: u64,
    pub fallidas: u64,
    pub ejemplos_de_error: Vec<String>,
}

pub fn clave(inode: i64, bytes: i64, mtime_ms: i64) -> String {
    format!(
        "{:x}-{:x}-{:x}",
        inode as u64, bytes as u64, mtime_ms as u64
    )
}

pub fn ruta_cache(dir: &Path, clave: &str, tamano: u32) -> PathBuf {
    let sub: String = clave.chars().take(2).collect();
    dir.join(sub).join(format!("{clave}-{tamano}.webp"))
}

/// Reduce una imagen a 512 y 128 px (sin agrandar) y la guarda como WebP; escribe primero a un temporal y luego
/// renombra, para que nunca quede una miniatura a medias.
pub fn guardar_webp(origen: &Path, dir: &Path, clave: &str) -> Result<(), String> {
    let img = image::ImageReader::open(origen)
        .map_err(|e| format!("no se pudo abrir: {e}"))?
        .with_guessed_format()
        .map_err(|e| format!("formato: {e}"))?
        .decode()
        .map_err(|e| format!("no se pudo leer la imagen: {e}"))?;
    guardar_imagen(&img, dir, clave)
}

/// Guarda una imagen ya leída en la caché (512 y 128 px, sin agrandar).
pub fn guardar_imagen(img: &image::DynamicImage, dir: &Path, clave: &str) -> Result<(), String> {
    for t in TAMANOS {
        let destino = ruta_cache(dir, clave, t);
        if let Some(p) = destino.parent() {
            std::fs::create_dir_all(p).map_err(|e| format!("caché: {e}"))?;
        }
        let im = if img.width() > t || img.height() > t {
            img.thumbnail(t, t)
        } else {
            img.clone()
        };
        let tmp = destino.with_extension("tmp");
        im.save_with_format(&tmp, image::ImageFormat::WebP)
            .map_err(|e| format!("no se pudo guardar la miniatura: {e}"))?;
        std::fs::rename(&tmp, &destino).map_err(|e| format!("caché: {e}"))?;
    }
    Ok(())
}

async fn render_malla(op: &Opciones, archivo: &Path, png: &Path) -> Result<(), String> {
    if let Some(p) = png.parent() {
        tokio::fs::create_dir_all(p)
            .await
            .map_err(|e| format!("caché: {e}"))?;
    }
    let mut cmd = tokio::process::Command::new(&op.f3d);
    cmd.arg(format!("--output={}", png.display()))
        .args([
            "--resolution=512,512",
            "--up=+Z",
            "--camera-direction=-1,1,-0.6",
            "--no-background",
            "--color=0.78,0.78,0.8",
            "--anti-aliasing",
            "--tone-mapping",
            "--filename=false",
            "--grid=false",
            "--axis=false",
            "--verbose=quiet",
            "--max-size=2048",
        ])
        .arg(archivo)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    let salida = tokio::time::timeout(LIMITE_RENDER, cmd.output())
        .await
        .map_err(|_| format!("f3d tardó más de {} s", LIMITE_RENDER.as_secs()))?
        .map_err(|e| format!("no se pudo ejecutar {}: {e}", op.f3d))?;
    if !salida.status.success() {
        let err = String::from_utf8_lossy(&salida.stderr);
        return Err(format!(
            "f3d falló ({}): {}",
            salida.status,
            err.trim().chars().take(300).collect::<String>()
        ));
    }
    if !png.is_file() {
        return Err("f3d no dejó imagen".into());
    }
    Ok(())
}

struct Grupo {
    clave: String,
    ruta: String,
    tipo: String,
    ext: String,
    archivos: Vec<i64>,
}

async fn generar_grupo(op: Arc<Opciones>, raiz: Arc<PathBuf>, g: &Grupo) -> Result<(), String> {
    let archivo = raiz.join(&g.ruta);
    if g.tipo == "imagen" {
        let (dir, clave) = (op.dir.clone(), g.clave.clone());
        return tokio::task::spawn_blocking(move || guardar_webp(&archivo, &dir, &clave))
            .await
            .map_err(|e| e.to_string())?;
    }
    if g.tipo == "escena" || g.tipo == "laminado" {
        let (dir, clave, ext) = (op.dir.clone(), g.clave.clone(), g.ext.clone());
        return tokio::task::spawn_blocking(move || {
            match crate::lectores::vista_previa(&archivo, &ext)? {
                Some(img) => guardar_imagen(&img, &dir, &clave),
                None => Err("no trae vista previa".into()),
            }
        })
        .await
        .map_err(|e| e.to_string())?;
    }
    if g.ext == "3mf" {
        // un 3MF de laminador trae la imagen de su placa; si no la trae, se renderiza
        let (dir, clave, ruta) = (op.dir.clone(), g.clave.clone(), archivo.clone());
        let hecho = tokio::task::spawn_blocking(move || {
            match crate::lectores::vista_previa(&ruta, "3mf") {
                Ok(Some(img)) => guardar_imagen(&img, &dir, &clave).is_ok(),
                _ => false,
            }
        })
        .await
        .unwrap_or(false);
        if hecho {
            return Ok(());
        }
    }
    let png = op.dir.join("tmp").join(format!("{}.png", g.clave));
    let r = render_malla(&op, &archivo, &png).await;
    let r = match r {
        Ok(()) => {
            let (dir, clave, p) = (op.dir.clone(), g.clave.clone(), png.clone());
            tokio::task::spawn_blocking(move || guardar_webp(&p, &dir, &clave))
                .await
                .map_err(|e| e.to_string())?
        }
        Err(e) => Err(e),
    };
    let _ = tokio::fs::remove_file(&png).await;
    r
}

async fn registrar(
    pool: &SqlitePool,
    archivos: &[i64],
    clave: &str,
    error: Option<&str>,
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    for id in archivos {
        sqlx::query(
            "INSERT INTO miniaturas (archivo_id, clave, estado, error, generada) VALUES (?, ?, ?, ?, ?)
             ON CONFLICT (archivo_id) DO UPDATE SET clave = excluded.clave, estado = excluded.estado,
                error = excluded.error, generada = excluded.generada",
        )
        .bind(id)
        .bind(clave)
        .bind(if error.is_some() { "error" } else { "ok" })
        .bind(error.unwrap_or(""))
        .bind(ahora())
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Qué miniaturas generar.
#[derive(Debug, Default, Clone)]
pub struct Cuales {
    /// Solo las de estos modelos.
    pub modelo_ids: Option<Vec<i64>>,
    /// Solo los archivos de las placas: escenas y laminados con vista previa y proyectos 3MF (rápidas: salen de
    /// la imagen que traen adentro).
    pub solo_placas: bool,
}

/// Genera las miniaturas que faltan (de todo o de lo indicado en `cuales`).
pub async fn generar(
    pool: &SqlitePool,
    op: Opciones,
    cuales: Cuales,
    forzar: bool,
    progreso: Arc<AtomicU64>,
) -> anyhow::Result<Resultado> {
    let raiz: String = sqlx::query_scalar(
        "SELECT ruta FROM bibliotecas WHERE tipo = 'principal' ORDER BY id LIMIT 1",
    )
    .fetch_optional(pool)
    .await?
    .context("no hay biblioteca principal")?;
    let sql = format!(
        "SELECT a.id, a.ruta, a.tipo, a.ext, a.inode, a.bytes, a.mtime_ms, a.modelo_id, mi.clave, mi.estado
         FROM archivos a JOIN bibliotecas b ON b.id = a.biblioteca_id AND b.tipo = 'principal'
         LEFT JOIN miniaturas mi ON mi.archivo_id = a.id
         WHERE a.estado = 'presente' AND {SQL_RENDERIZABLE} {}
         ORDER BY a.modelo_id, (a.tipo = 'imagen') DESC, a.bytes DESC",
        if cuales.solo_placas {
            "AND EXISTS (SELECT 1 FROM placa_archivos pa WHERE pa.archivo_id = a.id)"
        } else {
            ""
        }
    );
    #[allow(clippy::type_complexity)]
    let filas: Vec<(
        i64,
        String,
        String,
        String,
        i64,
        i64,
        i64,
        Option<i64>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .fetch_all(pool)
        .await?;
    let mut res = Resultado::default();
    let mut grupos: Vec<Grupo> = Vec::new();
    let mut por_clave: HashMap<String, usize> = HashMap::new();
    let mut ya_ok: Vec<(i64, String)> = Vec::new();
    for (id, ruta, tipo, ext, inode, bytes, mtime, modelo, mi_clave, mi_estado) in filas {
        if let Some(ids) = &cuales.modelo_ids
            && !modelo.is_some_and(|m| ids.contains(&m))
        {
            continue;
        }
        res.candidatos += 1;
        let k = clave(inode, bytes, mtime);
        let misma = mi_clave.as_deref() == Some(k.as_str());
        if misma && (mi_estado.as_deref() == Some("ok") || !forzar) {
            res.ya_estaban += 1;
            continue;
        }
        if !forzar && ruta_cache(&op.dir, &k, 128).is_file() {
            ya_ok.push((id, k)); // otro nombre del mismo archivo ya tiene miniatura
            continue;
        }
        match por_clave.get(&k) {
            Some(&i) => grupos[i].archivos.push(id),
            None => {
                por_clave.insert(k.clone(), grupos.len());
                grupos.push(Grupo {
                    clave: k,
                    ruta,
                    tipo,
                    ext,
                    archivos: vec![id],
                });
            }
        }
    }
    for (id, k) in &ya_ok {
        registrar(pool, &[*id], k, None).await?;
        res.ya_estaban += 1;
        progreso.fetch_add(1, Ordering::Relaxed);
    }

    let op = Arc::new(op);
    let raiz = Arc::new(PathBuf::from(raiz));
    let limite = Arc::new(Semaphore::new(op.paralelo.max(1)));
    let mut tareas: JoinSet<(Grupo, Result<(), String>)> = JoinSet::new();
    let mut pendientes = grupos.into_iter();
    loop {
        // mantiene la cola llena sin lanzar 35 mil tareas de golpe
        while tareas.len() < op.paralelo.max(1) * 2 {
            let Some(g) = pendientes.next() else { break };
            let (op, raiz, limite) = (op.clone(), raiz.clone(), limite.clone());
            tareas.spawn(async move {
                let _permiso = limite.acquire_owned().await;
                let r = generar_grupo(op, raiz, &g).await;
                (g, r)
            });
        }
        let Some(hecha) = tareas.join_next().await else {
            break;
        };
        let (g, r) = hecha.context("una tarea de miniaturas se cayó")?;
        match &r {
            Ok(()) => res.generadas += g.archivos.len() as u64,
            Err(e) => {
                res.fallidas += g.archivos.len() as u64;
                if res.ejemplos_de_error.len() < 20 {
                    res.ejemplos_de_error.push(format!("{}: {e}", g.ruta));
                }
            }
        }
        registrar(
            pool,
            &g.archivos,
            &g.clave,
            r.as_ref().err().map(String::as_str),
        )
        .await?;
        progreso.fetch_add(g.archivos.len() as u64, Ordering::Relaxed);
    }
    Ok(res)
}

/// La portada de un modelo: la elegida a mano o, si no, la imagen con miniatura que más parezca portada, la
/// malla más pesada con miniatura o la vista previa de una escena o un laminado. Es una expresión SQL sobre el
/// alias `m` de `modelos`.
pub const SQL_PORTADA: &str = "coalesce(m.portada_archivo_id,
    (SELECT a.id FROM archivos a JOIN miniaturas mi ON mi.archivo_id = a.id AND mi.estado = 'ok'
     WHERE a.modelo_id = m.id AND a.estado = 'presente' AND a.tipo = 'imagen'
     ORDER BY (lower(a.nombre) LIKE '%cover%' OR lower(a.nombre) LIKE '%portada%'
               OR lower(a.nombre) LIKE '%render%') DESC, a.bytes DESC LIMIT 1),
    (SELECT a.id FROM archivos a JOIN miniaturas mi ON mi.archivo_id = a.id AND mi.estado = 'ok'
     WHERE a.modelo_id = m.id AND a.estado = 'presente' AND a.tipo = 'malla'
     ORDER BY a.bytes DESC LIMIT 1),
    (SELECT a.id FROM archivos a JOIN miniaturas mi ON mi.archivo_id = a.id AND mi.estado = 'ok'
     WHERE a.modelo_id = m.id AND a.estado = 'presente' AND a.tipo IN ('escena', 'laminado')
     ORDER BY a.tipo = 'escena' DESC, a.bytes DESC LIMIT 1))";
