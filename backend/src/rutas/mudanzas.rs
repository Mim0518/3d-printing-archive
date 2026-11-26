//! Mudanzas de carpetas: armar el plan, ejecutarlo y deshacer la última. Exigen sesión.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{AppState, auth::Usuario, error::ApiError, mudanzas, trabajos};

/// id, estado, usuario, creada, ejecutada, deshecha, resumen
type FilaMudanza = (i64, String, String, i64, Option<i64>, Option<i64>, String);
/// estado, usuario, creada, ejecutada, deshecha, resumen
type DatosMudanza = (String, String, i64, Option<i64>, Option<i64>, String);

pub fn rutas() -> Router<AppState> {
    Router::new()
        .route("/mudanzas", get(lista))
        .route("/mudanzas/plan", post(plan))
        .route("/mudanzas/{id}", get(una))
        .route("/mudanzas/{id}/ejecutar", post(ejecutar))
        .route("/mudanzas/{id}/deshacer", post(deshacer))
}

#[derive(Debug, Default, Deserialize)]
struct PedidoPlan {
    /// Modelos a mover; sin esto, todos los pendientes revisados (OK o Corregido).
    modelo_ids: Option<Vec<i64>>,
}

async fn plan(
    u: Usuario,
    State(st): State<AppState>,
    body: Option<Json<PedidoPlan>>,
) -> Result<Json<Value>, ApiError> {
    let p = body.map(|Json(b)| b).unwrap_or_default();
    let r = mudanzas::armar(&st.pool, p.modelo_ids.as_deref(), &u.nombre).await?;
    Ok(Json(json!(r)))
}

async fn lista(_u: Usuario, State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let filas: Vec<FilaMudanza> = sqlx::query_as(
        "SELECT id, estado, usuario, creada, ejecutada, deshecha, resumen FROM mudanzas
         WHERE estado <> 'descartada' ORDER BY id DESC LIMIT 50",
    )
    .fetch_all(&st.pool)
    .await?;
    let deshacible = mudanzas::deshacible(&st.pool).await?;
    Ok(Json(json!({
        "deshacible": deshacible,
        "mudanzas": filas.iter().map(|(id, e, u, c, x, d, r)| {
            let r: Value = serde_json::from_str(r).unwrap_or(Value::Null);
            json!({ "id": id, "estado": e, "usuario": u, "creada": c, "ejecutada": x, "deshecha": d,
                    "modelos": r["modelos"].as_array().map(Vec::len).unwrap_or(0),
                    "archivos": r["archivos"], "bytes": r["bytes"], "conflictos": r["conflictos"].as_array().map(Vec::len).unwrap_or(0) })
        }).collect::<Vec<_>>(),
    })))
}

async fn una(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let fila: Option<DatosMudanza> = sqlx::query_as(
        "SELECT estado, usuario, creada, ejecutada, deshecha, resumen FROM mudanzas WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&st.pool)
    .await?;
    let (estado, usuario, creada, ejecutada, deshecha, resumen) =
        fila.ok_or(ApiError::NoEncontrado)?;
    let errores: Vec<(String, String)> = sqlx::query_as(
        "SELECT origen, mensaje FROM mudanza_archivos WHERE mudanza_id = ? AND estado = 'error' LIMIT 100",
    )
    .bind(id)
    .fetch_all(&st.pool)
    .await?;
    let cuenta: Vec<(String, i64)> = sqlx::query_as(
        "SELECT estado, count(*) FROM mudanza_archivos WHERE mudanza_id = ? GROUP BY estado",
    )
    .bind(id)
    .fetch_all(&st.pool)
    .await?;
    let muestra: Vec<(String, String)> = sqlx::query_as(
        "SELECT origen, destino FROM mudanza_archivos WHERE mudanza_id = ? ORDER BY id LIMIT 200",
    )
    .bind(id)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(json!({
        "id": id, "estado": estado, "usuario": usuario, "creada": creada, "ejecutada": ejecutada,
        "deshecha": deshecha, "deshacible": mudanzas::deshacible(&st.pool).await? == Some(id),
        "plan": serde_json::from_str::<Value>(&resumen).unwrap_or(Value::Null),
        "archivos_por_estado": cuenta.into_iter().collect::<std::collections::HashMap<_, _>>(),
        "errores": errores.iter().map(|(o, m)| json!({ "archivo": o, "mensaje": m })).collect::<Vec<_>>(),
        "muestra": muestra.iter().map(|(o, d)| json!({ "de": o, "a": d })).collect::<Vec<_>>(),
    })))
}

async fn ejecutar(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let t = trabajos::iniciar_mudanza(st.pool.clone(), st.trabajos.clone(), id, false).await?;
    Ok((StatusCode::ACCEPTED, Json(json!({ "trabajo_id": t }))))
}

async fn deshacer(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    if mudanzas::deshacible(&st.pool).await? != Some(id) {
        return Err(ApiError::Conflicto(
            "Solo se puede deshacer la última mudanza ejecutada".into(),
        ));
    }
    let t = trabajos::iniciar_mudanza(st.pool.clone(), st.trabajos.clone(), id, true).await?;
    Ok((StatusCode::ACCEPTED, Json(json!({ "trabajo_id": t }))))
}
