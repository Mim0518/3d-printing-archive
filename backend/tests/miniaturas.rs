//! Fase 3: miniaturas (con un f3d de mentira, para que corra en el CI), portadas y archivos para el visor.

use std::{
    fs,
    os::unix::fs::PermissionsExt,
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
    AppState, app, auth, catalogo,
    config::Config,
    db, inventario,
    miniaturas::{self, Opciones},
};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tower::ServiceExt;

const A: &str = "Wargames/Warhammer 40K/Necrons/Wraiths (SF)";
const B: &str = "Wargames/Warhammer 40K/Orks/Boyz (GW)";

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

fn png(ruta: &Path, ancho: u32, alto: u32) {
    fs::create_dir_all(ruta.parent().unwrap()).unwrap();
    image::RgbaImage::from_pixel(ancho, alto, image::Rgba([200, 100, 50, 255]))
        .save(ruta)
        .unwrap();
}

/// Un «f3d» que copia una imagen fija a --output, y falla si el archivo se llama «falla».
fn f3d_falso(dir: &Path) -> String {
    let fija = dir.join("render.png");
    png(&fija, 512, 512);
    let script = dir.join("f3d-falso.sh");
    fs::write(
        &script,
        format!(
            "#!/usr/bin/env bash\nout=\"\"\nfor a in \"$@\"; do case \"$a\" in --output=*) out=\"${{a#--output=}}\";; esac; done\n\
             archivo=\"${{@: -1}}\"\ncase \"$archivo\" in *falla*) echo 'no se pudo leer' >&2; exit 1;; esac\n\
             cp '{}' \"$out\"\n",
            fija.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    script.display().to_string()
}

async fn preparar(nombre: &str) -> (Juguete, SqlitePool, Router, String, Opciones) {
    let base = std::env::temp_dir().join(format!("pa3-{nombre}-{}", std::process::id()));
    fs::remove_dir_all(&base).ok();
    let raiz = base.join("coleccion");
    escribir(&raiz, &format!("{A}/a.stl"), b"solid a\nendsolid\n");
    fs::create_dir_all(raiz.join(format!("{A}/Supported"))).unwrap();
    fs::hard_link(
        raiz.join(format!("{A}/a.stl")),
        raiz.join(format!("{A}/Supported/a2.stl")),
    )
    .unwrap();
    png(&raiz.join(format!("{A}/cover.png")), 800, 600);
    escribir(&raiz, &format!("{A}/falla.stl"), b"basura");
    escribir(&raiz, &format!("{B}/b.obj"), b"v 0 0 0\n");
    let csv = base.join("hoja.csv");
    fs::write(
        &csv,
        format!(
            "Ruta actual,Juego,Facción / tipo,Modelo,Estudio,Subcarpeta,Etiquetas,Confianza,Notas,Revisado\n\
             {A},Warhammer 40K,Necrons,Wraiths,SF,,,alta,,OK\n{B},Warhammer 40K,Orks,Boyz,GW,,,alta,,OK\n"
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
    let mut config = Config::de_prueba(Some(raiz.clone()), None);
    config.miniaturas = base.join("cache");
    config.f3d = f3d_falso(&base);
    let opciones = config.opciones_miniaturas();
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
    (Juguete(base), pool, app, cookie, opciones)
}

async fn pedir(
    app: &Router,
    c: &str,
    m: Method,
    uri: &str,
    body: Option<Value>,
    rango: Option<&str>,
) -> (StatusCode, Vec<u8>, String) {
    let mut b = Request::builder()
        .method(m)
        .uri(uri)
        .header(header::COOKIE, c);
    if body.is_some() {
        b = b.header(header::CONTENT_TYPE, "application/json");
    }
    if let Some(r) = rango {
        b = b.header(header::RANGE, r);
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
    let tipo = res
        .headers()
        .get(header::CONTENT_TYPE)
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    (
        st,
        res.into_body().collect().await.unwrap().to_bytes().to_vec(),
        tipo,
    )
}

fn json_de(b: &[u8]) -> Value {
    serde_json::from_slice(b).unwrap()
}

async fn id_archivo(pool: &SqlitePool, nombre: &str) -> i64 {
    sqlx::query_scalar("SELECT id FROM archivos WHERE nombre = ?")
        .bind(nombre)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn genera_una_vez_por_archivo_y_recuerda_errores() {
    let (_j, pool, _app, _c, op) = preparar("generar").await;
    let r = miniaturas::generar(
        &pool,
        op.clone(),
        Default::default(),
        false,
        Arc::new(AtomicU64::new(0)),
    )
    .await
    .unwrap();
    // a.stl y su enlace a2.stl, cover.png, b.obj, falla.stl
    assert_eq!((r.candidatos, r.generadas, r.fallidas), (5, 4, 1), "{r:?}");
    let claves: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT clave FROM miniaturas WHERE estado = 'ok'")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(claves.len(), 3, "el enlace duro comparte miniatura");
    for k in &claves {
        for t in [512, 128] {
            let p = miniaturas::ruta_cache(&op.dir, k, t);
            let img = image::open(&p).unwrap();
            assert!(img.width() <= t && img.height() <= t, "{p:?}");
        }
    }
    // la foto de 800×600 conserva la proporción
    let k: String = sqlx::query_scalar(
        "SELECT mi.clave FROM miniaturas mi JOIN archivos a ON a.id = mi.archivo_id WHERE a.nombre = 'cover.png'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let img = image::open(miniaturas::ruta_cache(&op.dir, &k, 512)).unwrap();
    assert_eq!((img.width(), img.height()), (512, 384));

    let r = miniaturas::generar(
        &pool,
        op.clone(),
        Default::default(),
        false,
        Arc::new(AtomicU64::new(0)),
    )
    .await
    .unwrap();
    assert_eq!(
        (r.generadas, r.fallidas, r.ya_estaban),
        (0, 0, 5),
        "no repite ni reintenta errores"
    );
    let r = miniaturas::generar(
        &pool,
        op,
        Default::default(),
        true,
        Arc::new(AtomicU64::new(0)),
    )
    .await
    .unwrap();
    assert_eq!(r.fallidas, 1, "con forzar sí reintenta");
}

#[tokio::test]
async fn api_de_miniaturas_portadas_y_contenido() {
    let (_j, pool, app, c, _op) = preparar("api").await;
    let (st, b, _) = pedir(&app, &c, Method::POST, "/api/v1/miniaturas", None, None).await;
    assert_eq!(st, StatusCode::ACCEPTED);
    let t = json_de(&b)["trabajo_id"].as_i64().unwrap();
    for _ in 0..200 {
        let (_, b, _) = pedir(
            &app,
            &c,
            Method::GET,
            &format!("/api/v1/trabajos/{t}"),
            None,
            None,
        )
        .await;
        if json_de(&b)["estado"] != "en_curso" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let (_, b, _) = pedir(
        &app,
        &c,
        Method::GET,
        "/api/v1/miniaturas/resumen",
        None,
        None,
    )
    .await;
    let r = json_de(&b);
    assert_eq!(
        (
            r["archivos"].as_i64(),
            r["con_miniatura"].as_i64(),
            r["errores"].as_i64()
        ),
        (Some(5), Some(4), Some(1)),
        "{r}"
    );

    let stl = id_archivo(&pool, "a.stl").await;
    let (st, b, tipo) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/archivos/{stl}/miniatura?t=128"),
        None,
        None,
    )
    .await;
    assert_eq!((st, tipo.as_str()), (StatusCode::OK, "image/webp"));
    assert!(image::load_from_memory(&b).is_ok());
    let falla = id_archivo(&pool, "falla.stl").await;
    let (st, _, _) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/archivos/{falla}/miniatura"),
        None,
        None,
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    // portada automática: la imagen; elegida a mano: el STL; null: automática otra vez
    let modelo: i64 = sqlx::query_scalar("SELECT id FROM modelos WHERE nombre = 'Wraiths'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let cover = id_archivo(&pool, "cover.png").await;
    let (_, b, _) = pedir(
        &app,
        &c,
        Method::GET,
        "/api/v1/modelos?faccion=Necrons",
        None,
        None,
    )
    .await;
    assert_eq!(json_de(&b)["modelos"][0]["portada"]["id"], cover);
    let (st, _, _) = pedir(
        &app,
        &c,
        Method::PUT,
        &format!("/api/v1/modelos/{modelo}/portada"),
        Some(json!({ "archivo_id": stl })),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let (_, b, _) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/modelos/{modelo}"),
        None,
        None,
    )
    .await;
    let f = json_de(&b);
    assert_eq!(
        (f["portada"]["id"].as_i64(), f["portada_elegida"].as_i64()),
        (Some(stl), Some(stl))
    );
    assert!(
        f["archivos"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["miniatura"].is_string())
    );
    pedir(
        &app,
        &c,
        Method::PUT,
        &format!("/api/v1/modelos/{modelo}/portada"),
        Some(json!({ "archivo_id": null })),
        None,
    )
    .await;
    let (_, b, _) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/modelos/{modelo}"),
        None,
        None,
    )
    .await;
    assert_eq!(json_de(&b)["portada"]["id"], cover);
    let obj = id_archivo(&pool, "b.obj").await;
    let (st, _, _) = pedir(
        &app,
        &c,
        Method::PUT,
        &format!("/api/v1/modelos/{modelo}/portada"),
        Some(json!({ "archivo_id": obj })),
        None,
    )
    .await;
    assert_eq!(
        st,
        StatusCode::BAD_REQUEST,
        "un archivo de otro modelo no puede ser portada"
    );

    // el archivo tal cual, completo y por rangos
    let (st, b, _) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/archivos/{stl}/contenido"),
        None,
        None,
    )
    .await;
    assert_eq!(
        (st, b.as_slice()),
        (StatusCode::OK, b"solid a\nendsolid\n".as_slice())
    );
    let (st, b, _) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/archivos/{stl}/contenido"),
        None,
        Some("bytes=0-4"),
    )
    .await;
    assert_eq!(
        (st, b.as_slice()),
        (StatusCode::PARTIAL_CONTENT, b"solid".as_slice())
    );
}
