//! Escaneo de una biblioteca: recorre el disco (solo lee) y pone al día la tabla `archivos`.
//!
//! Es incremental: un archivo con el mismo tamaño, fecha e inode que en el escaneo anterior no se vuelve a abrir.
//! Lo que ya no está se marca «falta»; nunca se borra de la tabla. No sigue enlaces simbólicos.

use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::Read,
    os::unix::fs::MetadataExt,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use anyhow::Context;
use serde::Serialize;
use sqlx::SqlitePool;
use walkdir::WalkDir;

use crate::{
    catalogo,
    db::ahora,
    estructura::{extension, padre, tipo_de},
};

/// Un archivo tal como está en el disco.
#[derive(Debug, Clone)]
pub struct Entrada {
    pub ruta: String,
    pub bytes: i64,
    pub mtime_ms: i64,
    pub inode: i64,
    pub nlink: i64,
    /// `None` si no cambió desde el escaneo anterior (no se volvió a leer).
    pub stl: Option<(Option<&'static str>, Option<i64>)>,
}

#[derive(Debug, Default, Serialize, Clone)]
pub struct Resultado {
    pub archivos: u64,
    pub nuevos: u64,
    pub cambiados: u64,
    pub iguales: u64,
    pub faltan: u64,
    pub errores: Vec<String>,
}

/// Formato y número de triángulos de un STL leyendo solo su encabezado (84 bytes).
pub fn leer_stl(ruta: &Path, bytes: u64) -> (Option<&'static str>, Option<i64>) {
    let mut cab = [0u8; 84];
    let leidos = File::open(ruta).and_then(|mut f| {
        let mut n = 0;
        while n < cab.len() {
            match f.read(&mut cab[n..])? {
                0 => break,
                k => n += k,
            }
        }
        Ok(n)
    });
    let Ok(n) = leidos else {
        return (None, None);
    };
    if n == 84 {
        let tri = u32::from_le_bytes([cab[80], cab[81], cab[82], cab[83]]) as u64;
        if 84 + 50 * tri == bytes {
            return (Some("binario"), Some(tri as i64));
        }
    }
    if cab[..n.min(5)].eq_ignore_ascii_case(b"solid") {
        return (Some("texto"), None);
    }
    (Some("desconocido"), None)
}

/// Recorre `raiz` y devuelve cada archivo. `previos` (ruta → tamaño, fecha, inode) evita volver a leer
/// los encabezados de lo que no cambió. `vistos` cuenta los archivos recorridos (para el progreso).
pub fn recorrer(
    raiz: &Path,
    previos: &HashMap<String, (i64, i64, i64)>,
    vistos: &AtomicU64,
) -> (Vec<Entrada>, Vec<String>) {
    let mut out = Vec::new();
    let mut errores = Vec::new();
    for item in WalkDir::new(raiz).follow_links(false) {
        let item = match item {
            Ok(i) => i,
            Err(e) => {
                errores.push(e.to_string());
                continue;
            }
        };
        if !item.file_type().is_file() {
            continue;
        }
        let Ok(rel) = item.path().strip_prefix(raiz) else {
            continue;
        };
        let Some(ruta) = rel.to_str().map(str::to_string) else {
            errores.push(format!("nombre no UTF-8: {}", rel.display()));
            continue;
        };
        let meta = match item.metadata() {
            Ok(m) => m,
            Err(e) => {
                errores.push(format!("{ruta}: {e}"));
                continue;
            }
        };
        let bytes = meta.len() as i64;
        let mtime_ms = meta.mtime() * 1000 + meta.mtime_nsec() / 1_000_000;
        let inode = meta.ino() as i64;
        let igual = previos.get(&ruta) == Some(&(bytes, mtime_ms, inode));
        let stl = if igual {
            None
        } else if extension(&ruta) == "stl" {
            Some(leer_stl(item.path(), meta.len()))
        } else {
            Some((None, None))
        };
        out.push(Entrada {
            ruta,
            bytes,
            mtime_ms,
            inode,
            nlink: meta.nlink() as i64,
            stl,
        });
        vistos.fetch_add(1, Ordering::Relaxed);
    }
    (out, errores)
}

/// Escanea una biblioteca y pone al día `archivos`; luego liga los archivos a sus modelos.
pub async fn escanear(
    pool: &SqlitePool,
    biblioteca_id: i64,
    vistos: Arc<AtomicU64>,
) -> anyhow::Result<Resultado> {
    let raiz: String = sqlx::query_scalar("SELECT ruta FROM bibliotecas WHERE id = ?")
        .bind(biblioteca_id)
        .fetch_one(pool)
        .await
        .context("la biblioteca no existe")?;
    let raiz_p = Path::new(&raiz).to_path_buf();
    if !raiz_p.is_dir() {
        anyhow::bail!("la carpeta de la biblioteca no existe o no está montada: {raiz}");
    }
    let previos: HashMap<String, (i64, i64, i64)> = sqlx::query_as::<_, (String, i64, i64, i64)>(
        "SELECT ruta, bytes, mtime_ms, inode FROM archivos WHERE biblioteca_id = ?",
    )
    .bind(biblioteca_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(r, b, m, i)| (r, (b, m, i)))
    .collect();

    let previos = Arc::new(previos);
    let p2 = previos.clone();
    let v2 = vistos.clone();
    let (entradas, errores) =
        tokio::task::spawn_blocking(move || recorrer(&raiz_p, &p2, &v2)).await?;

    let t = ahora();
    let mut res = Resultado {
        archivos: entradas.len() as u64,
        errores,
        ..Default::default()
    };
    let presentes: HashSet<&str> = entradas.iter().map(|e| e.ruta.as_str()).collect();
    let mut tx = pool.begin().await?;
    for e in &entradas {
        match &e.stl {
            None => {
                res.iguales += 1;
                sqlx::query(
                    "UPDATE archivos SET estado = 'presente', visto_en = ?, nlink = ?
                     WHERE biblioteca_id = ? AND ruta = ?",
                )
                .bind(t)
                .bind(e.nlink)
                .bind(biblioteca_id)
                .bind(&e.ruta)
                .execute(&mut *tx)
                .await?;
            }
            Some((formato, tri)) => {
                if previos.contains_key(&e.ruta) {
                    res.cambiados += 1;
                } else {
                    res.nuevos += 1;
                }
                let nombre = e.ruta.rsplit('/').next().unwrap_or(&e.ruta);
                let ext = extension(nombre);
                sqlx::query(
                    "INSERT INTO archivos (biblioteca_id, ruta, carpeta, nombre, ext, tipo, bytes, mtime_ms, inode,
                                           nlink, stl_formato, triangulos, estado, visto_en)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'presente', ?)
                     ON CONFLICT (biblioteca_id, ruta) DO UPDATE SET
                        bytes = excluded.bytes, mtime_ms = excluded.mtime_ms, inode = excluded.inode,
                        nlink = excluded.nlink, stl_formato = excluded.stl_formato,
                        triangulos = excluded.triangulos, estado = 'presente', visto_en = excluded.visto_en",
                )
                .bind(biblioteca_id)
                .bind(&e.ruta)
                .bind(padre(&e.ruta))
                .bind(nombre)
                .bind(&ext)
                .bind(tipo_de(&ext))
                .bind(e.bytes)
                .bind(e.mtime_ms)
                .bind(e.inode)
                .bind(e.nlink)
                .bind(*formato)
                .bind(*tri)
                .bind(t)
                .execute(&mut *tx)
                .await?;
            }
        }
    }
    for ruta in previos.keys().filter(|r| !presentes.contains(r.as_str())) {
        let r = sqlx::query(
            "UPDATE archivos SET estado = 'falta' WHERE biblioteca_id = ? AND ruta = ? AND estado = 'presente'",
        )
        .bind(biblioteca_id)
        .bind(ruta)
        .execute(&mut *tx)
        .await?;
        res.faltan += r.rows_affected();
    }
    sqlx::query("UPDATE bibliotecas SET ultimo_escaneo = ? WHERE id = ?")
        .bind(t)
        .bind(biblioteca_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    catalogo::ligar_archivos(pool).await?;
    catalogo::actualizar_estadisticas(pool).await?;
    Ok(res)
}
