//! Rutas de la API (`/api/v1`).

mod auth;
mod catalogo;
mod edicion;
mod mudanzas;
mod trabajos;

use axum::Router;

use crate::{AppState, error::ApiError};

pub fn api() -> Router<AppState> {
    Router::new()
        .merge(auth::rutas())
        .merge(catalogo::rutas())
        .merge(edicion::rutas())
        .merge(mudanzas::rutas())
        .merge(trabajos::rutas())
        .fallback(|| async { ApiError::NoEncontrado })
}
