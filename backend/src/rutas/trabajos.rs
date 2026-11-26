//! Escaneos y trabajos en segundo plano. Exigen sesión.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    AppState,
    auth::Usuario,
    error::ApiError,
    trabajos::{self, Trabajo},
};

pub fn rutas() -> Router<AppState> {
    Router::new()
        .route("/escaneos", post(escanear))
        .route("/trabajos", get(lista))
        .route("/trabajos/{id}", get(uno))
}

#[derive(Debug, Default, Deserialize)]
struct PedidoEscaneo {
    biblioteca_id: Option<i64>,
}

/// Arranca un escaneo (por omisión, de la biblioteca principal). Responde 202 con el id del trabajo.
async fn escanear(
    _u: Usuario,
    State(st): State<AppState>,
    body: Option<Json<PedidoEscaneo>>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let pedido = body.map(|Json(b)| b).unwrap_or_default();
    let bib = match pedido.biblioteca_id {
        Some(id) => id,
        None => sqlx::query_scalar(
            "SELECT id FROM bibliotecas WHERE tipo = 'principal' ORDER BY id LIMIT 1",
        )
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| {
            ApiError::Invalido(
                "No hay biblioteca principal configurada (ARCHIVO_BIBLIOTECA)".into(),
            )
        })?,
    };
    let id = trabajos::iniciar_escaneo(st.pool.clone(), st.trabajos.clone(), bib).await?;
    Ok((StatusCode::ACCEPTED, Json(json!({ "trabajo_id": id }))))
}

fn con_progreso(st: &AppState, t: Trabajo) -> Value {
    let progreso = st.trabajos.progreso(t.id);
    let resultado: Value = serde_json::from_str(&t.resultado).unwrap_or(Value::Null);
    let mut v = serde_json::to_value(&t).unwrap_or(Value::Null);
    v["resultado"] = resultado;
    v["progreso"] = json!(progreso);
    v
}

async fn uno(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let t: Option<Trabajo> = sqlx::query_as(
        "SELECT id, tipo, estado, detalle, resultado, creado, terminado FROM trabajos WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&st.pool)
    .await?;
    let t = t.ok_or(ApiError::NoEncontrado)?;
    Ok(Json(con_progreso(&st, t)))
}

async fn lista(_u: Usuario, State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let ts: Vec<Trabajo> = sqlx::query_as(
        "SELECT id, tipo, estado, detalle, resultado, creado, terminado FROM trabajos ORDER BY id DESC LIMIT 20",
    )
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(json!(
        ts.into_iter()
            .map(|t| con_progreso(&st, t))
            .collect::<Vec<_>>()
    )))
}
