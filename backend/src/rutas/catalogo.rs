//! Catálogo: resumen, lista con búsqueda y filtros, opciones de los filtros y ficha de un modelo.
//! Todas las rutas exigen sesión.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{QueryBuilder, Sqlite};

use crate::{AppState, auth::Usuario, catalogo::consulta_fts, edicion, error::ApiError};

pub fn rutas() -> Router<AppState> {
    Router::new()
        .route("/resumen", get(resumen))
        .route("/bibliotecas", get(bibliotecas))
        .route("/modelos", get(lista))
        .route("/modelos/filtros", get(filtros))
        .route("/modelos/{id}", get(ficha))
}

const SEP: char = '\u{1f}';

fn etiquetas_de(s: Option<String>) -> Vec<String> {
    s.map(|s| s.split(SEP).map(str::to_string).collect())
        .unwrap_or_default()
}

// ---------- resumen ----------

async fn resumen(_u: Usuario, State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    let (modelos, revisados): (i64, i64) =
        sqlx::query_as("SELECT count(*), coalesce(sum(revisado <> ''), 0) FROM modelos")
            .fetch_one(p)
            .await?;
    let (archivos, bytes, sin_modelo, faltan): (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT coalesce(sum(a.estado = 'presente'), 0),
                coalesce((SELECT sum(b) FROM (SELECT max(x.bytes) AS b FROM archivos x
                          JOIN bibliotecas bx ON bx.id = x.biblioteca_id
                          WHERE bx.tipo = 'principal' AND x.estado = 'presente' GROUP BY x.inode)), 0),
                coalesce(sum(a.estado = 'presente' AND a.modelo_id IS NULL), 0),
                coalesce(sum(a.estado = 'falta'), 0)
         FROM archivos a JOIN bibliotecas b ON b.id = a.biblioteca_id WHERE b.tipo = 'principal'",
    )
    .fetch_one(p)
    .await?;
    let por_tipo: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT a.tipo, count(*), sum(a.bytes) FROM archivos a JOIN bibliotecas b ON b.id = a.biblioteca_id
         WHERE b.tipo = 'principal' AND a.estado = 'presente' GROUP BY a.tipo ORDER BY 3 DESC",
    )
    .fetch_all(p)
    .await?;
    let por_juego: Vec<(String, i64, i64, i64)> = sqlx::query_as(
        "SELECT juego, count(*), sum(bytes), sum(n_frio > 0) FROM modelos GROUP BY juego ORDER BY 2 DESC",
    )
    .fetch_all(p)
    .await?;
    // archivos fuera del catálogo, por carpeta de primer nivel (Utilidades, Yeet, Printers…)
    let fuera: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT CASE WHEN instr(a.ruta, '/') > 0 THEN substr(a.ruta, 1, instr(a.ruta, '/') - 1) ELSE '(raíz)' END,
                count(*), sum(a.bytes)
         FROM archivos a JOIN bibliotecas b ON b.id = a.biblioteca_id
         WHERE b.tipo = 'principal' AND a.estado = 'presente' AND a.modelo_id IS NULL
         GROUP BY 1 ORDER BY 3 DESC",
    )
    .fetch_all(p)
    .await?;
    let frio: Vec<(String, String, i64, i64, i64)> = sqlx::query_as(
        "SELECT u.nombre, u.remoto, count(c.id), coalesce(sum(c.bytes), 0), coalesce(sum(c.modelo_id IS NOT NULL), 0)
         FROM ubicaciones_frio u LEFT JOIN copias_frio c ON c.ubicacion_id = u.id GROUP BY u.id ORDER BY u.nombre",
    )
    .fetch_all(p)
    .await?;
    let ultimo_escaneo: Option<i64> =
        sqlx::query_scalar("SELECT max(ultimo_escaneo) FROM bibliotecas WHERE tipo = 'principal'")
            .fetch_one(p)
            .await?;
    let escaneo_en_curso: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM trabajos WHERE tipo = 'escaneo' AND estado = 'en_curso' LIMIT 1",
    )
    .fetch_optional(p)
    .await?;
    Ok(Json(json!({
        "modelos": { "total": modelos, "revisados": revisados },
        "archivos": { "total": archivos, "bytes": bytes, "sin_modelo": sin_modelo, "faltan": faltan },
        "por_tipo": por_tipo.iter().map(|(t, n, b)| json!({ "tipo": t, "archivos": n, "bytes": b })).collect::<Vec<_>>(),
        "por_juego": por_juego.iter().map(|(j, n, b, f)| json!({ "juego": j, "modelos": n, "bytes": b, "con_frio": f })).collect::<Vec<_>>(),
        "fuera_del_catalogo": fuera.iter().map(|(c, n, b)| json!({ "carpeta": c, "archivos": n, "bytes": b })).collect::<Vec<_>>(),
        "frio": frio.iter().map(|(n, r, c, b, l)| json!({ "ubicacion": n, "remoto": r, "copias": c, "bytes": b, "ligadas": l })).collect::<Vec<_>>(),
        "ultimo_escaneo": ultimo_escaneo,
        "escaneo_en_curso": escaneo_en_curso,
    })))
}

async fn bibliotecas(_u: Usuario, State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let filas: Vec<(i64, String, String, String, Option<i64>)> = sqlx::query_as(
        "SELECT id, nombre, ruta, tipo, ultimo_escaneo FROM bibliotecas ORDER BY id",
    )
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(json!(filas
        .iter()
        .map(|(id, n, r, t, u)| json!({ "id": id, "nombre": n, "ruta": r, "tipo": t, "ultimo_escaneo": u }))
        .collect::<Vec<_>>())))
}

// ---------- lista ----------

#[derive(Debug, Default, Deserialize)]
pub struct Filtros {
    q: Option<String>,
    juego: Option<String>,
    faccion: Option<String>,
    estudio: Option<String>,
    etiqueta: Option<String>,
    /// «con» o «sin» copias en frío
    frio: Option<String>,
    /// «si» o «no»
    revisado: Option<String>,
    /// «si»: solo los que no están en su ruta nueva
    pendiente: Option<String>,
    /// «si»: solo los que tienen avisos de las reglas de nombres
    avisos: Option<String>,
    /// nombre (por omisión) | tamano | archivos | reciente
    orden: Option<String>,
    pagina: Option<i64>,
    por_pagina: Option<i64>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct FilaModelo {
    id: i64,
    carpeta: String,
    juego: String,
    faccion: String,
    nombre: String,
    estudio: String,
    subcarpeta: String,
    confianza: String,
    revisado: String,
    n_archivos: i64,
    n_mallas: i64,
    bytes: i64,
    n_frio: i64,
    bytes_frio: i64,
    ruta_nueva: String,
    pendiente: bool,
    n_avisos: i64,
    #[serde(skip)]
    etiquetas_txt: Option<String>,
}

fn no_vacio(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

fn condiciones(qb: &mut QueryBuilder<Sqlite>, f: &Filtros) {
    qb.push(" WHERE 1 = 1");
    if let Some(fts) = no_vacio(&f.q).and_then(consulta_fts) {
        qb.push(" AND m.id IN (SELECT rowid FROM modelos_fts WHERE modelos_fts MATCH ")
            .push_bind(fts)
            .push(")");
    }
    for (col, v) in [
        ("m.juego", &f.juego),
        ("m.faccion", &f.faccion),
        ("m.estudio", &f.estudio),
    ] {
        if let Some(v) = no_vacio(v) {
            qb.push(format_args!(" AND {col} = "))
                .push_bind(v.to_string());
        }
    }
    if let Some(e) = no_vacio(&f.etiqueta) {
        qb.push(
            " AND m.id IN (SELECT me.modelo_id FROM modelo_etiquetas me JOIN etiquetas e ON e.id = me.etiqueta_id
              WHERE e.nombre = ",
        )
        .push_bind(e.to_string())
        .push(")");
    }
    match no_vacio(&f.frio) {
        Some("con") => {
            qb.push(" AND m.n_frio > 0");
        }
        Some("sin") => {
            qb.push(" AND m.n_frio = 0");
        }
        _ => {}
    }
    match no_vacio(&f.revisado) {
        Some("si") => {
            qb.push(" AND m.revisado <> ''");
        }
        Some("no") => {
            qb.push(" AND m.revisado = ''");
        }
        _ => {}
    }
    if no_vacio(&f.pendiente) == Some("si") {
        qb.push(" AND m.pendiente = 1");
    }
    if no_vacio(&f.avisos) == Some("si") {
        qb.push(" AND m.n_avisos > 0");
    }
}

async fn lista(
    _u: Usuario,
    State(st): State<AppState>,
    Query(f): Query<Filtros>,
) -> Result<Json<Value>, ApiError> {
    let por_pagina = f.por_pagina.unwrap_or(50).clamp(1, 500);
    let pagina = f.pagina.unwrap_or(1).max(1);

    let mut cuenta: QueryBuilder<Sqlite> = QueryBuilder::new("SELECT count(*) FROM modelos m");
    condiciones(&mut cuenta, &f);
    let total: i64 = cuenta.build_query_scalar().fetch_one(&st.pool).await?;

    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT m.id, m.carpeta, m.juego, m.faccion, m.nombre, m.estudio, m.subcarpeta, m.confianza, m.revisado,
                m.n_archivos, m.n_mallas, m.bytes, m.n_frio, m.bytes_frio, m.ruta_nueva, m.pendiente, m.n_avisos,
                (SELECT group_concat(e.nombre, char(31)) FROM modelo_etiquetas me
                 JOIN etiquetas e ON e.id = me.etiqueta_id WHERE me.modelo_id = m.id) AS etiquetas_txt
         FROM modelos m",
    );
    condiciones(&mut qb, &f);
    qb.push(match no_vacio(&f.orden) {
        Some("tamano") => " ORDER BY m.bytes DESC, m.nombre",
        Some("archivos") => " ORDER BY m.n_archivos DESC, m.nombre",
        Some("reciente") => " ORDER BY m.actualizado DESC, m.id DESC",
        _ => " ORDER BY m.juego, m.faccion, m.nombre, m.estudio, m.subcarpeta",
    });
    qb.push(" LIMIT ")
        .push_bind(por_pagina)
        .push(" OFFSET ")
        .push_bind((pagina - 1) * por_pagina);
    let filas: Vec<FilaModelo> = qb.build_query_as().fetch_all(&st.pool).await?;
    let modelos: Vec<Value> = filas
        .into_iter()
        .map(|mut m| {
            let et = etiquetas_de(m.etiquetas_txt.take());
            let mut v = serde_json::to_value(&m).unwrap_or(Value::Null);
            v["etiquetas"] = json!(et);
            v
        })
        .collect();
    Ok(Json(
        json!({ "total": total, "pagina": pagina, "por_pagina": por_pagina, "modelos": modelos }),
    ))
}

async fn filtros(_u: Usuario, State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    let juegos: Vec<(String, i64)> =
        sqlx::query_as("SELECT juego, count(*) FROM modelos GROUP BY juego ORDER BY 2 DESC")
            .fetch_all(p)
            .await?;
    let facciones: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT juego, faccion, count(*) FROM modelos WHERE faccion <> '' GROUP BY juego, faccion
         ORDER BY juego, faccion",
    )
    .fetch_all(p)
    .await?;
    let estudios: Vec<(String, i64)> = sqlx::query_as(
        "SELECT estudio, count(*) FROM modelos WHERE estudio <> '' GROUP BY estudio ORDER BY 2 DESC, 1",
    )
    .fetch_all(p)
    .await?;
    let etiquetas: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT e.nombre, e.tipo, count(*) FROM modelo_etiquetas me JOIN etiquetas e ON e.id = me.etiqueta_id
         GROUP BY e.id ORDER BY 3 DESC, 1",
    )
    .fetch_all(p)
    .await?;
    Ok(Json(json!({
        "juegos": juegos.iter().map(|(n, c)| json!({ "nombre": n, "modelos": c })).collect::<Vec<_>>(),
        "facciones": facciones.iter().map(|(j, n, c)| json!({ "juego": j, "nombre": n, "modelos": c })).collect::<Vec<_>>(),
        "estudios": estudios.iter().map(|(n, c)| json!({ "nombre": n, "modelos": c })).collect::<Vec<_>>(),
        "etiquetas": etiquetas.iter().map(|(n, t, c)| json!({ "nombre": n, "tipo": t, "modelos": c })).collect::<Vec<_>>(),
    })))
}

// ---------- ficha ----------

#[derive(Debug, Serialize, sqlx::FromRow)]
struct FilaArchivo {
    id: i64,
    ruta: String,
    nombre: String,
    ext: String,
    tipo: String,
    bytes: i64,
    nlink: i64,
    stl_formato: Option<String>,
    triangulos: Option<i64>,
    estado: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct FilaCopia {
    id: i64,
    ubicacion: String,
    remoto: String,
    ruta: String,
    bytes: i64,
    md5: String,
    ruta_original: String,
    entradas: i64,
}

async fn ficha(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    let m: Option<FilaModelo> = sqlx::query_as(
        "SELECT m.id, m.carpeta, m.juego, m.faccion, m.nombre, m.estudio, m.subcarpeta, m.confianza, m.revisado,
                m.n_archivos, m.n_mallas, m.bytes, m.n_frio, m.bytes_frio, m.ruta_nueva, m.pendiente, m.n_avisos,
                (SELECT group_concat(e.nombre, char(31)) FROM modelo_etiquetas me
                 JOIN etiquetas e ON e.id = me.etiqueta_id WHERE me.modelo_id = m.id) AS etiquetas_txt
         FROM modelos m WHERE m.id = ?",
    )
    .bind(id)
    .fetch_optional(p)
    .await?;
    let Some(mut m) = m else {
        return Err(ApiError::NoEncontrado);
    };
    let notas: String = sqlx::query_scalar("SELECT notas FROM modelos WHERE id = ?")
        .bind(id)
        .fetch_one(p)
        .await?;
    let archivos: Vec<FilaArchivo> = sqlx::query_as(
        "SELECT id, ruta, nombre, ext, tipo, bytes, nlink, stl_formato, triangulos, estado
         FROM archivos WHERE modelo_id = ? ORDER BY ruta LIMIT 20000",
    )
    .bind(id)
    .fetch_all(p)
    .await?;
    let copias: Vec<FilaCopia> = sqlx::query_as(
        "SELECT c.id, u.nombre AS ubicacion, u.remoto, c.ruta, c.bytes, c.md5, c.ruta_original,
                (SELECT count(*) FROM copias_contenido cc WHERE cc.copia_id = c.id) AS entradas
         FROM copias_frio c JOIN ubicaciones_frio u ON u.id = c.ubicacion_id
         WHERE c.modelo_id = ? ORDER BY u.nombre, c.ruta",
    )
    .bind(id)
    .fetch_all(p)
    .await?;
    let et = etiquetas_de(m.etiquetas_txt.take());
    let mut v = serde_json::to_value(&m).unwrap_or(Value::Null);
    v["etiquetas"] = json!(et);
    v["notas"] = json!(notas);
    if let Some(c) = edicion::campos(p, id).await? {
        let et = edicion::etiquetas_de(p, id).await?;
        v["avisos"] = json!(edicion::avisos(&c, &et));
    }
    v["archivos"] = json!(archivos);
    v["copias_frio"] = json!(copias);
    Ok(Json(v))
}
