//! Ventas (fase 7): clientes, pedidos con sus piezas y pagos, colecciones de venta y el resumen de lo vendido.
//! Exigen sesión.
//!
//! Una pieza de un pedido se surte apartando miniaturas de las existencias: cada apartado es un movimiento de
//! salida (motivo «venta») ligado a la pieza, con el costo promedio de la existencia en ese momento. Quitar la
//! pieza o cancelar el pedido borra esos movimientos y las miniaturas regresan.

use std::collections::HashMap;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, patch, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{QueryBuilder, Sqlite, SqliteConnection, SqlitePool};

use super::existencias::{
    acabado_valido, cantidad, como_json, doble_f, doble_i, notas_validas, portadas,
};
use crate::{
    AppState,
    auth::Usuario,
    costos::{self, Ajustes, SQL_CANTIDAD, SQL_COSTO},
    db::ahora,
    error::ApiError,
};

pub fn rutas() -> Router<AppState> {
    Router::new()
        .route("/clientes", get(clientes).post(crear_cliente))
        .route(
            "/clientes/{id}",
            get(cliente).patch(editar_cliente).delete(borrar_cliente),
        )
        .route("/pedidos", get(pedidos).post(crear_pedido))
        .route(
            "/pedidos/{id}",
            get(pedido).patch(editar_pedido).delete(borrar_pedido),
        )
        .route("/pedidos/{id}/piezas", post(agregar_pieza))
        .route(
            "/pedidos/{id}/piezas/{pieza}",
            patch(editar_pieza).delete(quitar_pieza),
        )
        .route("/pedidos/{id}/piezas/{pieza}/apartar", post(apartar))
        .route("/pedidos/{id}/piezas/{pieza}/desapartar", post(desapartar))
        .route("/pedidos/{id}/pagos", post(pagar))
        .route("/pagos/{id}", delete(borrar_pago))
        .route("/colecciones", get(colecciones).post(crear_coleccion))
        .route(
            "/colecciones/{id}",
            get(coleccion)
                .patch(editar_coleccion)
                .delete(borrar_coleccion),
        )
        .route("/colecciones/{id}/modelos", post(agregar_linea))
        .route(
            "/colecciones/{id}/modelos/{linea}",
            patch(editar_linea).delete(quitar_linea),
        )
        .route("/colecciones/{id}/pedido", post(coleccion_a_pedido))
        .route("/modelos/{id}/fotos", get(fotos_de_modelo))
        .route("/ventas/resumen", get(resumen))
}

pub const ESTADOS: [&str; 6] = [
    "cotizado",
    "confirmado",
    "imprimiendo",
    "listo",
    "entregado",
    "cancelado",
];

fn centavos(x: f64) -> f64 {
    // + 0.0: sin «-0» (la suma de nada en f64 da -0.0)
    (x * 100.0).round() / 100.0 + 0.0
}

fn texto_valido(etiqueta: &str, v: &str, max: usize, requerido: bool) -> Result<String, ApiError> {
    let v = v.trim();
    if requerido && v.is_empty() {
        return Err(ApiError::Invalido(format!("Falta {etiqueta}")));
    }
    if v.chars().count() > max {
        return Err(ApiError::Invalido(format!(
            "{etiqueta} pasa de {max} letras"
        )));
    }
    Ok(v.to_string())
}

fn dinero_valido(etiqueta: &str, x: f64) -> Result<f64, ApiError> {
    if !x.is_finite() || !(0.0..=10_000_000.0).contains(&x) {
        return Err(ApiError::Invalido(format!("{etiqueta} no es válido")));
    }
    Ok(centavos(x))
}

async fn existe(pool: &SqlitePool, sql: &'static str, id: i64) -> Result<bool, sqlx::Error> {
    let n: i64 = sqlx::query_scalar(sql).bind(id).fetch_one(pool).await?;
    Ok(n > 0)
}

// ---------- clientes ----------

#[derive(Debug, Serialize, sqlx::FromRow)]
struct FilaCliente {
    id: i64,
    nombre: String,
    whatsapp: String,
    telefono: String,
    correo: String,
    redes: String,
    notas: String,
    pedidos: i64,
    comprado: f64,
    ultimo: Option<i64>,
}

const SELECT_CLIENTE: &str = "SELECT c.id, c.nombre, c.whatsapp, c.telefono, c.correo, c.redes, c.notas,
        (SELECT count(*) FROM pedidos p WHERE p.cliente_id = c.id AND p.estado <> 'cancelado') AS pedidos,
        coalesce((SELECT sum(pp.cantidad * pp.precio) - 0.0 FROM pedido_piezas pp JOIN pedidos p ON p.id = pp.pedido_id
                  WHERE p.cliente_id = c.id AND p.estado = 'entregado'), 0.0)
          - coalesce((SELECT sum(p.descuento) FROM pedidos p WHERE p.cliente_id = c.id AND p.estado = 'entregado'), 0.0)
          AS comprado,
        (SELECT max(p.creado) FROM pedidos p WHERE p.cliente_id = c.id) AS ultimo
    FROM clientes c";

#[derive(Debug, Default, Deserialize)]
struct BuscarClientes {
    q: Option<String>,
}

async fn clientes(
    _u: Usuario,
    State(st): State<AppState>,
    Query(b): Query<BuscarClientes>,
) -> Result<Json<Value>, ApiError> {
    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(SELECT_CLIENTE);
    if let Some(q) = b.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        let patron = format!("%{}%", q.replace(['%', '_'], ""));
        qb.push(" WHERE c.nombre LIKE ")
            .push_bind(patron.clone())
            .push(" OR c.whatsapp LIKE ")
            .push_bind(patron.clone())
            .push(" OR c.telefono LIKE ")
            .push_bind(patron.clone())
            .push(" OR c.redes LIKE ")
            .push_bind(patron);
    }
    qb.push(" ORDER BY ultimo IS NULL, ultimo DESC, c.nombre LIMIT 1000");
    let filas: Vec<FilaCliente> = qb.build_query_as().fetch_all(&st.pool).await?;
    Ok(Json(json!(filas)))
}

#[derive(Debug, Default, Deserialize)]
struct DatosCliente {
    nombre: Option<String>,
    whatsapp: Option<String>,
    telefono: Option<String>,
    correo: Option<String>,
    redes: Option<String>,
    notas: Option<String>,
}

/// Los campos que vinieron, ya revisados.
fn campos_cliente(d: &DatosCliente) -> Result<Vec<(&'static str, String)>, ApiError> {
    let mut c = Vec::new();
    for (campo, etiqueta, v, max, req) in [
        ("nombre", "El nombre", &d.nombre, 120, true),
        ("whatsapp", "El WhatsApp", &d.whatsapp, 40, false),
        ("telefono", "El teléfono", &d.telefono, 40, false),
        ("correo", "El correo", &d.correo, 120, false),
        ("redes", "Las redes", &d.redes, 200, false),
        ("notas", "Las notas", &d.notas, 2000, false),
    ] {
        if let Some(v) = v {
            c.push((campo, texto_valido(etiqueta, v, max, req)?));
        }
    }
    Ok(c)
}

async fn guardar_cliente(
    tx: &mut SqliteConnection,
    id: i64,
    campos: Vec<(&'static str, String)>,
) -> Result<(), sqlx::Error> {
    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new("UPDATE clientes SET actualizado = ");
    qb.push_bind(ahora());
    for (k, v) in campos {
        qb.push(format!(", {k} = ")).push_bind(v);
    }
    qb.push(" WHERE id = ").push_bind(id);
    qb.build().execute(&mut *tx).await?;
    Ok(())
}

async fn nuevo_cliente(tx: &mut SqliteConnection, d: &DatosCliente) -> Result<i64, ApiError> {
    if d.nombre.as_deref().is_none_or(|n| n.trim().is_empty()) {
        return Err(ApiError::Invalido("Falta el nombre del cliente".into()));
    }
    let campos = campos_cliente(d)?;
    let t = ahora();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO clientes (nombre, creado, actualizado) VALUES ('', ?, ?) RETURNING id",
    )
    .bind(t)
    .bind(t)
    .fetch_one(&mut *tx)
    .await?;
    guardar_cliente(tx, id, campos).await?;
    Ok(id)
}

async fn crear_cliente(
    _u: Usuario,
    State(st): State<AppState>,
    Json(d): Json<DatosCliente>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let mut tx = st.pool.begin().await?;
    let id = nuevo_cliente(&mut tx, &d).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

async fn cliente(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let fila: FilaCliente = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SELECT_CLIENTE} WHERE c.id = ?"
    )))
    .bind(id)
    .fetch_optional(&st.pool)
    .await?
    .ok_or(ApiError::NoEncontrado)?;
    let mut v = como_json(&fila);
    v["lista"] = json!(
        listar_pedidos(
            &st.pool,
            &FiltrosPedidos {
                cliente_id: Some(id),
                ..Default::default()
            }
        )
        .await?["pedidos"]
    );
    Ok(Json(v))
}

async fn editar_cliente(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(d): Json<DatosCliente>,
) -> Result<Json<Value>, ApiError> {
    if !existe(&st.pool, "SELECT count(*) FROM clientes WHERE id = ?", id).await? {
        return Err(ApiError::NoEncontrado);
    }
    let campos = campos_cliente(&d)?;
    let mut tx = st.pool.begin().await?;
    guardar_cliente(&mut tx, id, campos).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

async fn borrar_cliente(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    if existe(
        &st.pool,
        "SELECT count(*) FROM pedidos WHERE cliente_id = ?",
        id,
    )
    .await?
    {
        return Err(ApiError::Conflicto(
            "Tiene pedidos: no se borra (su historial se perdería)".into(),
        ));
    }
    let r = sqlx::query("DELETE FROM clientes WHERE id = ?")
        .bind(id)
        .execute(&st.pool)
        .await?;
    if r.rows_affected() == 0 {
        return Err(ApiError::NoEncontrado);
    }
    Ok(Json(json!({ "ok": true })))
}

// ---------- pedidos ----------

#[derive(Debug, Default, Deserialize)]
struct FiltrosPedidos {
    /// un estado, o «abiertos» (ni entregados ni cancelados)
    estado: Option<String>,
    cliente_id: Option<i64>,
    q: Option<String>,
    pagina: Option<i64>,
    por_pagina: Option<i64>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct FilaPedido {
    id: i64,
    cliente_id: Option<i64>,
    cliente: Option<String>,
    estado: String,
    coleccion_id: Option<i64>,
    coleccion: Option<String>,
    descuento: f64,
    envio: f64,
    fecha_entrega: Option<i64>,
    entregado: Option<i64>,
    notas: String,
    creado: i64,
    actualizado: i64,
    piezas: i64,
    subtotal: f64,
    pagado: f64,
    apartadas: i64,
    costo: Option<f64>,
}

const SELECT_PEDIDO: &str = "SELECT p.id, p.cliente_id, c.nombre AS cliente, p.estado, p.coleccion_id,
        co.nombre AS coleccion, p.descuento, p.envio, p.fecha_entrega, p.entregado, p.notas, p.creado, p.actualizado,
        coalesce((SELECT sum(pp.cantidad) FROM pedido_piezas pp WHERE pp.pedido_id = p.id), 0) AS piezas,
        coalesce((SELECT sum(pp.cantidad * pp.precio) FROM pedido_piezas pp WHERE pp.pedido_id = p.id), 0.0) AS subtotal,
        coalesce((SELECT sum(pg.monto) FROM pagos pg WHERE pg.pedido_id = p.id), 0.0) AS pagado,
        coalesce((SELECT -sum(mv.delta) FROM movimientos mv JOIN pedido_piezas pp ON pp.id = mv.pedido_pieza_id
                  WHERE pp.pedido_id = p.id), 0) AS apartadas,
        (SELECT sum(-mv.delta * mv.costo_unitario) FROM movimientos mv JOIN pedido_piezas pp ON pp.id = mv.pedido_pieza_id
         WHERE pp.pedido_id = p.id) AS costo
    FROM pedidos p LEFT JOIN clientes c ON c.id = p.cliente_id LEFT JOIN colecciones co ON co.id = p.coleccion_id";

/// Totales de un pedido: total, saldo, lo que falta apartar y la ganancia (si ya se sabe el costo de todo).
fn pedido_json(f: &FilaPedido) -> Value {
    let mut v = como_json(f);
    v["costo"] = json!(f.costo.map(centavos));
    let total = centavos(f.subtotal - f.descuento + f.envio);
    v["total"] = json!(total);
    v["saldo"] = json!(centavos(total - f.pagado));
    v["faltan"] = json!(f.piezas - f.apartadas);
    v["ganancia"] = match f.costo {
        Some(c) if f.apartadas == f.piezas && f.piezas > 0 => {
            json!(centavos(f.subtotal - f.descuento - c))
        }
        _ => Value::Null,
    };
    v
}

async fn listar_pedidos(pool: &SqlitePool, f: &FiltrosPedidos) -> Result<Value, ApiError> {
    let por_pagina = f.por_pagina.unwrap_or(50).clamp(1, 500);
    let pagina = f.pagina.unwrap_or(1).max(1);
    let condiciones = |qb: &mut QueryBuilder<Sqlite>| {
        qb.push(" WHERE 1 = 1");
        match f.estado.as_deref() {
            Some("abiertos") => {
                qb.push(" AND p.estado NOT IN ('entregado', 'cancelado')");
            }
            Some(e) if !e.is_empty() => {
                qb.push(" AND p.estado = ").push_bind(e.to_string());
            }
            _ => {}
        }
        if let Some(c) = f.cliente_id {
            qb.push(" AND p.cliente_id = ").push_bind(c);
        }
        if let Some(q) = f.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
            let patron = format!("%{}%", q.replace(['%', '_'], ""));
            qb.push(" AND (c.nombre LIKE ")
                .push_bind(patron.clone())
                .push(" OR p.notas LIKE ")
                .push_bind(patron.clone())
                .push(
                    " OR EXISTS (SELECT 1 FROM pedido_piezas pp JOIN modelos m ON m.id = pp.modelo_id
                      WHERE pp.pedido_id = p.id AND (m.nombre LIKE ",
                )
                .push_bind(patron.clone())
                .push(" OR m.faccion LIKE ")
                .push_bind(patron)
                .push(")))");
        }
    };
    let mut cuenta: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT count(*) FROM pedidos p LEFT JOIN clientes c ON c.id = p.cliente_id",
    );
    condiciones(&mut cuenta);
    let total: i64 = cuenta.build_query_scalar().fetch_one(pool).await?;
    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(SELECT_PEDIDO);
    condiciones(&mut qb);
    qb.push(
        " ORDER BY p.estado IN ('entregado', 'cancelado'), coalesce(p.fecha_entrega, p.creado) DESC, p.id DESC LIMIT ",
    )
    .push_bind(por_pagina)
    .push(" OFFSET ")
    .push_bind((pagina - 1) * por_pagina);
    let filas: Vec<FilaPedido> = qb.build_query_as().fetch_all(pool).await?;
    Ok(json!({
        "total": total, "pagina": pagina, "por_pagina": por_pagina,
        "pedidos": filas.iter().map(pedido_json).collect::<Vec<_>>()
    }))
}

async fn pedidos(
    _u: Usuario,
    State(st): State<AppState>,
    Query(f): Query<FiltrosPedidos>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(listar_pedidos(&st.pool, &f).await?))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct FilaPieza {
    id: i64,
    modelo_id: i64,
    nombre: String,
    faccion: String,
    juego: String,
    estudio: String,
    carpeta: String,
    acabado: String,
    cantidad: i64,
    precio: f64,
    notas: String,
    apartadas: i64,
    costo: Option<f64>,
    placas: i64,
    #[serde(skip)]
    portada_id: Option<i64>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct Disponible {
    #[serde(skip)]
    modelo_id: i64,
    id: i64,
    acabado: String,
    ubicacion: String,
    cantidad: i64,
}

async fn pedido(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    let fila: FilaPedido = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SELECT_PEDIDO} WHERE p.id = ?"
    )))
    .bind(id)
    .fetch_optional(p)
    .await?
    .ok_or(ApiError::NoEncontrado)?;
    let piezas: Vec<FilaPieza> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT pp.id, pp.modelo_id, m.nombre, m.faccion, m.juego, m.estudio, m.carpeta, pp.acabado, pp.cantidad,
                pp.precio, pp.notas,
                coalesce((SELECT -sum(mv.delta) FROM movimientos mv WHERE mv.pedido_pieza_id = pp.id), 0) AS apartadas,
                (SELECT sum(-mv.delta * mv.costo_unitario) FROM movimientos mv WHERE mv.pedido_pieza_id = pp.id) AS costo,
                (SELECT count(*) FROM placa_modelos pm WHERE pm.modelo_id = pp.modelo_id
                   AND pm.estado = 'confirmado') AS placas,
                {} AS portada_id
         FROM pedido_piezas pp JOIN modelos m ON m.id = pp.modelo_id
         WHERE pp.pedido_id = ? ORDER BY m.juego, m.faccion, m.nombre, pp.acabado",
        crate::miniaturas::SQL_PORTADA
    )))
    .bind(id)
    .fetch_all(p)
    .await?;
    // existencias de esos modelos que se pueden apartar
    let mut disponibles: HashMap<i64, Vec<Disponible>> = HashMap::new();
    if !piezas.is_empty() {
        let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(format!(
            "SELECT e.modelo_id, e.id, e.acabado, e.ubicacion, {SQL_CANTIDAD} AS cantidad FROM existencias e
             WHERE {SQL_CANTIDAD} > 0 AND e.modelo_id IN ("
        ));
        let mut sep = qb.separated(", ");
        for x in &piezas {
            sep.push_bind(x.modelo_id);
        }
        qb.push(") ORDER BY e.acabado, cantidad DESC");
        for d in qb.build_query_as::<Disponible>().fetch_all(p).await? {
            disponibles.entry(d.modelo_id).or_default().push(d);
        }
    }
    let cl = portadas(
        p,
        &piezas
            .iter()
            .filter_map(|x| x.portada_id)
            .collect::<Vec<_>>(),
    )
    .await?;
    let pagos: Vec<(i64, f64, i64, String, String, String)> = sqlx::query_as(
        "SELECT id, monto, fecha, metodo, notas, usuario FROM pagos WHERE pedido_id = ? ORDER BY fecha, id",
    )
    .bind(id)
    .fetch_all(p)
    .await?;
    let cliente: Option<(String, String, String, String, String)> = match fila.cliente_id {
        Some(c) => {
            sqlx::query_as(
                "SELECT nombre, whatsapp, telefono, correo, redes FROM clientes WHERE id = ?",
            )
            .bind(c)
            .fetch_optional(p)
            .await?
        }
        None => None,
    };
    let mut v = pedido_json(&fila);
    v["lineas"] = json!(
        piezas
            .iter()
            .map(|x| {
                let mut y = como_json(x);
                y["faltan"] = json!(x.cantidad - x.apartadas);
                y["subtotal"] = json!(centavos(x.precio * x.cantidad as f64));
                y["disponibles"] = json!(
                    disponibles
                        .get(&x.modelo_id)
                        .map(|d| d.iter().collect::<Vec<_>>())
                );
                y["portada"] = match x.portada_id.and_then(|i| cl.get(&i).map(|c| (i, c))) {
                    Some((i, c)) => json!({ "id": i, "clave": c }),
                    None => Value::Null,
                };
                y
            })
            .collect::<Vec<_>>()
    );
    v["pagos"] = json!(
        pagos
            .iter()
            .map(|(i, m, f, me, n, u)| json!({ "id": i, "monto": m, "fecha": f, "metodo": me, "notas": n, "usuario": u }))
            .collect::<Vec<_>>()
    );
    v["datos_cliente"] = match cliente {
        Some((n, w, t, c, r)) => {
            json!({ "nombre": n, "whatsapp": w, "telefono": t, "correo": c, "redes": r })
        }
        None => Value::Null,
    };
    Ok(Json(v))
}

#[derive(Debug, Deserialize)]
struct NuevaPieza {
    modelo_id: i64,
    acabado: Option<String>,
    cantidad: i64,
    precio: Option<f64>,
    notas: Option<String>,
}

/// Una pieza revisada y con precio, lista para guardarse.
struct PiezaLista {
    modelo: i64,
    acabado: String,
    cantidad: i64,
    precio: f64,
    notas: String,
    sin_precio: bool,
}

/// Revisa una pieza y le pone precio: el que vino, el de venta o el sugerido (fuera de la transacción).
async fn preparar_pieza(
    pool: &SqlitePool,
    a: &Ajustes,
    n: &NuevaPieza,
) -> Result<PiezaLista, ApiError> {
    let acabado = n.acabado.clone().unwrap_or_else(|| "sin_pintar".into());
    acabado_valido(&acabado)?;
    if !(1..=10_000).contains(&n.cantidad) {
        return Err(ApiError::Invalido(
            "La cantidad debe ser de 1 a 10,000".into(),
        ));
    }
    if !existe(
        pool,
        "SELECT count(*) FROM modelos WHERE id = ?",
        n.modelo_id,
    )
    .await?
    {
        return Err(ApiError::Invalido("Ese modelo no existe".into()));
    }
    let notas = notas_validas(&n.notas)?;
    let (precio, sin_precio) = match n.precio {
        Some(x) => (dinero_valido("El precio", x)?, false),
        None => match costos::precio_de(pool, a, n.modelo_id, &acabado).await?.0 {
            Some(x) => (x, false),
            None => (0.0, true),
        },
    };
    Ok(PiezaLista {
        modelo: n.modelo_id,
        acabado,
        cantidad: n.cantidad,
        precio,
        notas,
        sin_precio,
    })
}

/// Agrega la pieza al pedido (o suma a la que ya hay del mismo modelo y acabado).
async fn poner_pieza(
    tx: &mut SqliteConnection,
    pedido: i64,
    x: &PiezaLista,
) -> Result<i64, sqlx::Error> {
    let hay: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM pedido_piezas WHERE pedido_id = ? AND modelo_id = ? AND acabado = ?",
    )
    .bind(pedido)
    .bind(x.modelo)
    .bind(&x.acabado)
    .fetch_optional(&mut *tx)
    .await?;
    match hay {
        Some(i) => {
            sqlx::query("UPDATE pedido_piezas SET cantidad = cantidad + ? WHERE id = ?")
                .bind(x.cantidad)
                .bind(i)
                .execute(&mut *tx)
                .await?;
            Ok(i)
        }
        None => {
            sqlx::query_scalar(
                "INSERT INTO pedido_piezas (pedido_id, modelo_id, acabado, cantidad, precio, notas)
                 VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
            )
            .bind(pedido)
            .bind(x.modelo)
            .bind(&x.acabado)
            .bind(x.cantidad)
            .bind(x.precio)
            .bind(&x.notas)
            .fetch_one(&mut *tx)
            .await
        }
    }
}

#[derive(Debug, Deserialize)]
struct NuevoPedido {
    cliente_id: Option<i64>,
    /// para dar de alta al cliente al mismo tiempo
    cliente: Option<DatosCliente>,
    fecha_entrega: Option<i64>,
    notas: Option<String>,
    #[serde(default)]
    piezas: Vec<NuevaPieza>,
}

async fn crear_pedido(
    _u: Usuario,
    State(st): State<AppState>,
    Json(n): Json<NuevoPedido>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let p = &st.pool;
    if let Some(c) = n.cliente_id
        && !existe(p, "SELECT count(*) FROM clientes WHERE id = ?", c).await?
    {
        return Err(ApiError::Invalido("Ese cliente no existe".into()));
    }
    let notas = notas_validas(&n.notas)?;
    let a = Ajustes::cargar(p).await?;
    let mut listas = Vec::new();
    for x in &n.piezas {
        listas.push(preparar_pieza(p, &a, x).await?);
    }
    let t = ahora();
    let mut tx = p.begin().await?;
    let cliente = match (&n.cliente, n.cliente_id) {
        (Some(d), None) => Some(nuevo_cliente(&mut tx, d).await?),
        (_, c) => c,
    };
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO pedidos (cliente_id, fecha_entrega, notas, creado, actualizado) VALUES (?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(cliente)
    .bind(n.fecha_entrega)
    .bind(&notas)
    .bind(t)
    .bind(t)
    .fetch_one(&mut *tx)
    .await?;
    for x in &listas {
        poner_pieza(&mut tx, id, x).await?;
    }
    let sin_precio = listas.iter().filter(|x| x.sin_precio).count();
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "id": id, "cliente_id": cliente, "sin_precio": sin_precio })),
    ))
}

#[derive(Debug, Deserialize)]
struct CambiosPedido {
    estado: Option<String>,
    #[serde(default, deserialize_with = "doble_i")]
    cliente_id: Option<Option<i64>>,
    #[serde(default, deserialize_with = "doble_i")]
    fecha_entrega: Option<Option<i64>>,
    descuento: Option<f64>,
    envio: Option<f64>,
    notas: Option<String>,
    /// entregar aunque falten miniaturas por apartar
    #[serde(default)]
    forzar: bool,
}

async fn estado_de(pool: &SqlitePool, id: i64) -> Result<String, ApiError> {
    let e: Option<String> = sqlx::query_scalar("SELECT estado FROM pedidos WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    e.ok_or(ApiError::NoEncontrado)
}

/// Un pedido entregado o cancelado ya no cambia sus piezas.
async fn abierto(pool: &SqlitePool, id: i64) -> Result<(), ApiError> {
    match estado_de(pool, id).await?.as_str() {
        "entregado" => Err(ApiError::Conflicto("El pedido ya se entregó".into())),
        "cancelado" => Err(ApiError::Conflicto("El pedido está cancelado".into())),
        _ => Ok(()),
    }
}

async fn editar_pedido(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(c): Json<CambiosPedido>,
) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    let actual = estado_de(p, id).await?;
    if let Some(e) = &c.estado
        && !ESTADOS.contains(&e.as_str())
    {
        return Err(ApiError::Invalido(format!("Estado desconocido: {e}")));
    }
    if let Some(Some(cl)) = c.cliente_id
        && !existe(p, "SELECT count(*) FROM clientes WHERE id = ?", cl).await?
    {
        return Err(ApiError::Invalido("Ese cliente no existe".into()));
    }
    let descuento = c
        .descuento
        .map(|x| dinero_valido("El descuento", x))
        .transpose()?;
    let envio = c.envio.map(|x| dinero_valido("El envío", x)).transpose()?;
    let notas = c
        .notas
        .as_ref()
        .map(|_| notas_validas(&c.notas))
        .transpose()?;
    let nuevo = c.estado.clone().unwrap_or_else(|| actual.clone());
    let fila: FilaPedido = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SELECT_PEDIDO} WHERE p.id = ?"
    )))
    .bind(id)
    .fetch_one(p)
    .await?;
    if nuevo == "cancelado" && actual == "entregado" {
        return Err(ApiError::Conflicto(
            "Ya se entregó: no se puede cancelar (registra una devolución con un ajuste en las existencias)".into(),
        ));
    }
    if nuevo == "entregado" && actual != "entregado" && fila.piezas > fila.apartadas && !c.forzar {
        return Err(ApiError::Conflicto(format!(
            "Faltan {} miniaturas por apartar de las existencias: apártalas o entrega de todos modos",
            fila.piezas - fila.apartadas
        )));
    }
    let mut tx = p.begin().await?;
    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new("UPDATE pedidos SET actualizado = ");
    qb.push_bind(ahora());
    if c.estado.is_some() {
        qb.push(", estado = ").push_bind(nuevo.clone());
        if nuevo == "entregado" && actual != "entregado" {
            qb.push(", entregado = ").push_bind(ahora());
        } else if nuevo != "entregado" {
            qb.push(", entregado = NULL");
        }
    }
    if let Some(cl) = c.cliente_id {
        qb.push(", cliente_id = ").push_bind(cl);
    }
    if let Some(f) = c.fecha_entrega {
        qb.push(", fecha_entrega = ").push_bind(f);
    }
    if let Some(x) = descuento {
        qb.push(", descuento = ").push_bind(x);
    }
    if let Some(x) = envio {
        qb.push(", envio = ").push_bind(x);
    }
    if let Some(n) = notas {
        qb.push(", notas = ").push_bind(n);
    }
    qb.push(" WHERE id = ").push_bind(id);
    qb.build().execute(&mut *tx).await?;
    let mut regresadas = 0;
    if nuevo == "cancelado" && actual != "cancelado" {
        // lo apartado regresa a las existencias
        regresadas = sqlx::query(
            "DELETE FROM movimientos WHERE pedido_pieza_id IN (SELECT id FROM pedido_piezas WHERE pedido_id = ?)",
        )
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    }
    tx.commit().await?;
    Ok(Json(
        json!({ "ok": true, "movimientos_deshechos": regresadas }),
    ))
}

async fn borrar_pedido(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    let estado = estado_de(p, id).await?;
    if estado != "cotizado" && estado != "cancelado" {
        return Err(ApiError::Conflicto(
            "Solo se borran los pedidos cotizados o cancelados; cancélalo primero".into(),
        ));
    }
    if existe(p, "SELECT count(*) FROM pagos WHERE pedido_id = ?", id).await? {
        return Err(ApiError::Conflicto("Tiene pagos: bórralos primero".into()));
    }
    sqlx::query("DELETE FROM pedidos WHERE id = ?")
        .bind(id)
        .execute(p)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

async fn agregar_pieza(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(n): Json<NuevaPieza>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let p = &st.pool;
    abierto(p, id).await?;
    let a = Ajustes::cargar(p).await?;
    let lista = preparar_pieza(p, &a, &n).await?;
    let mut tx = p.begin().await?;
    let pieza = poner_pieza(&mut tx, id, &lista).await?;
    let sin_precio = lista.sin_precio;
    sqlx::query("UPDATE pedidos SET actualizado = ? WHERE id = ?")
        .bind(ahora())
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "id": pieza, "sin_precio": sin_precio })),
    ))
}

/// La pieza, si es de ese pedido: (modelo, acabado, cantidad, apartadas).
async fn pieza_de(
    pool: &SqlitePool,
    pedido: i64,
    pieza: i64,
) -> Result<(i64, String, i64, i64), ApiError> {
    let x: Option<(i64, String, i64, i64)> = sqlx::query_as(
        "SELECT pp.modelo_id, pp.acabado, pp.cantidad,
                coalesce((SELECT -sum(mv.delta) FROM movimientos mv WHERE mv.pedido_pieza_id = pp.id), 0)
         FROM pedido_piezas pp WHERE pp.id = ? AND pp.pedido_id = ?",
    )
    .bind(pieza)
    .bind(pedido)
    .fetch_optional(pool)
    .await?;
    x.ok_or(ApiError::NoEncontrado)
}

#[derive(Debug, Deserialize)]
struct CambiosPieza {
    cantidad: Option<i64>,
    precio: Option<f64>,
    notas: Option<String>,
}

async fn editar_pieza(
    _u: Usuario,
    State(st): State<AppState>,
    Path((id, pieza)): Path<(i64, i64)>,
    Json(c): Json<CambiosPieza>,
) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    let (_, _, _, apartadas) = pieza_de(p, id, pieza).await?;
    abierto(p, id).await?;
    if let Some(n) = c.cantidad {
        if !(1..=10_000).contains(&n) {
            return Err(ApiError::Invalido(
                "La cantidad debe ser de 1 a 10,000".into(),
            ));
        }
        if n < apartadas {
            return Err(ApiError::Conflicto(format!(
                "Ya hay {apartadas} apartadas: desapártalas primero"
            )));
        }
    }
    let precio = c
        .precio
        .map(|x| dinero_valido("El precio", x))
        .transpose()?;
    let notas = c
        .notas
        .as_ref()
        .map(|_| notas_validas(&c.notas))
        .transpose()?;
    sqlx::query(
        "UPDATE pedido_piezas SET cantidad = coalesce(?, cantidad), precio = coalesce(?, precio),
            notas = coalesce(?, notas) WHERE id = ?",
    )
    .bind(c.cantidad)
    .bind(precio)
    .bind(notas)
    .bind(pieza)
    .execute(p)
    .await?;
    Ok(Json(json!({ "ok": true })))
}

/// Quita la pieza; lo que tenía apartado regresa a las existencias.
async fn quitar_pieza(
    _u: Usuario,
    State(st): State<AppState>,
    Path((id, pieza)): Path<(i64, i64)>,
) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    pieza_de(p, id, pieza).await?;
    abierto(p, id).await?;
    sqlx::query("DELETE FROM pedido_piezas WHERE id = ?")
        .bind(pieza)
        .execute(p)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

/// Aparta `cantidad` miniaturas de una existencia para una pieza (dentro de una transacción).
async fn apartar_de(
    tx: &mut SqliteConnection,
    existencia: i64,
    pieza: i64,
    pedido: i64,
    cant: i64,
    usuario: &str,
) -> Result<(), sqlx::Error> {
    let costo: Option<f64> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT {SQL_COSTO} FROM existencias e WHERE e.id = ?"
    )))
    .bind(existencia)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO movimientos (existencia_id, delta, motivo, costo_unitario, pedido_pieza_id, notas, usuario, fecha)
         VALUES (?, ?, 'venta', ?, ?, ?, ?, ?)",
    )
    .bind(existencia)
    .bind(-cant)
    .bind(costo)
    .bind(pieza)
    .bind(format!("Pedido #{pedido}"))
    .bind(usuario)
    .bind(ahora())
    .execute(&mut *tx)
    .await?;
    Ok(())
}

#[derive(Debug, Deserialize)]
struct PedidoApartar {
    /// de cuál existencia; si no, de las del mismo acabado (las que tienen más primero)
    existencia_id: Option<i64>,
    /// cuántas; si no, todas las que faltan (o las que alcancen)
    cantidad: Option<i64>,
}

async fn apartar(
    u: Usuario,
    State(st): State<AppState>,
    Path((id, pieza)): Path<(i64, i64)>,
    Json(a): Json<PedidoApartar>,
) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    let (modelo, acabado, cant, apartadas) = pieza_de(p, id, pieza).await?;
    abierto(p, id).await?;
    let faltan = cant - apartadas;
    let mut quiero = a.cantidad.unwrap_or(faltan);
    if quiero < 1 || quiero > faltan {
        return Err(ApiError::Invalido(format!(
            "Se pueden apartar de 1 a {faltan}"
        )));
    }
    let mut tx = p.begin().await?;
    let fuentes: Vec<(i64, i64)> = match a.existencia_id {
        Some(e) => {
            let de: Option<i64> =
                sqlx::query_scalar("SELECT modelo_id FROM existencias WHERE id = ?")
                    .bind(e)
                    .fetch_optional(&mut *tx)
                    .await?;
            if de != Some(modelo) {
                return Err(ApiError::Invalido(
                    "Esa existencia no es de este modelo".into(),
                ));
            }
            vec![(e, cantidad(&mut tx, e).await?)]
        }
        None => {
            sqlx::query_as(sqlx::AssertSqlSafe(format!(
                "SELECT e.id, {SQL_CANTIDAD} AS hay FROM existencias e
             WHERE e.modelo_id = ? AND e.acabado = ? AND {SQL_CANTIDAD} > 0 ORDER BY hay DESC"
            )))
            .bind(modelo)
            .bind(&acabado)
            .fetch_all(&mut *tx)
            .await?
        }
    };
    let hay: i64 = fuentes.iter().map(|f| f.1).sum();
    if hay == 0 {
        return Err(ApiError::Conflicto(
            "No hay existencias de este modelo con ese acabado: imprímelas o elige otra existencia"
                .into(),
        ));
    }
    if a.existencia_id.is_some() && hay < quiero {
        return Err(ApiError::Conflicto(format!(
            "En esa existencia solo hay {hay}"
        )));
    }
    let mut hechas = 0;
    for (e, n) in fuentes {
        if quiero == 0 {
            break;
        }
        let c = n.min(quiero);
        apartar_de(&mut tx, e, pieza, id, c, &u.nombre).await?;
        quiero -= c;
        hechas += c;
    }
    tx.commit().await?;
    Ok(Json(
        json!({ "apartadas": hechas, "faltan": faltan - hechas }),
    ))
}

async fn desapartar(
    _u: Usuario,
    State(st): State<AppState>,
    Path((id, pieza)): Path<(i64, i64)>,
) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    pieza_de(p, id, pieza).await?;
    abierto(p, id).await?;
    let r = sqlx::query("DELETE FROM movimientos WHERE pedido_pieza_id = ?")
        .bind(pieza)
        .execute(p)
        .await?;
    Ok(Json(
        json!({ "ok": true, "movimientos_deshechos": r.rows_affected() }),
    ))
}

/// Al registrar una impresión para un pedido: lo que entró a las existencias se aparta para las piezas del
/// pedido con el mismo modelo y acabado que todavía faltan.
pub(crate) async fn apartar_impresas(
    tx: &mut SqliteConnection,
    pedido: i64,
    impresion: i64,
    usuario: &str,
) -> Result<i64, sqlx::Error> {
    let entradas: Vec<(i64, i64, i64, String)> = sqlx::query_as(
        "SELECT mv.existencia_id, mv.delta, e.modelo_id, e.acabado FROM movimientos mv
         JOIN existencias e ON e.id = mv.existencia_id WHERE mv.impresion_id = ? AND mv.delta > 0",
    )
    .bind(impresion)
    .fetch_all(&mut *tx)
    .await?;
    let mut total = 0;
    for (existencia, delta, modelo, acabado) in entradas {
        let pieza: Option<(i64, i64)> = sqlx::query_as(
            "SELECT pp.id, pp.cantidad - coalesce((SELECT -sum(mv.delta) FROM movimientos mv
                                                  WHERE mv.pedido_pieza_id = pp.id), 0)
             FROM pedido_piezas pp WHERE pp.pedido_id = ? AND pp.modelo_id = ? AND pp.acabado = ?",
        )
        .bind(pedido)
        .bind(modelo)
        .bind(&acabado)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some((pieza, faltan)) = pieza.filter(|x| x.1 > 0) {
            let c = faltan.min(delta);
            apartar_de(tx, existencia, pieza, pedido, c, usuario).await?;
            total += c;
        }
    }
    Ok(total)
}

#[derive(Debug, Deserialize)]
struct NuevoPago {
    monto: f64,
    fecha: Option<i64>,
    metodo: Option<String>,
    notas: Option<String>,
}

async fn pagar(
    u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(n): Json<NuevoPago>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let p = &st.pool;
    estado_de(p, id).await?;
    if !n.monto.is_finite() || n.monto == 0.0 || n.monto.abs() > 10_000_000.0 {
        return Err(ApiError::Invalido(
            "El monto no es válido (en negativo, si es una devolución)".into(),
        ));
    }
    let metodo = n.metodo.clone().unwrap_or_else(|| "efectivo".into());
    if !["efectivo", "transferencia", "tarjeta", "otro"].contains(&metodo.as_str()) {
        return Err(ApiError::Invalido(format!(
            "Forma de pago desconocida: {metodo}"
        )));
    }
    let notas = notas_validas(&n.notas)?;
    let id_pago: i64 = sqlx::query_scalar(
        "INSERT INTO pagos (pedido_id, monto, fecha, metodo, notas, usuario, creado) VALUES (?, ?, ?, ?, ?, ?, ?)
         RETURNING id",
    )
    .bind(id)
    .bind(centavos(n.monto))
    .bind(n.fecha.unwrap_or_else(ahora))
    .bind(&metodo)
    .bind(&notas)
    .bind(&u.nombre)
    .bind(ahora())
    .fetch_one(p)
    .await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id_pago }))))
}

async fn borrar_pago(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let r = sqlx::query("DELETE FROM pagos WHERE id = ?")
        .bind(id)
        .execute(&st.pool)
        .await?;
    if r.rows_affected() == 0 {
        return Err(ApiError::NoEncontrado);
    }
    Ok(Json(json!({ "ok": true })))
}

// ---------- colecciones ----------

#[derive(Debug, Serialize, sqlx::FromRow)]
struct FilaColeccion {
    id: i64,
    nombre: String,
    descripcion: String,
    estado: String,
    precio_fijo: Option<f64>,
    creada: i64,
    actualizada: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct FilaLinea {
    id: i64,
    modelo_id: i64,
    nombre: String,
    faccion: String,
    juego: String,
    estudio: String,
    carpeta: String,
    acabado: String,
    cantidad: i64,
    #[serde(rename = "precio_a_mano")]
    precio: Option<f64>,
    foto_id: Option<i64>,
    nota: String,
    orden: i64,
    hay: i64,
    placas: i64,
    #[serde(skip)]
    imagen_id: Option<i64>,
}

/// Una colección con sus modelos, precios, fotos y si alcanzan las existencias.
async fn coleccion_json(pool: &SqlitePool, c: &FilaColeccion) -> Result<Value, ApiError> {
    let a = Ajustes::cargar(pool).await?;
    let lineas: Vec<FilaLinea> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT l.id, l.modelo_id, m.nombre, m.faccion, m.juego, m.estudio, m.carpeta, l.acabado, l.cantidad, l.precio,
                l.foto_id, l.nota, l.orden,
                coalesce((SELECT sum(mv.delta) FROM movimientos mv JOIN existencias e ON e.id = mv.existencia_id
                          WHERE e.modelo_id = l.modelo_id AND e.acabado = l.acabado), 0) AS hay,
                (SELECT count(*) FROM placa_modelos pm WHERE pm.modelo_id = l.modelo_id
                   AND pm.estado = 'confirmado') AS placas,
                coalesce((SELECT f.id FROM archivos f JOIN miniaturas mi ON mi.archivo_id = f.id AND mi.estado = 'ok'
                          WHERE f.id = l.foto_id), {}) AS imagen_id
         FROM coleccion_modelos l JOIN modelos m ON m.id = l.modelo_id
         WHERE l.coleccion_id = ? ORDER BY l.orden, l.id",
        crate::miniaturas::SQL_PORTADA
    )))
    .bind(c.id)
    .fetch_all(pool)
    .await?;
    let cl = portadas(
        pool,
        &lineas
            .iter()
            .filter_map(|l| l.imagen_id)
            .collect::<Vec<_>>(),
    )
    .await?;
    let mut suma = 0.0;
    let mut costo_total = Some(0.0);
    let mut sin_precio = 0;
    let mut faltan_total = 0;
    let mut piezas = 0;
    let mut filas = Vec::new();
    for l in &lineas {
        let (precio_venta, costo) = costos::precio_de(pool, &a, l.modelo_id, &l.acabado).await?;
        let precio = l.precio.or(precio_venta);
        match precio {
            Some(p) => suma += p * l.cantidad as f64,
            None => sin_precio += 1,
        }
        costo_total = match (costo_total, costo) {
            (Some(t), Some(c)) => Some(t + c * l.cantidad as f64),
            _ => None,
        };
        let faltan = (l.cantidad - l.hay).max(0);
        faltan_total += faltan;
        piezas += l.cantidad;
        let mut v = como_json(l);
        v["precio"] = json!(precio);
        v["precio_sugerido"] = json!(precio_venta);
        v["costo"] = json!(costo);
        v["subtotal"] = json!(precio.map(|p| centavos(p * l.cantidad as f64)));
        v["faltan"] = json!(faltan);
        v["foto"] = match l.imagen_id.and_then(|i| cl.get(&i).map(|k| (i, k))) {
            Some((i, k)) => json!({ "id": i, "clave": k }),
            None => Value::Null,
        };
        filas.push(v);
    }
    let precio = c.precio_fijo.unwrap_or(centavos(suma));
    let mut v = como_json(c);
    v["lineas"] = json!(filas);
    v["piezas"] = json!(piezas);
    v["suma"] = json!(centavos(suma));
    v["precio"] = json!(precio);
    v["sin_precio"] = json!(sin_precio);
    v["costo"] = json!(costo_total.filter(|_| !lineas.is_empty()).map(centavos));
    v["ganancia"] = json!(
        costo_total
            .filter(|_| !lineas.is_empty())
            .map(|t| centavos(precio - t))
    );
    v["faltan"] = json!(faltan_total);
    v["alcanza"] = json!(!lineas.is_empty() && faltan_total == 0);
    v["moneda"] = json!(a.moneda);
    Ok(v)
}

#[derive(Debug, Default, Deserialize)]
struct FiltrosColecciones {
    estado: Option<String>,
}

async fn colecciones(
    _u: Usuario,
    State(st): State<AppState>,
    Query(f): Query<FiltrosColecciones>,
) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    let filas: Vec<FilaColeccion> = sqlx::query_as(
        "SELECT id, nombre, descripcion, estado, precio_fijo, creada, actualizada FROM colecciones
         WHERE (?1 IS NULL OR estado = ?1) ORDER BY estado = 'archivada', estado = 'borrador', actualizada DESC",
    )
    .bind(f.estado.filter(|e| !e.is_empty()))
    .fetch_all(p)
    .await?;
    let mut lista = Vec::new();
    for c in &filas {
        let mut v = coleccion_json(p, c).await?;
        // en la lista basta con la primera foto
        let fotos: Vec<Value> = v["lineas"]
            .as_array()
            .map(|l| {
                l.iter()
                    .filter_map(|x| x.get("foto").cloned())
                    .filter(|f| !f.is_null())
                    .take(4)
                    .collect()
            })
            .unwrap_or_default();
        v["n_modelos"] = json!(v["lineas"].as_array().map(|l| l.len()).unwrap_or(0));
        v["fotos"] = json!(fotos);
        v.as_object_mut().map(|o| o.remove("lineas"));
        lista.push(v);
    }
    Ok(Json(json!(lista)))
}

#[derive(Debug, Default, Deserialize)]
struct DatosColeccion {
    nombre: Option<String>,
    descripcion: Option<String>,
    estado: Option<String>,
    #[serde(default, deserialize_with = "doble_f")]
    precio_fijo: Option<Option<f64>>,
}

async fn crear_coleccion(
    _u: Usuario,
    State(st): State<AppState>,
    Json(d): Json<DatosColeccion>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let nombre = texto_valido("El nombre", d.nombre.as_deref().unwrap_or(""), 120, true)?;
    let descripcion = texto_valido(
        "La descripción",
        d.descripcion.as_deref().unwrap_or(""),
        4000,
        false,
    )?;
    let t = ahora();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO colecciones (nombre, descripcion, creada, actualizada) VALUES (?, ?, ?, ?) RETURNING id",
    )
    .bind(nombre)
    .bind(descripcion)
    .bind(t)
    .bind(t)
    .fetch_one(&st.pool)
    .await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

async fn fila_coleccion(pool: &SqlitePool, id: i64) -> Result<FilaColeccion, ApiError> {
    let c: Option<FilaColeccion> = sqlx::query_as(
        "SELECT id, nombre, descripcion, estado, precio_fijo, creada, actualizada FROM colecciones WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    c.ok_or(ApiError::NoEncontrado)
}

async fn coleccion(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let c = fila_coleccion(&st.pool, id).await?;
    Ok(Json(coleccion_json(&st.pool, &c).await?))
}

async fn editar_coleccion(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(d): Json<DatosColeccion>,
) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    fila_coleccion(p, id).await?;
    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new("UPDATE colecciones SET actualizada = ");
    qb.push_bind(ahora());
    if let Some(n) = &d.nombre {
        qb.push(", nombre = ")
            .push_bind(texto_valido("El nombre", n, 120, true)?);
    }
    if let Some(n) = &d.descripcion {
        qb.push(", descripcion = ")
            .push_bind(texto_valido("La descripción", n, 4000, false)?);
    }
    if let Some(e) = &d.estado {
        if !["borrador", "activa", "archivada"].contains(&e.as_str()) {
            return Err(ApiError::Invalido(format!("Estado desconocido: {e}")));
        }
        qb.push(", estado = ").push_bind(e.clone());
    }
    if let Some(pf) = d.precio_fijo {
        let pf = pf.map(|x| dinero_valido("El precio", x)).transpose()?;
        qb.push(", precio_fijo = ").push_bind(pf);
    }
    qb.push(" WHERE id = ").push_bind(id);
    qb.build().execute(p).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn borrar_coleccion(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let r = sqlx::query("DELETE FROM colecciones WHERE id = ?")
        .bind(id)
        .execute(&st.pool)
        .await?;
    if r.rows_affected() == 0 {
        return Err(ApiError::NoEncontrado);
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Debug, Default, Deserialize)]
struct DatosLinea {
    modelo_id: Option<i64>,
    acabado: Option<String>,
    cantidad: Option<i64>,
    #[serde(default, deserialize_with = "doble_f")]
    precio: Option<Option<f64>>,
    #[serde(default, deserialize_with = "doble_i")]
    foto_id: Option<Option<i64>>,
    nota: Option<String>,
    orden: Option<i64>,
}

/// La foto debe ser un archivo de ese modelo con miniatura (imagen, malla o vista previa).
async fn revisar_foto(pool: &SqlitePool, modelo: i64, foto: Option<i64>) -> Result<(), ApiError> {
    if let Some(f) = foto {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM archivos a JOIN miniaturas mi ON mi.archivo_id = a.id AND mi.estado = 'ok'
             WHERE a.id = ? AND a.modelo_id = ?",
        )
        .bind(f)
        .bind(modelo)
        .fetch_one(pool)
        .await?;
        if n == 0 {
            return Err(ApiError::Invalido(
                "La foto debe ser una imagen de ese modelo con miniatura".into(),
            ));
        }
    }
    Ok(())
}

async fn agregar_linea(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(d): Json<DatosLinea>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let p = &st.pool;
    fila_coleccion(p, id).await?;
    let modelo = d
        .modelo_id
        .ok_or_else(|| ApiError::Invalido("Falta el modelo".into()))?;
    if !existe(p, "SELECT count(*) FROM modelos WHERE id = ?", modelo).await? {
        return Err(ApiError::Invalido("Ese modelo no existe".into()));
    }
    let acabado = d.acabado.clone().unwrap_or_else(|| "sin_pintar".into());
    acabado_valido(&acabado)?;
    let cantidad = d.cantidad.unwrap_or(1);
    if !(1..=1000).contains(&cantidad) {
        return Err(ApiError::Invalido(
            "La cantidad debe ser de 1 a 1,000".into(),
        ));
    }
    let precio = d
        .precio
        .flatten()
        .map(|x| dinero_valido("El precio", x))
        .transpose()?;
    let foto = d.foto_id.flatten();
    revisar_foto(p, modelo, foto).await?;
    let nota = texto_valido("La nota", d.nota.as_deref().unwrap_or(""), 500, false)?;
    let orden: i64 = sqlx::query_scalar(
        "SELECT coalesce(max(orden), 0) + 1 FROM coleccion_modelos WHERE coleccion_id = ?",
    )
    .bind(id)
    .fetch_one(p)
    .await?;
    let r: Result<i64, sqlx::Error> = sqlx::query_scalar(
        "INSERT INTO coleccion_modelos (coleccion_id, modelo_id, acabado, cantidad, precio, foto_id, nota, orden)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(id)
    .bind(modelo)
    .bind(&acabado)
    .bind(cantidad)
    .bind(precio)
    .bind(foto)
    .bind(&nota)
    .bind(orden)
    .fetch_one(p)
    .await;
    let linea = match r {
        Ok(l) => l,
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Err(ApiError::Conflicto(
                "Ese modelo con ese acabado ya está en la colección: cambia su cantidad".into(),
            ));
        }
        Err(e) => return Err(e.into()),
    };
    sqlx::query("UPDATE colecciones SET actualizada = ? WHERE id = ?")
        .bind(ahora())
        .bind(id)
        .execute(p)
        .await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": linea }))))
}

async fn editar_linea(
    _u: Usuario,
    State(st): State<AppState>,
    Path((id, linea)): Path<(i64, i64)>,
    Json(d): Json<DatosLinea>,
) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    let modelo: Option<i64> = sqlx::query_scalar(
        "SELECT modelo_id FROM coleccion_modelos WHERE id = ? AND coleccion_id = ?",
    )
    .bind(linea)
    .bind(id)
    .fetch_optional(p)
    .await?;
    let modelo = modelo.ok_or(ApiError::NoEncontrado)?;
    let mut qb: QueryBuilder<Sqlite> =
        QueryBuilder::new("UPDATE coleccion_modelos SET orden = orden");
    if let Some(a) = &d.acabado {
        acabado_valido(a)?;
        qb.push(", acabado = ").push_bind(a.clone());
    }
    if let Some(c) = d.cantidad {
        if !(1..=1000).contains(&c) {
            return Err(ApiError::Invalido(
                "La cantidad debe ser de 1 a 1,000".into(),
            ));
        }
        qb.push(", cantidad = ").push_bind(c);
    }
    if let Some(pr) = d.precio {
        let pr = pr.map(|x| dinero_valido("El precio", x)).transpose()?;
        qb.push(", precio = ").push_bind(pr);
    }
    if let Some(f) = d.foto_id {
        revisar_foto(p, modelo, f).await?;
        qb.push(", foto_id = ").push_bind(f);
    }
    if let Some(n) = &d.nota {
        qb.push(", nota = ")
            .push_bind(texto_valido("La nota", n, 500, false)?);
    }
    if let Some(o) = d.orden {
        qb.push(", orden = ").push_bind(o);
    }
    qb.push(" WHERE id = ").push_bind(linea);
    match qb.build().execute(p).await {
        Ok(_) => {}
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Err(ApiError::Conflicto(
                "Ese modelo ya está en la colección con ese acabado".into(),
            ));
        }
        Err(e) => return Err(e.into()),
    }
    sqlx::query("UPDATE colecciones SET actualizada = ? WHERE id = ?")
        .bind(ahora())
        .bind(id)
        .execute(p)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

async fn quitar_linea(
    _u: Usuario,
    State(st): State<AppState>,
    Path((id, linea)): Path<(i64, i64)>,
) -> Result<Json<Value>, ApiError> {
    let r = sqlx::query("DELETE FROM coleccion_modelos WHERE id = ? AND coleccion_id = ?")
        .bind(linea)
        .bind(id)
        .execute(&st.pool)
        .await?;
    if r.rows_affected() == 0 {
        return Err(ApiError::NoEncontrado);
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Debug, Deserialize)]
struct PedidoDeColeccion {
    cliente_id: Option<i64>,
    cliente: Option<DatosCliente>,
    /// cuántas colecciones
    veces: Option<i64>,
}

/// Convierte la colección en un pedido cotizado con sus piezas. Con precio fijo, los precios de las piezas se
/// ajustan para que el total sea ese.
async fn coleccion_a_pedido(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(d): Json<PedidoDeColeccion>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let p = &st.pool;
    let c = fila_coleccion(p, id).await?;
    let veces = d.veces.unwrap_or(1);
    if !(1..=1000).contains(&veces) {
        return Err(ApiError::Invalido(
            "Las veces deben ser de 1 a 1,000".into(),
        ));
    }
    if let Some(cl) = d.cliente_id
        && !existe(p, "SELECT count(*) FROM clientes WHERE id = ?", cl).await?
    {
        return Err(ApiError::Invalido("Ese cliente no existe".into()));
    }
    let v = coleccion_json(p, &c).await?;
    let lineas = v["lineas"].as_array().cloned().unwrap_or_default();
    if lineas.is_empty() {
        return Err(ApiError::Invalido("La colección no tiene modelos".into()));
    }
    // (modelo, acabado, cantidad, precio)
    let mut piezas: Vec<(i64, String, i64, f64)> = lineas
        .iter()
        .map(|l| {
            (
                l["modelo_id"].as_i64().unwrap_or(0),
                l["acabado"].as_str().unwrap_or("sin_pintar").to_string(),
                l["cantidad"].as_i64().unwrap_or(1) * veces,
                l["precio"].as_f64().unwrap_or(0.0),
            )
        })
        .collect();
    let suma: f64 = piezas.iter().map(|x| x.3 * x.2 as f64).sum();
    let mut descuento = 0.0;
    if let Some(fijo) = c.precio_fijo {
        let meta = fijo * veces as f64;
        if suma > 0.0 {
            let factor = meta / suma;
            for x in piezas.iter_mut() {
                x.3 = centavos(x.3 * factor);
            }
        }
        let ahora_suma: f64 = piezas.iter().map(|x| x.3 * x.2 as f64).sum();
        // lo que no cuadra por redondeo: si sobra, descuento; si falta, a la última pieza
        if ahora_suma > meta {
            descuento = centavos(ahora_suma - meta);
        } else if let Some(x) = piezas.last_mut()
            && meta > ahora_suma
        {
            x.3 = centavos(x.3 + (meta - ahora_suma) / x.2 as f64);
        }
    }
    let t = ahora();
    let mut tx = p.begin().await?;
    let cliente = match (&d.cliente, d.cliente_id) {
        (Some(dc), None) => Some(nuevo_cliente(&mut tx, dc).await?),
        (_, cl) => cl,
    };
    let pedido: i64 = sqlx::query_scalar(
        "INSERT INTO pedidos (cliente_id, coleccion_id, descuento, notas, creado, actualizado)
         VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(cliente)
    .bind(id)
    .bind(descuento)
    .bind(if veces > 1 {
        format!("{veces} × {}", c.nombre)
    } else {
        c.nombre.clone()
    })
    .bind(t)
    .bind(t)
    .fetch_one(&mut *tx)
    .await?;
    for (modelo, acabado, cant, precio) in &piezas {
        sqlx::query(
            "INSERT INTO pedido_piezas (pedido_id, modelo_id, acabado, cantidad, precio) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(pedido)
        .bind(modelo)
        .bind(acabado)
        .bind(cant)
        .bind(precio)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "id": pedido, "sin_precio": v["sin_precio"] })),
    ))
}

/// Imágenes del modelo que pueden ser la foto de una colección (con miniatura): primero las imágenes, luego
/// las mallas y las vistas previas.
async fn fotos_de_modelo(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let filas: Vec<(i64, String, String, String)> = sqlx::query_as(
        "SELECT a.id, a.nombre, a.tipo, mi.clave FROM archivos a
         JOIN miniaturas mi ON mi.archivo_id = a.id AND mi.estado = 'ok'
         WHERE a.modelo_id = ? AND a.estado = 'presente'
         ORDER BY a.tipo <> 'imagen', a.tipo <> 'malla', a.bytes DESC LIMIT 80",
    )
    .bind(id)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(json!(
        filas
            .iter()
            .map(|(i, n, t, c)| json!({ "id": i, "nombre": n, "tipo": t, "clave": c }))
            .collect::<Vec<_>>()
    )))
}

// ---------- resumen ----------

#[derive(Debug, Default, Deserialize)]
struct Periodo {
    dias: Option<i64>,
}

async fn resumen(
    _u: Usuario,
    State(st): State<AppState>,
    Query(pe): Query<Periodo>,
) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    let dias = pe.dias.unwrap_or(90).clamp(1, 3650);
    let desde = ahora() - dias * 86_400;
    let todos: Vec<FilaPedido> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SELECT_PEDIDO} WHERE p.estado <> 'cancelado'"
    )))
    .fetch_all(p)
    .await?;
    let lista: Vec<Value> = todos.iter().map(pedido_json).collect();
    let abiertos: Vec<&Value> = lista
        .iter()
        .filter(|v| v["estado"] != "entregado")
        .collect();
    let vendidos: Vec<&Value> = lista
        .iter()
        .filter(|v| {
            v["estado"] == "entregado" && v["entregado"].as_i64().is_some_and(|e| e >= desde)
        })
        .collect();
    let suma =
        |vs: &[&Value], k: &str| centavos(vs.iter().filter_map(|v| v[k].as_f64()).sum::<f64>());
    let ingresos = centavos(
        vendidos
            .iter()
            .map(|v| v["subtotal"].as_f64().unwrap_or(0.0) - v["descuento"].as_f64().unwrap_or(0.0))
            .sum::<f64>(),
    );
    let por_cobrar = centavos(
        lista
            .iter()
            .filter_map(|v| v["saldo"].as_f64())
            .filter(|s| *s > 0.0)
            .sum::<f64>(),
    );
    let atrasados = abiertos
        .iter()
        .filter(|v| v["fecha_entrega"].as_i64().is_some_and(|f| f < ahora()))
        .count();
    let top: Vec<(i64, String, String, String, i64, f64, i64)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT m.id, m.nombre, m.faccion, m.juego, sum(pp.cantidad) AS n, sum(pp.cantidad * pp.precio) AS ingreso,
                (SELECT coalesce(sum(mv.delta), 0) FROM movimientos mv JOIN existencias e ON e.id = mv.existencia_id
                 WHERE e.modelo_id = m.id) AS hay
         FROM pedido_piezas pp JOIN pedidos p ON p.id = pp.pedido_id JOIN modelos m ON m.id = pp.modelo_id
         WHERE p.estado = 'entregado' AND p.entregado >= {desde}
         GROUP BY m.id ORDER BY n DESC, ingreso DESC LIMIT 15"
    )))
    .fetch_all(p)
    .await?;
    let facciones: Vec<(String, String, i64, f64)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT m.faccion, m.juego, sum(pp.cantidad) AS n, sum(pp.cantidad * pp.precio) AS ingreso
         FROM pedido_piezas pp JOIN pedidos p ON p.id = pp.pedido_id JOIN modelos m ON m.id = pp.modelo_id
         WHERE p.estado = 'entregado' AND p.entregado >= {desde}
         GROUP BY m.juego, m.faccion ORDER BY n DESC, ingreso DESC LIMIT 10"
    )))
    .fetch_all(p)
    .await?;
    Ok(Json(json!({
        "dias": dias,
        "abiertos": abiertos.len(),
        "atrasados": atrasados,
        "por_cobrar": por_cobrar,
        "vendidos": vendidos.len(),
        "piezas": vendidos.iter().filter_map(|v| v["piezas"].as_i64()).sum::<i64>(),
        "ingresos": ingresos,
        "envios": suma(&vendidos, "envio"),
        "costo": suma(&vendidos, "costo"),
        "ganancia": suma(&vendidos, "ganancia"),
        "sin_costo": vendidos.iter().filter(|v| v["ganancia"].is_null()).count(),
        "mas_vendidos": top.iter().map(|(id, n, f, j, c, i, hay)| json!({
            "modelo_id": id, "nombre": n, "faccion": f, "juego": j, "cantidad": c, "ingreso": centavos(*i), "hay": hay
        })).collect::<Vec<_>>(),
        "facciones": facciones.iter().map(|(f, j, c, i)| json!({
            "faccion": f, "juego": j, "cantidad": c, "ingreso": centavos(*i)
        })).collect::<Vec<_>>(),
    })))
}
