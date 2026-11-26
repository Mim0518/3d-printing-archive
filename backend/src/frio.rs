//! Almacenamiento frío (fase 1: solo lectura). Importa la lista de lo que está guardado fuera del disco
//! (Drive con rclone, o un disco externo) y lo liga a los modelos del catálogo.
//!
//! Formato de entrada (lo genera la exportación de los datos de la limpieza):
//! - `frio.csv`: ubicacion, tipo, remoto, ruta, bytes, md5, modificado, ruta_original, carpeta_modelo
//! - `contenido.csv` (opcional): ubicacion, ruta, entrada, bytes, crc — lo que trae cada comprimido

use std::{collections::HashMap, path::Path};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::{catalogo, db::ahora};

#[derive(Debug, Deserialize)]
struct FilaFrio {
    ubicacion: String,
    tipo: String,
    remoto: String,
    ruta: String,
    bytes: i64,
    #[serde(default)]
    md5: String,
    #[serde(default)]
    modificado: String,
    #[serde(default)]
    ruta_original: String,
    #[serde(default)]
    carpeta_modelo: String,
}

#[derive(Debug, Deserialize)]
struct FilaContenido {
    ubicacion: String,
    ruta: String,
    entrada: String,
    bytes: i64,
    #[serde(default)]
    crc: String,
}

#[derive(Debug, Default, Serialize)]
pub struct ResumenFrio {
    pub copias: u64,
    pub ligadas: u64,
    pub entradas: u64,
    pub ubicaciones: Vec<String>,
}

pub async fn importar(
    pool: &SqlitePool,
    frio_csv: &Path,
    contenido_csv: Option<&Path>,
) -> anyhow::Result<ResumenFrio> {
    let filas: Vec<FilaFrio> = csv::Reader::from_path(frio_csv)
        .with_context(|| format!("no se pudo abrir {}", frio_csv.display()))?
        .deserialize()
        .collect::<Result<_, _>>()
        .context("frio.csv no tiene las columnas esperadas")?;
    let modelos: HashMap<String, i64> =
        sqlx::query_as::<_, (String, i64)>("SELECT carpeta, id FROM modelos")
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect();
    let duenos = catalogo::Duenos::cargar(pool).await?;
    let t = ahora();
    let mut res = ResumenFrio::default();
    let mut ubic: HashMap<String, i64> = HashMap::new();
    let mut copias: HashMap<(String, String), i64> = HashMap::new();
    let mut tx = pool.begin().await?;
    for f in &filas {
        let uid = match ubic.get(&f.ubicacion) {
            Some(id) => *id,
            None => {
                let id: i64 = sqlx::query_scalar(
                    "INSERT INTO ubicaciones_frio (nombre, tipo, remoto, ultimo_listado) VALUES (?, ?, ?, ?)
                     ON CONFLICT (nombre) DO UPDATE SET tipo = excluded.tipo, remoto = excluded.remoto,
                        ultimo_listado = excluded.ultimo_listado
                     RETURNING id",
                )
                .bind(&f.ubicacion)
                .bind(&f.tipo)
                .bind(&f.remoto)
                .bind(t)
                .fetch_one(&mut *tx)
                .await?;
                ubic.insert(f.ubicacion.clone(), id);
                res.ubicaciones.push(f.ubicacion.clone());
                id
            }
        };
        let modelo = if f.carpeta_modelo.is_empty() {
            None
        } else {
            modelos
                .get(&f.carpeta_modelo)
                .copied()
                .or_else(|| duenos.de_carpeta(&f.carpeta_modelo))
        };
        if modelo.is_some() {
            res.ligadas += 1;
        }
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO copias_frio (ubicacion_id, ruta, bytes, md5, modificado, ruta_original, modelo_id)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (ubicacion_id, ruta) DO UPDATE SET bytes = excluded.bytes, md5 = excluded.md5,
                modificado = excluded.modificado, ruta_original = excluded.ruta_original,
                modelo_id = excluded.modelo_id
             RETURNING id",
        )
        .bind(uid)
        .bind(&f.ruta)
        .bind(f.bytes)
        .bind(&f.md5)
        .bind(&f.modificado)
        .bind(&f.ruta_original)
        .bind(modelo)
        .fetch_one(&mut *tx)
        .await?;
        copias.insert((f.ubicacion.clone(), f.ruta.clone()), id);
        res.copias += 1;
    }
    if let Some(p) = contenido_csv {
        let mut lector = csv::Reader::from_path(p)
            .with_context(|| format!("no se pudo abrir {}", p.display()))?;
        let mut limpias = std::collections::HashSet::new();
        for fila in lector.deserialize::<FilaContenido>() {
            let c = fila.context("contenido.csv no tiene las columnas esperadas")?;
            let Some(&cid) = copias.get(&(c.ubicacion.clone(), c.ruta.clone())) else {
                continue;
            };
            if limpias.insert(cid) {
                sqlx::query("DELETE FROM copias_contenido WHERE copia_id = ?")
                    .bind(cid)
                    .execute(&mut *tx)
                    .await?;
            }
            sqlx::query(
                "INSERT OR REPLACE INTO copias_contenido (copia_id, entrada, bytes, crc) VALUES (?, ?, ?, ?)",
            )
            .bind(cid)
            .bind(&c.entrada)
            .bind(c.bytes)
            .bind(&c.crc)
            .execute(&mut *tx)
            .await?;
            res.entradas += 1;
        }
    }
    tx.commit().await?;
    catalogo::actualizar_estadisticas(pool).await?;
    Ok(res)
}
