//! Fase 1: escaneo, importación del catálogo y del frío, búsqueda y ficha, con una colección de juguete.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicU64},
    time::Duration,
};

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use printing_archive::{AppState, app, auth, catalogo, config::Config, db, frio, inventario};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tower::ServiceExt;

const WRAITHS: &str = "Wargames/Warhammer 40K/Necrons/Canoptek Wraiths (Station Forge)";
const CAPTAIN: &str = "Wargames/Warhammer 40K/Space Marines/Captain (Games Workshop)";
const SATURNINE: &str = "Wargames/Warhammer 40K/Space Marines/Terminator Squad/Saturnine";

struct Juguete {
    raiz: PathBuf,
}

impl Drop for Juguete {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.raiz).ok();
    }
}

fn escribir(raiz: &Path, rel: &str, datos: &[u8]) {
    let p = raiz.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, datos).unwrap();
}

/// STL binario con `n` triángulos (encabezado de 80 bytes + cuenta + 50 bytes por triángulo).
fn stl_binario(n: u32) -> Vec<u8> {
    let mut v = vec![0u8; 80];
    v.extend_from_slice(&n.to_le_bytes());
    v.extend(std::iter::repeat_n(0u8, 50 * n as usize));
    v
}

fn coleccion(nombre: &str) -> Juguete {
    let raiz = std::env::temp_dir().join(format!("pa-{nombre}-{}", std::process::id()));
    fs::remove_dir_all(&raiz).ok();
    escribir(
        &raiz,
        &format!("{WRAITHS}/Supported/wraith.stl"),
        &stl_binario(2),
    );
    fs::hard_link(
        raiz.join(format!("{WRAITHS}/Supported/wraith.stl")),
        raiz.join(format!("{WRAITHS}/Supported/wraith_copia.stl")),
    )
    .unwrap();
    escribir(&raiz, &format!("{WRAITHS}/cover.jpg"), b"jpg");
    escribir(
        &raiz,
        &format!("{CAPTAIN}/captain.stl"),
        b"solid captain\nendsolid\n",
    );
    escribir(&raiz, &format!("{SATURNINE}/t.stl"), &stl_binario(1));
    // suelto en la carpeta del modelo (Terminator Squad), fuera de la subcarpeta de la fila
    escribir(
        &raiz,
        "Wargames/Warhammer 40K/Space Marines/Terminator Squad/portada.jpg",
        b"jpg",
    );
    escribir(&raiz, "Yeet/fuera.stl", &stl_binario(3));
    Juguete { raiz }
}

fn csv_catalogo(dir: &Path) -> PathBuf {
    let p = dir.join("hoja.csv");
    let mut w = csv::Writer::from_path(&p).unwrap();
    w.write_record([
        "Ruta actual",
        "Archivos",
        "MB",
        "Juego",
        "Facción / tipo",
        "Modelo",
        "Estudio",
        "Subcarpeta",
        "Etiquetas",
        "Ruta nueva",
        "Confianza",
        "Notas",
        "Revisado",
    ])
    .unwrap();
    w.write_record([
        WRAITHS,
        "3",
        "0",
        "Warhammer 40K",
        "Necrons",
        "Canoptek Wraiths",
        "Station Forge",
        "",
        "Socratis; Proxy",
        WRAITHS,
        "alta",
        "",
        "OK",
    ])
    .unwrap();
    w.write_record([
        CAPTAIN,
        "1",
        "0",
        "Warhammer 40K",
        "Space Marines",
        "Captain",
        "Games Workshop",
        "",
        "Horus Heresy",
        CAPTAIN,
        "alta",
        "nota del capitán",
        "Corregido",
    ])
    .unwrap();
    w.write_record([
        SATURNINE,
        "1",
        "0",
        "Warhammer 40K",
        "Space Marines",
        "Terminator Squad",
        "",
        "Saturnine",
        "Horus Heresy; Saturnine (Imperial Fists)",
        SATURNINE,
        "media",
        "",
        "",
    ])
    .unwrap();
    w.flush().unwrap();
    p
}

async fn preparar(nombre: &str) -> (Juguete, SqlitePool, Router, String) {
    let j = coleccion(nombre);
    let pool = db::en_memoria().await.unwrap();
    db::asegurar_biblioteca_principal(&pool, &j.raiz)
        .await
        .unwrap();
    auth::guardar_usuario(&pool, "memo", "contraseña-larga")
        .await
        .unwrap();
    let config = Config::de_prueba(Some(j.raiz.clone()), None);
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
    (j, pool, app, cookie)
}

async fn get(app: &Router, cookie: &str, uri: &str) -> (StatusCode, Value) {
    let res = app
        .clone()
        .oneshot(
            Request::get(uri)
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let st = res.status();
    let b = res.into_body().collect().await.unwrap().to_bytes();
    (st, serde_json::from_slice(&b).unwrap_or(Value::Null))
}

async fn escanear(pool: &SqlitePool) -> inventario::Resultado {
    inventario::escanear(pool, 1, Arc::new(AtomicU64::new(0)))
        .await
        .unwrap()
}

#[tokio::test]
async fn escaneo_incremental_y_enlaces() {
    let (j, pool, _app, _c) = preparar("escaneo").await;
    let r = escanear(&pool).await;
    assert_eq!((r.archivos, r.nuevos, r.iguales, r.faltan), (7, 7, 0, 0));

    let (formato, tri): (Option<String>, Option<i64>) =
        sqlx::query_as("SELECT stl_formato, triangulos FROM archivos WHERE nombre = 'wraith.stl'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((formato.as_deref(), tri), (Some("binario"), Some(2)));
    let (formato, nlink): (Option<String>, i64) =
        sqlx::query_as("SELECT stl_formato, nlink FROM archivos WHERE nombre = 'captain.stl'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((formato.as_deref(), nlink), (Some("texto"), 1));
    let tipo: String = sqlx::query_scalar("SELECT tipo FROM archivos WHERE nombre = 'cover.jpg'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(tipo, "imagen");

    // segundo escaneo: nada cambió
    let r = escanear(&pool).await;
    assert_eq!((r.nuevos, r.cambiados, r.iguales, r.faltan), (0, 0, 7, 0));

    // un archivo desaparece y otro cambia
    fs::remove_file(j.raiz.join(format!("{WRAITHS}/cover.jpg"))).unwrap();
    std::thread::sleep(Duration::from_millis(20));
    escribir(&j.raiz, &format!("{CAPTAIN}/captain.stl"), &stl_binario(5));
    let r = escanear(&pool).await;
    assert_eq!((r.cambiados, r.faltan), (1, 1));
    let tri: Option<i64> =
        sqlx::query_scalar("SELECT triangulos FROM archivos WHERE nombre = 'captain.stl'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(tri, Some(5));
}

#[tokio::test]
async fn catalogo_liga_archivos_y_cuenta() {
    let (j, pool, _app, _c) = preparar("ligar").await;
    escanear(&pool).await;
    let r = catalogo::importar_catalogo(&pool, &csv_catalogo(&j.raiz))
        .await
        .unwrap();
    assert_eq!((r.filas, r.nuevos, r.etiquetas), (3, 3, 5));

    let filas: Vec<(String, i64, i64, i64)> =
        sqlx::query_as("SELECT nombre, n_archivos, n_mallas, bytes FROM modelos ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    // Wraiths: 2 nombres del mismo STL (184 bytes, se cuenta una vez) + la portada (3 bytes)
    assert_eq!(filas[0], ("Canoptek Wraiths".into(), 3, 2, 184 + 3));
    assert_eq!(filas[1].1, 1);
    // Saturnine: su STL y la portada suelta en la carpeta del modelo (Terminator Squad)
    assert_eq!(filas[2].1, 2);
    let fuera: i64 = sqlx::query_scalar("SELECT count(*) FROM archivos WHERE modelo_id IS NULL")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(fuera, 1, "solo Yeet queda fuera del catálogo");

    // volver a importar no duplica
    let r = catalogo::importar_catalogo(&pool, &csv_catalogo(&j.raiz))
        .await
        .unwrap();
    assert_eq!((r.nuevos, r.actualizados), (0, 3));
}

#[tokio::test]
async fn busqueda_filtros_ficha_y_frio() {
    let (j, pool, app, c) = preparar("api").await;
    escanear(&pool).await;
    catalogo::importar_catalogo(&pool, &csv_catalogo(&j.raiz))
        .await
        .unwrap();

    let frio_csv = j.raiz.join("frio.csv");
    fs::write(
        &frio_csv,
        format!(
            "ubicacion,tipo,remoto,ruta,bytes,md5,modificado,ruta_original,carpeta_modelo\n\
             3DP-archivo,drive,DriveMemo:3DP-archivo,Wargames/Studios/GW/Captain.rar,1000,abc,,Wargames/Studios/GW/Captain.rar,{CAPTAIN}\n\
             3DP-archivo,drive,DriveMemo:3DP-archivo,Otro/sin-modelo.zip,50,def,,,\n"
        ),
    )
    .unwrap();
    let cont_csv = j.raiz.join("contenido.csv");
    fs::write(
        &cont_csv,
        "ubicacion,ruta,entrada,bytes,crc\n\
         3DP-archivo,Wargames/Studios/GW/Captain.rar,Captain/a.stl,600,11\n\
         3DP-archivo,Wargames/Studios/GW/Captain.rar,Captain/b.stl,400,22\n",
    )
    .unwrap();
    let r = frio::importar(&pool, &frio_csv, Some(&cont_csv))
        .await
        .unwrap();
    assert_eq!((r.copias, r.ligadas, r.entradas), (2, 1, 2));

    // búsqueda por prefijo y sin acentos
    let (st, v) = get(&app, &c, "/api/v1/modelos?q=wraith").await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["total"], 1);
    assert_eq!(v["modelos"][0]["nombre"], "Canoptek Wraiths");
    let (_, v) = get(&app, &c, "/api/v1/modelos?q=capitan").await;
    assert_eq!(v["total"], 1, "«capitán» en las notas, buscado sin acento");

    let (_, v) = get(&app, &c, "/api/v1/modelos?etiqueta=Horus%20Heresy").await;
    assert_eq!(v["total"], 2);
    let (_, v) = get(&app, &c, "/api/v1/modelos?faccion=Necrons").await;
    assert_eq!(v["total"], 1);
    let (_, v) = get(&app, &c, "/api/v1/modelos?frio=con").await;
    assert_eq!(v["total"], 1);
    assert_eq!(v["modelos"][0]["n_frio"], 1);
    let (_, v) = get(&app, &c, "/api/v1/modelos?revisado=no").await;
    assert_eq!(v["total"], 1);
    let (_, v) = get(&app, &c, "/api/v1/modelos?por_pagina=2&pagina=2").await;
    assert_eq!(
        (v["total"].as_i64(), v["modelos"].as_array().map(Vec::len)),
        (Some(3), Some(1))
    );

    let (_, v) = get(&app, &c, "/api/v1/modelos/filtros").await;
    assert_eq!(v["juegos"][0]["nombre"], "Warhammer 40K");
    assert_eq!(v["etiquetas"][0]["nombre"], "Horus Heresy");

    let (_, lista) = get(
        &app,
        &c,
        "/api/v1/modelos?faccion=Space%20Marines&orden=nombre",
    )
    .await;
    let id = lista["modelos"][0]["id"].as_i64().unwrap();
    let (st, v) = get(&app, &c, &format!("/api/v1/modelos/{id}")).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["nombre"], "Captain");
    assert_eq!(v["notas"], "nota del capitán");
    assert_eq!(v["archivos"].as_array().unwrap().len(), 1);
    assert_eq!(v["copias_frio"][0]["entradas"], 2);
    assert_eq!(v["etiquetas"], json!(["Horus Heresy"]));

    let (st, _) = get(&app, &c, "/api/v1/modelos/9999").await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    let (_, v) = get(&app, &c, "/api/v1/resumen").await;
    assert_eq!(v["modelos"]["total"], 3);
    assert_eq!(v["archivos"]["sin_modelo"], 1);
    assert_eq!(v["frio"][0]["copias"], 2);
    assert_eq!(v["fuera_del_catalogo"][0]["carpeta"], "Yeet");
}

#[tokio::test]
async fn escaneo_por_la_api_en_segundo_plano() {
    let (_j, _pool, app, c) = preparar("trabajo").await;
    let res = app
        .clone()
        .oneshot(
            Request::post("/api/v1/escaneos")
                .header(header::COOKIE, &c)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::ACCEPTED);
    let b = res.into_body().collect().await.unwrap().to_bytes();
    let id = serde_json::from_slice::<Value>(&b).unwrap()["trabajo_id"]
        .as_i64()
        .unwrap();
    let mut estado = Value::Null;
    for _ in 0..100 {
        let (_, v) = get(&app, &c, &format!("/api/v1/trabajos/{id}")).await;
        if v["estado"] != "en_curso" {
            estado = v;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(estado["estado"], "terminado", "{estado}");
    assert_eq!(estado["resultado"]["archivos"], 7);
}

#[tokio::test]
async fn catalogo_exige_sesion() {
    let (_j, _pool, app, _c) = preparar("sesion").await;
    for uri in [
        "/api/v1/modelos",
        "/api/v1/resumen",
        "/api/v1/modelos/1",
        "/api/v1/trabajos",
    ] {
        let res = app
            .clone()
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{uri}");
    }
}
