//! Fase 4: placas y escenas (detección, sugerencias de modelo, confirmación, placas a mano y vistas previas).

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicU64},
};

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use http_body_util::BodyExt;
use printing_archive::{
    AppState, app, auth, catalogo, config::Config, db, inventario, miniaturas, placas,
};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tower::ServiceExt;

const A: &str = "Wargames/Warhammer 40K/Necrons/Wraiths (SF)";
const B: &str = "Wargames/Warhammer 40K/Orks/Boyz Mob (GW)";

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

/// Una escena de Lychee mínima con su vista previa.
fn lychee() -> Vec<u8> {
    let mut png = Vec::new();
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(8, 8, image::Rgb([9, 9, 200])))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let json = format!(
        r#"{{"mangoFiles":{{"scene.bin":{{"size":3,"offset":"0"}},"preview.png":{{"size":{},"offset":"3"}}}}}}"#,
        png.len()
    );
    let indice = (8 + json.len()).div_ceil(4) * 4;
    let mut v = Vec::new();
    for x in [4, indice as u32, indice as u32 - 4, json.len() as u32] {
        v.extend_from_slice(&x.to_le_bytes());
    }
    v.extend_from_slice(json.as_bytes());
    v.resize(8 + indice, 0);
    v.extend_from_slice(b"abc");
    v.extend_from_slice(&png);
    v
}

fn zip(raiz: &Path, rel: &str, entradas: &[&str]) {
    let p = raiz.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    let mut z = zip::ZipWriter::new(fs::File::create(p).unwrap());
    let op =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for e in entradas {
        z.start_file(*e, op).unwrap();
        if e.ends_with(".png") {
            let mut png = Vec::new();
            image::DynamicImage::ImageRgb8(image::RgbImage::new(4, 4))
                .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                .unwrap();
            z.write_all(&png).unwrap();
        } else {
            z.write_all(b"x").unwrap();
        }
    }
    z.finish().unwrap();
}

async fn preparar(nombre: &str) -> (Juguete, SqlitePool, Router, String, Config) {
    let base = std::env::temp_dir().join(format!("pa4-{nombre}-{}", std::process::id()));
    fs::remove_dir_all(&base).ok();
    let raiz = base.join("coleccion");
    // Wraiths: una escena con su laminado y sus autoguardados, una escena sin vista previa con su malla
    // gemela, y una carpeta de autoguardados sin escena (renombrada)
    escribir(&raiz, &format!("{A}/Supported/Torso Main.lys"), &lychee());
    escribir(
        &raiz,
        &format!("{A}/Supported/Torso Main.photon"),
        &[7u8; 200],
    ); // no es un photon de verdad
    escribir(
        &raiz,
        &format!("{A}/Supported/Torso Main_autosave/Torso Main_2025-01-01_10-00-00.lyt"),
        &lychee(),
    );
    escribir(&raiz, &format!("{A}/Supported/Arm.chitubox"), b"chitubox");
    escribir(
        &raiz,
        &format!("{A}/Supported/Arm.stl"),
        b"solid a\nendsolid\n",
    );
    escribir(
        &raiz,
        &format!("{A}/Escuadra_autosave/Grey Knights_2023-08-18_12-58-34.lyt"),
        &lychee(),
    );
    escribir(&raiz, &format!("{B}/b.obj"), b"v 0 0 0\n");
    // fuera de los modelos: una placa con el nombre de un modelo, una de calibración y dos 3MF
    escribir(&raiz, "Placas/Boyz Mob x10.chitubox", b"chitubox");
    escribir(&raiz, "Printers/SLA/R_E_R_F.pm4n", b"ANYCUBIC basura");
    escribir(&raiz, "Printers/SLA/Calibrado.pwsr", b"[]");
    zip(
        &raiz,
        "Utilidades/caja.3mf",
        &[
            "3D/3dmodel.model",
            "Metadata/project_settings.config",
            "Metadata/plate_1.png",
        ],
    );
    zip(&raiz, "Utilidades/pieza.3mf", &["3D/3dmodel.model"]);
    let csv = base.join("hoja.csv");
    fs::write(
        &csv,
        format!(
            "Ruta actual,Juego,Facción / tipo,Modelo,Estudio,Subcarpeta,Etiquetas,Confianza,Notas,Revisado\n\
             {A},Warhammer 40K,Necrons,Wraiths,SF,,,alta,,OK\n{B},Warhammer 40K,Orks,Boyz Mob,GW,,,alta,,OK\n"
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
    // primero se escanea sin catálogo (las placas se detectan sin modelo) y luego se importa (se sugieren)
    inventario::escanear(&pool, 1, Arc::new(AtomicU64::new(0)))
        .await
        .unwrap();
    catalogo::importar_catalogo(&pool, &csv).await.unwrap();
    let mut config = Config::de_prueba(Some(raiz.clone()), None);
    config.miniaturas = base.join("cache");
    config.f3d = "/no/existe/f3d".into();
    let app = app(AppState::new(pool.clone(), config.clone()));
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
    (Juguete(base), pool, app, cookie, config)
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

async fn placa_de(pool: &SqlitePool, nombre_archivo: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT pa.placa_id FROM placa_archivos pa JOIN archivos a ON a.id = pa.archivo_id WHERE a.nombre = ?",
    )
    .bind(nombre_archivo)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn modelo(pool: &SqlitePool, nombre: &str) -> i64 {
    sqlx::query_scalar("SELECT id FROM modelos WHERE nombre = ?")
        .bind(nombre)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn agrupa_y_sugiere_modelos() {
    let (_j, pool, app, c, _) = preparar("agrupa").await;
    let filas: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT p.nombre, p.carpeta, group_concat(a.nombre || ':' || pa.rol, ' | ')
         FROM placas p JOIN placa_archivos pa ON pa.placa_id = p.id JOIN archivos a ON a.id = pa.archivo_id
         GROUP BY p.id ORDER BY p.carpeta, p.nombre",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let nombres: Vec<&str> = filas.iter().map(|f| f.0.as_str()).collect();
    // .pwsr (perfil de resina) y el 3MF que solo es malla no son placas
    assert_eq!(
        nombres,
        [
            "Boyz Mob x10",
            "R_E_R_F",
            "caja",
            "Escuadra",
            "Arm",
            "Torso Main"
        ],
        "{filas:#?}"
    );
    let torso = &filas[5].2;
    assert!(torso.contains("Torso Main.lys:escena"), "{torso}");
    assert!(torso.contains("Torso Main.photon:laminado"), "{torso}");
    assert!(torso.contains(":autoguardado"), "{torso}");
    assert_eq!(filas[3].1, A, "el autoguardado va en la carpeta de arriba");

    // la gemela de la escena sin vista previa
    let gemela: Option<String> = sqlx::query_scalar(
        "SELECT a.nombre FROM placas p JOIN archivos a ON a.id = p.gemela_id WHERE p.nombre = 'Arm'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(gemela.as_deref(), Some("Arm.stl"));

    // sugerencias: por la carpeta, por el nombre, o ninguna
    let sug: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT p.nombre, m.nombre, pm.origen FROM placa_modelos pm JOIN placas p ON p.id = pm.placa_id
         JOIN modelos m ON m.id = pm.modelo_id WHERE pm.estado = 'sugerido' ORDER BY p.nombre",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let esperado: Vec<(String, String, String)> = [
        ("Arm", "Wraiths", "carpeta"),
        ("Boyz Mob x10", "Boyz Mob", "nombre"),
        ("Escuadra", "Wraiths", "carpeta"),
        ("Torso Main", "Wraiths", "carpeta"),
    ]
    .iter()
    .map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string()))
    .collect();
    assert_eq!(sug, esperado);

    // los datos del laminado que no se pudo leer quedan con su error, sin tumbar la detección
    let error: String = sqlx::query_scalar(
        "SELECT d.error FROM datos_placa d JOIN archivos a ON a.id = d.archivo_id WHERE a.nombre = 'Torso Main.photon'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!error.is_empty());

    // la segunda vez no se vuelve a leer nada ni cambia nada
    let r = placas::detectar(&pool).await.unwrap();
    assert_eq!(
        (r.leidos, r.nuevas, r.placas, r.sugerencias_nuevas),
        (0, 0, 6, 0),
        "{r:?}"
    );

    let (st, v) = pedir(&app, &c, Method::GET, "/api/v1/placas/resumen", None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["total"], 6);
    assert_eq!(v["por_confirmar"], 6);
    assert_eq!(v["sin_sugerencia"], 2);
    assert_eq!(v["sugerencias_por_carpeta"], 3);
    assert_eq!(v["con_laminado"], 2);
}

#[tokio::test]
async fn confirma_rechaza_y_lo_rechazado_no_vuelve() {
    let (_j, pool, app, c, _) = preparar("confirma").await;
    let wraiths = modelo(&pool, "Wraiths").await;
    let boyz = modelo(&pool, "Boyz Mob").await;
    let arm = placa_de(&pool, "Arm.chitubox").await;
    let suelta = placa_de(&pool, "Boyz Mob x10.chitubox").await;
    let calibracion = placa_de(&pool, "R_E_R_F.pm4n").await;

    // rechazar la sugerencia de Arm y confirmar el resto de lo que viene de la carpeta
    let (st, _) = pedir(
        &app,
        &c,
        Method::DELETE,
        &format!("/api/v1/placas/{arm}/modelos/{wraiths}"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let (_, v) = pedir(&app, &c, Method::POST, "/api/v1/placas/confirmar", None).await;
    assert_eq!(v["confirmadas"], 2, "solo las de carpeta: {v}");
    // la sugerencia por nombre se confirma placa por placa, con su cantidad
    let (st, _) = pedir(
        &app,
        &c,
        Method::PUT,
        &format!("/api/v1/placas/{suelta}/modelos/{boyz}"),
        Some(json!({ "estado": "confirmado", "cantidad": 10 })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let (st, v) = pedir(
        &app,
        &c,
        Method::PUT,
        &format!("/api/v1/placas/{suelta}/modelos/{boyz}"),
        Some(json!({ "cantidad": 0 })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{v}");
    // Arm sí es de los Boyz (agregado a mano), y la de calibración no es de ningún modelo
    pedir(
        &app,
        &c,
        Method::PUT,
        &format!("/api/v1/placas/{arm}/modelos/{boyz}"),
        Some(json!({})),
    )
    .await;
    let (st, _) = pedir(
        &app,
        &c,
        Method::PATCH,
        &format!("/api/v1/placas/{calibracion}"),
        Some(json!({ "sin_modelo": true, "estado": "probada", "favorita": true, "notas": " exposición 2.8 s " })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let (st, _) = pedir(
        &app,
        &c,
        Method::PATCH,
        &format!("/api/v1/placas/{calibracion}"),
        Some(json!({ "estado": "rota" })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // volver a detectar no revive lo rechazado
    placas::detectar(&pool).await.unwrap();
    let estado: String =
        sqlx::query_scalar("SELECT estado FROM placa_modelos WHERE placa_id = ? AND modelo_id = ?")
            .bind(arm)
            .bind(wraiths)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(estado, "rechazado");

    let (_, v) = pedir(&app, &c, Method::GET, "/api/v1/placas/resumen", None).await;
    assert_eq!(
        (v["confirmadas"].as_i64(), v["sin_modelo"].as_i64()),
        (Some(4), Some(1)),
        "{v}"
    );
    assert_eq!(v["por_confirmar"], 1, "solo queda caja.3mf: {v}");
    let (_, v) = pedir(
        &app,
        &c,
        Method::GET,
        "/api/v1/placas?revision=por_confirmar",
        None,
    )
    .await;
    assert_eq!(v["placas"][0]["nombre"], "caja");
    let (_, v) = pedir(&app, &c, Method::GET, "/api/v1/placas?favorita=si", None).await;
    assert_eq!(v["total"], 1);
    assert_eq!(v["placas"][0]["notas"], "exposición 2.8 s");
    let (_, v) = pedir(&app, &c, Method::GET, "/api/v1/placas?q=boyz", None).await;
    assert_eq!(v["total"], 2, "por nombre y por modelo: {v}");

    // la ficha de un modelo muestra sus placas (las rechazadas no)
    let (_, v) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/modelos/{wraiths}/placas"),
        None,
    )
    .await;
    let nombres: Vec<&str> = v["placas"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["nombre"].as_str().unwrap())
        .collect();
    assert_eq!(
        nombres,
        ["Torso Main", "Escuadra"],
        "las que tienen laminado van primero"
    );
    let (_, v) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/modelos/{boyz}/placas"),
        None,
    )
    .await;
    assert_eq!(v["total"], 2);
    let la_suelta = v["placas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == suelta)
        .unwrap();
    assert_eq!(la_suelta["modelos"][0]["cantidad"], 10);
}

#[tokio::test]
async fn placas_a_mano() {
    let (_j, pool, app, c, _) = preparar("a-mano").await;
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM archivos WHERE nombre IN ('Arm.chitubox', 'Torso Main.photon') ORDER BY nombre",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let antes = placa_de(&pool, "Arm.chitubox").await;
    let wraiths = modelo(&pool, "Wraiths").await;
    let (st, v) = pedir(
        &app,
        &c,
        Method::POST,
        "/api/v1/placas",
        Some(json!({ "nombre": "Wraiths ×3", "archivo_ids": ids, "modelo_ids": [wraiths] })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{v}");
    let nueva = v["id"].as_i64().unwrap();
    assert_eq!(placa_de(&pool, "Arm.chitubox").await, nueva);
    assert_eq!(placa_de(&pool, "Torso Main.photon").await, nueva);
    // la placa detectada que se quedó vacía (y sin nada del usuario) se quita; la de Torso sigue con la escena
    let quedan: i64 = sqlx::query_scalar("SELECT count(*) FROM placas WHERE id = ?")
        .bind(antes)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(quedan, 0);
    // volver a detectar respeta lo que se puso a mano
    placas::detectar(&pool).await.unwrap();
    assert_eq!(placa_de(&pool, "Arm.chitubox").await, nueva);
    let (_, v) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/placas/{nueva}"),
        None,
    )
    .await;
    assert_eq!(v["origen"], "manual");
    assert_eq!(v["archivos"].as_array().unwrap().len(), 2);
    assert_eq!(v["modelos"][0]["estado"], "confirmado");

    // archivos que no pueden ir en una placa
    let malla: i64 = sqlx::query_scalar("SELECT id FROM archivos WHERE nombre = 'b.obj'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let (st, _) = pedir(
        &app,
        &c,
        Method::POST,
        &format!("/api/v1/placas/{nueva}/archivos"),
        Some(json!({ "archivo_ids": [malla] })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // una detectada no se borra; la hecha a mano sí, y sus archivos vuelven a detectarse
    let torso = placa_de(&pool, "Torso Main.lys").await;
    let (st, _) = pedir(
        &app,
        &c,
        Method::DELETE,
        &format!("/api/v1/placas/{torso}"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _) = pedir(
        &app,
        &c,
        Method::DELETE,
        &format!("/api/v1/placas/{nueva}"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(placa_de(&pool, "Torso Main.photon").await, torso);
    let arm = placa_de(&pool, "Arm.chitubox").await;
    assert_ne!(arm, nueva, "los ids de las placas borradas no se reusan");
    let origen: String = sqlx::query_scalar("SELECT origen FROM placas WHERE id = ?")
        .bind(arm)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(origen, "detectada");
}

#[tokio::test]
async fn vistas_previas_y_portada() {
    let (_j, pool, app, c, config) = preparar("vistas").await;
    let (_, v) = pedir(&app, &c, Method::GET, "/api/v1/placas/resumen", None).await;
    assert_eq!(v["vistas_pendientes"], 4);
    let r = miniaturas::generar(
        &pool,
        config.opciones_miniaturas(),
        miniaturas::Cuales {
            solo_placas: true,
            ..Default::default()
        },
        false,
        Arc::new(AtomicU64::new(0)),
    )
    .await
    .unwrap();
    // solo los archivos de las placas: las tres escenas de Lychee y el proyecto 3MF (con la imagen de su placa,
    // sin f3d); las demás mallas ni se intentan
    assert_eq!((r.candidatos, r.generadas, r.fallidas), (4, 4, 0), "{r:?}");
    let (_, v) = pedir(&app, &c, Method::GET, "/api/v1/placas/resumen", None).await;
    assert_eq!(v["vistas_pendientes"], 0);
    let ok: Vec<String> = sqlx::query_scalar(
        "SELECT a.nombre FROM miniaturas mi JOIN archivos a ON a.id = mi.archivo_id WHERE mi.estado = 'ok'
         ORDER BY a.nombre",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        ok,
        [
            "Grey Knights_2023-08-18_12-58-34.lyt",
            "Torso Main.lys",
            "Torso Main_2025-01-01_10-00-00.lyt",
            "caja.3mf"
        ],
        "{r:?}"
    );
    let torso = placa_de(&pool, "Torso Main.lys").await;
    let lys: i64 = sqlx::query_scalar("SELECT id FROM archivos WHERE nombre = 'Torso Main.lys'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let (_, v) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/placas/{torso}"),
        None,
    )
    .await;
    assert_eq!(
        v["portada"]["id"], lys,
        "la escena va antes que el autoguardado: {v}"
    );
    let photon = v["archivos"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["nombre"] == "Torso Main.photon")
        .unwrap();
    assert_eq!(photon["rol"], "laminado");
    assert!(
        photon["datos"]["error"]
            .as_str()
            .unwrap()
            .contains("no reconocido")
    );

    // portada a mano: solo archivos de la placa, su gemela o sus modelos
    let ajeno: i64 = sqlx::query_scalar("SELECT id FROM archivos WHERE nombre = 'b.obj'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let (st, _) = pedir(
        &app,
        &c,
        Method::PUT,
        &format!("/api/v1/placas/{torso}/portada"),
        Some(json!({ "archivo_id": ajeno })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let auto: i64 = sqlx::query_scalar(
        "SELECT id FROM archivos WHERE nombre = 'Torso Main_2025-01-01_10-00-00.lyt'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let (st, _) = pedir(
        &app,
        &c,
        Method::PUT,
        &format!("/api/v1/placas/{torso}/portada"),
        Some(json!({ "archivo_id": auto })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let (_, v) = pedir(&app, &c, Method::GET, "/api/v1/placas?q=torso", None).await;
    assert_eq!(v["placas"][0]["portada"]["id"], auto);
    assert_eq!(v["placas"][0]["autoguardados"], 1);
    assert_eq!(v["placas"][0]["formatos"], json!(["lys", "lyt", "photon"]));
}
