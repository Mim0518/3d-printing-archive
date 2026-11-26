//! Salud del servidor e inicio de sesión.

use std::time::Duration;

use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    AppState,
    auth::{self, COOKIE, Usuario},
    error::ApiError,
};

pub fn rutas() -> Router<AppState> {
    Router::new()
        .route("/salud", get(salud))
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/yo", get(yo))
}

/// Pública: sirve para saber si el servidor y la base responden.
async fn salud(State(st): State<AppState>) -> Json<Value> {
    let base = sqlx::query_scalar::<_, i64>("SELECT 1")
        .fetch_one(&st.pool)
        .await
        .is_ok();
    Json(json!({
        "estado": if base { "ok" } else { "sin base" },
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

#[derive(Deserialize)]
struct Login {
    usuario: String,
    contrasena: String,
}

async fn login(
    State(st): State<AppState>,
    jar: CookieJar,
    Json(body): Json<Login>,
) -> Result<(CookieJar, Json<Usuario>), ApiError> {
    let nombre = body.usuario.trim().to_string();
    if nombre.is_empty() || body.contrasena.is_empty() {
        return Err(ApiError::Invalido("Escribe usuario y contraseña".into()));
    }
    st.intentos.revisar(&nombre).map_err(ApiError::Bloqueado)?;
    let fila: Option<(i64, String)> =
        sqlx::query_as("SELECT id, hash FROM usuarios WHERE nombre = ?")
            .bind(&nombre)
            .fetch_optional(&st.pool)
            .await?;
    let hash = fila
        .as_ref()
        .map(|(_, h)| h.clone())
        .unwrap_or_else(|| auth::hash_de_relleno().to_string());
    let contrasena = body.contrasena;
    let ok = tokio::task::spawn_blocking(move || auth::verificar_contrasena(&contrasena, &hash))
        .await
        .unwrap_or(false);
    let Some((id, _)) = fila.filter(|_| ok) else {
        st.intentos.fallo(&nombre);
        tokio::time::sleep(Duration::from_millis(300)).await;
        return Err(ApiError::Credenciales);
    };
    st.intentos.exito(&nombre);
    let token = auth::crear_sesion(&st.pool, id, st.config.dias_sesion).await?;
    let cookie = Cookie::build((COOKIE, token))
        .http_only(true)
        .same_site(SameSite::Strict)
        .secure(st.config.cookie_segura)
        .path("/")
        .max_age(time::Duration::days(st.config.dias_sesion))
        .build();
    Ok((jar.add(cookie), Json(Usuario { id, nombre })))
}

async fn logout(
    State(st): State<AppState>,
    jar: CookieJar,
) -> Result<(CookieJar, Json<Value>), ApiError> {
    if let Some(c) = jar.get(COOKIE) {
        auth::cerrar_sesion(&st.pool, c.value()).await?;
    }
    let jar = jar.remove(Cookie::build(COOKIE).path("/"));
    Ok((jar, Json(json!({ "ok": true }))))
}

async fn yo(usuario: Usuario) -> Json<Usuario> {
    Json(usuario)
}
