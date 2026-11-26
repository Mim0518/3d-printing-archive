//! Fase 2: edición del catálogo, etiquetas, exportación y mudanzas de carpetas (en una colección de juguete).

use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicU64},
    time::Duration,
};

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use http_body_util::BodyExt;
use printing_archive::{AppState, app, auth, catalogo, config::Config, db, inventario};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tower::ServiceExt;

const CAPTAIN: &str = "Wargames/Warhammer 40K/Space Marines/Captain (Games Workshop)";
const WRAITHS: &str = "Wargames/Warhammer 40K/Necrons/Canoptek Wraiths (Station Forge)";

struct Juguete(PathBuf);

impl Drop for Juguete {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}

fn escribir(raiz: &Path, rel: &str, datos: &[u8]) {
    let p = raiz.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, datos).unwrap();
}

async fn preparar(nombre: &str) -> (Juguete, SqlitePool, Router, String) {
    let raiz = std::env::temp_dir().join(format!("pa2-{nombre}-{}", std::process::id()));
    fs::remove_dir_all(&raiz).ok();
    escribir(
        &raiz,
        &format!("{CAPTAIN}/Supported/captain.stl"),
        b"solid a\nendsolid\n",
    );
    escribir(&raiz, &format!("{CAPTAIN}/captain.jpg"), b"jpg");
    fs::hard_link(
        raiz.join(format!("{CAPTAIN}/captain.jpg")),
        raiz.join("Wargames/portada-enlace.jpg"),
    )
    .unwrap();
    escribir(
        &raiz,
        &format!("{WRAITHS}/wraith.stl"),
        b"solid w\nendsolid\n",
    );
    let csv = raiz.join("hoja.csv");
    fs::write(
        &csv,
        format!(
            "Ruta actual,Juego,Facción / tipo,Modelo,Estudio,Subcarpeta,Etiquetas,Confianza,Notas,Revisado\n\
             {CAPTAIN},Warhammer 40K,Space Marines,Captain,Games Workshop,,Horus Heresy,alta,,OK\n\
             {WRAITHS},Warhammer 40K,Necrons,Canoptek Wraiths,Station Forge,,Socratis,alta,,OK\n"
        ),
    )
    .unwrap();
    let pool = db::en_memoria().await.unwrap();
    db::asegurar_biblioteca_principal(&pool, &raiz)
        .await
        .unwrap();
    auth::guardar_usuario(&pool, "memo", "contraseña-larga")
        .await
        .unwrap();
    inventario::escanear(&pool, 1, Arc::new(AtomicU64::new(0)))
        .await
        .unwrap();
    catalogo::importar_catalogo(&pool, &csv).await.unwrap();
    let config = Config::de_prueba(Some(raiz.clone()), None);
    let app = app(AppState::new(pool.clone(), config));
    let res = app
        .clone()
        .oneshot(
            Request::post("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "usuario": "memo", "contrasena": "contraseña-larga" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let cookie = res.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    (Juguete(raiz), pool, app, cookie)
}

async fn pedir(
    app: &Router,
    c: &str,
    metodo: Method,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut b = Request::builder()
        .method(metodo)
        .uri(uri)
        .header(header::COOKIE, c);
    if body.is_some() {
        b = b.header(header::CONTENT_TYPE, "application/json");
    }
    let req = b
        .body(
            body.map(|v| Body::from(v.to_string()))
                .unwrap_or_else(Body::empty),
        )
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let st = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        st,
        serde_json::from_slice(&bytes)
            .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into())),
    )
}

async fn id_de(pool: &SqlitePool, nombre: &str) -> i64 {
    sqlx::query_scalar("SELECT id FROM modelos WHERE nombre = ?")
        .bind(nombre)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn esperar(app: &Router, c: &str, trabajo: i64) -> Value {
    for _ in 0..200 {
        let (_, v) = pedir(
            app,
            c,
            Method::GET,
            &format!("/api/v1/trabajos/{trabajo}"),
            None,
        )
        .await;
        if v["estado"] != "en_curso" {
            return v;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("el trabajo {trabajo} no terminó");
}

#[tokio::test]
async fn editar_con_historial_y_validacion() {
    let (_j, pool, app, c) = preparar("editar").await;
    let id = id_de(&pool, "Captain").await;
    let (st, v) = pedir(
        &app, &c, Method::PATCH, &format!("/api/v1/modelos/{id}"),
        Some(json!({ "faccion": "Blood Angels", "notas": "  nota nueva ", "etiquetas": ["Horus Heresy", "Sanguinius"] })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(v["cambios"], 3);

    let (_, f) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/modelos/{id}"),
        None,
    )
    .await;
    assert_eq!(f["faccion"], "Blood Angels");
    assert_eq!(f["notas"], "nota nueva");
    assert_eq!(f["pendiente"], true);
    assert_eq!(
        f["ruta_nueva"],
        "Wargames/Warhammer 40K/Blood Angels/Captain (Games Workshop)"
    );
    assert_eq!(f["etiquetas"], json!(["Horus Heresy", "Sanguinius"]));

    let (_, h) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/modelos/{id}/historial"),
        None,
    )
    .await;
    let campos: Vec<&str> = h
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["campo"].as_str().unwrap())
        .collect();
    assert!(
        campos.contains(&"faccion") && campos.contains(&"etiquetas") && campos.contains(&"notas")
    );
    assert_eq!(h[0]["usuario"], "memo");

    // la búsqueda ve lo nuevo
    let (_, l) = pedir(&app, &c, Method::GET, "/api/v1/modelos?q=sanguinius", None).await;
    assert_eq!(l["total"], 1);
    let (_, l) = pedir(&app, &c, Method::GET, "/api/v1/modelos?pendiente=si", None).await;
    assert_eq!(l["total"], 1);

    // guardar lo mismo no cambia nada
    let (_, v) = pedir(
        &app,
        &c,
        Method::PATCH,
        &format!("/api/v1/modelos/{id}"),
        Some(json!({ "faccion": "Blood Angels" })),
    )
    .await;
    assert_eq!(v["cambios"], 0);

    for malo in [
        json!({ "nombre": "Captain: Sword" }),
        json!({ "faccion": "A/B" }),
        json!({ "revisado": "Quizá" }),
        json!({ "subcarpeta": "../x" }),
    ] {
        let (st, v) = pedir(
            &app,
            &c,
            Method::PATCH,
            &format!("/api/v1/modelos/{id}"),
            Some(malo.clone()),
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{malo} → {v}");
    }
}

#[tokio::test]
async fn avisos_de_reglas() {
    let (_j, pool, app, c) = preparar("avisos").await;
    let id = id_de(&pool, "Captain").await;
    pedir(
        &app,
        &c,
        Method::PATCH,
        &format!("/api/v1/modelos/{id}"),
        Some(json!({ "etiquetas": ["Dark Angels"] })),
    )
    .await;
    let (_, f) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/modelos/{id}"),
        None,
    )
    .await;
    assert!(
        f["avisos"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a.as_str().unwrap().contains("ejército propio")),
        "{f}"
    );
    let (_, l) = pedir(&app, &c, Method::GET, "/api/v1/modelos?avisos=si", None).await;
    assert_eq!(l["total"], 1);
}

#[tokio::test]
async fn editar_varios_y_etiquetas() {
    let (_j, pool, app, c) = preparar("varios").await;
    let a = id_de(&pool, "Captain").await;
    let b = id_de(&pool, "Canoptek Wraiths").await;
    let (st, v) = pedir(
        &app, &c, Method::PATCH, "/api/v1/modelos/lote",
        Some(json!({ "ids": [a, b], "cambios": { "confianza": "media" }, "agregar_etiquetas": ["Para vender"], "quitar_etiquetas": ["Socratis"] })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    let (_, l) = pedir(
        &app,
        &c,
        Method::GET,
        "/api/v1/modelos?etiqueta=Para%20vender",
        None,
    )
    .await;
    assert_eq!(l["total"], 2);
    let (_, l) = pedir(
        &app,
        &c,
        Method::GET,
        "/api/v1/modelos?etiqueta=Socratis",
        None,
    )
    .await;
    assert_eq!(l["total"], 0);

    // renombrar «Para vender» a «Horus Heresy» las une
    let (_, et) = pedir(&app, &c, Method::GET, "/api/v1/etiquetas", None).await;
    let pv = et
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["nombre"] == "Para vender")
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    let (st, _) = pedir(
        &app,
        &c,
        Method::PATCH,
        &format!("/api/v1/etiquetas/{pv}"),
        Some(json!({ "nombre": "Horus Heresy", "color": "red" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let (_, et) = pedir(&app, &c, Method::GET, "/api/v1/etiquetas", None).await;
    let hh = et
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["nombre"] == "Horus Heresy")
        .unwrap();
    assert_eq!(
        (hh["modelos"].as_i64(), hh["color"].as_str()),
        (Some(2), Some("red"))
    );
    assert!(
        !et.as_array()
            .unwrap()
            .iter()
            .any(|e| e["nombre"] == "Para vender")
    );
}

#[tokio::test]
async fn exportar_csv() {
    let (_j, _pool, app, c) = preparar("csv").await;
    let (st, v) = pedir(&app, &c, Method::GET, "/api/v1/exportar/catalogo.csv", None).await;
    assert_eq!(st, StatusCode::OK);
    let t = v.as_str().unwrap();
    assert!(t.starts_with("Ruta actual,Archivos,MB,Juego,"));
    assert!(t.contains("Canoptek Wraiths"));
}

#[tokio::test]
async fn mudanza_mueve_y_se_deshace() {
    let (j, pool, app, c) = preparar("mudanza").await;
    let raiz = j.0.clone();
    let id = id_de(&pool, "Captain").await;
    let ino_antes = fs::metadata(raiz.join(format!("{CAPTAIN}/Supported/captain.stl")))
        .unwrap()
        .ino();
    pedir(
        &app,
        &c,
        Method::PATCH,
        &format!("/api/v1/modelos/{id}"),
        Some(json!({ "faccion": "Blood Angels" })),
    )
    .await;

    let (st, plan) = pedir(&app, &c, Method::POST, "/api/v1/mudanzas/plan", None).await;
    assert_eq!(st, StatusCode::OK, "{plan}");
    assert_eq!(plan["archivos"], 2);
    assert_eq!(plan["conflictos"], json!([]));
    let mid = plan["id"].as_i64().unwrap();

    let (st, t) = pedir(
        &app,
        &c,
        Method::POST,
        &format!("/api/v1/mudanzas/{mid}/ejecutar"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::ACCEPTED, "{t}");
    let r = esperar(&app, &c, t["trabajo_id"].as_i64().unwrap()).await;
    assert_eq!(r["estado"], "terminado", "{r}");
    assert_eq!(r["resultado"]["movidos"], 2);

    let nuevo = "Wargames/Warhammer 40K/Blood Angels/Captain (Games Workshop)";
    assert!(
        raiz.join(format!("{nuevo}/Supported/captain.stl"))
            .is_file()
    );
    assert!(
        !raiz.join(CAPTAIN).exists(),
        "la carpeta vieja vacía se quita"
    );
    assert!(
        !raiz.join("Wargames/Warhammer 40K/Space Marines").exists(),
        "y la de la facción si quedó vacía"
    );
    assert_eq!(
        fs::metadata(raiz.join(format!("{nuevo}/Supported/captain.stl")))
            .unwrap()
            .ino(),
        ino_antes,
        "mismo archivo, sin copiar"
    );
    assert!(
        raiz.join("Wargames/portada-enlace.jpg").is_file(),
        "el otro nombre del enlace duro sigue"
    );
    let (_, f) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/modelos/{id}"),
        None,
    )
    .await;
    assert_eq!(f["carpeta"], nuevo);
    assert_eq!(f["pendiente"], false);
    assert_eq!(f["n_archivos"], 2);

    // un escaneo después no ve nada nuevo ni nada que falte
    let e = inventario::escanear(&pool, 1, Arc::new(AtomicU64::new(0)))
        .await
        .unwrap();
    assert_eq!((e.nuevos, e.faltan), (0, 0));

    let (st, t) = pedir(
        &app,
        &c,
        Method::POST,
        &format!("/api/v1/mudanzas/{mid}/deshacer"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::ACCEPTED, "{t}");
    let r = esperar(&app, &c, t["trabajo_id"].as_i64().unwrap()).await;
    assert_eq!(r["estado"], "terminado", "{r}");
    assert!(
        raiz.join(format!("{CAPTAIN}/Supported/captain.stl"))
            .is_file()
    );
    assert!(!raiz.join("Wargames/Warhammer 40K/Blood Angels").exists());
    let (_, f) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/modelos/{id}"),
        None,
    )
    .await;
    assert_eq!(f["carpeta"], CAPTAIN);
    assert_eq!(
        f["pendiente"], true,
        "los campos siguen diciendo Blood Angels"
    );
    let (st, _) = pedir(
        &app,
        &c,
        Method::POST,
        &format!("/api/v1/mudanzas/{mid}/deshacer"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT, "no se deshace dos veces");
}

#[tokio::test]
async fn mudanza_con_choques_no_se_ejecuta() {
    let (j, pool, app, c) = preparar("choques").await;
    let raiz = j.0.clone();
    let id = id_de(&pool, "Captain").await;
    // 1) el destino es la carpeta de otro modelo
    pedir(&app, &c, Method::PATCH, &format!("/api/v1/modelos/{id}"),
          Some(json!({ "faccion": "Necrons", "nombre": "Canoptek Wraiths", "estudio": "Station Forge" }))).await;
    let (_, plan) = pedir(&app, &c, Method::POST, "/api/v1/mudanzas/plan", None).await;
    assert!(!plan["conflictos"].as_array().unwrap().is_empty(), "{plan}");
    let mid = plan["id"].as_i64().unwrap();
    let (_, t) = pedir(
        &app,
        &c,
        Method::POST,
        &format!("/api/v1/mudanzas/{mid}/ejecutar"),
        None,
    )
    .await;
    let r = esperar(&app, &c, t["trabajo_id"].as_i64().unwrap()).await;
    assert_eq!(r["estado"], "fallo");
    assert!(
        raiz.join(format!("{CAPTAIN}/captain.jpg")).is_file(),
        "no se movió nada"
    );

    // 2) el disco cambió después de armar el plan
    pedir(
        &app,
        &c,
        Method::PATCH,
        &format!("/api/v1/modelos/{id}"),
        Some(
            json!({ "faccion": "Blood Angels", "nombre": "Captain", "estudio": "Games Workshop" }),
        ),
    )
    .await;
    let (_, plan) = pedir(&app, &c, Method::POST, "/api/v1/mudanzas/plan", None).await;
    assert_eq!(plan["conflictos"], json!([]), "{plan}");
    fs::write(raiz.join(format!("{CAPTAIN}/captain.jpg")), b"jpg cambiado").unwrap();
    let mid = plan["id"].as_i64().unwrap();
    let (_, t) = pedir(
        &app,
        &c,
        Method::POST,
        &format!("/api/v1/mudanzas/{mid}/ejecutar"),
        None,
    )
    .await;
    let r = esperar(&app, &c, t["trabajo_id"].as_i64().unwrap()).await;
    assert_eq!(r["estado"], "fallo", "{r}");
    assert!(r["detalle"].as_str().unwrap().contains("no se movió nada"));
    assert!(
        raiz.join(format!("{CAPTAIN}/Supported/captain.stl"))
            .is_file()
    );
    assert!(!raiz.join("Wargames/Warhammer 40K/Blood Angels").exists());

    // 3) ya hay un archivo en el destino
    let dest = "Wargames/Warhammer 40K/Blood Angels/Captain (Games Workshop)/captain.jpg";
    escribir(&raiz, dest, b"otro");
    inventario::escanear(&pool, 1, Arc::new(AtomicU64::new(0)))
        .await
        .unwrap();
    let (_, plan) = pedir(&app, &c, Method::POST, "/api/v1/mudanzas/plan", None).await;
    assert!(
        plan["conflictos"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x.as_str().unwrap().contains("ya existe")),
        "{plan}"
    );
    assert_eq!(
        fs::read(raiz.join(dest)).unwrap(),
        b"otro",
        "nunca se sobrescribe"
    );
}
