//! Base de datos SQLite: conexión, migraciones y utilidades de tiempo.

use std::{
    path::Path,
    str::FromStr,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::Context;
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};

/// Abre (o crea) la base en `ruta` con WAL y llaves foráneas, y aplica las migraciones.
pub async fn abrir(ruta: &Path) -> anyhow::Result<SqlitePool> {
    if let Some(dir) = ruta.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("no se pudo crear la carpeta {}", dir.display()))?;
    }
    let opciones = SqliteConnectOptions::new()
        .filename(ruta)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(opciones)
        .await
        .with_context(|| format!("no se pudo abrir la base {}", ruta.display()))?;
    migrar(&pool).await?;
    Ok(pool)
}

/// Base en memoria para pruebas (una sola conexión, para que todas vean los mismos datos).
pub async fn en_memoria() -> anyhow::Result<SqlitePool> {
    let opciones = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opciones)
        .await?;
    migrar(&pool).await?;
    Ok(pool)
}

async fn migrar(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .context("no se pudieron aplicar las migraciones")
}

/// Segundos Unix de ahora.
pub fn ahora() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
