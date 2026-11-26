//! Rutas de la API (`/api/v1`).

mod auth;
mod catalogo;
mod trabajos;

use axum::Router;

use crate::{AppState, error::ApiError};

pub fn api() -> Router<AppState> {
    Router::new()
        .merge(auth::rutas())
        .merge(catalogo::rutas())
        .merge(trabajos::rutas())
        .fallback(|| async { ApiError::NoEncontrado })
}
