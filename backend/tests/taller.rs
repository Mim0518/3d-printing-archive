//! Fase 5: impresoras, materiales y perfiles (importación de los archivos de perfiles, reconocer la impresora
//! y el perfil de un laminado, edición con versiones y las reglas de qué cuadra con qué).

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
use printing_archive::{AppState, app, auth, config::Config, db, inventario};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tower::ServiceExt;

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

fn zip(raiz: &Path, rel: &str, entradas: &[(&str, String)]) {
    let p = raiz.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    let mut z = zip::ZipWriter::new(fs::File::create(p).unwrap());
    let op = zip::write::SimpleFileOptions::default();
    for (nombre, contenido) in entradas {
        z.start_file(*nombre, op).unwrap();
        z.write_all(contenido.as_bytes()).unwrap();
    }
    z.finish().unwrap();
}

/// Un perfil de resina como los guarda Photon Workshop.
fn resina(nombre: &str, marca: &str, resina: &str, precio: f64, exp: f64, base: f64) -> Value {
    json!({
        "property": { "name": nombre, "brand_name": marca, "resin_name": resina, "type": "Standard Resin",
                      "price": precio, "volume": 1000.0, "density": 1.2000000476837158 },
        "slicepara": { "zthick": 0.029999999329447746, "exposure_time": exp, "bott_time": base,
                       "bott_layers": 5, "zup_height": 8.0, "zup_speed": 3.0, "zdown_speed": 5.0,
                       "off_time": 1.0, "anti_count": 8 },
        "slice_extpara": { "transition_layercount": 15 }
    })
}

const SUNLU: &str = "Sunlu@Standard Resin Calibrado@NFEP@Normal Print";

async fn preparar(nombre: &str) -> (Juguete, SqlitePool, Router, String) {
    let base = std::env::temp_dir().join(format!("pa5-{nombre}-{}", std::process::id()));
    fs::remove_dir_all(&base).ok();
    let raiz = base.join("coleccion");
    // Photon Workshop: una escena con la impresora y dos resinas propias (la de Sunlu es la activa)
    let pwsp = json!({
        "machine_type": { "name": "Anycubic Photon Mono 4", "res_x": 9024, "res_y": 5120, "xy_pixel": 17.0,
                          "print_xsize": 153.4080047607422, "print_ysize": 87.04000091552735,
                          "print_zsize": 165.0 },
        "machine_extern": {
            "user_resins": [
                resina(SUNLU, "Sunlu", "Standard Resin Calibrado", 437.0, 1.7999999523162842, 15.0),
                resina("Anycubic@Water Wash@NFEP@Normal Print", "Anycubic", "Water Wash", 23.99, 2.0, 19.0),
            ],
            "active_resins": [SUNLU]
        }
    });
    let nombres =
        json!([{ "brand_name": "Sunlu", "name": "Standard Resin Calibrado", "density": 1.107 }]);
    zip(
        &raiz,
        "Printers/SLA/Photon Mono 4/R_E_R_F.pwscene",
        &[
            ("config/anycubic_photon_resins.pwsp", pwsp.to_string()),
            ("config/custom_resin_names.json", nombres.to_string()),
            ("preview/image_0.png", "x".into()),
        ],
    );
    // el mismo perfil de Sunlu exportado aparte, con otro nombre
    escribir(
        &raiz,
        "Printers/SLA/Photon Mono 4/Calibrado.pwsr",
        json!([resina("Calibrado", "", "", 437.0, 1.8, 15.0)])
            .to_string()
            .as_bytes(),
    );
    // Lychee: cifrado, solo se liga
    escribir(
        &raiz,
        "Printers/SLA/Configs/Anycubic Photon Mono 4 Resin.lyr",
        &[0xf7, 0x16, 0x65, 0xab],
    );
    // Orca: un paquete con la impresora, un filamento y un proceso
    zip(
        &raiz,
        "Printers/FDM/Ender3v3KE/Ender 0.4 nozzle.orca_printer",
        &[
            (
                "printer/Ender 0.4 nozzle.json",
                json!({ "name": "Creality Ender-3 V3 KE 0.4 nozzle", "printer_model": "Creality Ender-3 V3 KE",
                        "printer_settings_id": "x", "printer_technology": "FFF", "nozzle_diameter": ["0.6"],
                        "printable_area": ["0x0", "220x0", "220x220", "0x220"], "printable_height": "245" })
                .to_string(),
            ),
            (
                "filament/ML PLA.json",
                json!({ "name": "ML PLA Básico @Creality Ender-3 V3 KE 0.4 nozzle",
                        "filament_settings_id": ["ML PLA Básico @Creality Ender-3 V3 KE 0.4 nozzle"],
                        "filament_vendor": ["ML"], "filament_type": ["PLA"], "filament_cost": ["380"],
                        "filament_density": ["1.24"], "nozzle_temperature": ["220"],
                        "textured_plate_temp": ["55"], "fan_max_speed": ["100"] })
                .to_string(),
            ),
            (
                "process/0.20mm.json",
                json!({ "name": "0.20mm Standard @Creality Ender3V3KE - Copy", "print_settings_id": "x",
                        "sparse_infill_density": "7%", "sparse_infill_pattern": "gyroid" })
                .to_string(),
            ),
            ("bundle_structure.json", "{}".into()),
        ],
    );
    // un laminado de la Mono 4 en la carpeta de un modelo (su contenido se pone abajo, a mano)
    escribir(
        &raiz,
        "Wargames/Warhammer 40K/Necrons/Wraiths (SF)/Torso.pm4n",
        b"ANYCUBIC basura",
    );
    escribir(
        &raiz,
        "Wargames/Warhammer 40K/Necrons/Wraiths (SF)/Manual.pdf",
        b"%PDF",
    );

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
    // lo que habría leído un .pm4n de verdad
    sqlx::query(
        "UPDATE datos_placa SET res_x = 5120, res_y = 9024, capa_mm = 0.03, exposicion_s = 1.8,
                exposicion_base_s = 15, error = ''
         WHERE archivo_id = (SELECT id FROM archivos WHERE nombre = 'Torso.pm4n')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let mut config = Config::de_prueba(Some(raiz.clone()), None);
    config.miniaturas = base.join("cache");
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

fn buscar<'a>(lista: &'a Value, campo: &str, valor: &str) -> &'a Value {
    lista
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x[campo] == valor)
        .unwrap_or_else(|| panic!("no hay {campo} = {valor} en {lista}"))
}

async fn id_archivo(pool: &SqlitePool, nombre: &str) -> i64 {
    sqlx::query_scalar("SELECT id FROM archivos WHERE nombre = ?")
        .bind(nombre)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn importa_los_perfiles_y_reconoce_los_laminados() {
    let (_j, pool, app, c) = preparar("importa").await;
    // los perfiles de los laminadores ya no son escenas ni placas
    let tipo: String =
        sqlx::query_scalar("SELECT tipo FROM archivos WHERE nombre = 'Calibrado.pwsr'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(tipo, "perfil");

    let (st, r) = pedir(&app, &c, Method::GET, "/api/v1/taller/importar", None).await;
    assert_eq!(st, StatusCode::OK, "{r}");
    assert_eq!(r["errores"], json!([]), "{r}");
    let props = &r["propuestas"];
    let clases: Vec<String> = props
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            format!(
                "{}:{}",
                p["clase"].as_str().unwrap(),
                p["nombre"].as_str().unwrap()
            )
        })
        .collect();
    assert_eq!(
        clases,
        [
            "impresora:Anycubic Photon Mono 4",
            "impresora:Creality Ender-3 V3 KE",
            "material:Anycubic Standard Resin",
            "material:ML PLA Básico",
            "material:Sunlu Standard Resin",
            "perfil:Anycubic Photon Mono 4 Resin (Lychee)",
            "perfil:Anycubic Water Wash",
            "perfil:Sunlu Standard Resin Calibrado",
            "perfil:ML PLA Básico",
        ]
    );
    let mono = buscar(props, "nombre", "Anycubic Photon Mono 4");
    assert_eq!(mono["datos"]["res_x"], 9024);
    assert_eq!(mono["datos"]["volumen_x_mm"], 153.4);
    assert_eq!(mono["datos"]["carpeta"], "Printers/SLA/Photon Mono 4");
    let ender = buscar(props, "nombre", "Creality Ender-3 V3 KE");
    assert_eq!(ender["datos"]["boquilla_mm"], 0.6);
    assert_eq!(ender["datos"]["volumen_z_mm"], 245.0);
    assert!(
        ender["avisos"][0].as_str().unwrap().contains("0.6 mm"),
        "{ender}"
    );
    let sunlu = buscar(props, "nombre", "Sunlu Standard Resin");
    assert_eq!(sunlu["datos"]["densidad"], 1.107);
    assert_eq!(sunlu["datos"]["precio"], 437.0);
    let agua = buscar(props, "nombre", "Anycubic Standard Resin");
    assert!(
        agua["avisos"][0].as_str().unwrap().contains("dólares"),
        "{agua}"
    );
    // el .pwsr con otro nombre se ligó al perfil de Sunlu (mismos valores), que es el activo
    let perfil = buscar(props, "nombre", "Sunlu Standard Resin Calibrado");
    assert_eq!(perfil["predeterminado"], true);
    assert_eq!(
        perfil["archivos"],
        json!([id_archivo(&pool, "Calibrado.pwsr").await])
    );
    assert_eq!(perfil["datos"]["capa_mm"], 0.03);
    assert_eq!(perfil["datos"]["parametros"]["exposicion_s"], 1.8);
    assert_eq!(perfil["datos"]["parametros"]["capas_base"], 5);
    let pla = buscar(props, "nombre", "ML PLA Básico");
    let pla = if pla["clase"] == "perfil" {
        pla
    } else {
        props
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["clase"] == "perfil" && p["nombre"] == "ML PLA Básico")
            .unwrap()
    };
    assert_eq!(pla["datos"]["capa_mm"], 0.2);
    assert_eq!(pla["datos"]["parametros"]["temp_boquilla_c"], 220);
    assert_eq!(pla["datos"]["parametros"]["relleno_pct"], 7.0);
    assert_eq!(pla["datos"]["parametros"]["patron_relleno"], "gyroid");
    let lychee = buscar(props, "nombre", "Anycubic Photon Mono 4 Resin (Lychee)");
    assert_eq!(lychee["impresora"], mono["clave"]);
    assert_eq!(lychee["datos"]["parametros"], json!({}));

    // se eligen solo dos perfiles: sus impresoras y materiales entran solos
    let (st, r) = pedir(
        &app,
        &c,
        Method::POST,
        "/api/v1/taller/importar",
        Some(json!({ "claves": [perfil["clave"], pla["clave"]] })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{r}");
    assert_eq!(
        r,
        json!({ "impresoras": 2, "materiales": 2, "perfiles": 2, "archivos": 2 })
    );
    // otra vez: ya no se importan dos veces
    let (_, r) = pedir(&app, &c, Method::GET, "/api/v1/taller/importar", None).await;
    let ya: Vec<&str> = r["propuestas"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["ya"].is_number())
        .map(|p| p["nombre"].as_str().unwrap())
        .collect();
    assert_eq!(ya.len(), 6, "{ya:?}");
    let (_, r) = pedir(
        &app,
        &c,
        Method::POST,
        "/api/v1/taller/importar",
        Some(json!({ "claves": [perfil["clave"]] })),
    )
    .await;
    assert_eq!(r["perfiles"], 0);

    // la Mono 4 reconoce su laminado por la pantalla y el perfil por capa y exposición
    let (_, imps) = pedir(&app, &c, Method::GET, "/api/v1/impresoras", None).await;
    let m4 = buscar(&imps, "nombre", "Anycubic Photon Mono 4");
    assert_eq!(m4["perfil"], "Sunlu Standard Resin Calibrado");
    assert_eq!(m4["n_laminados"], 1);
    assert_eq!(m4["n_placas"], 1);
    let (_, placas) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/placas?impresora_id={}", m4["id"]),
        None,
    )
    .await;
    assert_eq!(placas["total"], 1, "{placas}");
    let pl = &placas["placas"][0];
    assert_eq!(pl["impresoras"][0]["nombre"], "Anycubic Photon Mono 4");
    assert_eq!(pl["impresoras"][0]["por"], "laminado");
    assert_eq!(pl["perfil"]["nombre"], "Sunlu Standard Resin Calibrado");
    assert_eq!(pl["material"]["nombre"], "Sunlu Standard Resin");
    assert_eq!(
        pl["laminados"][0]["perfil"]["nombre"],
        "Sunlu Standard Resin Calibrado"
    );
    let ender_id = buscar(&imps, "nombre", "Creality Ender-3 V3 KE")["id"].clone();
    let (_, nada) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/placas?impresora_id={ender_id}"),
        None,
    )
    .await;
    assert_eq!(nada["total"], 0);

    // la ficha del perfil sabe qué placas se imprimen con él y qué archivos lleva
    let perfil_id = m4["perfil_id"].as_i64().unwrap();
    let (_, f) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/perfiles/{perfil_id}"),
        None,
    )
    .await;
    assert_eq!(f["placas"][0]["por"], "laminado", "{f}");
    assert_eq!(f["archivos"][0]["nombre"], "Calibrado.pwsr");
    assert_eq!(f["predeterminado"], true);
    // la de la impresora lista los documentos de su carpeta
    let (_, fi) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/impresoras/{}", m4["id"]),
        None,
    )
    .await;
    let docs: Vec<&str> = fi["documentos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["nombre"].as_str().unwrap())
        .collect();
    assert_eq!(docs, ["Calibrado.pwsr", "R_E_R_F.pwscene"]);

    let (_, res) = pedir(&app, &c, Method::GET, "/api/v1/taller/resumen", None).await;
    assert_eq!(res["perfiles"], 2);
    assert_eq!(res["laminados_sin_impresora"], json!([]));
}

#[tokio::test]
async fn perfiles_a_mano_con_versiones_y_reglas() {
    let (_j, pool, app, c) = preparar("mano").await;
    let (st, r) = pedir(
        &app,
        &c,
        Method::POST,
        "/api/v1/impresoras",
        Some(json!({ "nombre": "Ender", "tecnologia": "fdm", "boquilla_mm": 0.4, "carpeta": "/Printers/FDM/" })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{r}");
    let ender = r["id"].as_i64().unwrap();
    let (_, r) = pedir(
        &app,
        &c,
        Method::POST,
        "/api/v1/impresoras",
        Some(json!({ "nombre": "Mono", "tecnologia": "resina", "res_x": 2560, "res_y": 1620 })),
    )
    .await;
    let mono = r["id"].as_i64().unwrap();
    let (st, _) = pedir(
        &app,
        &c,
        Method::POST,
        "/api/v1/impresoras",
        Some(json!({ "nombre": "Mala", "tecnologia": "resina", "carpeta": "../afuera" })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (_, r) = pedir(
        &app,
        &c,
        Method::POST,
        "/api/v1/materiales",
        Some(json!({ "tipo": "filamento", "marca": "ML", "nombre": "PLA", "presentacion": 1000, "precio": 380 })),
    )
    .await;
    let pla = r["id"].as_i64().unwrap();
    let (_, r) = pedir(
        &app,
        &c,
        Method::POST,
        "/api/v1/materiales",
        Some(json!({ "tipo": "resina", "nombre": "Gris", "presentacion": 1000, "precio": 450, "color_hex": "#888888" })),
    )
    .await;
    let gris = r["id"].as_i64().unwrap();
    let (_, mats) = pedir(&app, &c, Method::GET, "/api/v1/materiales", None).await;
    assert_eq!(buscar(&mats, "nombre", "PLA")["precio_unidad"], 0.38);

    // un campo de resina en una FDM, una resina en una FDM, un número fuera de rango: no
    for (cuerpo, error) in [
        (
            json!({ "nombre": "x", "impresora_id": ender, "parametros": { "exposicion_s": 2 } }),
            "no es un campo",
        ),
        (
            json!({ "nombre": "x", "impresora_id": ender, "material_id": gris }),
            "usa filamento",
        ),
        (
            json!({ "nombre": "x", "impresora_id": ender, "parametros": { "temp_boquilla_c": 900 } }),
            "entre",
        ),
    ] {
        let (st, r) = pedir(&app, &c, Method::POST, "/api/v1/perfiles", Some(cuerpo)).await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        assert!(r["error"].as_str().unwrap().contains(error), "{r}");
    }
    let (st, r) = pedir(
        &app,
        &c,
        Method::POST,
        "/api/v1/perfiles",
        Some(json!({ "nombre": "PLA minis", "impresora_id": ender, "material_id": pla, "capa_mm": 0.12,
                     "parametros": { "temp_boquilla_c": 210, "temp_cama_c": 60, "patron_relleno": "" },
                     "predeterminado": true })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{r}");
    let perfil = r["id"].as_i64().unwrap();
    let (_, f) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/perfiles/{perfil}"),
        None,
    )
    .await;
    assert_eq!(
        f["parametros"],
        json!({ "temp_boquilla_c": 210, "temp_cama_c": 60 })
    );
    assert_eq!(f["version"], 1);
    assert_eq!(f["predeterminado"], true);
    assert_eq!(f["material"], "ML PLA");

    // cambiar las notas no hace versión; cambiar los valores sí, y la anterior queda guardada
    let (_, r) = pedir(
        &app,
        &c,
        Method::PATCH,
        &format!("/api/v1/perfiles/{perfil}"),
        Some(json!({ "notas": "Para minis de 32 mm" })),
    )
    .await;
    assert_eq!(r["version"], 1);
    let (_, r) = pedir(
        &app,
        &c,
        Method::PATCH,
        &format!("/api/v1/perfiles/{perfil}"),
        Some(
            json!({ "parametros": { "temp_boquilla_c": 215, "temp_cama_c": 60 }, "capa_mm": 0.1 }),
        ),
    )
    .await;
    assert_eq!(r["version"], 2, "{r}");
    let (_, f) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/perfiles/{perfil}"),
        None,
    )
    .await;
    assert_eq!(f["version"], 2);
    assert_eq!(f["capa_mm"], 0.1);
    assert_eq!(f["notas"], "Para minis de 32 mm");
    assert_eq!(f["versiones"][0]["version"], 1);
    assert_eq!(f["versiones"][0]["capa_mm"], 0.12);
    assert_eq!(f["versiones"][0]["parametros"]["temp_boquilla_c"], 210);
    assert_eq!(f["versiones"][0]["usuario"], "memo");
    // pasarlo a una impresora de resina: sus valores de FDM ya no sirven
    let (st, _) = pedir(
        &app,
        &c,
        Method::PATCH,
        &format!("/api/v1/perfiles/{perfil}"),
        Some(json!({ "impresora_id": mono, "material_id": null })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // archivos del laminador
    let (_, enc) = pedir(
        &app,
        &c,
        Method::GET,
        "/api/v1/archivos/buscar?q=orca%20ender",
        None,
    )
    .await;
    assert_eq!(enc[0]["nombre"], "Ender 0.4 nozzle.orca_printer", "{enc}");
    let (_, _) = pedir(
        &app,
        &c,
        Method::POST,
        &format!("/api/v1/perfiles/{perfil}/archivos"),
        Some(json!({ "archivo_id": enc[0]["id"] })),
    )
    .await;
    let (_, f) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/perfiles/{perfil}"),
        None,
    )
    .await;
    assert_eq!(f["n_archivos"], 1);

    // la impresora y el material en uso no se borran
    let (st, r) = pedir(
        &app,
        &c,
        Method::DELETE,
        &format!("/api/v1/impresoras/{ender}"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT, "{r}");
    let (st, _) = pedir(
        &app,
        &c,
        Method::DELETE,
        &format!("/api/v1/materiales/{pla}"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);
    let (st, _) = pedir(
        &app,
        &c,
        Method::DELETE,
        &format!("/api/v1/materiales/{gris}"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    // ni se cambia la tecnología de una impresora con perfiles
    let (st, _) = pedir(
        &app,
        &c,
        Method::PATCH,
        &format!("/api/v1/impresoras/{ender}"),
        Some(json!({ "tecnologia": "resina" })),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);
    let (_, fi) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/impresoras/{ender}"),
        None,
    )
    .await;
    assert_eq!(fi["carpeta"], "Printers/FDM");
    assert_eq!(fi["perfil"], "PLA minis");
    assert_eq!(
        fi["documentos"].as_array().unwrap().len(),
        1,
        "{}",
        fi["documentos"]
    );

    // una placa: el perfil de otra impresora no; la impresora con su material y perfil sí
    let placa: i64 = sqlx::query_scalar("SELECT id FROM placas LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    let (st, _) = pedir(
        &app,
        &c,
        Method::PATCH,
        &format!("/api/v1/placas/{placa}"),
        Some(json!({ "impresora_id": mono, "perfil_id": perfil })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, r) = pedir(
        &app,
        &c,
        Method::PATCH,
        &format!("/api/v1/placas/{placa}"),
        Some(json!({ "impresora_id": ender, "perfil_id": perfil, "material_id": pla })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{r}");
    let (_, f) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/placas/{placa}"),
        None,
    )
    .await;
    assert_eq!(
        f["impresoras"][0],
        json!({ "id": ender, "nombre": "Ender", "por": "elegida" })
    );
    assert_eq!(f["perfil"]["nombre"], "PLA minis");
    assert_eq!(f["material"]["nombre"], "ML PLA");
    // se quita la impresora y el perfil: el material queda
    let (_, _) = pedir(
        &app,
        &c,
        Method::PATCH,
        &format!("/api/v1/placas/{placa}"),
        Some(json!({ "impresora_id": null, "perfil_id": null })),
    )
    .await;
    let (_, f) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/placas/{placa}"),
        None,
    )
    .await;
    assert_eq!(f["perfil"], Value::Null);
    assert_eq!(f["material"]["nombre"], "ML PLA");

    // borrar el perfil: la impresora se queda sin perfil normal y ya se puede borrar
    let (st, _) = pedir(
        &app,
        &c,
        Method::DELETE,
        &format!("/api/v1/perfiles/{perfil}"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let (st, _) = pedir(
        &app,
        &c,
        Method::DELETE,
        &format!("/api/v1/impresoras/{ender}"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
}
