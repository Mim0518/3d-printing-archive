//! Fase 6: impresiones, existencias y costos (registrar una impresión, que la existencia y el costo cuadren,
//! cambios de acabado y de lugar, mermas, deshacer y precio sugerido).

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicU64},
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

const A: &str = "Wargames/Warhammer 40K/Necrons/Wraiths (SF)";

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
    let base = std::env::temp_dir().join(format!("pa6-{nombre}-{}", std::process::id()));
    fs::remove_dir_all(&base).ok();
    let raiz = base.join("coleccion");
    escribir(&raiz, &format!("{A}/Supported/Torso.photon"), &[7u8; 200]);
    escribir(
        &raiz,
        &format!("{A}/Supported/Torso.stl"),
        b"solid a\nendsolid\n",
    );
    let csv = base.join("hoja.csv");
    fs::write(
        &csv,
        format!(
            "Ruta actual,Juego,Facción / tipo,Modelo,Estudio,Subcarpeta,Etiquetas,Confianza,Notas,Revisado\n\
             {A},Warhammer 40K,Necrons,Wraiths,SF,,,alta,,OK\n"
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
    // lo que habría leído un .photon de verdad: 40 ml en 3 horas
    sqlx::query(
        "UPDATE datos_placa SET res_x = 1440, res_y = 2560, capa_mm = 0.05, exposicion_s = 2, volumen_ml = 40,
                tiempo_s = 10800, error = ''
         WHERE archivo_id = (SELECT id FROM archivos WHERE nombre = 'Torso.photon')",
    )
    .execute(&pool)
    .await
    .unwrap();
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
    (Juguete(base), pool, app, cookie)
}

async fn pedir(
    app: &Router,
    c: &str,
    m: Method,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut b = Request::builder()
        .method(m)
        .uri(uri)
        .header(header::COOKIE, c);
    if body.is_some() {
        b = b.header(header::CONTENT_TYPE, "application/json");
    }
    let res = app
        .clone()
        .oneshot(
            b.body(
                body.map(|v| Body::from(v.to_string()))
                    .unwrap_or_else(Body::empty),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let st = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (st, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

fn cerca(v: &Value, esperado: f64) -> bool {
    v.as_f64().is_some_and(|x| (x - esperado).abs() < 0.011)
}

/// La impresora, el material, un perfil normal, los ajustes y la placa con su modelo confirmado (3 por placa).
async fn taller(app: &Router, c: &str, pool: &SqlitePool) -> (i64, i64, i64, i64, i64) {
    let (_, r) = pedir(
        app,
        c,
        Method::POST,
        "/api/v1/impresoras",
        Some(json!({ "nombre": "Photon", "tecnologia": "resina", "res_x": 1440, "res_y": 2560, "potencia_w": 60 })),
    )
    .await;
    let imp = r["id"].as_i64().unwrap();
    let (_, r) = pedir(
        app,
        c,
        Method::POST,
        "/api/v1/materiales",
        Some(
            json!({ "tipo": "resina", "nombre": "Gris", "presentacion": 1000, "precio": 400,
                     "densidad": 1.1, "queda": 1000 }),
        ),
    )
    .await;
    let mat = r["id"].as_i64().unwrap();
    let (_, r) = pedir(
        app,
        c,
        Method::POST,
        "/api/v1/perfiles",
        Some(
            json!({ "nombre": "Gris 0.05", "impresora_id": imp, "material_id": mat, "capa_mm": 0.05,
                     "parametros": { "exposicion_s": 2.5 }, "predeterminado": true }),
        ),
    )
    .await;
    let perfil = r["id"].as_i64().unwrap();
    let (st, r) = pedir(
        app,
        c,
        Method::PUT,
        "/api/v1/ajustes/costos",
        Some(json!({ "valores": {
            "precio_kwh": 2, "limpieza_resina": 10, "fallas_pct": 10, "margen": 3, "empaque_pieza": 5,
            "imprimacion_pieza": 2, "pintura_material_pieza": 8, "pintura_horas_pieza": 1, "mano_obra_hora": 100
        } })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{r}");
    let placa: i64 = sqlx::query_scalar("SELECT id FROM placas WHERE nombre = 'Torso'")
        .fetch_one(pool)
        .await
        .unwrap();
    let modelo: i64 = sqlx::query_scalar("SELECT id FROM modelos WHERE nombre = 'Wraiths'")
        .fetch_one(pool)
        .await
        .unwrap();
    let (st, r) = pedir(
        app,
        c,
        Method::PUT,
        &format!("/api/v1/placas/{placa}/modelos/{modelo}"),
        Some(json!({ "estado": "confirmado", "cantidad": 3 })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{r}");
    (imp, mat, perfil, placa, modelo)
}

#[tokio::test]
async fn una_impresion_cuadra_existencia_y_costo() {
    let (_j, pool, app, c) = preparar("cuadra").await;
    // los ajustes se validan
    for malo in [
        json!({ "margen": 0.5 }),
        json!({ "otro": 1 }),
        json!({ "precio_kwh": "caro" }),
    ] {
        let (st, _) = pedir(
            &app,
            &c,
            Method::PUT,
            "/api/v1/ajustes/costos",
            Some(json!({ "valores": malo })),
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
    }
    let (imp, mat, perfil, placa, modelo) = taller(&app, &c, &pool).await;

    // lo que propone para la placa: la impresora por el laminado, su perfil normal y el material del perfil
    let (_, pr) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/placas/{placa}/impresion"),
        None,
    )
    .await;
    assert_eq!(pr["impresora_id"], imp, "{pr}");
    assert_eq!(pr["perfil_id"], perfil);
    assert_eq!(pr["material_id"], mat);
    assert_eq!(pr["consumo"], 40.0);
    assert_eq!(pr["unidad"], "ml");
    assert_eq!(pr["consumo_origen"], "laminado");
    assert_eq!(pr["tiempo_s"], 10800);
    assert_eq!(pr["piezas"][0]["cantidad"], 3);
    // 40 ml × $0.40 = 16; luz 60 W × 3 h × $2 = 0.36; limpieza 10; fallas 10% de 26.36 = 2.64 → 29.00
    let est = &pr["estimado"];
    assert!(cerca(&est["desglose"]["total"], 29.0), "{est}");
    assert!(cerca(&est["costo_unitario"], 9.67), "{est}");
    // (9.67 × 3) + 5 = 34.01 → 35
    assert_eq!(est["precio"]["sugerido"], 35.0);

    // se registra: entran 3 a la Caja 1, se descuenta el material y la placa queda probada
    let (st, r) = pedir(
        &app,
        &c,
        Method::POST,
        "/api/v1/impresiones",
        Some(json!({ "placa_id": placa, "impresora_id": imp, "material_id": mat, "perfil_id": perfil,
                     "resultado": "bien", "consumo": 40, "unidad": "ml", "consumo_origen": "laminado",
                     "tiempo_s": 10800,
                     "piezas": [{ "modelo_id": modelo, "cantidad": 3, "ubicacion": " Caja 1 " }] })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{r}");
    let primera = r["id"].as_i64().unwrap();
    assert!(cerca(&r["costo_unitario"], 9.67));
    let queda: f64 = sqlx::query_scalar("SELECT queda FROM materiales WHERE id = ?")
        .bind(mat)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(queda, 960.0);
    let (_, f) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/placas/{placa}"),
        None,
    )
    .await;
    assert_eq!(f["estado"], "probada");
    let (_, ex) = pedir(&app, &c, Method::GET, "/api/v1/existencias", None).await;
    assert_eq!(ex.as_array().unwrap().len(), 1, "{ex}");
    assert_eq!(ex[0]["cantidad"], 3);
    assert_eq!(ex[0]["ubicacion"], "Caja 1");
    assert_eq!(ex[0]["acabado"], "sin_pintar");
    assert!(cerca(&ex[0]["costo_unitario"], 9.67));
    assert_eq!(ex[0]["precio_venta"], 35.0);
    let sin_pintar = ex[0]["id"].as_i64().unwrap();

    // una segunda, parcial: salen 2. El costo es el promedio de las dos
    let (_, r) = pedir(
        &app,
        &c,
        Method::POST,
        "/api/v1/impresiones",
        Some(json!({ "placa_id": placa, "impresora_id": imp, "material_id": mat, "resultado": "parcial",
                     "consumo": 22, "unidad": "g", "tiempo_s": 3600,
                     "piezas": [{ "modelo_id": modelo, "cantidad": 2, "ubicacion": "Caja 1" }] })),
    )
    .await;
    let segunda = r["id"].as_i64().unwrap();
    // 22 g ÷ 1.1 = 20 ml de resina
    assert!(cerca(&r["desglose"]["material"], 8.0), "{r}");
    let u2 = r["costo_unitario"].as_f64().unwrap();
    let promedio = (3.0 * 9.67 + 2.0 * u2) / 5.0;
    let (_, e) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/existencias/{sin_pintar}"),
        None,
    )
    .await;
    assert_eq!(e["cantidad"], 5);
    assert!(cerca(&e["costo_unitario"], promedio), "{e}");
    assert_eq!(e["movimientos"].as_array().unwrap().len(), 2);

    // se pintan 2: pasan a otra existencia con el mismo costo y otro precio
    let (st, r) = pedir(
        &app,
        &c,
        Method::POST,
        &format!("/api/v1/existencias/{sin_pintar}/mover"),
        Some(json!({ "cantidad": 2, "acabado": "pintada" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{r}");
    let pintada = r["destino"].as_i64().unwrap();
    let (_, e) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/existencias/{pintada}"),
        None,
    )
    .await;
    assert_eq!(e["cantidad"], 2);
    assert!(cerca(&e["costo_unitario"], promedio));
    // (costo + 2 de imprimación + 8 de pintura + 1 h × 100) × 3 + 5
    let costo_redondeado = (promedio * 100.0).round() / 100.0;
    assert_eq!(
        e["precio_sugerido"]["sugerido"].as_f64().unwrap(),
        ((costo_redondeado + 110.0) * 3.0 + 5.0).ceil()
    );
    assert_eq!(e["movimientos"][0]["par_acabado"], "sin_pintar");
    // no se pueden pintar más de las que hay, ni mover al mismo lugar
    let (st, _) = pedir(
        &app,
        &c,
        Method::POST,
        &format!("/api/v1/existencias/{sin_pintar}/mover"),
        Some(json!({ "cantidad": 9, "acabado": "pintada" })),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);
    let (st, _) = pedir(
        &app,
        &c,
        Method::POST,
        &format!("/api/v1/existencias/{sin_pintar}/mover"),
        Some(json!({ "cantidad": 1 })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // una se rompe; una merma no suma y no se sacan más de las que hay
    let (st, r) = pedir(
        &app,
        &c,
        Method::POST,
        &format!("/api/v1/existencias/{sin_pintar}/movimientos"),
        Some(json!({ "delta": -1, "motivo": "merma", "notas": "se rompió una lanza" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{r}");
    assert_eq!(r["cantidad"], 2);
    for (cuerpo, esperado) in [
        (
            json!({ "delta": 1, "motivo": "merma" }),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({ "delta": -10, "motivo": "ajuste" }),
            StatusCode::CONFLICT,
        ),
        (
            json!({ "delta": -1, "motivo": "venta" }),
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let (st, _) = pedir(
            &app,
            &c,
            Method::POST,
            &format!("/api/v1/existencias/{sin_pintar}/movimientos"),
            Some(cuerpo),
        )
        .await;
        assert_eq!(st, esperado);
    }

    // deshacer: la segunda sí (sus 2 siguen ahí) y devuelve el material; la primera ya no (sus 3 salieron)
    let (st, r) = pedir(
        &app,
        &c,
        Method::DELETE,
        &format!("/api/v1/impresiones/{segunda}"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{r}");
    let queda: f64 = sqlx::query_scalar("SELECT queda FROM materiales WHERE id = ?")
        .bind(mat)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(queda, 960.0);
    let (st, r) = pedir(
        &app,
        &c,
        Method::DELETE,
        &format!("/api/v1/impresiones/{primera}"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT, "{r}");
    let (_, e) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/existencias/{sin_pintar}"),
        None,
    )
    .await;
    assert_eq!(e["cantidad"], 0);

    // una fallida no mete miniaturas (si trae, es error), pero su costo cuenta
    let (st, _) = pedir(
        &app,
        &c,
        Method::POST,
        "/api/v1/impresiones",
        Some(json!({ "placa_id": placa, "resultado": "fallida",
                     "piezas": [{ "modelo_id": modelo, "cantidad": 1 }] })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, r) = pedir(
        &app,
        &c,
        Method::POST,
        "/api/v1/impresiones",
        Some(json!({ "placa_id": placa, "impresora_id": imp, "material_id": mat, "resultado": "fallida",
                     "consumo": 10, "tiempo_s": 600, "notas": "se despegó" })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{r}");
    assert_eq!(r["piezas"], 0);
    assert!(r["costo_unitario"].is_null());
    let (_, lista) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/impresiones?placa_id={placa}"),
        None,
    )
    .await;
    assert_eq!(lista["total"], 2);
    assert_eq!(lista["impresiones"][0]["resultado"], "fallida");
    assert_eq!(lista["impresiones"][1]["miniaturas"][0]["cantidad"], 3);
    assert_eq!(lista["impresiones"][1]["perfil"], "Gris 0.05");
    assert_eq!(lista["impresiones"][1]["perfil_version"], 1);
    // la placa ya estaba probada: una fallida después no la cambia
    let (_, f) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/placas/{placa}"),
        None,
    )
    .await;
    assert_eq!(f["estado"], "probada");

    // la ficha del modelo: sus existencias (aunque estén en cero) y lo que cuesta
    let (_, m) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/modelos/{modelo}/existencias"),
        None,
    )
    .await;
    assert_eq!(m["existencias"].as_array().unwrap().len(), 2, "{m}");
    assert!(cerca(&m["costo_promedio"], 9.67), "{m}");
    assert_eq!(m["precios"]["sin_pintar"]["sugerido"], 35.0);
    assert_eq!(m["impresiones"]["total"], 2);
}

#[tokio::test]
async fn existencias_a_mano_minimos_y_resumen() {
    let (_j, pool, app, c) = preparar("mano").await;
    let modelo: i64 = sqlx::query_scalar("SELECT id FROM modelos WHERE nombre = 'Wraiths'")
        .fetch_one(&pool)
        .await
        .unwrap();
    // sin ajustes: el resumen dice qué falta y no hay precio sugerido
    let (_, r) = pedir(&app, &c, Method::GET, "/api/v1/existencias/resumen", None).await;
    assert_eq!(r["piezas"], 0, "{r}");
    assert_eq!(r["ajustes_faltan"].as_array().unwrap().len(), 10);
    let (st, r) = pedir(
        &app,
        &c,
        Method::POST,
        "/api/v1/existencias",
        Some(json!({ "modelo_id": modelo, "acabado": "imprimada", "ubicacion": "Vitrina", "cantidad": 4,
                     "costo_unitario": 12 })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{r}");
    let e = r["id"].as_i64().unwrap();
    for malo in [
        json!({ "modelo_id": modelo, "cantidad": 0 }),
        json!({ "modelo_id": modelo, "cantidad": 1, "acabado": "dorada" }),
        json!({ "modelo_id": 9999, "cantidad": 1 }),
    ] {
        let (st, _) = pedir(&app, &c, Method::POST, "/api/v1/existencias", Some(malo)).await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
    }
    let (_, x) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/existencias/{e}"),
        None,
    )
    .await;
    assert_eq!(x["cantidad"], 4);
    assert_eq!(x["costo_unitario"], 12.0);
    assert!(x["precio_sugerido"]["sugerido"].is_null());
    assert_eq!(x["movimientos"][0]["notas"], "Existencia inicial");
    // precio a mano y mínimo
    let (st, _) = pedir(
        &app,
        &c,
        Method::PATCH,
        &format!("/api/v1/existencias/{e}"),
        Some(json!({ "precio": 99, "minimo": 10 })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let (_, lista) = pedir(
        &app,
        &c,
        Method::GET,
        "/api/v1/existencias?ver=bajo_minimo",
        None,
    )
    .await;
    assert_eq!(lista[0]["precio_venta"], 99.0);
    assert_eq!(lista[0]["bajo_minimo"], true);
    assert_eq!(lista[0]["valor_venta"], 396.0);
    // un ajuste que suma conserva el costo promedio
    let (_, _) = pedir(
        &app,
        &c,
        Method::POST,
        &format!("/api/v1/existencias/{e}/movimientos"),
        Some(json!({ "delta": 2, "motivo": "ajuste", "notas": "aparecieron en otra caja" })),
    )
    .await;
    let (_, x) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/existencias/{e}"),
        None,
    )
    .await;
    assert_eq!(x["cantidad"], 6);
    assert_eq!(x["costo_unitario"], 12.0);
    // se mueven 2 a otra caja (mismo acabado)
    let (_, r) = pedir(
        &app,
        &c,
        Method::POST,
        &format!("/api/v1/existencias/{e}/mover"),
        Some(json!({ "cantidad": 2, "ubicacion": "Caja 3" })),
    )
    .await;
    let otra = r["destino"].as_i64().unwrap();
    let (_, x) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/existencias/{otra}"),
        None,
    )
    .await;
    assert_eq!(x["acabado"], "imprimada");
    assert_eq!(x["movimientos"][0]["motivo"], "traslado");
    let (_, r) = pedir(&app, &c, Method::GET, "/api/v1/existencias/resumen", None).await;
    assert_eq!(r["piezas"], 6);
    assert_eq!(r["modelos"], 1);
    assert_eq!(r["bajo_minimo"], 1);
    assert_eq!(r["valor_costo"], 72.0);
    let lugares: Vec<&str> = r["ubicaciones"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["ubicacion"].as_str().unwrap())
        .collect();
    assert_eq!(lugares, ["Vitrina", "Caja 3"]);
    // filtros
    let (_, l) = pedir(
        &app,
        &c,
        Method::GET,
        "/api/v1/existencias?ubicacion=Caja%203",
        None,
    )
    .await;
    assert_eq!(l.as_array().unwrap().len(), 1);
    let (_, l) = pedir(&app, &c, Method::GET, "/api/v1/existencias?q=wrai", None).await;
    assert_eq!(l.as_array().unwrap().len(), 2);
    // el cálculo suelto avisa lo que falta
    let (_, k) = pedir(
        &app,
        &c,
        Method::POST,
        "/api/v1/impresiones/calcular",
        Some(json!({ "consumo": 10, "piezas": 2 })),
    )
    .await;
    assert!(
        k["desglose"]["faltan"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f == "el material"),
        "{k}"
    );
}
