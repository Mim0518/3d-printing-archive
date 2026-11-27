//! Fase 7: clientes, pedidos, pagos y colecciones de venta (una colección se convierte en pedido, se aparta
//! de las existencias, lo que falta se imprime para el pedido, se cobra y se entrega).

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

async fn preparar(nombre: &str) -> (Juguete, SqlitePool, Router, String) {
    let base = std::env::temp_dir().join(format!("pa7-{nombre}-{}", std::process::id()));
    fs::remove_dir_all(&base).ok();
    let raiz = base.join("coleccion");
    escribir(&raiz, &format!("{A}/Supported/Torso.photon"), &[7u8; 200]);
    escribir(&raiz, &format!("{B}/b.obj"), b"v 0 0 0\n");
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

async fn modelo(pool: &SqlitePool, nombre: &str) -> i64 {
    sqlx::query_scalar("SELECT id FROM modelos WHERE nombre = ?")
        .bind(nombre)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn hay(app: &Router, c: &str, modelo: i64) -> i64 {
    let (_, m) = pedir(
        app,
        c,
        Method::GET,
        &format!("/api/v1/modelos/{modelo}/existencias"),
        None,
    )
    .await;
    m["existencias"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["cantidad"].as_i64().unwrap())
        .sum()
}

fn linea<'a>(p: &'a Value, nombre: &str) -> &'a Value {
    p["lineas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["nombre"] == nombre)
        .unwrap()
}

#[tokio::test]
async fn coleccion_pedido_apartado_cobro_y_entrega() {
    let (_j, pool, app, c) = preparar("venta").await;
    let wraiths = modelo(&pool, "Wraiths").await;
    let boyz = modelo(&pool, "Boyz Mob").await;
    pedir(
        &app,
        &c,
        Method::PUT,
        "/api/v1/ajustes/costos",
        Some(json!({ "valores": { "margen": 3, "empaque_pieza": 5, "imprimacion_pieza": 2,
                                  "pintura_material_pieza": 8, "pintura_horas_pieza": 1, "mano_obra_hora": 100 } })),
    )
    .await;
    for (m, acabado, lugar, cant, costo) in [
        (wraiths, "sin_pintar", "Caja 1", 5, 10.0),
        (boyz, "pintada", "Vitrina", 2, 20.0),
    ] {
        let (st, r) = pedir(
            &app,
            &c,
            Method::POST,
            "/api/v1/existencias",
            Some(
                json!({ "modelo_id": m, "acabado": acabado, "ubicacion": lugar, "cantidad": cant,
                         "costo_unitario": costo }),
            ),
        )
        .await;
        assert_eq!(st, StatusCode::CREATED, "{r}");
    }

    // la colección: 3 Wraiths sin pintar (precio sugerido 10 × 3 + 5 = 35) y 1 Boyz pintado a $300 a mano
    let (st, r) = pedir(
        &app,
        &c,
        Method::POST,
        "/api/v1/colecciones",
        Some(json!({ "nombre": "Escuadra de inicio", "descripcion": "Para empezar a jugar" })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    let col = r["id"].as_i64().unwrap();
    for cuerpo in [
        json!({ "modelo_id": wraiths, "cantidad": 3 }),
        json!({ "modelo_id": boyz, "acabado": "pintada", "precio": 300 }),
    ] {
        let (st, r) = pedir(
            &app,
            &c,
            Method::POST,
            &format!("/api/v1/colecciones/{col}/modelos"),
            Some(cuerpo),
        )
        .await;
        assert_eq!(st, StatusCode::CREATED, "{r}");
    }
    let (st, _) = pedir(
        &app,
        &c,
        Method::POST,
        &format!("/api/v1/colecciones/{col}/modelos"),
        Some(json!({ "modelo_id": wraiths })),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);
    let foto_ajena: i64 = sqlx::query_scalar("SELECT id FROM archivos WHERE nombre = 'b.obj'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let (st, _) = pedir(
        &app,
        &c,
        Method::POST,
        &format!("/api/v1/colecciones/{col}/modelos"),
        Some(json!({ "modelo_id": wraiths, "acabado": "pintada", "foto_id": foto_ajena })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (_, v) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/colecciones/{col}"),
        None,
    )
    .await;
    assert_eq!(linea(&v, "Wraiths")["precio"], 35.0, "{v}");
    assert_eq!(linea(&v, "Boyz Mob")["precio"], 300.0);
    assert_eq!(linea(&v, "Boyz Mob")["precio_sugerido"], 395.0);
    assert_eq!(v["suma"], 405.0);
    assert_eq!(v["precio"], 405.0);
    assert_eq!(v["costo"], 50.0);
    assert_eq!(v["ganancia"], 355.0);
    assert_eq!(v["alcanza"], true);
    // precio fijo
    pedir(
        &app,
        &c,
        Method::PATCH,
        &format!("/api/v1/colecciones/{col}"),
        Some(json!({ "precio_fijo": 400, "estado": "activa" })),
    )
    .await;
    let (_, l) = pedir(
        &app,
        &c,
        Method::GET,
        "/api/v1/colecciones?estado=activa",
        None,
    )
    .await;
    assert_eq!(l[0]["precio"], 400.0);
    assert_eq!(l[0]["n_modelos"], 2);

    // dos colecciones para Ana (cliente nuevo): los precios se ajustan para que el total sea 2 × 400
    let (st, r) = pedir(
        &app,
        &c,
        Method::POST,
        &format!("/api/v1/colecciones/{col}/pedido"),
        Some(json!({ "veces": 2, "cliente": { "nombre": "Ana", "whatsapp": "33 1234 5678" } })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{r}");
    let pedido = r["id"].as_i64().unwrap();
    let (_, p) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/pedidos/{pedido}"),
        None,
    )
    .await;
    assert_eq!(p["estado"], "cotizado");
    assert_eq!(p["cliente"], "Ana");
    assert_eq!(p["coleccion"], "Escuadra de inicio");
    assert_eq!(p["total"], 800.0, "{p}");
    assert_eq!(p["piezas"], 8);
    assert_eq!(p["faltan"], 8);
    assert_eq!(linea(&p, "Wraiths")["cantidad"], 6);
    assert_eq!(linea(&p, "Wraiths")["disponibles"][0]["cantidad"], 5);

    // apartar: de los Wraiths sin pintar alcanzan 5 de 6; los Boyz de la vitrina
    let w = linea(&p, "Wraiths")["id"].as_i64().unwrap();
    let b = linea(&p, "Boyz Mob")["id"].as_i64().unwrap();
    let (st, r) = pedir(
        &app,
        &c,
        Method::POST,
        &format!("/api/v1/pedidos/{pedido}/piezas/{w}/apartar"),
        Some(json!({})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{r}");
    assert_eq!(r, json!({ "apartadas": 5, "faltan": 1 }));
    assert_eq!(hay(&app, &c, wraiths).await, 0);
    let existencia_boyz = linea(&p, "Boyz Mob")["disponibles"][0]["id"].clone();
    let (st, _) = pedir(
        &app,
        &c,
        Method::POST,
        &format!("/api/v1/pedidos/{pedido}/piezas/{b}/apartar"),
        Some(json!({ "existencia_id": existencia_boyz, "cantidad": 2 })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    // no se aparta de más ni de otro modelo
    let (st, _) = pedir(
        &app,
        &c,
        Method::POST,
        &format!("/api/v1/pedidos/{pedido}/piezas/{b}/apartar"),
        Some(json!({ "cantidad": 1 })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _) = pedir(
        &app,
        &c,
        Method::POST,
        &format!("/api/v1/pedidos/{pedido}/piezas/{w}/apartar"),
        Some(json!({ "existencia_id": existencia_boyz })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    // no se entrega con faltantes
    let (st, r) = pedir(
        &app,
        &c,
        Method::PATCH,
        &format!("/api/v1/pedidos/{pedido}"),
        Some(json!({ "estado": "entregado" })),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT, "{r}");

    // el que falta se imprime para el pedido: salen 2, uno se aparta solo y el otro se queda en la caja
    let placa: i64 = sqlx::query_scalar("SELECT id FROM placas WHERE nombre = 'Torso'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let (st, r) = pedir(
        &app,
        &c,
        Method::POST,
        "/api/v1/impresiones",
        Some(
            json!({ "placa_id": placa, "resultado": "bien", "pedido_id": pedido,
                     "piezas": [{ "modelo_id": wraiths, "cantidad": 2, "ubicacion": "Caja 1" }] }),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{r}");
    assert_eq!(r["apartadas"], 1);
    assert_eq!(hay(&app, &c, wraiths).await, 1);
    let (_, imp) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/impresiones?pedido_id={pedido}"),
        None,
    )
    .await;
    assert_eq!(imp["total"], 1);

    // anticipo y saldo
    for (monto, metodo) in [(300, "transferencia"), (500, "efectivo")] {
        let (st, _) = pedir(
            &app,
            &c,
            Method::POST,
            &format!("/api/v1/pedidos/{pedido}/pagos"),
            Some(json!({ "monto": monto, "metodo": metodo })),
        )
        .await;
        assert_eq!(st, StatusCode::CREATED);
    }
    for malo in [
        json!({ "monto": 0 }),
        json!({ "monto": 10, "metodo": "trueque" }),
    ] {
        let (st, _) = pedir(
            &app,
            &c,
            Method::POST,
            &format!("/api/v1/pedidos/{pedido}/pagos"),
            Some(malo),
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
    }
    let (st, _) = pedir(
        &app,
        &c,
        Method::PATCH,
        &format!("/api/v1/pedidos/{pedido}"),
        Some(json!({ "estado": "entregado", "envio": 0 })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let (_, p) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/pedidos/{pedido}"),
        None,
    )
    .await;
    assert_eq!(p["estado"], "entregado");
    assert!(p["entregado"].is_number());
    assert_eq!(p["pagado"], 800.0);
    assert_eq!(p["saldo"], 0.0);
    assert_eq!(p["faltan"], 0);
    // costo: 6 Wraiths a $10 (la impresión no tenía costos capturados: no baja el promedio) y 2 Boyz a $20
    assert_eq!(p["costo"], 100.0, "{p}");
    assert_eq!(p["ganancia"], 700.0);
    assert_eq!(p["pagos"].as_array().unwrap().len(), 2);
    // ya entregado: no se cancela ni cambia sus piezas
    let (st, _) = pedir(
        &app,
        &c,
        Method::PATCH,
        &format!("/api/v1/pedidos/{pedido}"),
        Some(json!({ "estado": "cancelado" })),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);
    let (st, _) = pedir(
        &app,
        &c,
        Method::POST,
        &format!("/api/v1/pedidos/{pedido}/piezas"),
        Some(json!({ "modelo_id": boyz, "cantidad": 1 })),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);

    // el resumen de ventas
    let (_, r) = pedir(&app, &c, Method::GET, "/api/v1/ventas/resumen", None).await;
    assert_eq!(r["vendidos"], 1, "{r}");
    assert_eq!(r["piezas"], 8);
    assert_eq!(r["ingresos"], 800.0);
    assert_eq!(r["ganancia"], 700.0);
    assert_eq!(r["por_cobrar"], 0.0);
    assert_eq!(r["mas_vendidos"][0]["nombre"], "Wraiths");
    assert_eq!(r["mas_vendidos"][0]["cantidad"], 6);
    assert_eq!(r["mas_vendidos"][0]["hay"], 1);
    assert_eq!(r["facciones"][0]["faccion"], "Necrons");

    // el cliente y sus pedidos
    let cliente = p["cliente_id"].as_i64().unwrap();
    let (_, cl) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/clientes/{cliente}"),
        None,
    )
    .await;
    assert_eq!(cl["whatsapp"], "33 1234 5678");
    assert_eq!(cl["pedidos"], 1);
    assert_eq!(cl["comprado"], 800.0);
    assert_eq!(cl["lista"][0]["id"], pedido);
    let (st, _) = pedir(
        &app,
        &c,
        Method::DELETE,
        &format!("/api/v1/clientes/{cliente}"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);
}

#[tokio::test]
async fn cancelar_y_quitar_regresan_las_miniaturas() {
    let (_j, pool, app, c) = preparar("cancela").await;
    let wraiths = modelo(&pool, "Wraiths").await;
    pedir(
        &app,
        &c,
        Method::POST,
        "/api/v1/existencias",
        Some(json!({ "modelo_id": wraiths, "cantidad": 4, "costo_unitario": 12 })),
    )
    .await;
    let (st, r) = pedir(
        &app,
        &c,
        Method::POST,
        "/api/v1/clientes",
        Some(json!({ "nombre": "Beto", "redes": "@beto_minis" })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    let beto = r["id"].as_i64().unwrap();
    let (st, _) = pedir(
        &app,
        &c,
        Method::POST,
        "/api/v1/clientes",
        Some(json!({ "nombre": " " })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    // pedido a mano: precio puesto y sin precio (no hay margen: queda en 0 y se avisa)
    let (st, r) = pedir(
        &app,
        &c,
        Method::POST,
        "/api/v1/pedidos",
        Some(json!({ "cliente_id": beto, "piezas": [
            { "modelo_id": wraiths, "cantidad": 3, "precio": 50 },
        ] })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{r}");
    let pedido = r["id"].as_i64().unwrap();
    let (st, r) = pedir(
        &app,
        &c,
        Method::POST,
        &format!("/api/v1/pedidos/{pedido}/piezas"),
        Some(json!({ "modelo_id": modelo(&pool, "Boyz Mob").await, "cantidad": 1 })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    assert_eq!(r["sin_precio"], true);
    let boyz_pieza = r["id"].as_i64().unwrap();
    let (_, p) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/pedidos/{pedido}"),
        None,
    )
    .await;
    let w = linea(&p, "Wraiths")["id"].as_i64().unwrap();
    // se suma a la misma pieza
    pedir(
        &app,
        &c,
        Method::POST,
        &format!("/api/v1/pedidos/{pedido}/piezas"),
        Some(json!({ "modelo_id": wraiths, "cantidad": 1 })),
    )
    .await;
    let (_, p) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/pedidos/{pedido}"),
        None,
    )
    .await;
    assert_eq!(linea(&p, "Wraiths")["cantidad"], 4);
    assert_eq!(p["subtotal"], 200.0);
    // descuento y envío
    pedir(
        &app,
        &c,
        Method::PATCH,
        &format!("/api/v1/pedidos/{pedido}"),
        Some(json!({ "descuento": 20, "envio": 90, "estado": "confirmado" })),
    )
    .await;
    let (_, p) = pedir(
        &app,
        &c,
        Method::GET,
        &format!("/api/v1/pedidos/{pedido}"),
        None,
    )
    .await;
    assert_eq!(p["total"], 270.0);
    // apartar 3, bajar la cantidad debajo de lo apartado no se puede
    pedir(
        &app,
        &c,
        Method::POST,
        &format!("/api/v1/pedidos/{pedido}/piezas/{w}/apartar"),
        Some(json!({ "cantidad": 3 })),
    )
    .await;
    assert_eq!(hay(&app, &c, wraiths).await, 1);
    let (st, _) = pedir(
        &app,
        &c,
        Method::PATCH,
        &format!("/api/v1/pedidos/{pedido}/piezas/{w}"),
        Some(json!({ "cantidad": 2 })),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);
    // desapartar regresa
    pedir(
        &app,
        &c,
        Method::POST,
        &format!("/api/v1/pedidos/{pedido}/piezas/{w}/desapartar"),
        None,
    )
    .await;
    assert_eq!(hay(&app, &c, wraiths).await, 4);
    // apartar otra vez y quitar la pieza: regresan
    pedir(
        &app,
        &c,
        Method::POST,
        &format!("/api/v1/pedidos/{pedido}/piezas/{w}/apartar"),
        Some(json!({ "cantidad": 2 })),
    )
    .await;
    assert_eq!(hay(&app, &c, wraiths).await, 2);
    let (st, _) = pedir(
        &app,
        &c,
        Method::DELETE,
        &format!("/api/v1/pedidos/{pedido}/piezas/{w}"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(hay(&app, &c, wraiths).await, 4);
    // otra vez y cancelar el pedido: regresan
    let (_, r) = pedir(
        &app,
        &c,
        Method::POST,
        &format!("/api/v1/pedidos/{pedido}/piezas"),
        Some(json!({ "modelo_id": wraiths, "cantidad": 2, "precio": 50 })),
    )
    .await;
    let w2 = r["id"].as_i64().unwrap();
    pedir(
        &app,
        &c,
        Method::POST,
        &format!("/api/v1/pedidos/{pedido}/piezas/{w2}/apartar"),
        Some(json!({})),
    )
    .await;
    assert_eq!(hay(&app, &c, wraiths).await, 2);
    // con un pago no se borra; cancelado sí regresa las miniaturas
    let (_, pago) = pedir(
        &app,
        &c,
        Method::POST,
        &format!("/api/v1/pedidos/{pedido}/pagos"),
        Some(json!({ "monto": 100 })),
    )
    .await;
    let (st, _) = pedir(
        &app,
        &c,
        Method::DELETE,
        &format!("/api/v1/pedidos/{pedido}"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);
    let (st, r) = pedir(
        &app,
        &c,
        Method::PATCH,
        &format!("/api/v1/pedidos/{pedido}"),
        Some(json!({ "estado": "cancelado" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(r["movimientos_deshechos"], 1);
    assert_eq!(hay(&app, &c, wraiths).await, 4);
    let (st, _) = pedir(
        &app,
        &c,
        Method::POST,
        &format!("/api/v1/pedidos/{pedido}/piezas/{boyz_pieza}/apartar"),
        Some(json!({})),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);
    // el pago se devuelve (se borra) y ya se puede borrar el pedido
    pedir(
        &app,
        &c,
        Method::DELETE,
        &format!("/api/v1/pagos/{}", pago["id"]),
        None,
    )
    .await;
    let (st, _) = pedir(
        &app,
        &c,
        Method::DELETE,
        &format!("/api/v1/pedidos/{pedido}"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    // sin pedidos, el cliente se puede borrar
    let (st, _) = pedir(
        &app,
        &c,
        Method::DELETE,
        &format!("/api/v1/clientes/{beto}"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    // filtros de la lista
    let (_, l) = pedir(
        &app,
        &c,
        Method::GET,
        "/api/v1/pedidos?estado=abiertos",
        None,
    )
    .await;
    assert_eq!(l["total"], 0);
}
