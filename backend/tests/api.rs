//! Pruebas de la API con una base en memoria.

use std::path::PathBuf;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use printing_archive::{AppState, app, auth, config::Config, db};
use serde_json::{Value, json};
use tower::ServiceExt;

fn config(estaticos: Option<PathBuf>) -> Config {
    Config {
        direccion: "127.0.0.1:0".parse().unwrap(),
        base: ":memory:".into(),
        estaticos,
        cookie_segura: false,
        dias_sesion: 30,
        biblioteca: None,
    }
}

async fn app_con_usuario() -> Router {
    let pool = db::en_memoria().await.unwrap();
    auth::guardar_usuario(&pool, "memo", "contraseña-larga")
        .await
        .unwrap();
    app(AppState::new(pool, config(None)))
}

async fn pedir(app: &Router, req: Request<Body>) -> (StatusCode, Option<String>, Value) {
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let cookie = res
        .headers()
        .get(header::SET_COOKIE)
        .map(|v| v.to_str().unwrap().to_string());
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, cookie, body)
}

fn login(usuario: &str, contrasena: &str) -> Request<Body> {
    Request::post("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({ "usuario": usuario, "contrasena": contrasena }).to_string(),
        ))
        .unwrap()
}

fn con_cookie(metodo: &str, uri: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .method(metodo)
        .uri(uri)
        .header(header::COOKIE, cookie)
        .body(Body::empty())
        .unwrap()
}

/// «sesion=abc; HttpOnly; …» → «sesion=abc»
fn par(set_cookie: &str) -> String {
    set_cookie.split(';').next().unwrap().to_string()
}

#[tokio::test]
async fn salud_responde_sin_sesion() {
    let app = app_con_usuario().await;
    let (st, _, body) = pedir(
        &app,
        Request::get("/api/v1/salud").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body["estado"], "ok");
}

#[tokio::test]
async fn yo_sin_sesion_es_401() {
    let app = app_con_usuario().await;
    let (st, _, body) = pedir(
        &app,
        Request::get("/api/v1/auth/yo").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn login_logout_completo() {
    let app = app_con_usuario().await;

    let (st, cookie, body) = pedir(&app, login("memo", "contraseña-larga")).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body["nombre"], "memo");
    let set = cookie.expect("debe poner la cookie de sesión");
    assert!(set.contains("HttpOnly"));
    assert!(set.contains("SameSite=Strict"));
    let c = par(&set);

    let (st, _, body) = pedir(&app, con_cookie("GET", "/api/v1/auth/yo", &c)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body["nombre"], "memo");

    let (st, _, _) = pedir(&app, con_cookie("POST", "/api/v1/auth/logout", &c)).await;
    assert_eq!(st, StatusCode::OK);

    // la sesión ya no sirve aunque el navegador guarde la cookie vieja
    let (st, _, _) = pedir(&app, con_cookie("GET", "/api/v1/auth/yo", &c)).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn contrasena_o_usuario_incorrectos() {
    let app = app_con_usuario().await;
    let (st, cookie, _) = pedir(&app, login("memo", "otra-cosa")).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    assert!(cookie.is_none());
    let (st, _, _) = pedir(&app, login("nadie", "contraseña-larga")).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn bloqueo_tras_cinco_fallos() {
    let app = app_con_usuario().await;
    for _ in 0..5 {
        let (st, _, _) = pedir(&app, login("memo", "mal")).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);
    }
    // bloqueado aunque ahora la contraseña sea la buena
    let (st, cookie, _) = pedir(&app, login("memo", "contraseña-larga")).await;
    assert_eq!(st, StatusCode::TOO_MANY_REQUESTS);
    assert!(cookie.is_none());
}

#[tokio::test]
async fn cambiar_contrasena_cierra_sesiones() {
    let pool = db::en_memoria().await.unwrap();
    auth::guardar_usuario(&pool, "memo", "contraseña-larga")
        .await
        .unwrap();
    let app = app(AppState::new(pool.clone(), config(None)));
    let (_, cookie, _) = pedir(&app, login("memo", "contraseña-larga")).await;
    let c = par(&cookie.unwrap());
    auth::guardar_usuario(&pool, "memo", "otra-contraseña")
        .await
        .unwrap();
    let (st, _, _) = pedir(&app, con_cookie("GET", "/api/v1/auth/yo", &c)).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn ruta_api_desconocida_es_404_json() {
    let app = app_con_usuario().await;
    let (st, _, body) = pedir(
        &app,
        Request::get("/api/v1/no-existe")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn sirve_el_frontend_y_sus_rutas() {
    let dir = std::env::temp_dir().join(format!("pa-estaticos-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::write(dir.join("index.html"), "<html>inicio</html>").unwrap();
    std::fs::write(dir.join("assets/app.js"), "console.log(1)").unwrap();
    let pool = db::en_memoria().await.unwrap();
    let app = app(AppState::new(pool, config(Some(dir.clone()))));

    for (uri, esperado) in [
        ("/", "inicio"),
        ("/assets/app.js", "console"),
        ("/modelos/42", "inicio"),
    ] {
        let res = app
            .clone()
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK, "{uri}");
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        assert!(String::from_utf8_lossy(&bytes).contains(esperado), "{uri}");
    }
    std::fs::remove_dir_all(dir).ok();
}
