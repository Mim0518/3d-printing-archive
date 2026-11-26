//! Edición del catálogo: un modelo, varios a la vez, historial, etiquetas y exportación a CSV. Exigen sesión.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::header,
    response::IntoResponse,
    routing::{get, patch},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    AppState,
    auth::Usuario,
    edicion::{self, Cambios},
    error::ApiError,
};

pub fn rutas() -> Router<AppState> {
    Router::new()
        .route("/modelos/{id}", patch(editar))
        .route("/modelos/{id}/historial", get(historial))
        .route("/modelos/lote", patch(editar_varios))
        .route("/etiquetas", get(etiquetas))
        .route("/etiquetas/{id}", patch(editar_etiqueta))
        .route("/exportar/catalogo.csv", get(exportar))
}

async fn editar(
    u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(c): Json<Cambios>,
) -> Result<Json<Value>, ApiError> {
    let n = edicion::aplicar(&st.pool, id, &c, &u.nombre).await?;
    Ok(Json(json!({ "cambios": n })))
}

#[derive(Deserialize)]
struct Lote {
    ids: Vec<i64>,
    #[serde(default)]
    cambios: Cambios,
    #[serde(default)]
    agregar_etiquetas: Vec<String>,
    #[serde(default)]
    quitar_etiquetas: Vec<String>,
}

async fn editar_varios(
    u: Usuario,
    State(st): State<AppState>,
    Json(l): Json<Lote>,
) -> Result<Json<Value>, ApiError> {
    if l.cambios.etiquetas.is_some() {
        return Err(ApiError::Invalido(
            "Para varios modelos usa agregar_etiquetas y quitar_etiquetas".into(),
        ));
    }
    let n = edicion::aplicar_varios(
        &st.pool,
        &l.ids,
        &l.cambios,
        &l.agregar_etiquetas,
        &l.quitar_etiquetas,
        &u.nombre,
    )
    .await?;
    Ok(Json(json!({ "modelos": l.ids.len(), "cambios": n })))
}

async fn historial(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(edicion::historial(&st.pool, id).await?)))
}

async fn etiquetas(_u: Usuario, State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let filas: Vec<(i64, String, String, String, i64)> = sqlx::query_as(
        "SELECT e.id, e.nombre, e.tipo, e.color, count(me.modelo_id) FROM etiquetas e
         LEFT JOIN modelo_etiquetas me ON me.etiqueta_id = e.id GROUP BY e.id ORDER BY 5 DESC, 2",
    )
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(json!(
        filas
            .iter()
            .map(|(id, n, t, c, m)| json!({ "id": id, "nombre": n, "tipo": t, "color": c, "modelos": m }))
            .collect::<Vec<_>>()
    )))
}

#[derive(Deserialize)]
struct CambioEtiqueta {
    nombre: Option<String>,
    tipo: Option<String>,
    color: Option<String>,
}

async fn editar_etiqueta(
    u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(c): Json<CambioEtiqueta>,
) -> Result<Json<Value>, ApiError> {
    let destino = edicion::editar_etiqueta(
        &st.pool,
        id,
        c.nombre.as_deref(),
        c.tipo.as_deref(),
        c.color.as_deref(),
        &u.nombre,
    )
    .await?;
    Ok(Json(json!({ "id": destino })))
}

/// El catálogo con las mismas columnas que la antigua pestaña de la hoja (sirve de respaldo y para volver a
/// importarlo con `importar catalogo`).
async fn exportar(_u: Usuario, State(st): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    #[allow(clippy::type_complexity)]
    let filas: Vec<(String, i64, i64, String, String, String, String, String, String, String, String, String, String)> =
        sqlx::query_as(
            "SELECT m.carpeta, m.n_archivos, m.bytes, m.juego, m.faccion, m.nombre, m.estudio, m.subcarpeta,
                    coalesce((SELECT group_concat(e.nombre, '; ') FROM modelo_etiquetas me
                              JOIN etiquetas e ON e.id = me.etiqueta_id WHERE me.modelo_id = m.id), ''),
                    m.ruta_nueva, m.confianza, m.notas, m.revisado
             FROM modelos m ORDER BY m.juego, m.faccion, m.nombre, m.subcarpeta",
        )
        .fetch_all(&st.pool)
        .await?;
    let mut w = csv::Writer::from_writer(Vec::new());
    w.write_record([
        "Ruta actual",
        "Archivos",
        "MB",
        "Juego",
        "Facción / tipo",
        "Modelo",
        "Estudio",
        "Subcarpeta",
        "Etiquetas",
        "Ruta nueva",
        "Confianza",
        "Notas",
        "Revisado",
    ])
    .map_err(|e| ApiError::Interno(e.into()))?;
    for (carpeta, n, bytes, juego, fac, nombre, estudio, sub, et, ruta, conf, notas, rev) in &filas
    {
        let mb = format!("{:.1}", *bytes as f64 / 1e6);
        let n = n.to_string();
        w.write_record([
            carpeta, &n, &mb, juego, fac, nombre, estudio, sub, et, ruta, conf, notas, rev,
        ])
        .map_err(|e| ApiError::Interno(e.into()))?;
    }
    let datos = w
        .into_inner()
        .map_err(|e| ApiError::Interno(anyhow::anyhow!("{e}")))?;
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"catalogo.csv\"",
            ),
        ],
        datos,
    ))
}
