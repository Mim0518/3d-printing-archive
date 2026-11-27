//! Rutas de la API (`/api/v1`).

mod auth;
mod catalogo;
mod edicion;
mod existencias;
mod miniaturas;
mod mudanzas;
mod placas;
mod taller;
mod trabajos;

use axum::Router;

use crate::{AppState, error::ApiError};

pub fn api() -> Router<AppState> {
    Router::new()
        .merge(auth::rutas())
        .merge(catalogo::rutas())
        .merge(edicion::rutas())
        .merge(existencias::rutas())
        .merge(miniaturas::rutas())
        .merge(mudanzas::rutas())
        .merge(placas::rutas())
        .merge(taller::rutas())
        .merge(trabajos::rutas())
        .fallback(|| async { ApiError::NoEncontrado })
}
