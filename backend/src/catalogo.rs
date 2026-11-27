//! Catálogo: importar las filas de la antigua hoja de Google, ligar cada archivo a su modelo, estadísticas y
//! búsqueda de texto.

use std::{collections::HashMap, path::Path};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::{
    db::ahora,
    estructura::{SUELTOS, padre, ruta_nueva},
};

/// Una fila de la pestaña Catálogo exportada a CSV (los encabezados de la hoja).
#[derive(Debug, Deserialize)]
pub struct FilaHoja {
    #[serde(rename = "Ruta actual")]
    pub ruta_actual: String,
    #[serde(rename = "Juego", default)]
    pub juego: String,
    #[serde(rename = "Facción / tipo", default)]
    pub faccion: String,
    #[serde(rename = "Modelo", default)]
    pub modelo: String,
    #[serde(rename = "Estudio", default)]
    pub estudio: String,
    #[serde(rename = "Subcarpeta", default)]
    pub subcarpeta: String,
    #[serde(rename = "Etiquetas", default)]
    pub etiquetas: String,
    #[serde(rename = "Confianza", default)]
    pub confianza: String,
    #[serde(rename = "Notas", default)]
    pub notas: String,
    #[serde(rename = "Revisado", default)]
    pub revisado: String,
}

#[derive(Debug, Default, Serialize)]
pub struct ResumenImportacion {
    pub filas: u64,
    pub nuevos: u64,
    pub actualizados: u64,
    pub etiquetas: u64,
    /// Modelos que ya estaban en la base y no vienen en el CSV (no se borran).
    pub no_vienen: Vec<String>,
}

pub fn separar_etiquetas(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in s.split(';').map(str::trim).filter(|t| !t.is_empty()) {
        if !out.iter().any(|x| x == t) {
            out.push(t.to_string());
        }
    }
    out
}

fn tipo_etiqueta(nombre: &str) -> &'static str {
    if nombre.starts_with("40K: ") {
        "equivalencia"
    } else if nombre == "Proxy temático" {
        "proxy"
    } else {
        ""
    }
}

/// Importa (o vuelve a importar) el catálogo desde el CSV de la hoja. Los modelos se reconocen por su carpeta
/// («Ruta actual»); los campos y las etiquetas se reemplazan con lo del CSV.
pub async fn importar_catalogo(
    pool: &SqlitePool,
    csv_path: &Path,
) -> anyhow::Result<ResumenImportacion> {
    let mut lector = csv::Reader::from_path(csv_path)
        .with_context(|| format!("no se pudo abrir {}", csv_path.display()))?;
    let filas: Vec<FilaHoja> = lector
        .deserialize()
        .collect::<Result<_, _>>()
        .context("el CSV no tiene las columnas de la pestaña Catálogo")?;
    let antes: Vec<String> = sqlx::query_scalar("SELECT carpeta FROM modelos")
        .fetch_all(pool)
        .await?;
    let t = ahora();
    let mut res = ResumenImportacion::default();
    let mut tx = pool.begin().await?;
    for f in &filas {
        let carpeta = f.ruta_actual.trim();
        if carpeta.is_empty() {
            continue;
        }
        res.filas += 1;
        let existia = antes.iter().any(|c| c == carpeta);
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO modelos (carpeta, juego, faccion, nombre, estudio, subcarpeta, confianza, notas, revisado,
                                  creado, actualizado)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (carpeta) DO UPDATE SET
                juego = excluded.juego, faccion = excluded.faccion, nombre = excluded.nombre,
                estudio = excluded.estudio, subcarpeta = excluded.subcarpeta, confianza = excluded.confianza,
                notas = excluded.notas, revisado = excluded.revisado, actualizado = excluded.actualizado
             RETURNING id",
        )
        .bind(carpeta)
        .bind(f.juego.trim())
        .bind(f.faccion.trim())
        .bind(f.modelo.trim())
        .bind(f.estudio.trim())
        .bind(f.subcarpeta.trim())
        .bind(f.confianza.trim())
        .bind(f.notas.trim())
        .bind(f.revisado.trim())
        .bind(t)
        .bind(t)
        .fetch_one(&mut *tx)
        .await?;
        if existia {
            res.actualizados += 1;
        } else {
            res.nuevos += 1;
        }
        sqlx::query("DELETE FROM modelo_etiquetas WHERE modelo_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        for e in separar_etiquetas(&f.etiquetas) {
            let eid: i64 = sqlx::query_scalar(
                "INSERT INTO etiquetas (nombre, tipo) VALUES (?, ?)
                 ON CONFLICT (nombre) DO UPDATE SET nombre = excluded.nombre RETURNING id",
            )
            .bind(&e)
            .bind(tipo_etiqueta(&e))
            .fetch_one(&mut *tx)
            .await?;
            sqlx::query(
                "INSERT OR IGNORE INTO modelo_etiquetas (modelo_id, etiqueta_id) VALUES (?, ?)",
            )
            .bind(id)
            .bind(eid)
            .execute(&mut *tx)
            .await?;
            res.etiquetas += 1;
        }
    }
    tx.commit().await?;
    let nuevas: std::collections::HashSet<&str> =
        filas.iter().map(|f| f.ruta_actual.trim()).collect();
    res.no_vienen = antes
        .into_iter()
        .filter(|c| !nuevas.contains(c.as_str()))
        .collect();
    reconstruir_busqueda(pool).await?;
    ligar_archivos(pool).await?;
    actualizar_estadisticas(pool).await?;
    crate::edicion::recalcular(pool, None).await?;
    // los archivos pudieron cambiar de modelo: las placas sugieren otros
    crate::placas::sugerir(pool).await?;
    Ok(res)
}

#[derive(sqlx::FromRow)]
struct ModeloRuta {
    id: i64,
    carpeta: String,
    juego: String,
    faccion: String,
    nombre: String,
    estudio: String,
}

/// Dueño de cada ruta según las carpetas del catálogo:
/// 1. una fila cuya clave es el archivo mismo;
/// 2. la carpeta de una fila (o una de sus subcarpetas), la más profunda;
/// 3. una fila «<carpeta>/(sueltos)»: los archivos sueltos de esa carpeta;
/// 4. lo suelto en la carpeta del modelo de una fila (su ruta sin la subcarpeta) o entre ella y la de la fila.
pub struct Duenos {
    archivo: HashMap<String, i64>,
    carpeta: HashMap<String, i64>,
    sueltos: HashMap<String, i64>,
    directo: HashMap<String, i64>,
}

impl Duenos {
    pub async fn cargar(pool: &SqlitePool) -> anyhow::Result<Self> {
        let modelos: Vec<ModeloRuta> = sqlx::query_as(
            "SELECT id, carpeta, juego, faccion, nombre, estudio FROM modelos ORDER BY id",
        )
        .fetch_all(pool)
        .await?;
        let rutas: std::collections::HashSet<String> = sqlx::query_scalar(
            "SELECT a.ruta FROM archivos a JOIN bibliotecas b ON b.id = a.biblioteca_id
                                WHERE b.tipo = 'principal'",
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .collect();
        let mut d = Duenos {
            archivo: HashMap::new(),
            carpeta: HashMap::new(),
            sueltos: HashMap::new(),
            directo: HashMap::new(),
        };
        for m in &modelos {
            let clave = m.carpeta.as_str();
            let dir = if let Some(s) = clave.strip_suffix(SUELTOS) {
                d.sueltos.insert(s.to_string(), m.id);
                s
            } else if rutas.contains(clave) {
                d.archivo.insert(clave.to_string(), m.id);
                padre(clave)
            } else {
                d.carpeta.insert(clave.to_string(), m.id);
                clave
            };
            let mf = ruta_nueva(&m.juego, &m.faccion, &m.nombre, &m.estudio, "");
            if dir == mf || dir.starts_with(&format!("{mf}/")) {
                let partes: Vec<&str> = dir.split('/').collect();
                for n in mf.split('/').count()..=partes.len() {
                    d.directo.entry(partes[..n].join("/")).or_insert(m.id);
                }
            }
        }
        Ok(d)
    }

    pub fn de_archivo(&self, ruta: &str) -> Option<i64> {
        if let Some(id) = self.archivo.get(ruta) {
            return Some(*id);
        }
        self.de_carpeta(padre(ruta))
    }

    pub fn de_carpeta(&self, dir: &str) -> Option<i64> {
        let mut a = dir;
        loop {
            if let Some(id) = self.carpeta.get(a) {
                return Some(*id);
            }
            if a.is_empty() {
                break;
            }
            a = padre(a);
        }
        self.sueltos
            .get(dir)
            .or_else(|| self.directo.get(dir))
            .copied()
    }
}

/// Liga cada archivo presente de la biblioteca principal a su modelo.
pub async fn ligar_archivos(pool: &SqlitePool) -> anyhow::Result<()> {
    let duenos = Duenos::cargar(pool).await?;
    let archivos: Vec<(i64, String, Option<i64>)> = sqlx::query_as(
        "SELECT a.id, a.ruta, a.modelo_id FROM archivos a JOIN bibliotecas b ON b.id = a.biblioteca_id
         WHERE b.tipo = 'principal'",
    )
    .fetch_all(pool)
    .await?;
    let mut tx = pool.begin().await?;
    for (id, ruta, actual) in archivos {
        let nuevo = duenos.de_archivo(&ruta);
        if nuevo != actual {
            sqlx::query("UPDATE archivos SET modelo_id = ? WHERE id = ?")
                .bind(nuevo)
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

/// Recalcula archivos, mallas, bytes (sin contar dos veces los enlaces duros) y copias en frío de cada modelo.
pub async fn actualizar_estadisticas(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE modelos SET
            n_archivos = (SELECT count(*) FROM archivos a WHERE a.modelo_id = modelos.id AND a.estado = 'presente'),
            n_mallas = (SELECT count(*) FROM archivos a
                        WHERE a.modelo_id = modelos.id AND a.estado = 'presente' AND a.tipo = 'malla'),
            bytes = (SELECT coalesce(sum(b), 0) FROM
                        (SELECT max(a.bytes) AS b FROM archivos a
                         WHERE a.modelo_id = modelos.id AND a.estado = 'presente' GROUP BY a.inode)),
            n_frio = (SELECT count(*) FROM copias_frio c WHERE c.modelo_id = modelos.id AND c.estado = 'presente'),
            bytes_frio = (SELECT coalesce(sum(c.bytes), 0) FROM copias_frio c
                          WHERE c.modelo_id = modelos.id AND c.estado = 'presente')",
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Vuelve a llenar el índice de búsqueda con los modelos y sus etiquetas.
pub async fn reconstruir_busqueda(pool: &SqlitePool) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM modelos_fts")
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO modelos_fts (rowid, nombre, juego, faccion, estudio, subcarpeta, etiquetas, notas, carpeta)
         SELECT m.id, m.nombre, m.juego, m.faccion, m.estudio, m.subcarpeta,
                coalesce((SELECT group_concat(e.nombre, ' ') FROM modelo_etiquetas me
                          JOIN etiquetas e ON e.id = me.etiqueta_id WHERE me.modelo_id = m.id), ''),
                m.notas, m.carpeta
         FROM modelos m",
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Convierte lo que escribe el usuario en una consulta FTS5: cada palabra como prefijo, todas obligatorias.
pub fn consulta_fts(q: &str) -> Option<String> {
    let partes: Vec<String> = q
        .split_whitespace()
        .map(|p| p.replace('"', ""))
        .filter(|p| !p.is_empty())
        .map(|p| format!("\"{p}\"*"))
        .collect();
    (!partes.is_empty()).then(|| partes.join(" "))
}
