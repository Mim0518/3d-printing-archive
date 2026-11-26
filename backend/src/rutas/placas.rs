//! Placas y escenas: lista con filtros, ficha, edición, confirmación de modelos, placas hechas a mano y la
//! detección en segundo plano. Exigen sesión.

use std::collections::HashMap;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post, put},
};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use crate::{
    AppState,
    auth::Usuario,
    db::ahora,
    error::ApiError,
    placas::{self, SQL_PORTADA},
    trabajos,
};

pub fn rutas() -> Router<AppState> {
    Router::new()
        .route("/placas", get(lista).post(crear))
        .route("/placas/resumen", get(resumen))
        .route("/placas/detectar", post(detectar))
        .route("/placas/confirmar", post(confirmar))
        .route("/placas/{id}", get(ficha).patch(editar).delete(borrar))
        .route("/placas/{id}/portada", put(portada))
        .route("/placas/{id}/archivos", post(poner_archivos))
        .route(
            "/placas/{id}/modelos/{modelo_id}",
            put(poner_modelo).delete(quitar_modelo),
        )
        .route("/modelos/{id}/placas", get(del_modelo))
}

const ESTADOS: [&str; 3] = ["sin_probar", "probada", "fallida"];

// ---------- lista ----------

#[derive(Debug, Default, Deserialize)]
pub struct Filtros {
    q: Option<String>,
    /// por_confirmar | confirmadas | sin_modelo | sin_sugerencia
    revision: Option<String>,
    /// sin_probar | probada | fallida
    estado: Option<String>,
    /// «si»
    favorita: Option<String>,
    /// «si»: solo las que traen un laminado
    laminado: Option<String>,
    juego: Option<String>,
    modelo_id: Option<i64>,
    /// nombre (por omisión: carpeta y nombre) | reciente | laminado (las que tienen laminado primero)
    orden: Option<String>,
    pagina: Option<i64>,
    por_pagina: Option<i64>,
}

fn no_vacio(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

fn condiciones(qb: &mut QueryBuilder<Sqlite>, f: &Filtros) {
    qb.push(" WHERE 1 = 1");
    if let Some(q) = no_vacio(&f.q) {
        let patron = format!("%{}%", q.replace(['%', '_'], ""));
        qb.push(" AND (p.nombre LIKE ")
            .push_bind(patron.clone())
            .push(" OR p.carpeta LIKE ")
            .push_bind(patron.clone())
            .push(
                " OR EXISTS (SELECT 1 FROM placa_modelos pm JOIN modelos m ON m.id = pm.modelo_id
                   WHERE pm.placa_id = p.id AND pm.estado <> 'rechazado' AND m.nombre LIKE ",
            )
            .push_bind(patron)
            .push("))");
    }
    match no_vacio(&f.revision) {
        Some("por_confirmar") => {
            qb.push(
                " AND p.sin_modelo = 0 AND NOT EXISTS (SELECT 1 FROM placa_modelos pm
                   WHERE pm.placa_id = p.id AND pm.estado = 'confirmado')",
            );
        }
        Some("confirmadas") => {
            qb.push(
                " AND EXISTS (SELECT 1 FROM placa_modelos pm WHERE pm.placa_id = p.id AND pm.estado = 'confirmado')",
            );
        }
        Some("sin_modelo") => {
            qb.push(" AND p.sin_modelo = 1");
        }
        Some("sin_sugerencia") => {
            qb.push(
                " AND p.sin_modelo = 0 AND NOT EXISTS (SELECT 1 FROM placa_modelos pm
                   WHERE pm.placa_id = p.id AND pm.estado <> 'rechazado')",
            );
        }
        _ => {}
    }
    if let Some(e) = no_vacio(&f.estado) {
        qb.push(" AND p.estado = ").push_bind(e.to_string());
    }
    if no_vacio(&f.favorita) == Some("si") {
        qb.push(" AND p.favorita = 1");
    }
    if no_vacio(&f.laminado) == Some("si") {
        qb.push(" AND EXISTS (SELECT 1 FROM placa_archivos pa WHERE pa.placa_id = p.id AND pa.rol = 'laminado')");
    }
    if let Some(j) = no_vacio(&f.juego) {
        qb.push(
            " AND EXISTS (SELECT 1 FROM placa_modelos pm JOIN modelos m ON m.id = pm.modelo_id
              WHERE pm.placa_id = p.id AND pm.estado <> 'rechazado' AND m.juego = ",
        )
        .push_bind(j.to_string())
        .push(")");
    }
    if let Some(m) = f.modelo_id {
        qb.push(
            " AND EXISTS (SELECT 1 FROM placa_modelos pm WHERE pm.placa_id = p.id AND pm.estado <> 'rechazado'
              AND pm.modelo_id = ",
        )
        .push_bind(m)
        .push(")");
    }
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct FilaPlaca {
    id: i64,
    nombre: String,
    carpeta: String,
    origen: String,
    estado: String,
    favorita: bool,
    sin_modelo: bool,
    notas: String,
    n_archivos: i64,
    n_presentes: i64,
    #[serde(skip)]
    portada_id: Option<i64>,
}

static SELECT_PLACA: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    format!(
        "SELECT p.id, p.nombre, p.carpeta, p.origen, p.estado, p.favorita, p.sin_modelo, p.notas,
                (SELECT count(*) FROM placa_archivos pa WHERE pa.placa_id = p.id) AS n_archivos,
                (SELECT count(*) FROM placa_archivos pa JOIN archivos a ON a.id = pa.archivo_id
                 WHERE pa.placa_id = p.id AND a.estado = 'presente') AS n_presentes,
                {SQL_PORTADA} AS portada_id
         FROM placas p"
    )
});

#[derive(Debug, Serialize, sqlx::FromRow)]
struct DatosArchivo {
    #[serde(skip)]
    placa_id: i64,
    #[serde(skip)]
    archivo_id: i64,
    ext: String,
    rol: String,
    formato: Option<String>,
    maquina: Option<String>,
    res_x: Option<i64>,
    res_y: Option<i64>,
    capa_mm: Option<f64>,
    exposicion_s: Option<f64>,
    exposicion_base_s: Option<f64>,
    capas_base: Option<i64>,
    capas: Option<i64>,
    alto_mm: Option<f64>,
    tiempo_s: Option<i64>,
    volumen_ml: Option<f64>,
    peso_g: Option<f64>,
    n_placas: Option<i64>,
    error: Option<String>,
}

const SELECT_DATOS: &str = "SELECT pa.placa_id, pa.archivo_id, a.ext, pa.rol, d.formato, d.maquina, d.res_x,
        d.res_y, d.capa_mm, d.exposicion_s, d.exposicion_base_s, d.capas_base, d.capas, d.alto_mm, d.tiempo_s,
        d.volumen_ml, d.peso_g, d.n_placas, d.error
    FROM placa_archivos pa JOIN archivos a ON a.id = pa.archivo_id
    LEFT JOIN datos_placa d ON d.archivo_id = pa.archivo_id";

#[derive(Debug, Serialize, sqlx::FromRow)]
struct ModeloDePlaca {
    #[serde(skip)]
    placa_id: i64,
    id: i64,
    nombre: String,
    faccion: String,
    juego: String,
    estudio: String,
    carpeta: String,
    estado: String,
    origen: String,
    cantidad: Option<i64>,
}

fn en_lista(qb: &mut QueryBuilder<Sqlite>, ids: &[i64]) {
    qb.push(" (");
    let mut sep = qb.separated(", ");
    for id in ids {
        sep.push_bind(*id);
    }
    qb.push(")");
}

/// `{ id, clave }` de las miniaturas ok de estos archivos.
async fn claves(pool: &SqlitePool, ids: &[i64]) -> Result<HashMap<i64, String>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT archivo_id, clave FROM miniaturas WHERE estado = 'ok' AND archivo_id IN",
    );
    en_lista(&mut qb, ids);
    Ok(qb
        .build_query_as()
        .fetch_all(pool)
        .await?
        .into_iter()
        .collect())
}

fn portada_json(id: Option<i64>, claves: &HashMap<i64, String>) -> Value {
    match id.and_then(|p| claves.get(&p).map(|c| (p, c))) {
        Some((p, c)) => json!({ "id": p, "clave": c }),
        None => Value::Null,
    }
}

async fn modelos_de(
    pool: &SqlitePool,
    placas: &[i64],
    con_rechazados: bool,
) -> Result<Vec<ModeloDePlaca>, sqlx::Error> {
    if placas.is_empty() {
        return Ok(Vec::new());
    }
    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT pm.placa_id, m.id, m.nombre, m.faccion, m.juego, m.estudio, m.carpeta, pm.estado, pm.origen,
                pm.cantidad
         FROM placa_modelos pm JOIN modelos m ON m.id = pm.modelo_id WHERE pm.placa_id IN",
    );
    en_lista(&mut qb, placas);
    if !con_rechazados {
        qb.push(" AND pm.estado <> 'rechazado'");
    }
    qb.push(" ORDER BY pm.estado = 'rechazado', m.juego, m.faccion, m.nombre");
    qb.build_query_as().fetch_all(pool).await
}

async fn datos_de(pool: &SqlitePool, placas: &[i64]) -> Result<Vec<DatosArchivo>, sqlx::Error> {
    if placas.is_empty() {
        return Ok(Vec::new());
    }
    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(SELECT_DATOS);
    qb.push(" WHERE pa.placa_id IN");
    en_lista(&mut qb, placas);
    qb.push(" ORDER BY pa.rol <> 'laminado', a.mtime_ms DESC");
    qb.build_query_as().fetch_all(pool).await
}

/// Filas de la lista (también para la ficha de un modelo).
async fn listar(pool: &SqlitePool, f: &Filtros) -> Result<Value, ApiError> {
    let por_pagina = f.por_pagina.unwrap_or(48).clamp(1, 500);
    let pagina = f.pagina.unwrap_or(1).max(1);
    let mut cuenta: QueryBuilder<Sqlite> = QueryBuilder::new("SELECT count(*) FROM placas p");
    condiciones(&mut cuenta, f);
    let total: i64 = cuenta.build_query_scalar().fetch_one(pool).await?;

    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(SELECT_PLACA.as_str());
    condiciones(&mut qb, f);
    qb.push(match no_vacio(&f.orden) {
        // las que tienen laminado (listas para imprimir) primero
        Some("laminado") => {
            " ORDER BY NOT EXISTS (SELECT 1 FROM placa_archivos pa WHERE pa.placa_id = p.id AND pa.rol = 'laminado'),
                       p.carpeta, p.nombre"
        }
        Some("reciente") => {
            " ORDER BY (SELECT max(a.mtime_ms) FROM placa_archivos pa JOIN archivos a ON a.id = pa.archivo_id
                        WHERE pa.placa_id = p.id) DESC, p.id DESC"
        }
        _ => " ORDER BY p.carpeta, p.nombre",
    });
    qb.push(" LIMIT ")
        .push_bind(por_pagina)
        .push(" OFFSET ")
        .push_bind((pagina - 1) * por_pagina);
    let filas: Vec<FilaPlaca> = qb.build_query_as().fetch_all(pool).await?;
    let ids: Vec<i64> = filas.iter().map(|p| p.id).collect();
    let cl = claves(
        pool,
        &filas
            .iter()
            .filter_map(|p| p.portada_id)
            .collect::<Vec<_>>(),
    )
    .await?;
    let mut modelos: HashMap<i64, Vec<ModeloDePlaca>> = HashMap::new();
    for m in modelos_de(pool, &ids, false).await? {
        modelos.entry(m.placa_id).or_default().push(m);
    }
    let mut datos: HashMap<i64, Vec<DatosArchivo>> = HashMap::new();
    for d in datos_de(pool, &ids).await? {
        datos.entry(d.placa_id).or_default().push(d);
    }
    let placas: Vec<Value> = filas
        .into_iter()
        .map(|p| {
            let mut v = serde_json::to_value(&p).unwrap_or(Value::Null);
            v["portada"] = portada_json(p.portada_id, &cl);
            let ds = datos.remove(&p.id).unwrap_or_default();
            let mut formatos: Vec<&str> = ds.iter().map(|d| d.ext.as_str()).collect();
            formatos.sort();
            formatos.dedup();
            v["formatos"] = json!(formatos);
            v["autoguardados"] = json!(ds.iter().filter(|d| d.rol == "autoguardado").count());
            v["laminados"] = json!(
                ds.iter()
                    .filter(|d| d.rol == "laminado")
                    .collect::<Vec<_>>()
            );
            v["n_placas"] = json!(ds.iter().find_map(|d| d.n_placas));
            v["modelos"] = json!(modelos.remove(&p.id).unwrap_or_default());
            v
        })
        .collect();
    Ok(json!({ "total": total, "pagina": pagina, "por_pagina": por_pagina, "placas": placas }))
}

async fn lista(
    _u: Usuario,
    State(st): State<AppState>,
    Query(f): Query<Filtros>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(listar(&st.pool, &f).await?))
}

async fn del_modelo(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let f = Filtros {
        modelo_id: Some(id),
        orden: Some("laminado".into()),
        por_pagina: Some(500),
        ..Default::default()
    };
    Ok(Json(listar(&st.pool, &f).await?))
}

async fn resumen(_u: Usuario, State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    #[allow(clippy::type_complexity)]
    let (
        total,
        confirmadas,
        por_confirmar,
        sin_modelo,
        sin_sugerencia,
        con_laminado,
        favoritas,
        probadas,
        fallidas,
    ): (i64, i64, i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT count(*),
                coalesce(sum(EXISTS (SELECT 1 FROM placa_modelos pm WHERE pm.placa_id = p.id
                                     AND pm.estado = 'confirmado')), 0),
                coalesce(sum(p.sin_modelo = 0 AND NOT EXISTS (SELECT 1 FROM placa_modelos pm
                    WHERE pm.placa_id = p.id AND pm.estado = 'confirmado')), 0),
                coalesce(sum(p.sin_modelo), 0),
                coalesce(sum(p.sin_modelo = 0 AND NOT EXISTS (SELECT 1 FROM placa_modelos pm
                    WHERE pm.placa_id = p.id AND pm.estado <> 'rechazado')), 0),
                coalesce(sum(EXISTS (SELECT 1 FROM placa_archivos pa WHERE pa.placa_id = p.id
                                     AND pa.rol = 'laminado')), 0),
                coalesce(sum(p.favorita), 0),
                coalesce(sum(p.estado = 'probada'), 0),
                coalesce(sum(p.estado = 'fallida'), 0)
         FROM placas p",
    )
    .fetch_one(p)
    .await?;
    let sugeridas_carpeta: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM placa_modelos WHERE estado = 'sugerido' AND origen = 'carpeta'",
    )
    .fetch_one(p)
    .await?;
    // archivos de placas que pueden llevar miniatura y todavía no la tienen (las que fallaron no cuentan)
    let vistas_pendientes: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT count(*) FROM placa_archivos pa JOIN archivos a ON a.id = pa.archivo_id
         LEFT JOIN miniaturas mi ON mi.archivo_id = a.id
         WHERE a.estado = 'presente' AND {}
           AND (mi.archivo_id IS NULL
                OR (mi.estado = 'ok' AND mi.clave <> printf('%x-%x-%x', a.inode, a.bytes, a.mtime_ms)))",
        crate::miniaturas::SQL_RENDERIZABLE
    )))
    .fetch_one(p)
    .await?;
    let en_curso: Option<(i64, String)> = sqlx::query_as(
        "SELECT id, tipo FROM trabajos WHERE tipo IN ('placas', 'escaneo', 'miniaturas') AND estado = 'en_curso'
         LIMIT 1",
    )
    .fetch_optional(p)
    .await?;
    Ok(Json(json!({
        "total": total,
        "confirmadas": confirmadas,
        "sin_modelo": sin_modelo,
        "por_confirmar": por_confirmar,
        "sin_sugerencia": sin_sugerencia,
        "sugerencias_por_carpeta": sugeridas_carpeta,
        "con_laminado": con_laminado,
        "favoritas": favoritas,
        "probadas": probadas,
        "fallidas": fallidas,
        "vistas_pendientes": vistas_pendientes,
        "en_curso": en_curso.map(|(id, tipo)| json!({ "id": id, "tipo": tipo })),
    })))
}

async fn detectar(
    _u: Usuario,
    State(st): State<AppState>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let id = trabajos::iniciar_placas(st.pool.clone(), st.trabajos.clone()).await?;
    Ok((StatusCode::ACCEPTED, Json(json!({ "trabajo_id": id }))))
}

// ---------- ficha ----------

#[derive(Debug, Serialize, sqlx::FromRow)]
struct ArchivoDePlaca {
    id: i64,
    ruta: String,
    nombre: String,
    ext: String,
    tipo: String,
    bytes: i64,
    mtime_ms: i64,
    estado: String,
    rol: String,
    auto: bool,
    modelo_id: Option<i64>,
    miniatura: Option<String>,
}

async fn existe(pool: &SqlitePool, id: i64) -> Result<(), ApiError> {
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM placas WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?;
    if n == 0 {
        return Err(ApiError::NoEncontrado);
    }
    Ok(())
}

async fn ficha(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    let fila: Option<FilaPlaca> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{} WHERE p.id = ?",
        *SELECT_PLACA
    )))
    .bind(id)
    .fetch_optional(p)
    .await?;
    let fila = fila.ok_or(ApiError::NoEncontrado)?;
    let (elegida, gemela, creada, actualizada): (Option<i64>, Option<i64>, i64, i64) =
        sqlx::query_as(
            "SELECT portada_archivo_id, gemela_id, creada, actualizada FROM placas WHERE id = ?",
        )
        .bind(id)
        .fetch_one(p)
        .await?;
    let archivos: Vec<ArchivoDePlaca> = sqlx::query_as(
        "SELECT a.id, a.ruta, a.nombre, a.ext, a.tipo, a.bytes, a.mtime_ms, a.estado, pa.rol, pa.auto, a.modelo_id,
                mi.clave AS miniatura
         FROM placa_archivos pa JOIN archivos a ON a.id = pa.archivo_id
         LEFT JOIN miniaturas mi ON mi.archivo_id = a.id AND mi.estado = 'ok'
         WHERE pa.placa_id = ? ORDER BY pa.rol = 'autoguardado', pa.rol, a.mtime_ms DESC",
    )
    .bind(id)
    .fetch_all(p)
    .await?;
    let mut datos: HashMap<i64, DatosArchivo> = datos_de(p, &[id])
        .await?
        .into_iter()
        .map(|d| (d.archivo_id, d))
        .collect();
    let modelos = modelos_de(p, &[id], true).await?;
    let gemela_fila: Option<(i64, String, Option<String>)> = match gemela {
        Some(g) => {
            sqlx::query_as(
                "SELECT a.id, a.ruta, mi.clave FROM archivos a
                 LEFT JOIN miniaturas mi ON mi.archivo_id = a.id AND mi.estado = 'ok' WHERE a.id = ?",
            )
            .bind(g)
            .fetch_optional(p)
            .await?
        }
        None => None,
    };
    let cl = claves(p, &fila.portada_id.into_iter().collect::<Vec<_>>()).await?;
    let mut v = serde_json::to_value(&fila).unwrap_or(Value::Null);
    v["portada"] = portada_json(fila.portada_id, &cl);
    v["portada_elegida"] = json!(elegida);
    v["creada"] = json!(creada);
    v["actualizada"] = json!(actualizada);
    v["archivos"] = json!(
        archivos
            .iter()
            .map(|a| {
                let mut x = serde_json::to_value(a).unwrap_or(Value::Null);
                x["datos"] = json!(datos.remove(&a.id));
                x
            })
            .collect::<Vec<_>>()
    );
    v["modelos"] = json!(modelos);
    v["gemela"] = match gemela_fila {
        Some((gid, ruta, clave)) => json!({ "id": gid, "ruta": ruta, "miniatura": clave }),
        None => Value::Null,
    };
    Ok(Json(v))
}

// ---------- edición ----------

#[derive(Debug, Deserialize)]
struct CambiosPlaca {
    nombre: Option<String>,
    estado: Option<String>,
    favorita: Option<bool>,
    sin_modelo: Option<bool>,
    notas: Option<String>,
}

async fn editar(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(c): Json<CambiosPlaca>,
) -> Result<Json<Value>, ApiError> {
    existe(&st.pool, id).await?;
    if let Some(n) = &c.nombre
        && (n.trim().is_empty() || n.chars().count() > 200)
    {
        return Err(ApiError::Invalido(
            "El nombre no puede quedar vacío ni pasar de 200 letras".into(),
        ));
    }
    if let Some(e) = &c.estado
        && !ESTADOS.contains(&e.as_str())
    {
        return Err(ApiError::Invalido(format!("Estado desconocido: {e}")));
    }
    if c.notas.as_ref().is_some_and(|n| n.chars().count() > 5000) {
        return Err(ApiError::Invalido("Las notas pasan de 5,000 letras".into()));
    }
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "UPDATE placas SET nombre = coalesce(?, nombre), estado = coalesce(?, estado),
            favorita = coalesce(?, favorita), sin_modelo = coalesce(?, sin_modelo), notas = coalesce(?, notas),
            actualizada = ? WHERE id = ?",
    )
    .bind(c.nombre.as_deref().map(str::trim))
    .bind(&c.estado)
    .bind(c.favorita)
    .bind(c.sin_modelo)
    .bind(c.notas.as_deref().map(str::trim))
    .bind(ahora())
    .bind(id)
    .execute(&mut *tx)
    .await?;
    if c.sin_modelo == Some(true) {
        // sin modelo: las sugerencias sobran (lo confirmado y lo rechazado se queda)
        sqlx::query("DELETE FROM placa_modelos WHERE placa_id = ? AND estado = 'sugerido'")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    if c.sin_modelo == Some(false) {
        placas::sugerir(&st.pool).await?;
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Debug, Deserialize)]
struct PedidoPortada {
    archivo_id: Option<i64>,
}

/// Portada a mano: un archivo de la placa, su malla gemela o un archivo de uno de sus modelos.
async fn portada(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(p): Json<PedidoPortada>,
) -> Result<Json<Value>, ApiError> {
    existe(&st.pool, id).await?;
    if let Some(a) = p.archivo_id {
        let valido: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM archivos a WHERE a.id = ? AND (
                EXISTS (SELECT 1 FROM placa_archivos pa WHERE pa.archivo_id = a.id AND pa.placa_id = ?)
                OR EXISTS (SELECT 1 FROM placas x WHERE x.id = ? AND x.gemela_id = a.id)
                OR EXISTS (SELECT 1 FROM placa_modelos pm WHERE pm.placa_id = ? AND pm.modelo_id = a.modelo_id
                           AND pm.estado <> 'rechazado'))",
        )
        .bind(a)
        .bind(id)
        .bind(id)
        .bind(id)
        .fetch_one(&st.pool)
        .await?;
        if valido == 0 {
            return Err(ApiError::Invalido(
                "Ese archivo no es de la placa ni de sus modelos".into(),
            ));
        }
    }
    sqlx::query("UPDATE placas SET portada_archivo_id = ?, actualizada = ? WHERE id = ?")
        .bind(p.archivo_id)
        .bind(ahora())
        .bind(id)
        .execute(&st.pool)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

/// `null` en el JSON es «sin cantidad»; si no viene, no se toca.
fn doble<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<i64>>, D::Error> {
    Option::<i64>::deserialize(d).map(Some)
}

#[derive(Debug, Deserialize)]
struct PedidoModelo {
    estado: Option<String>,
    #[serde(default, deserialize_with = "doble")]
    cantidad: Option<Option<i64>>,
}

/// Confirma, rechaza o agrega un modelo a la placa, o cambia su cantidad.
async fn poner_modelo(
    _u: Usuario,
    State(st): State<AppState>,
    Path((id, modelo)): Path<(i64, i64)>,
    Json(p): Json<PedidoModelo>,
) -> Result<Json<Value>, ApiError> {
    existe(&st.pool, id).await?;
    let hay: i64 = sqlx::query_scalar("SELECT count(*) FROM modelos WHERE id = ?")
        .bind(modelo)
        .fetch_one(&st.pool)
        .await?;
    if hay == 0 {
        return Err(ApiError::Invalido("Ese modelo no existe".into()));
    }
    if let Some(e) = &p.estado
        && e != "confirmado"
        && e != "rechazado"
    {
        return Err(ApiError::Invalido(
            "El estado va «confirmado» o «rechazado»".into(),
        ));
    }
    if let Some(Some(c)) = p.cantidad
        && !(1..=1000).contains(&c)
    {
        return Err(ApiError::Invalido(
            "La cantidad va de 1 a 1,000 (o vacía si la placa solo trae partes)".into(),
        ));
    }
    let t = ahora();
    let mut tx = st.pool.begin().await?;
    let antes: Option<String> =
        sqlx::query_scalar("SELECT estado FROM placa_modelos WHERE placa_id = ? AND modelo_id = ?")
            .bind(id)
            .bind(modelo)
            .fetch_optional(&mut *tx)
            .await?;
    match antes {
        Some(_) => {
            let mut qb: QueryBuilder<Sqlite> =
                QueryBuilder::new("UPDATE placa_modelos SET actualizado = ");
            qb.push_bind(t);
            if let Some(e) = &p.estado {
                qb.push(", estado = ").push_bind(e.clone());
            }
            if let Some(c) = p.cantidad {
                qb.push(", cantidad = ").push_bind(c);
            }
            qb.push(" WHERE placa_id = ")
                .push_bind(id)
                .push(" AND modelo_id = ")
                .push_bind(modelo);
            qb.build().execute(&mut *tx).await?;
        }
        None => {
            sqlx::query(
                "INSERT INTO placa_modelos (placa_id, modelo_id, estado, origen, cantidad, actualizado)
                 VALUES (?, ?, ?, 'manual', ?, ?)",
            )
            .bind(id)
            .bind(modelo)
            .bind(p.estado.as_deref().unwrap_or("confirmado"))
            .bind(p.cantidad.flatten())
            .bind(t)
            .execute(&mut *tx)
            .await?;
        }
    }
    if p.estado.as_deref().unwrap_or("confirmado") == "confirmado" {
        sqlx::query("UPDATE placas SET sin_modelo = 0, actualizada = ? WHERE id = ?")
            .bind(t)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

/// Quita un modelo que se puso a mano; uno sugerido queda rechazado para que no vuelva.
async fn quitar_modelo(
    _u: Usuario,
    State(st): State<AppState>,
    Path((id, modelo)): Path<(i64, i64)>,
) -> Result<Json<Value>, ApiError> {
    let origen: Option<String> =
        sqlx::query_scalar("SELECT origen FROM placa_modelos WHERE placa_id = ? AND modelo_id = ?")
            .bind(id)
            .bind(modelo)
            .fetch_optional(&st.pool)
            .await?;
    match origen.as_deref() {
        None => return Err(ApiError::NoEncontrado),
        Some("manual") => {
            sqlx::query("DELETE FROM placa_modelos WHERE placa_id = ? AND modelo_id = ?")
                .bind(id)
                .bind(modelo)
                .execute(&st.pool)
                .await?;
        }
        Some(_) => {
            sqlx::query(
                "UPDATE placa_modelos SET estado = 'rechazado', actualizado = ? WHERE placa_id = ? AND modelo_id = ?",
            )
            .bind(ahora())
            .bind(id)
            .bind(modelo)
            .execute(&st.pool)
            .await?;
        }
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Debug, Default, Deserialize)]
struct PedidoConfirmar {
    placa_ids: Option<Vec<i64>>,
}

/// Confirma sugerencias de golpe: las de las placas indicadas, o todas las que vienen de la carpeta.
async fn confirmar(
    _u: Usuario,
    State(st): State<AppState>,
    body: Option<Json<PedidoConfirmar>>,
) -> Result<Json<Value>, ApiError> {
    let p = body.map(|Json(b)| b).unwrap_or_default();
    let mut qb: QueryBuilder<Sqlite> =
        QueryBuilder::new("UPDATE placa_modelos SET estado = 'confirmado', actualizado = ");
    qb.push_bind(ahora());
    qb.push(" WHERE estado = 'sugerido'");
    match &p.placa_ids {
        Some(ids) if ids.is_empty() => return Ok(Json(json!({ "confirmadas": 0 }))),
        Some(ids) => {
            qb.push(" AND placa_id IN");
            en_lista(&mut qb, ids);
        }
        None => {
            qb.push(" AND origen = 'carpeta'");
        }
    }
    let n = qb.build().execute(&st.pool).await?.rows_affected();
    Ok(Json(json!({ "confirmadas": n })))
}

// ---------- placas hechas a mano ----------

#[derive(Debug, Deserialize)]
struct PedidoArchivos {
    archivo_ids: Vec<i64>,
}

/// Revisa que los archivos existan y puedan ir en una placa; devuelve (id, carpeta, nombre, tipo, biblioteca).
async fn archivos_validos(
    pool: &SqlitePool,
    ids: &[i64],
) -> Result<Vec<(i64, String, String, String, i64)>, ApiError> {
    if ids.is_empty() {
        return Err(ApiError::Invalido("Elige al menos un archivo".into()));
    }
    if ids.len() > 500 {
        return Err(ApiError::Invalido("Son demasiados archivos".into()));
    }
    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT id, carpeta, nombre, tipo, biblioteca_id FROM archivos
         WHERE (tipo IN ('escena', 'laminado') OR ext = '3mf') AND id IN",
    );
    en_lista(&mut qb, ids);
    let filas: Vec<(i64, String, String, String, i64)> =
        qb.build_query_as().fetch_all(pool).await?;
    if filas.len() != {
        let mut u = ids.to_vec();
        u.sort();
        u.dedup();
        u.len()
    } {
        return Err(ApiError::Invalido(
            "Algún archivo no existe o no es una escena, un laminado ni un 3MF".into(),
        ));
    }
    Ok(filas)
}

async fn mover_a(
    pool: &SqlitePool,
    placa: i64,
    archivos: &[(i64, String, String, String, i64)],
) -> Result<(), ApiError> {
    let mut tx = pool.begin().await?;
    for (id, carpeta, nombre, tipo, _) in archivos {
        let rol = placas::lugar(carpeta, nombre, tipo).rol;
        sqlx::query(
            "INSERT INTO placa_archivos (archivo_id, placa_id, rol, auto) VALUES (?, ?, ?, 0)
             ON CONFLICT (archivo_id) DO UPDATE SET placa_id = excluded.placa_id, rol = excluded.rol, auto = 0",
        )
        .bind(id)
        .bind(placa)
        .bind(rol)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("UPDATE placas SET actualizada = ? WHERE id = ?")
        .bind(ahora())
        .bind(placa)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    placas::limpiar(pool).await?;
    placas::sugerir(pool).await?;
    Ok(())
}

/// Pasa archivos a esta placa (por ejemplo, para unir dos placas que se detectaron separadas).
async fn poner_archivos(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(p): Json<PedidoArchivos>,
) -> Result<Json<Value>, ApiError> {
    existe(&st.pool, id).await?;
    let archivos = archivos_validos(&st.pool, &p.archivo_ids).await?;
    mover_a(&st.pool, id, &archivos).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Debug, Deserialize)]
struct PlacaNueva {
    nombre: String,
    archivo_ids: Vec<i64>,
    #[serde(default)]
    modelo_ids: Vec<i64>,
}

/// Arma una placa a mano con archivos que ya están en la colección.
async fn crear(
    _u: Usuario,
    State(st): State<AppState>,
    Json(p): Json<PlacaNueva>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let nombre = p.nombre.trim();
    if nombre.is_empty() || nombre.chars().count() > 200 {
        return Err(ApiError::Invalido(
            "El nombre no puede quedar vacío ni pasar de 200 letras".into(),
        ));
    }
    let archivos = archivos_validos(&st.pool, &p.archivo_ids).await?;
    let t = ahora();
    let (_, carpeta, _, _, bib) = &archivos[0];
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO placas (nombre, biblioteca_id, carpeta, origen, creada, actualizada)
         VALUES (?, ?, ?, 'manual', ?, ?) RETURNING id",
    )
    .bind(nombre)
    .bind(bib)
    .bind(carpeta)
    .bind(t)
    .bind(t)
    .fetch_one(&st.pool)
    .await?;
    for m in &p.modelo_ids {
        sqlx::query(
            "INSERT INTO placa_modelos (placa_id, modelo_id, estado, origen, actualizado)
             SELECT ?, id, 'confirmado', 'manual', ? FROM modelos WHERE id = ?
             ON CONFLICT (placa_id, modelo_id) DO NOTHING",
        )
        .bind(id)
        .bind(t)
        .bind(m)
        .execute(&st.pool)
        .await?;
    }
    mover_a(&st.pool, id, &archivos).await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

/// Borra una placa hecha a mano; sus archivos vuelven a las placas que se detectan solas.
async fn borrar(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let origen: Option<String> = sqlx::query_scalar("SELECT origen FROM placas WHERE id = ?")
        .bind(id)
        .fetch_optional(&st.pool)
        .await?;
    match origen.as_deref() {
        None => return Err(ApiError::NoEncontrado),
        Some("manual") => {}
        Some(_) => {
            return Err(ApiError::Invalido(
                "Solo se borran las placas hechas a mano; una detectada vuelve a aparecer mientras sus archivos estén"
                    .into(),
            ));
        }
    }
    sqlx::query("DELETE FROM placas WHERE id = ?")
        .bind(id)
        .execute(&st.pool)
        .await?;
    placas::detectar(&st.pool).await?;
    Ok(Json(json!({ "ok": true })))
}
