//! Miniaturas, archivos para el visor 3D y portada de cada modelo. Exigen sesión.

use axum::{
    Json, Router,
    body::Body,
    extract::{Path, Query, Request, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use serde::Deserialize;
use serde_json::{Value, json};
use tower::ServiceExt;
use tower_http::services::ServeFile;

use crate::{
    AppState,
    auth::Usuario,
    db::ahora,
    error::ApiError,
    miniaturas::{self, SQL_RENDERIZABLE},
    trabajos,
};

pub fn rutas() -> Router<AppState> {
    Router::new()
        .route("/miniaturas", post(generar))
        .route("/miniaturas/resumen", get(resumen))
        .route("/archivos/{id}/miniatura", get(miniatura))
        .route("/archivos/{id}/contenido", get(contenido))
        .route("/modelos/{id}/portada", put(portada))
}

#[derive(Debug, Default, Deserialize)]
struct PedidoMiniaturas {
    modelo_ids: Option<Vec<i64>>,
    #[serde(default)]
    forzar: bool,
    /// Solo las vistas previas de escenas y laminados.
    #[serde(default)]
    solo_placas: bool,
}

async fn generar(
    _u: Usuario,
    State(st): State<AppState>,
    body: Option<Json<PedidoMiniaturas>>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let p = body.map(|Json(b)| b).unwrap_or_default();
    let id = trabajos::iniciar_miniaturas(
        st.pool.clone(),
        st.trabajos.clone(),
        st.config.opciones_miniaturas(),
        miniaturas::Cuales {
            modelo_ids: p.modelo_ids,
            solo_placas: p.solo_placas,
        },
        p.forzar,
    )
    .await?;
    Ok((StatusCode::ACCEPTED, Json(json!({ "trabajo_id": id }))))
}

async fn resumen(_u: Usuario, State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let sql = format!(
        "SELECT count(*),
                coalesce(sum(mi.estado = 'ok' AND mi.clave = printf('%x-%x-%x', a.inode, a.bytes, a.mtime_ms)), 0),
                coalesce(sum(mi.estado = 'error'), 0)
         FROM archivos a JOIN bibliotecas b ON b.id = a.biblioteca_id AND b.tipo = 'principal'
         LEFT JOIN miniaturas mi ON mi.archivo_id = a.id
         WHERE a.estado = 'presente' AND {SQL_RENDERIZABLE}"
    );
    let (total, ok, errores): (i64, i64, i64) = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .fetch_one(&st.pool)
        .await?;
    let en_curso: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM trabajos WHERE tipo = 'miniaturas' AND estado = 'en_curso' LIMIT 1",
    )
    .fetch_optional(&st.pool)
    .await?;
    Ok(Json(json!({
        "archivos": total, "con_miniatura": ok, "errores": errores,
        "pendientes": total - ok - errores, "en_curso": en_curso,
    })))
}

#[derive(Debug, Deserialize)]
struct Tamano {
    t: Option<u32>,
}

/// La miniatura en WebP (512 o 128 px). La URL lleva la clave, así que el navegador la guarda para siempre.
async fn miniatura(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Query(q): Query<Tamano>,
) -> Result<Response, ApiError> {
    let t = if q.t == Some(128) { 128 } else { 512 };
    let clave: Option<String> =
        sqlx::query_scalar("SELECT clave FROM miniaturas WHERE archivo_id = ? AND estado = 'ok'")
            .bind(id)
            .fetch_optional(&st.pool)
            .await?;
    let clave = clave.ok_or(ApiError::NoEncontrado)?;
    let ruta = miniaturas::ruta_cache(&st.config.miniaturas, &clave, t);
    let datos = tokio::fs::read(&ruta)
        .await
        .map_err(|_| ApiError::NoEncontrado)?;
    Ok((
        [
            (header::CONTENT_TYPE, "image/webp"),
            (
                header::CACHE_CONTROL,
                "private, max-age=31536000, immutable",
            ),
        ],
        datos,
    )
        .into_response())
}

/// El archivo tal cual (para el visor 3D o para descargarlo). Acepta rangos.
async fn contenido(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    req: Request,
) -> Result<Response, ApiError> {
    let fila: Option<(String, String, String)> = sqlx::query_as(
        "SELECT b.ruta, a.ruta, a.nombre FROM archivos a JOIN bibliotecas b ON b.id = a.biblioteca_id
         WHERE a.id = ? AND a.estado = 'presente'",
    )
    .bind(id)
    .fetch_optional(&st.pool)
    .await?;
    let (raiz, ruta, nombre) = fila.ok_or(ApiError::NoEncontrado)?;
    if ruta.split('/').any(|s| s == ".." || s.is_empty()) {
        return Err(ApiError::NoEncontrado);
    }
    let p = std::path::Path::new(&raiz).join(&ruta);
    let res = ServeFile::new(p)
        .oneshot(req.map(|_| Body::empty()))
        .await
        .map_err(|e| ApiError::Interno(anyhow::anyhow!("{e}")))?;
    let mut res = res.map(Body::new);
    let disp = format!("inline; filename*=UTF-8''{}", codificar(&nombre));
    if let Ok(v) = disp.parse() {
        res.headers_mut().insert(header::CONTENT_DISPOSITION, v);
    }
    res.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("private, max-age=3600"),
    );
    Ok(res)
}

/// Codificación de RFC 5987 para el nombre del archivo.
fn codificar(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[derive(Debug, Deserialize)]
struct PedidoPortada {
    /// Archivo del modelo; `null` vuelve a la portada automática.
    archivo_id: Option<i64>,
}

async fn portada(
    u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(p): Json<PedidoPortada>,
) -> Result<Json<Value>, ApiError> {
    if let Some(a) = p.archivo_id {
        let dueno: Option<Option<i64>> =
            sqlx::query_scalar("SELECT modelo_id FROM archivos WHERE id = ?")
                .bind(a)
                .fetch_optional(&st.pool)
                .await?;
        if dueno.flatten() != Some(id) {
            return Err(ApiError::Invalido(
                "Ese archivo no es de este modelo".into(),
            ));
        }
    }
    let antes: Option<Option<i64>> =
        sqlx::query_scalar("SELECT portada_archivo_id FROM modelos WHERE id = ?")
            .bind(id)
            .fetch_optional(&st.pool)
            .await?;
    let antes = antes.ok_or(ApiError::NoEncontrado)?;
    sqlx::query("UPDATE modelos SET portada_archivo_id = ? WHERE id = ?")
        .bind(p.archivo_id)
        .bind(id)
        .execute(&st.pool)
        .await?;
    let txt = |v: Option<i64>| {
        v.map(|x| format!("archivo {x}"))
            .unwrap_or_else(|| "automática".into())
    };
    sqlx::query(
        "INSERT INTO historial (modelo_id, campo, antes, despues, usuario, fecha) VALUES (?, 'portada', ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(txt(antes))
    .bind(txt(p.archivo_id))
    .bind(&u.nombre)
    .bind(ahora())
    .execute(&st.pool)
    .await?;
    Ok(Json(json!({ "ok": true })))
}
