//! Fase 8: almacenamiento frío con un «disco externo» que es una carpeta temporal: mandar y verificar, quitar a
//! la papelera, restaurar, vaciar (con frase y volviendo a comprobar la copia), traer de regreso idéntico,
//! comprimidos y el listado de una ubicación.

use std::{
    fs,
    io::Write,
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
use printing_archive::{AppState, almacen, app, auth, catalogo, config::Config, db, inventario};
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

struct Prueba {
    _j: Juguete,
    pool: SqlitePool,
    app: Router,
    c: String,
    raiz: PathBuf,
    frio: PathBuf,
}

async fn preparar(nombre: &str) -> Prueba {
    let base = std::env::temp_dir().join(format!("pa8-{nombre}-{}", std::process::id()));
    fs::remove_dir_all(&base).ok();
    let raiz = base.join("coleccion");
    let frio = base.join("disco");
    fs::create_dir_all(&frio).unwrap();
    escribir(
        &raiz,
        &format!("{A}/Supported/Torso.stl"),
        b"solid torso\nendsolid\n",
    );
    escribir(
        &raiz,
        &format!("{A}/Supported/Arm.stl"),
        b"solid arm\nendsolid\n",
    );
    escribir(&raiz, &format!("{A}/render.png"), &[1u8; 300]);
    escribir(&raiz, &format!("{B}/b.obj"), b"v 0 0 0\n");
    // un archivo compartido entre los dos modelos (enlace duro)
    escribir(&raiz, &format!("{B}/base.stl"), b"solid base\nendsolid\n");
    fs::hard_link(
        raiz.join(format!("{B}/base.stl")),
        raiz.join(format!("{A}/base.stl")),
    )
    .unwrap();
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
    inventario::escanear(&pool, 1, Arc::new(AtomicU64::new(0)))
        .await
        .unwrap();
    catalogo::importar_catalogo(&pool, &csv).await.unwrap();
    let mut config = Config::de_prueba(Some(raiz.clone()), None);
    config.rclone = "/no/existe/rclone".into();
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
    let c = res.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    Prueba {
        _j: Juguete(base),
        pool,
        app,
        c,
        raiz,
        frio,
    }
}

impl Prueba {
    async fn pedir(&self, m: Method, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
        let mut b = Request::builder()
            .method(m)
            .uri(uri)
            .header(header::COOKIE, &self.c);
        if body.is_some() {
            b = b.header(header::CONTENT_TYPE, "application/json");
        }
        let res = self
            .app
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

    /// Espera a que termine un trabajo y devuelve su resultado.
    async fn esperar(&self, r: &Value) -> Value {
        let id = r["trabajo_id"]
            .as_i64()
            .unwrap_or_else(|| panic!("sin trabajo: {r}"));
        for _ in 0..400 {
            let (_, t) = self
                .pedir(Method::GET, &format!("/api/v1/trabajos/{id}"), None)
                .await;
            if t["estado"] != "en_curso" {
                assert_eq!(t["estado"], "terminado", "{t}");
                return t["resultado"].clone();
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("el trabajo {id} no terminó");
    }

    async fn modelo(&self, nombre: &str) -> i64 {
        sqlx::query_scalar("SELECT id FROM modelos WHERE nombre = ?")
            .bind(nombre)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    async fn escanear(&self) {
        inventario::escanear(&self.pool, 1, Arc::new(AtomicU64::new(0)))
            .await
            .unwrap();
    }
}

fn md5(p: &Path) -> String {
    almacen::md5_de(p).unwrap()
}

#[tokio::test]
async fn ida_papelera_y_vuelta() {
    let t = preparar("vuelta").await;
    let w = t.modelo("Wraiths").await;
    let ruta_base = t.frio.to_string_lossy().to_string();
    // un disco nuevo pide permiso para escribir su archivo marca
    let (st, _) = t
        .pedir(
            Method::POST,
            "/api/v1/frio/ubicaciones",
            Some(json!({ "nombre": "Disco", "tipo": "disco_externo", "ruta_base": ruta_base })),
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT);
    let (st, r) = t
        .pedir(
            Method::POST,
            "/api/v1/frio/ubicaciones",
            Some(json!({ "nombre": "Disco", "tipo": "disco_externo", "ruta_base": ruta_base, "crear_marca": true })),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "{r}");
    let disco = r["id"].as_i64().unwrap();
    assert!(t.frio.join(almacen::MARCA).exists());
    let (_, r) = t
        .pedir(
            Method::POST,
            &format!("/api/v1/frio/ubicaciones/{disco}/probar"),
            None,
        )
        .await;
    assert_eq!(r["ok"], true);
    for malo in [
        json!({ "nombre": "x", "tipo": "drive", "remoto": "--delete" }),
        json!({ "nombre": "x", "tipo": "disco_externo", "ruta_base": "/no/existe" }),
        json!({ "nombre": "x", "tipo": "nube" }),
    ] {
        let (st, _) = t
            .pedir(Method::POST, "/api/v1/frio/ubicaciones", Some(malo))
            .await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
    }

    // el plan
    let (_, p) = t
        .pedir(
            Method::GET,
            &format!("/api/v1/frio/modelos/{w}/plan?ubicacion_id={disco}"),
            None,
        )
        .await;
    assert_eq!(p["archivos"], 4, "{p}");
    assert_eq!(p["compartidos"], 1);
    assert_eq!(p["ya_en_frio"], 0);

    // mandar (sin quitar): llega con su misma ruta y se verifica
    let (st, r) = t
        .pedir(
            Method::POST,
            &format!("/api/v1/frio/modelos/{w}/subir"),
            Some(json!({ "ubicacion_id": disco })),
        )
        .await;
    assert_eq!(st, StatusCode::ACCEPTED, "{r}");
    let res = t.esperar(&r).await;
    assert_eq!(
        (res["subidos"].as_i64(), res["verificados"].as_i64()),
        (Some(4), Some(4)),
        "{res}"
    );
    assert_eq!(
        md5(&t.frio.join(format!("{A}/Supported/Torso.stl"))),
        md5(&t.raiz.join(format!("{A}/Supported/Torso.stl")))
    );
    let (_, e) = t
        .pedir(Method::GET, &format!("/api/v1/frio/modelos/{w}"), None)
        .await;
    assert_eq!(e["estado"], "ambos", "{e}");
    assert_eq!(e["ubicaciones"][0]["verificadas"], 4);
    // otra vez: ya estaban
    let (_, r) = t
        .pedir(
            Method::POST,
            &format!("/api/v1/frio/modelos/{w}/subir"),
            Some(json!({ "ubicacion_id": disco })),
        )
        .await;
    let res = t.esperar(&r).await;
    assert_eq!(
        (res["subidos"].as_i64(), res["ya_estaban"].as_i64()),
        (Some(0), Some(4))
    );

    // Arm cambia en el disco: ya no tiene copia igual, así que no se quita
    escribir(
        &t.raiz,
        &format!("{A}/Supported/Arm.stl"),
        b"solid arm v2\nendsolid\n",
    );
    t.escanear().await;
    let (_, r) = t
        .pedir(
            Method::POST,
            &format!("/api/v1/frio/modelos/{w}/quitar"),
            Some(json!({ "ubicacion_id": disco })),
        )
        .await;
    let res = t.esperar(&r).await;
    assert_eq!(res["a_papelera"], 3, "{res}");
    assert_eq!(res["sin_copia"], 1);
    let lote = res["lote_papelera"].as_i64().unwrap();
    assert!(!t.raiz.join(format!("{A}/Supported/Torso.stl")).exists());
    assert!(t.raiz.join(format!("{A}/Supported/Arm.stl")).exists());
    assert!(
        t.raiz
            .join(format!(".archivo-papelera/{lote}/{A}/Supported/Torso.stl"))
            .exists()
    );
    // el compartido sigue en el otro modelo
    assert!(t.raiz.join(format!("{B}/base.stl")).exists());
    let (_, e) = t
        .pedir(Method::GET, &format!("/api/v1/frio/modelos/{w}"), None)
        .await;
    assert_eq!(e["estado"], "parcial", "{e}");
    assert_eq!(e["disco"]["archivos"], 1);
    // el escaneo no ve la papelera
    t.escanear().await;
    let en_papelera: i64 =
        sqlx::query_scalar("SELECT count(*) FROM archivos WHERE ruta LIKE '.archivo-%'")
            .fetch_one(&t.pool)
            .await
            .unwrap();
    assert_eq!(en_papelera, 0);

    // restaurar los regresa
    let (st, r) = t
        .pedir(
            Method::POST,
            &format!("/api/v1/papelera/{lote}/restaurar"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{r}");
    assert_eq!(r["restaurados"], 3);
    assert!(t.raiz.join(format!("{A}/Supported/Torso.stl")).exists());
    let (st, _) = t
        .pedir(
            Method::POST,
            &format!("/api/v1/papelera/{lote}/restaurar"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT);

    // quitar otra vez, y vaciar: con la frase y comprobando la copia
    t.escanear().await;
    let (_, r) = t
        .pedir(
            Method::POST,
            &format!("/api/v1/frio/modelos/{w}/quitar"),
            Some(json!({ "ubicacion_id": disco })),
        )
        .await;
    let lote = t.esperar(&r).await["lote_papelera"].as_i64().unwrap();
    let (st, _) = t
        .pedir(
            Method::POST,
            &format!("/api/v1/papelera/{lote}/vaciar"),
            Some(json!({ "frase": "sí" })),
        )
        .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    // alguien cambió la copia en el disco externo: no se borra nada
    let torso_frio = t.frio.join(format!("{A}/Supported/Torso.stl"));
    let original = fs::read(&torso_frio).unwrap();
    fs::write(&torso_frio, b"otra cosa").unwrap();
    let (st, r) = t
        .pedir(
            Method::POST,
            &format!("/api/v1/papelera/{lote}/vaciar"),
            Some(json!({ "frase": "borrar para siempre" })),
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT);
    assert!(
        r["error"].as_str().unwrap().contains("No se borra nada"),
        "{r}"
    );
    assert!(
        t.raiz
            .join(format!(".archivo-papelera/{lote}/{A}/Supported/Torso.stl"))
            .exists()
    );
    fs::write(&torso_frio, original).unwrap();
    let (st, r) = t
        .pedir(
            Method::POST,
            &format!("/api/v1/papelera/{lote}/vaciar"),
            Some(json!({ "frase": "borrar para siempre" })),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{r}");
    assert_eq!(r["borrados"], 3);
    assert!(r["bytes_compartidos"].as_i64().unwrap() > 0);
    assert!(!t.raiz.join(".archivo-papelera").exists());
    let (_, l) = t
        .pedir(Method::GET, "/api/v1/papelera?ver=todos", None)
        .await;
    assert_eq!(l["lotes"][0]["estado"], "vaciado");

    // traer: regresan idénticos; Arm choca con el del disco (distinto) y se queda aparte
    let (st, r) = t
        .pedir(
            Method::POST,
            &format!("/api/v1/frio/modelos/{w}/traer"),
            Some(json!({ "ubicacion_id": disco })),
        )
        .await;
    assert_eq!(st, StatusCode::ACCEPTED, "{r}");
    let res = t.esperar(&r).await;
    assert_eq!(res["traidos"], 3, "{res}");
    assert_eq!(res["conflictos"].as_array().unwrap().len(), 1, "{res}");
    let aparte = res["apartado_en"].as_str().unwrap();
    assert!(
        t.raiz
            .join(aparte)
            .join(format!("{A}/Supported/Arm.stl"))
            .exists()
    );
    assert_eq!(
        md5(&t.raiz.join(format!("{A}/Supported/Torso.stl"))),
        md5(&torso_frio)
    );
    assert_eq!(
        fs::read(t.raiz.join(format!("{A}/Supported/Arm.stl"))).unwrap(),
        b"solid arm v2\nendsolid\n"
    );
    let (_, e) = t
        .pedir(Method::GET, &format!("/api/v1/frio/modelos/{w}"), None)
        .await;
    assert_eq!(e["estado"], "ambos", "{e}");
    assert_eq!(e["disco"]["archivos"], 4);
    let (_, tr) = t
        .pedir(
            Method::GET,
            &format!("/api/v1/frio/transferencias?modelo_id={w}"),
            None,
        )
        .await;
    assert_eq!(tr[0]["direccion"], "traer");
    assert_eq!(tr[0]["estado"], "con_errores");
}

#[tokio::test]
async fn listado_comprimidos_y_lo_que_falta() {
    let t = preparar("listado").await;
    let w = t.modelo("Wraiths").await;
    fs::write(t.frio.join(almacen::MARCA), "marca-de-prueba").unwrap();
    let (_, r) = t
        .pedir(
            Method::POST,
            "/api/v1/frio/ubicaciones",
            Some(json!({ "nombre": "Disco", "tipo": "disco_externo",
                         "ruta_base": t.frio.to_string_lossy(), "escribible": false })),
        )
        .await;
    let disco = r["id"].as_i64().unwrap();
    // no escribible: no se manda nada ahí
    let (st, _) = t
        .pedir(
            Method::POST,
            &format!("/api/v1/frio/modelos/{w}/subir"),
            Some(json!({ "ubicacion_id": disco })),
        )
        .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    // lo que ya estaba en el disco externo: un comprimido del modelo y un archivo suelto de nadie
    let zip_ruta = t.frio.join(format!("{A}/Extras.zip"));
    fs::create_dir_all(zip_ruta.parent().unwrap()).unwrap();
    {
        let mut z = zip::ZipWriter::new(fs::File::create(&zip_ruta).unwrap());
        let op = zip::write::SimpleFileOptions::default();
        z.start_file("Extras/Escudo.stl", op).unwrap();
        z.write_all(b"solid escudo\nendsolid\n").unwrap();
        z.start_file("Extras/Lanza.stl", op).unwrap();
        z.write_all(b"solid lanza\nendsolid\n").unwrap();
        z.finish().unwrap();
    }
    escribir(&t.frio, "Sueltos/nada.stl", b"x");
    let (_, r) = t
        .pedir(
            Method::POST,
            &format!("/api/v1/frio/ubicaciones/{disco}/listar"),
            None,
        )
        .await;
    let res = t.esperar(&r).await;
    assert_eq!(
        (
            res["archivos"].as_i64(),
            res["nuevos"].as_i64(),
            res["ligados"].as_i64()
        ),
        (Some(2), Some(2), Some(1)),
        "{res}"
    );
    let (_, u) = t.pedir(Method::GET, "/api/v1/frio/ubicaciones", None).await;
    assert_eq!(u[0]["copias"], 2);
    assert_eq!(u[0]["modelos"], 1);
    // traer el comprimido: se baja, se comprueba y se extrae en la carpeta del modelo
    let (_, e) = t
        .pedir(Method::GET, &format!("/api/v1/frio/modelos/{w}"), None)
        .await;
    assert_eq!(e["ubicaciones"][0]["comprimidos"], 1, "{e}");
    let zip_id = e["copias"][0]["id"].clone();
    let (_, r) = t
        .pedir(
            Method::POST,
            &format!("/api/v1/frio/modelos/{w}/traer"),
            Some(json!({ "ubicacion_id": disco, "copia_ids": [zip_id] })),
        )
        .await;
    let res = t.esperar(&r).await;
    assert_eq!(
        (res["traidos"].as_i64(), res["extraidos"].as_i64()),
        (Some(1), Some(2)),
        "{res}"
    );
    assert!(
        t.raiz
            .join(format!("{A}/Extras/Extras/Escudo.stl"))
            .exists()
    );
    assert!(!t.raiz.join(".archivo-entrante").exists());
    // ya en el inventario, del modelo
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM archivos WHERE modelo_id = ? AND nombre = 'Lanza.stl'",
    )
    .bind(w)
    .fetch_one(&t.pool)
    .await
    .unwrap();
    assert_eq!(n, 1);
    // otra vez: la carpeta ya existe, choca
    let (_, r) = t
        .pedir(
            Method::POST,
            &format!("/api/v1/frio/modelos/{w}/traer"),
            Some(json!({ "ubicacion_id": disco, "copia_ids": [zip_id] })),
        )
        .await;
    let res = t.esperar(&r).await;
    assert_eq!(res["conflictos"].as_array().unwrap().len(), 1, "{res}");
    // se borra algo del disco externo: el listado lo marca como que falta
    fs::remove_file(t.frio.join("Sueltos/nada.stl")).unwrap();
    let (_, r) = t
        .pedir(
            Method::POST,
            &format!("/api/v1/frio/ubicaciones/{disco}/listar"),
            None,
        )
        .await;
    let res = t.esperar(&r).await;
    assert_eq!(res["ya_no_estan"], 1, "{res}");
    // el disco se desconecta (otra marca): no se puede usar
    fs::write(t.frio.join(almacen::MARCA), "otro-disco").unwrap();
    let (_, r) = t
        .pedir(
            Method::POST,
            &format!("/api/v1/frio/ubicaciones/{disco}/probar"),
            None,
        )
        .await;
    assert_eq!(r["ok"], false);
    let (_, resumen) = t.pedir(Method::GET, "/api/v1/frio/resumen", None).await;
    assert_eq!(resumen["frase_vaciar"], "borrar para siempre");
}

/// rclone de verdad contra una carpeta local (`:local:`), si está instalado (en el CI no).
#[test]
fn rclone_ida_y_vuelta_local() {
    if std::process::Command::new("rclone")
        .arg("version")
        .output()
        .is_err()
    {
        eprintln!("sin rclone: se omite");
        return;
    }
    let t = std::env::temp_dir().join(format!("pa8-rclone-{}", std::process::id()));
    fs::remove_dir_all(&t).ok();
    let raiz = t.join("raiz");
    escribir(&raiz, "M/a b.stl", b"hola");
    escribir(&raiz, "M/sub/c.stl", b"mundo");
    let a = almacen::Almacen::Rclone {
        programa: "rclone".into(),
        remoto: format!(":local:{}", t.join("remoto").display()),
    };
    let p = AtomicU64::new(0);
    let rutas = vec!["M/a b.stl".to_string(), "M/sub/c.stl".to_string()];
    a.subir(&raiz, &rutas, &p).unwrap();
    let mut l = a.listar("M").unwrap();
    l.sort_by(|x, y| x.ruta.cmp(&y.ruta));
    assert_eq!(l.len(), 2);
    assert_eq!(l[0].ruta, "M/a b.stl");
    assert_eq!(
        l[0].md5.as_deref(),
        Some("4d186321c1a7f0f354b297e8914ab240")
    );
    assert!(a.listar("No/Existe").unwrap().is_empty());
    a.bajar(&rutas, &t.join("vuelta"), &p).unwrap();
    assert_eq!(fs::read(t.join("vuelta/M/sub/c.stl")).unwrap(), b"mundo");
    fs::remove_dir_all(&t).ok();
}
