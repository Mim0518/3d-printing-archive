//! Fase 9: bandeja «Por catalogar». Detecta lo nuevo en «Por catalogar» y lo que se puso directo en su lugar,
//! propone los campos, aprueba (fila nueva y mudanza, o juntar con un modelo que ya está en su lugar), manda a
//! «No mover» y extrae un zip comprobando el CRC; el comprimido original va a frío al aprobar.

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
use printing_archive::{AppState, app, auth, catalogo, config::Config, db, inventario};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tower::ServiceExt;

const A: &str = "Wargames/Warhammer 40K/Necrons/Wraiths (SF)";
const B: &str = "Wargames/Warhammer 40K/Orks/Boyz Mob (GW)";
const NECRONS: &str = "Wargames/Warhammer 40K/Necrons";
const CANOPTEK: &str = "Por catalogar/Necrons/Canoptek Wraiths (SF)";
const EXTRA: &str = "Por catalogar/Boyz Mob Extra";
const TOMB: &str = "Wargames/Warhammer 40K/Necrons/Tomb Blades (SF)";

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
    let base = std::env::temp_dir().join(format!("pa9-{nombre}-{}", std::process::id()));
    fs::remove_dir_all(&base).ok();
    let raiz = base.join("coleccion");
    let frio = base.join("disco");
    fs::create_dir_all(&frio).unwrap();
    // lo que ya está en el catálogo
    escribir(
        &raiz,
        &format!("{A}/Supported/Torso.stl"),
        b"solid a\nendsolid\n",
    );
    escribir(&raiz, &format!("{A}/render.png"), &[1u8; 300]);
    escribir(&raiz, &format!("{B}/b.obj"), b"v 0 0 0\n");
    escribir(
        &raiz,
        &format!("{B}/Heads/h1.stl"),
        b"solid viejo\nendsolid\n",
    );
    // descargas nuevas en «Por catalogar»
    escribir(
        &raiz,
        &format!("{CANOPTEK}/Supported/Body.stl"),
        b"solid body\nendsolid\n",
    );
    escribir(
        &raiz,
        &format!("{CANOPTEK}/Supported/Arm.stl"),
        b"solid arm\nendsolid\n",
    );
    escribir(
        &raiz,
        &format!("{EXTRA}/Heads/h1.stl"),
        b"solid h1\nendsolid\n",
    );
    escribir(
        &raiz,
        &format!("{EXTRA}/Bodies/b1.stl"),
        b"solid b1\nendsolid\n",
    );
    escribir(
        &raiz,
        "Por catalogar/suelto.stl",
        b"solid suelto\nendsolid\n",
    );
    escribir(&raiz, "Por catalogar/.DS_Store", b"basura");
    {
        let p = raiz.join("Por catalogar/Pack.zip");
        let mut z = zip::ZipWriter::new(fs::File::create(&p).unwrap());
        let op = zip::write::SimpleFileOptions::default();
        z.add_directory("Pack/", op).unwrap();
        z.start_file("Pack/Lich.stl", op).unwrap();
        z.write_all(b"solid lich\nendsolid\n").unwrap();
        z.start_file("Pack/Supported/Lich_sup.stl", op).unwrap();
        z.write_all(b"solid lich sup\nendsolid\n").unwrap();
        z.start_file("__MACOSX/Pack/._Lich.stl", op).unwrap();
        z.write_all(b"mac").unwrap();
        z.finish().unwrap();
    }
    // puestos directo en su lugar
    escribir(&raiz, &format!("{TOMB}/t.stl"), b"solid tomb\nendsolid\n");
    escribir(&raiz, &format!("{NECRONS}/foto.png"), &[2u8; 100]);
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

    async fn bandeja(&self, todos: bool) -> Value {
        let uri = if todos {
            "/api/v1/bandeja?ver=todos"
        } else {
            "/api/v1/bandeja"
        };
        let (st, b) = self.pedir(Method::GET, uri, None).await;
        assert_eq!(st, StatusCode::OK, "{b}");
        b
    }

    async fn grupo(&self, clave: &str) -> Option<Value> {
        self.bandeja(false).await["grupos"]
            .as_array()
            .unwrap()
            .iter()
            .find(|g| g["clave"] == clave)
            .cloned()
    }

    /// La aprobación con los campos de la propuesta (y lo que se cambie).
    async fn aprobar(&self, clave: &str, cambios: Value) -> (StatusCode, Value) {
        let g = self
            .grupo(clave)
            .await
            .unwrap_or_else(|| panic!("no está {clave}"));
        let mut cuerpo = g["propuesta"].clone();
        cuerpo["clave"] = json!(clave);
        for (k, v) in cambios.as_object().unwrap() {
            cuerpo[k] = v.clone();
        }
        self.pedir(Method::POST, "/api/v1/bandeja/aprobar", Some(cuerpo))
            .await
    }

    async fn modelo(&self, carpeta: &str) -> Option<(i64, String, i64, i64)> {
        sqlx::query_as("SELECT id, revisado, pendiente, n_archivos FROM modelos WHERE carpeta = ?")
            .bind(carpeta)
            .fetch_optional(&self.pool)
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn detecta_y_propone() {
    let t = preparar("detecta").await;
    let b = t.bandeja(false).await;
    assert_eq!(b["existe_entrada"], true);
    assert_eq!(b["entrada"], "Por catalogar");
    let claves: Vec<&str> = b["grupos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["clave"].as_str().unwrap())
        .collect();
    assert_eq!(
        claves,
        [
            "Wargames/Warhammer 40K/Necrons/(sueltos)",
            TOMB,
            EXTRA,
            CANOPTEK,
            "Por catalogar/Pack.zip",
            "Por catalogar/suelto.stl",
        ],
        "{b}"
    );
    let g = t.grupo(CANOPTEK).await.unwrap();
    assert_eq!(g["seccion"], "por_catalogar");
    assert_eq!(g["archivos"], 2);
    assert_eq!(g["propuesta"]["faccion"], "Necrons");
    assert_eq!(g["propuesta"]["nombre"], "Canoptek Wraiths");
    assert_eq!(g["propuesta"]["estudio"], "SF");
    assert_eq!(g["propuesta"]["confianza"], "media");
    assert_eq!(
        g["ruta_nueva"],
        "Wargames/Warhammer 40K/Necrons/Canoptek Wraiths (SF)"
    );
    // una descarga de un modelo que ya está: alta, y con el estudio del catálogo
    let g = t.grupo(EXTRA).await.unwrap();
    assert_eq!(g["propuesta"]["nombre"], "Boyz Mob");
    assert_eq!(g["propuesta"]["estudio"], "GW");
    assert_eq!(g["propuesta"]["confianza"], "alta");
    assert_eq!(g["ruta_nueva"], B);
    // puesto directo en su lugar: los campos salen de la ruta
    let g = t.grupo(TOMB).await.unwrap();
    assert_eq!(g["seccion"], "nueva");
    assert_eq!(
        (
            g["propuesta"]["juego"].as_str(),
            g["propuesta"]["faccion"].as_str(),
            g["propuesta"]["nombre"].as_str(),
            g["propuesta"]["estudio"].as_str()
        ),
        (
            Some("Warhammer 40K"),
            Some("Necrons"),
            Some("Tomb Blades"),
            Some("SF")
        )
    );
    // el zip todavía no se extrae: se ve su contenido
    let g = t.grupo("Por catalogar/Pack.zip").await.unwrap();
    assert_eq!(g["comprimidos"][0]["se_puede_extraer"], true);
    let id = g["comprimidos"][0]["id"].as_i64().unwrap();
    let (st, d) = t
        .pedir(
            Method::GET,
            "/api/v1/bandeja/grupo?clave=Por%20catalogar%2FPack.zip",
            None,
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{d}");
    assert_eq!(d["contenidos"][id.to_string()]["entradas"], 3);

    // revisar campos mientras se editan
    let (_, r) = t
        .pedir(
            Method::POST,
            "/api/v1/bandeja/revisar",
            Some(json!({ "clave": EXTRA, "juego": "Warhammer 40K", "faccion": "Orks", "nombre": "Boyz Mob",
                         "estudio": "GW", "subcarpeta": "Extra" })),
        )
        .await;
    assert_eq!(r["ruta_nueva"], format!("{B}/Extra"));
    assert_eq!(r["se_mueve"], true);
    assert_eq!(r["juntar"][0]["carpeta"], B, "{r}");
    let (_, r) = t
        .pedir(
            Method::POST,
            "/api/v1/bandeja/revisar",
            Some(json!({ "clave": EXTRA, "juego": "", "nombre": "a:b" })),
        )
        .await;
    assert!(r["errores"].as_array().unwrap().len() >= 2, "{r}");
}

#[tokio::test]
async fn aprobar_juntar_y_descartar() {
    let t = preparar("aprobar").await;

    // errores de validación y grupos que no existen
    let (st, _) = t.aprobar(CANOPTEK, json!({ "juego": "" })).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _) = t
        .aprobar(CANOPTEK, json!({ "nombre": "Canoptek: Wraiths" }))
        .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _) = t
        .pedir(
            Method::POST,
            "/api/v1/bandeja/aprobar",
            Some(json!({ "clave": "Por catalogar/no existe", "juego": "Warhammer 40K", "nombre": "X" })),
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT);
    assert!(t.modelo(CANOPTEK).await.is_none());

    // aprobar tal cual: fila nueva «OK» y mudanza a su ruta
    let (st, r) = t.aprobar(CANOPTEK, json!({ "etiquetas": ["nuevo"] })).await;
    assert_eq!(st, StatusCode::ACCEPTED, "{r}");
    assert_eq!(r["estado"], "mover");
    let res = t.esperar(&r).await;
    assert_eq!(res["mudanza"]["movidos"], 2, "{res}");
    let destino = "Wargames/Warhammer 40K/Necrons/Canoptek Wraiths (SF)";
    assert!(t.raiz.join(destino).join("Supported/Body.stl").exists());
    assert!(!t.raiz.join("Por catalogar/Necrons").exists());
    let (_, revisado, pendiente, n) = t.modelo(destino).await.unwrap();
    assert_eq!((revisado.as_str(), pendiente, n), ("OK", 0, 2));
    assert!(t.grupo(CANOPTEK).await.is_none());

    // la descarga extra de Boyz Mob cae en la carpeta del modelo: como fila nueva choca y no deja nada
    let (st, r) = t.aprobar(EXTRA, json!({})).await;
    assert_eq!(st, StatusCode::OK, "{r}");
    assert_eq!(r["estado"], "conflictos");
    assert!(
        r["conflictos"][0].as_str().unwrap().contains("Boyz Mob"),
        "{r}"
    );
    assert!(t.modelo(EXTRA).await.is_none());
    assert!(t.grupo(EXTRA).await.is_some());
    // juntar sin subcarpeta: Heads/h1.stl ya existe ahí
    let boyz = t.modelo(B).await.unwrap().0;
    let (_, r) = t.aprobar(EXTRA, json!({ "juntar_con": boyz })).await;
    assert_eq!(r["estado"], "conflictos", "{r}");
    assert!(
        r["conflictos"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c.as_str().unwrap().contains("ya existe")),
        "{r}"
    );
    // con una subcarpeta sí
    let (st, r) = t
        .aprobar(EXTRA, json!({ "juntar_con": boyz, "subcarpeta": "Extra" }))
        .await;
    assert_eq!(st, StatusCode::ACCEPTED, "{r}");
    assert_eq!(r["modelo_id"], boyz);
    let res = t.esperar(&r).await;
    assert_eq!(res["mudanza"]["movidos"], 2, "{res}");
    assert!(t.raiz.join(B).join("Extra/Heads/h1.stl").exists());
    assert!(t.raiz.join(B).join("Heads/h1.stl").exists());
    let (_, _, pendiente, n) = t.modelo(B).await.unwrap();
    assert_eq!((pendiente, n), (0, 4));
    let (n,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM historial WHERE modelo_id = ? AND campo = 'bandeja'")
            .bind(boyz)
            .fetch_one(&t.pool)
            .await
            .unwrap();
    assert_eq!(n, 1);
    // y se puede deshacer: los archivos vuelven a la bandeja
    let mudanza = r["mudanza_id"].as_i64().unwrap();
    let (st, d) = t
        .pedir(
            Method::POST,
            &format!("/api/v1/mudanzas/{mudanza}/deshacer"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::ACCEPTED, "{d}");
    t.esperar(&d).await;
    assert!(t.raiz.join(EXTRA).join("Heads/h1.stl").exists());
    assert_eq!(t.modelo(B).await.unwrap().3, 2);
    assert!(t.grupo(EXTRA).await.is_some());

    // puesto directo en su lugar: se agrega sin mover nada
    let (st, r) = t.aprobar(TOMB, json!({})).await;
    assert_eq!(st, StatusCode::OK, "{r}");
    assert_eq!(r["estado"], "listo");
    assert!(r["trabajo_id"].is_null());
    let (_, revisado, pendiente, n) = t.modelo(TOMB).await.unwrap();
    assert_eq!((revisado.as_str(), pendiente, n), ("OK", 0, 1));
    // los sueltos de la facción: fila de sueltos, sin modelo, en su lugar
    let sueltos = "Wargames/Warhammer 40K/Necrons/(sueltos)";
    let g = t.grupo(sueltos).await.unwrap();
    assert_eq!(g["propuesta"]["nombre"], "");
    let (st, r) = t.aprobar(sueltos, json!({})).await;
    assert_eq!(st, StatusCode::OK, "{r}");
    assert_eq!(r["estado"], "listo");
    assert_eq!(t.modelo(sueltos).await.unwrap().3, 1);
    // una fila de sueltos no estorba para mover otro modelo a una subcarpeta de su carpeta
    let (_, r) = t
        .pedir(
            Method::POST,
            "/api/v1/bandeja/revisar",
            Some(json!({ "clave": "Por catalogar/suelto.stl", "juego": "Warhammer 40K", "faccion": "Necrons",
                         "nombre": "Suelto" })),
        )
        .await;
    assert_eq!(r["choques"].as_array().unwrap().len(), 0, "{r}");

    // «No mover»: no se vuelve a proponer, salvo que se pida
    let (st, _) = t
        .pedir(
            Method::POST,
            "/api/v1/bandeja/descartar",
            Some(json!({ "clave": "Por catalogar/suelto.stl", "motivo": "prueba de impresora" })),
        )
        .await;
    assert_eq!(st, StatusCode::OK);
    assert!(t.grupo("Por catalogar/suelto.stl").await.is_none());
    let b = t.bandeja(true).await;
    assert_eq!(b["descartados"], 1);
    assert!(
        b["grupos"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["clave"] == "Por catalogar/suelto.stl" && g["descartado"] == true)
    );
    let (st, _) = t
        .pedir(
            Method::POST,
            "/api/v1/bandeja/volver",
            Some(json!({ "clave": "Por catalogar/suelto.stl" })),
        )
        .await;
    assert_eq!(st, StatusCode::OK);
    assert!(t.grupo("Por catalogar/suelto.stl").await.is_some());
}

#[tokio::test]
async fn zip_extraer_y_a_frio() {
    let t = preparar("zip").await;
    let g = t.grupo("Por catalogar/Pack.zip").await.unwrap();
    let zip_id = g["comprimidos"][0]["id"].as_i64().unwrap();
    let (st, r) = t
        .pedir(
            Method::POST,
            "/api/v1/bandeja/extraer",
            Some(json!({ "archivo_id": zip_id })),
        )
        .await;
    assert_eq!(st, StatusCode::ACCEPTED, "{r}");
    let res = t.esperar(&r).await;
    assert_eq!(res["carpeta"], "Por catalogar/Pack");
    assert_eq!(res["entradas"], 2, "sin la basura de macOS: {res}");
    // sin la carpeta «Pack/» repetida, y el zip guardado dentro
    let pack = t.raiz.join("Por catalogar/Pack");
    assert!(pack.join("Lich.stl").exists());
    assert!(pack.join("Supported/Lich_sup.stl").exists());
    assert!(pack.join("Pack.zip").exists());
    assert!(!pack.join("__MACOSX").exists());
    assert!(!t.raiz.join("Por catalogar/Pack.zip").exists());
    assert!(!t.raiz.join(".archivo-entrante").exists());
    // no se extrae dos veces
    let (st, _) = t
        .pedir(
            Method::POST,
            "/api/v1/bandeja/extraer",
            Some(json!({ "archivo_id": zip_id })),
        )
        .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let g = t.grupo("Por catalogar/Pack").await.unwrap();
    assert_eq!(g["archivos"], 3);
    assert_eq!(g["comprimidos"][0]["extraido_en"], "Por catalogar/Pack");
    // un zip corrupto no deja nada a medias
    escribir(
        &t.raiz,
        "Por catalogar/Roto.zip",
        b"PK\x03\x04 esto no es un zip",
    );
    inventario::escanear(&t.pool, 1, Arc::new(AtomicU64::new(0)))
        .await
        .unwrap();
    let roto: i64 =
        sqlx::query_scalar("SELECT id FROM archivos WHERE ruta = 'Por catalogar/Roto.zip'")
            .fetch_one(&t.pool)
            .await
            .unwrap();
    let (_, r) = t
        .pedir(
            Method::POST,
            "/api/v1/bandeja/extraer",
            Some(json!({ "archivo_id": roto })),
        )
        .await;
    let id = r["trabajo_id"].as_i64().unwrap();
    for _ in 0..200 {
        let (_, j) = t
            .pedir(Method::GET, &format!("/api/v1/trabajos/{id}"), None)
            .await;
        if j["estado"] != "en_curso" {
            assert_eq!(j["estado"], "fallo", "{j}");
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(t.raiz.join("Por catalogar/Roto.zip").exists());
    assert!(!t.raiz.join("Por catalogar/Roto").exists());
    assert!(!t.raiz.join(".archivo-entrante").exists());

    // sin una ubicación predeterminada no se puede mandar el comprimido a frío
    let campos =
        json!({ "juego": "Warhammer 40K", "faccion": "Necrons", "nombre": "Lich", "estudio": "" });
    let (st, r) = t.aprobar("Por catalogar/Pack", campos.clone()).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{r}");
    assert!(r["error"].as_str().unwrap().contains("predeterminada"));
    let (st, u) = t
        .pedir(
            Method::POST,
            "/api/v1/frio/ubicaciones",
            Some(json!({ "nombre": "Disco", "tipo": "disco_externo",
                         "ruta_base": t.frio.to_string_lossy(), "crear_marca": true })),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "{u}");
    let disco = u["id"].as_i64().unwrap();
    let (st, _) = t
        .pedir(
            Method::PATCH,
            &format!("/api/v1/frio/ubicaciones/{disco}"),
            Some(json!({ "predeterminada": true })),
        )
        .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(t.bandeja(false).await["destino_frio"]["id"], disco);

    // aprobar: se mueve, el zip se sube, se verifica y se quita del disco (a la papelera)
    let (st, r) = t.aprobar("Por catalogar/Pack", campos).await;
    assert_eq!(st, StatusCode::ACCEPTED, "{r}");
    assert_eq!(r["ubicacion"], "Disco");
    let res = t.esperar(&r).await;
    assert_eq!(res["mudanza"]["movidos"], 3, "{res}");
    assert!(res["frio_error"].is_null(), "{res}");
    assert_eq!(res["frio"]["verificados"], 1, "{res}");
    assert_eq!(res["frio"]["a_papelera"], 1, "{res}");
    let lich = "Wargames/Warhammer 40K/Necrons/Lich";
    assert!(t.raiz.join(lich).join("Lich.stl").exists());
    assert!(t.raiz.join(lich).join("Supported/Lich_sup.stl").exists());
    assert!(!t.raiz.join(lich).join("Pack.zip").exists());
    assert!(t.frio.join(lich).join("Pack.zip").exists());
    assert!(
        !t.frio.join(lich).join("Lich.stl").exists(),
        "solo el comprimido"
    );
    let (id, revisado, pendiente, n) = t.modelo(lich).await.unwrap();
    assert_eq!((revisado.as_str(), pendiente, n), ("Corregido", 0, 2));
    let (n_frio,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM copias_frio WHERE modelo_id = ? AND ruta LIKE '%/Pack.zip'",
    )
    .bind(id)
    .fetch_one(&t.pool)
    .await
    .unwrap();
    assert_eq!(n_frio, 1);
    let (estado, solo): (String, String) =
        sqlx::query_as("SELECT estado, solo_archivos FROM transferencias WHERE modelo_id = ?")
            .bind(id)
            .fetch_one(&t.pool)
            .await
            .unwrap();
    assert_eq!(estado, "terminada");
    assert_eq!(solo, format!("[{zip_id}]"));
}
