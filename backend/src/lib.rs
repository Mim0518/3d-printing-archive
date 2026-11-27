//! 3D Printing Archive: servidor (API JSON + frontend compilado).

pub mod almacen;
pub mod auth;
pub mod catalogo;
pub mod config;
pub mod costos;
pub mod db;
pub mod edicion;
pub mod error;
pub mod estructura;
pub mod frio;
pub mod inventario;
pub mod lectores;
pub mod miniaturas;
pub mod mudanzas;
pub mod placas;
pub mod rutas;
pub mod taller;
pub mod trabajos;
pub mod transferencias;

use std::sync::Arc;

use axum::Router;
use sqlx::SqlitePool;
use tower_http::{
    compression::CompressionLayer,
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

use crate::{auth::Intentos, config::Config, trabajos::Trabajos};

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: Arc<Config>,
    pub intentos: Arc<Intentos>,
    pub trabajos: Arc<Trabajos>,
}

impl AppState {
    pub fn new(pool: SqlitePool, config: Config) -> Self {
        Self {
            pool,
            config: Arc::new(config),
            intentos: Arc::new(Intentos::default()),
            trabajos: Arc::new(Trabajos::default()),
        }
    }
}

/// La aplicación completa: `/api/v1/…` y, si hay frontend compilado, todo lo demás va a él
/// (con `index.html` para las rutas del lado del cliente).
pub fn app(state: AppState) -> Router {
    let mut router = Router::new().nest("/api/v1", rutas::api());
    if let Some(dir) = &state.config.estaticos {
        let index = ServeFile::new(dir.join("index.html"));
        // `fallback` (no `not_found_service`): las rutas del cliente deben responder 200, no 404
        router = router.fallback_service(ServeDir::new(dir).fallback(index));
    }
    router
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
