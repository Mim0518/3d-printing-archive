//! Errores de la API: siempre responden JSON `{ "error": "…" }`.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

#[derive(Debug)]
pub enum ApiError {
    NoAutenticado,
    Credenciales,
    Bloqueado(u64),
    Invalido(String),
    NoEncontrado,
    Conflicto(String),
    Interno(anyhow::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (estado, mensaje) = match self {
            ApiError::NoAutenticado => (StatusCode::UNAUTHORIZED, "Inicia sesión".to_string()),
            ApiError::Credenciales => (
                StatusCode::UNAUTHORIZED,
                "Usuario o contraseña incorrectos".to_string(),
            ),
            ApiError::Bloqueado(s) => (
                StatusCode::TOO_MANY_REQUESTS,
                format!("Demasiados intentos; espera {s} segundos"),
            ),
            ApiError::Invalido(m) => (StatusCode::BAD_REQUEST, m),
            ApiError::NoEncontrado => (StatusCode::NOT_FOUND, "No existe".to_string()),
            ApiError::Conflicto(m) => (StatusCode::CONFLICT, m),
            ApiError::Interno(e) => {
                tracing::error!("error interno: {e:#}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Error interno del servidor".to_string(),
                )
            }
        };
        (estado, Json(json!({ "error": mensaje }))).into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        ApiError::Interno(e.into())
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        ApiError::Interno(e)
    }
}
