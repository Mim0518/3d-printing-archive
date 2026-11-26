//! Usuario único con contraseña (argon2id) y sesiones por cookie.
//!
//! La cookie lleva un token aleatorio de 32 bytes; en la base solo se guarda su SHA-256, así que una copia de la
//! base no sirve para entrar. Tras 5 intentos fallidos seguidos con el mismo usuario, se bloquea un minuto.

use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use anyhow::anyhow;
use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};
use axum::{extract::FromRequestParts, http::request::Parts};
use axum_extra::extract::cookie::CookieJar;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::{AppState, db::ahora, error::ApiError};

pub const COOKIE: &str = "sesion";
const MAX_FALLOS: u32 = 5;
const BLOQUEO: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct Usuario {
    pub id: i64,
    pub nombre: String,
}

pub fn hash_contrasena(contrasena: &str) -> anyhow::Result<String> {
    Argon2::default()
        .hash_password(contrasena.as_bytes())
        .map(|h| h.to_string())
        .map_err(|e| anyhow!("no se pudo calcular el hash: {e}"))
}

pub fn verificar_contrasena(contrasena: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|h| {
            Argon2::default()
                .verify_password(contrasena.as_bytes(), &h)
                .is_ok()
        })
        .unwrap_or(false)
}

/// Hash de relleno para que un usuario que no existe tarde lo mismo que una contraseña equivocada.
pub fn hash_de_relleno() -> &'static str {
    static H: OnceLock<String> = OnceLock::new();
    H.get_or_init(|| hash_contrasena("relleno-sin-uso").unwrap_or_default())
}

/// Crea el usuario o le cambia la contraseña. Al cambiarla se cierran todas sus sesiones.
pub async fn guardar_usuario(
    pool: &SqlitePool,
    nombre: &str,
    contrasena: &str,
) -> anyhow::Result<()> {
    let hash = hash_contrasena(contrasena)?;
    sqlx::query(
        "INSERT INTO usuarios (nombre, hash, creado) VALUES (?, ?, ?)
         ON CONFLICT (nombre) DO UPDATE SET hash = excluded.hash",
    )
    .bind(nombre)
    .bind(hash)
    .bind(ahora())
    .execute(pool)
    .await?;
    sqlx::query(
        "DELETE FROM sesiones WHERE usuario_id = (SELECT id FROM usuarios WHERE nombre = ?)",
    )
    .bind(nombre)
    .execute(pool)
    .await?;
    Ok(())
}

fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// Abre una sesión y devuelve el token para la cookie.
pub async fn crear_sesion(pool: &SqlitePool, usuario_id: i64, dias: i64) -> anyhow::Result<String> {
    let token = hex::encode(rand::random::<[u8; 32]>());
    let t = ahora();
    sqlx::query("DELETE FROM sesiones WHERE expira <= ?")
        .bind(t)
        .execute(pool)
        .await?;
    sqlx::query(
        "INSERT INTO sesiones (token_hash, usuario_id, creada, expira, ultimo_uso) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(hash_token(&token))
    .bind(usuario_id)
    .bind(t)
    .bind(t + dias * 86_400)
    .bind(t)
    .execute(pool)
    .await?;
    Ok(token)
}

pub async fn usuario_de_sesion(pool: &SqlitePool, token: &str) -> anyhow::Result<Option<Usuario>> {
    let h = hash_token(token);
    let u = sqlx::query_as::<_, Usuario>(
        "SELECT u.id, u.nombre FROM sesiones s JOIN usuarios u ON u.id = s.usuario_id
         WHERE s.token_hash = ? AND s.expira > ?",
    )
    .bind(&h)
    .bind(ahora())
    .fetch_optional(pool)
    .await?;
    if u.is_some() {
        sqlx::query("UPDATE sesiones SET ultimo_uso = ? WHERE token_hash = ?")
            .bind(ahora())
            .bind(&h)
            .execute(pool)
            .await?;
    }
    Ok(u)
}

pub async fn cerrar_sesion(pool: &SqlitePool, token: &str) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM sesiones WHERE token_hash = ?")
        .bind(hash_token(token))
        .execute(pool)
        .await?;
    Ok(())
}

/// Intentos fallidos de inicio de sesión por usuario (en memoria).
#[derive(Default)]
pub struct Intentos(Mutex<HashMap<String, (u32, Option<Instant>)>>);

impl Intentos {
    /// `Err(segundos)` si el usuario está bloqueado.
    pub fn revisar(&self, nombre: &str) -> Result<(), u64> {
        let mut m = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, Some(hasta))) = m.get(nombre) {
            let ahora = Instant::now();
            if *hasta > ahora {
                return Err((*hasta - ahora).as_secs().max(1));
            }
            m.remove(nombre);
        }
        Ok(())
    }

    pub fn fallo(&self, nombre: &str) {
        let mut m = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let e = m.entry(nombre.to_string()).or_insert((0, None));
        e.0 += 1;
        if e.0 >= MAX_FALLOS {
            e.1 = Some(Instant::now() + BLOQUEO);
        }
    }

    pub fn exito(&self, nombre: &str) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(nombre);
    }
}

/// Extractor: la ruta exige sesión. Responde 401 si no hay.
impl FromRequestParts<AppState> for Usuario {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar
            .get(COOKIE)
            .map(|c| c.value().to_string())
            .ok_or(ApiError::NoAutenticado)?;
        usuario_de_sesion(&state.pool, &token)
            .await?
            .ok_or(ApiError::NoAutenticado)
    }
}
