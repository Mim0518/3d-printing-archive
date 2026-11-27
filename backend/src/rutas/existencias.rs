//! Impresiones, existencias y costos (fase 6). Exigen sesión.
//!
//! Registrar una impresión guarda lo que costó y mete a las existencias las miniaturas que salieron bien. La
//! cantidad de una existencia es la suma de sus movimientos: entradas por impresión, salidas por merma o venta,
//! ajustes, y los pares de un cambio de acabado o de lugar.

use std::collections::HashMap;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value, json};
use sqlx::{QueryBuilder, Sqlite, SqliteConnection, SqlitePool};

use crate::{
    AppState,
    auth::Usuario,
    costos::{self, ACABADOS, AJUSTES, Ajustes, Entrada, MaterialCosto, SQL_CANTIDAD, SQL_COSTO},
    db::ahora,
    error::ApiError,
    taller::Taller,
};

pub fn rutas() -> Router<AppState> {
    Router::new()
        .route("/ajustes/costos", get(ver_ajustes).put(poner_ajustes))
        .route("/placas/{id}/impresion", get(propuesta))
        .route("/impresiones", get(impresiones).post(registrar))
        .route("/impresiones/calcular", post(calcular))
        .route("/impresiones/{id}", get(impresion).delete(deshacer))
        .route("/existencias", get(existencias).post(crear_existencia))
        .route("/existencias/resumen", get(resumen))
        .route(
            "/existencias/{id}",
            get(existencia).patch(editar_existencia),
        )
        .route("/existencias/{id}/movimientos", post(mover_cantidad))
        .route("/existencias/{id}/mover", post(cambiar))
        .route("/modelos/{id}/existencias", get(del_modelo))
}

pub(crate) fn doble_f<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<f64>>, D::Error> {
    Option::<f64>::deserialize(d).map(Some)
}

pub(crate) fn doble_i<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<i64>>, D::Error> {
    Option::<i64>::deserialize(d).map(Some)
}

pub(crate) fn como_json<T: Serialize>(x: &T) -> Value {
    serde_json::to_value(x).unwrap_or(Value::Null)
}

pub(crate) fn acabado_valido(a: &str) -> Result<(), ApiError> {
    if ACABADOS.contains(&a) {
        Ok(())
    } else {
        Err(ApiError::Invalido(format!("Acabado desconocido: {a}")))
    }
}

fn ubicacion_valida(u: &str) -> Result<String, ApiError> {
    let u = u.trim();
    if u.chars().count() > 80 {
        return Err(ApiError::Invalido("La ubicación pasa de 80 letras".into()));
    }
    Ok(u.to_string())
}

pub(crate) fn notas_validas(n: &Option<String>) -> Result<String, ApiError> {
    let n = n.as_deref().unwrap_or("").trim();
    if n.chars().count() > 2000 {
        return Err(ApiError::Invalido("Las notas pasan de 2,000 letras".into()));
    }
    Ok(n.to_string())
}

// ---------- ajustes ----------

async fn ver_ajustes(_u: Usuario, State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let a = Ajustes::cargar(&st.pool).await?;
    Ok(Json(
        json!({ "campos": AJUSTES, "valores": a.valores, "moneda": a.moneda }),
    ))
}

#[derive(Debug, Deserialize)]
struct PedidoAjustes {
    valores: Map<String, Value>,
    moneda: Option<String>,
}

/// Pone los ajustes que vienen; `null` borra uno (deja de contarse).
async fn poner_ajustes(
    _u: Usuario,
    State(st): State<AppState>,
    Json(p): Json<PedidoAjustes>,
) -> Result<Json<Value>, ApiError> {
    let t = ahora();
    let mut tx = st.pool.begin().await?;
    for (k, v) in &p.valores {
        let campo = AJUSTES
            .iter()
            .find(|a| a.clave == k)
            .ok_or_else(|| ApiError::Invalido(format!("Ajuste desconocido: {k}")))?;
        match v {
            Value::Null => {
                sqlx::query("DELETE FROM ajustes WHERE clave = ?")
                    .bind(k)
                    .execute(&mut *tx)
                    .await?;
            }
            _ => {
                let n = v.as_f64().filter(|n| n.is_finite()).ok_or_else(|| {
                    ApiError::Invalido(format!("«{}» debe ser un número", campo.etiqueta))
                })?;
                if n < campo.min || n > campo.max {
                    return Err(ApiError::Invalido(format!(
                        "«{}» debe estar entre {} y {}",
                        campo.etiqueta, campo.min, campo.max
                    )));
                }
                sqlx::query(
                    "INSERT INTO ajustes (clave, valor, actualizado) VALUES (?, ?, ?)
                     ON CONFLICT (clave) DO UPDATE SET valor = excluded.valor, actualizado = excluded.actualizado",
                )
                .bind(k)
                .bind(json!(n).to_string())
                .bind(t)
                .execute(&mut *tx)
                .await?;
            }
        }
    }
    if let Some(m) = &p.moneda {
        let m = m.trim().to_uppercase();
        if m.len() != 3 || !m.chars().all(|c| c.is_ascii_alphabetic()) {
            return Err(ApiError::Invalido(
                "La moneda va en tres letras, como MXN".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO ajustes (clave, valor, actualizado) VALUES ('moneda', ?, ?)
             ON CONFLICT (clave) DO UPDATE SET valor = excluded.valor, actualizado = excluded.actualizado",
        )
        .bind(json!(m).to_string())
        .bind(t)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

// ---------- cálculo ----------

#[derive(Debug, Clone, sqlx::FromRow)]
struct ImpresoraCosto {
    tecnologia: String,
    potencia_w: Option<f64>,
}

async fn impresora_costo(
    pool: &SqlitePool,
    id: Option<i64>,
) -> Result<Option<ImpresoraCosto>, ApiError> {
    let Some(id) = id else { return Ok(None) };
    let i: Option<ImpresoraCosto> =
        sqlx::query_as("SELECT tecnologia, potencia_w FROM impresoras WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    i.map(Some)
        .ok_or_else(|| ApiError::Invalido("Esa impresora no existe".into()))
}

async fn material_costo(
    pool: &SqlitePool,
    id: Option<i64>,
) -> Result<Option<MaterialCosto>, ApiError> {
    let Some(id) = id else { return Ok(None) };
    MaterialCosto::cargar(pool, id)
        .await?
        .map(Some)
        .ok_or_else(|| ApiError::Invalido("Ese material no existe".into()))
}

#[derive(Debug, Deserialize)]
struct PedidoCalculo {
    impresora_id: Option<i64>,
    material_id: Option<i64>,
    consumo: Option<f64>,
    unidad: Option<String>,
    tiempo_s: Option<i64>,
    /// miniaturas que salen bien (para el costo por miniatura)
    piezas: Option<i64>,
}

fn unidad_de(
    unidad: &Option<String>,
    material: Option<&MaterialCosto>,
    tecnologia: Option<&str>,
) -> String {
    match unidad.as_deref() {
        Some("ml") => "ml".into(),
        Some("g") => "g".into(),
        _ => match (material, tecnologia) {
            (Some(m), _) => m.unidad().into(),
            (None, Some("fdm")) => "g".into(),
            _ => "ml".into(),
        },
    }
}

/// Costo de una impresión, por miniatura y precio sugerido sin pintar.
fn costear(
    a: &Ajustes,
    imp: Option<&ImpresoraCosto>,
    mat: Option<&MaterialCosto>,
    consumo: Option<f64>,
    unidad: &str,
    tiempo_s: Option<i64>,
    piezas: i64,
) -> (costos::Desglose, Option<f64>, costos::Precio) {
    let d = costos::calcular(
        a,
        &Entrada {
            tecnologia: imp.map(|i| i.tecnologia.as_str()),
            potencia_w: imp.and_then(|i| i.potencia_w),
            material: mat,
            consumo,
            unidad,
            tiempo_s,
        },
    );
    let sabido = d.material.is_some() || d.luz.is_some() || d.limpieza.is_some();
    let unitario =
        (piezas > 0 && sabido).then(|| (d.total / piezas as f64 * 100.0).round() / 100.0);
    let precio = costos::precio_sugerido(a, unitario, "sin_pintar");
    (d, unitario, precio)
}

async fn calcular(
    _u: Usuario,
    State(st): State<AppState>,
    Json(p): Json<PedidoCalculo>,
) -> Result<Json<Value>, ApiError> {
    let a = Ajustes::cargar(&st.pool).await?;
    let imp = impresora_costo(&st.pool, p.impresora_id).await?;
    let mat = material_costo(&st.pool, p.material_id).await?;
    let unidad = unidad_de(
        &p.unidad,
        mat.as_ref(),
        imp.as_ref().map(|i| i.tecnologia.as_str()),
    );
    let (d, unitario, precio) = costear(
        &a,
        imp.as_ref(),
        mat.as_ref(),
        p.consumo,
        &unidad,
        p.tiempo_s,
        p.piezas.unwrap_or(0),
    );
    Ok(Json(
        json!({ "desglose": d, "costo_unitario": unitario, "precio": precio, "unidad": unidad }),
    ))
}

// ---------- propuesta para registrar una impresión de una placa ----------

/// nombre, impresora, material y perfil elegidos de una placa
type PlacaElegida = (String, Option<i64>, Option<i64>, Option<i64>);

/// impresión, modelo, nombre, facción, carpeta, acabado, ubicación y cantidad
type PiezaDeImpresion = (i64, i64, String, String, String, String, String, i64);

#[derive(Debug, sqlx::FromRow)]
struct UltimaImpresion {
    impresora_id: Option<i64>,
    perfil_id: Option<i64>,
    material_id: Option<i64>,
    consumo: Option<f64>,
    unidad: String,
    tiempo_s: Option<i64>,
    piezas: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct LaminadoCosto {
    archivo_id: i64,
    nombre: String,
    res_x: Option<i64>,
    res_y: Option<i64>,
    capa_mm: Option<f64>,
    exposicion_s: Option<f64>,
    exposicion_base_s: Option<f64>,
    tiempo_s: Option<i64>,
    volumen_ml: Option<f64>,
    peso_g: Option<f64>,
}

/// Con qué impresora, perfil y material se imprimiría la placa, lo que gasta según su laminado, qué
/// miniaturas salen y cuánto costaría.
async fn propuesta(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    let placa: Option<PlacaElegida> = sqlx::query_as(
        "SELECT nombre, impresora_id, material_id, perfil_id FROM placas WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(p)
    .await?;
    let (nombre, p_imp, p_mat, p_perfil) = placa.ok_or(ApiError::NoEncontrado)?;
    let taller = Taller::cargar(p).await?;
    let laminados: Vec<LaminadoCosto> = sqlx::query_as(
        "SELECT a.id AS archivo_id, a.nombre, d.res_x, d.res_y, d.capa_mm, d.exposicion_s, d.exposicion_base_s,
                d.tiempo_s, d.volumen_ml, d.peso_g
         FROM placa_archivos pa JOIN archivos a ON a.id = pa.archivo_id
         JOIN datos_placa d ON d.archivo_id = a.id
         WHERE pa.placa_id = ? AND pa.rol = 'laminado' AND a.estado = 'presente' AND d.error = ''
         ORDER BY a.mtime_ms DESC",
    )
    .bind(id)
    .fetch_all(p)
    .await?;
    // la última vez que se imprimió: lo que no se elija ni se reconozca sale de ahí
    let ultima: Option<UltimaImpresion> = sqlx::query_as(
        "SELECT impresora_id, perfil_id, material_id, consumo, unidad, tiempo_s, piezas FROM impresiones
         WHERE placa_id = ? ORDER BY fecha DESC, id DESC LIMIT 1",
    )
    .bind(id)
    .fetch_optional(p)
    .await?;
    let ult = ultima.as_ref();
    // impresora: la elegida, la del laminado más reciente que se reconozca o la de la última vez
    let impresora = p_imp
        .or_else(|| {
            laminados
                .iter()
                .find_map(|l| taller.impresora_de(l.res_x, l.res_y).map(|i| i.id))
        })
        .or(ult.and_then(|u| u.impresora_id));
    let misma_que_antes = ult.is_some_and(|u| u.impresora_id == impresora);
    let laminado = laminados.iter().find(|l| {
        impresora.is_some() && taller.impresora_de(l.res_x, l.res_y).map(|i| i.id) == impresora
    });
    let normal: Option<i64> = match impresora {
        Some(i) => sqlx::query_scalar("SELECT perfil_id FROM impresoras WHERE id = ?")
            .bind(i)
            .fetch_optional(p)
            .await?
            .flatten(),
        None => None,
    };
    let perfil = p_perfil
        .or_else(|| {
            let (i, l) = (impresora?, laminado?);
            taller
                .perfil_de(i, l.capa_mm, l.exposicion_s, l.exposicion_base_s)
                .map(|x| x.id)
        })
        .or(ult.filter(|_| misma_que_antes).and_then(|u| u.perfil_id))
        .or(normal);
    let material = p_mat
        .or_else(|| {
            perfil
                .and_then(|x| taller.perfil(x))
                .and_then(|x| x.material_id)
        })
        .or(ult.filter(|_| misma_que_antes).and_then(|u| u.material_id));
    let imp = impresora_costo(p, impresora).await?;
    let mat = material_costo(p, material).await?;
    let fdm = imp.as_ref().is_some_and(|i| i.tecnologia == "fdm");
    let (consumo, unidad) = match laminado {
        Some(l) if fdm => (l.peso_g, "g"),
        Some(l) => (
            l.volumen_ml.or(l.peso_g),
            if l.volumen_ml.is_some() { "ml" } else { "g" },
        ),
        None => (None, if fdm { "g" } else { "ml" }),
    };
    // sin laminado: lo que gastó la última vez con la misma impresora
    let (consumo, unidad, origen) = match (consumo, ult.filter(|_| misma_que_antes)) {
        (Some(c), _) => (Some(c), unidad.to_string(), "laminado"),
        (None, Some(u)) if u.consumo.is_some() => (u.consumo, u.unidad.clone(), "anterior"),
        _ => (None, unidad.to_string(), "capturado"),
    };
    let unidad = unidad.as_str();
    let piezas: Vec<(i64, String, String, String, Option<i64>, String)> = sqlx::query_as(
        "SELECT m.id, m.nombre, m.faccion, m.carpeta, pm.cantidad, pm.estado FROM placa_modelos pm
         JOIN modelos m ON m.id = pm.modelo_id
         WHERE pm.placa_id = ? AND pm.estado <> 'rechazado' ORDER BY pm.estado = 'confirmado' DESC, m.nombre",
    )
    .bind(id)
    .fetch_all(p)
    .await?;
    let n_piezas: i64 = piezas
        .iter()
        .filter(|x| x.5 == "confirmado")
        .filter_map(|x| x.4)
        .sum();
    // si la placa no dice cuántas salen, las que salieron la última vez
    let n_piezas = if n_piezas > 0 {
        n_piezas
    } else {
        ult.map(|u| u.piezas).unwrap_or(0)
    };
    let a = Ajustes::cargar(p).await?;
    let tiempo = laminado
        .and_then(|l| l.tiempo_s)
        .or(ult.filter(|_| misma_que_antes).and_then(|u| u.tiempo_s));
    let (d, unitario, precio) = costear(
        &a,
        imp.as_ref(),
        mat.as_ref(),
        consumo,
        unidad,
        tiempo,
        n_piezas,
    );
    // pedidos abiertos a los que les faltan miniaturas de los modelos de esta placa
    let pedidos: Vec<(i64, String, i64)> = sqlx::query_as(
        "SELECT p.id, coalesce(c.nombre, '') AS cliente,
                sum(pp.cantidad - coalesce((SELECT -sum(mv.delta) FROM movimientos mv
                                            WHERE mv.pedido_pieza_id = pp.id), 0)) AS faltan
         FROM pedidos p LEFT JOIN clientes c ON c.id = p.cliente_id
         JOIN pedido_piezas pp ON pp.pedido_id = p.id
         WHERE p.estado NOT IN ('entregado', 'cancelado')
           AND pp.modelo_id IN (SELECT modelo_id FROM placa_modelos WHERE placa_id = ? AND estado <> 'rechazado')
         GROUP BY p.id HAVING faltan > 0
         ORDER BY p.fecha_entrega IS NULL, p.fecha_entrega, p.id",
    )
    .bind(id)
    .fetch_all(p)
    .await?;
    let ubicaciones: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT ubicacion FROM existencias WHERE ubicacion <> '' ORDER BY ubicacion",
    )
    .fetch_all(p)
    .await?;
    Ok(Json(json!({
        "placa_id": id,
        "placa": nombre,
        "impresora_id": impresora,
        "perfil_id": perfil,
        "material_id": material,
        "laminado": laminado.map(|l| json!({ "id": l.archivo_id, "nombre": l.nombre })),
        "consumo": consumo.map(|c| (c * 100.0).round() / 100.0),
        "unidad": unidad,
        "consumo_origen": origen,
        "tiempo_s": tiempo,
        "piezas": piezas.iter().map(|(mid, n, f, c, cant, estado)| json!({
            "modelo_id": mid, "nombre": n, "faccion": f, "carpeta": c, "cantidad": cant, "estado": estado
        })).collect::<Vec<_>>(),
        "ubicaciones": ubicaciones,
        "pedidos": pedidos.iter().map(|(i, c, f)| json!({ "id": i, "cliente": c, "faltan": f })).collect::<Vec<_>>(),
        "estimado": { "desglose": d, "costo_unitario": unitario, "precio": precio, "piezas": n_piezas },
    })))
}

// ---------- registrar y deshacer ----------

#[derive(Debug, Deserialize)]
struct PiezaImpresa {
    modelo_id: i64,
    cantidad: i64,
    acabado: Option<String>,
    ubicacion: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NuevaImpresion {
    placa_id: Option<i64>,
    impresora_id: Option<i64>,
    material_id: Option<i64>,
    perfil_id: Option<i64>,
    fecha: Option<i64>,
    resultado: String,
    consumo: Option<f64>,
    unidad: Option<String>,
    consumo_origen: Option<String>,
    tiempo_s: Option<i64>,
    #[serde(default)]
    piezas: Vec<PiezaImpresa>,
    notas: Option<String>,
    /// descontar el consumo de lo que queda del material (si se lleva la cuenta)
    descontar_material: Option<bool>,
    /// la impresión se hizo para este pedido: lo que sale se aparta para sus piezas
    pedido_id: Option<i64>,
}

/// La existencia de ese modelo, acabado y lugar (se crea si no está).
pub(crate) async fn existencia_de(
    tx: &mut SqliteConnection,
    modelo: i64,
    acabado: &str,
    ubicacion: &str,
) -> Result<i64, sqlx::Error> {
    let t = ahora();
    sqlx::query_scalar(
        "INSERT INTO existencias (modelo_id, acabado, ubicacion, creada, actualizada) VALUES (?, ?, ?, ?, ?)
         ON CONFLICT (modelo_id, acabado, ubicacion) DO UPDATE SET actualizada = excluded.actualizada
         RETURNING id",
    )
    .bind(modelo)
    .bind(acabado)
    .bind(ubicacion)
    .bind(t)
    .bind(t)
    .fetch_one(&mut *tx)
    .await
}

#[allow(clippy::too_many_arguments)]
async fn movimiento(
    tx: &mut SqliteConnection,
    existencia: i64,
    delta: i64,
    motivo: &str,
    costo: Option<f64>,
    impresion: Option<i64>,
    notas: &str,
    usuario: &str,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO movimientos (existencia_id, delta, motivo, costo_unitario, impresion_id, notas, usuario, fecha)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(existencia)
    .bind(delta)
    .bind(motivo)
    .bind(costo)
    .bind(impresion)
    .bind(notas)
    .bind(usuario)
    .bind(ahora())
    .fetch_one(&mut *tx)
    .await
}

pub(crate) async fn cantidad(
    tx: &mut SqliteConnection,
    existencia: i64,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT coalesce(sum(delta), 0) FROM movimientos WHERE existencia_id = ?")
        .bind(existencia)
        .fetch_one(&mut *tx)
        .await
}

async fn registrar(
    u: Usuario,
    State(st): State<AppState>,
    Json(n): Json<NuevaImpresion>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let p = &st.pool;
    if !["bien", "parcial", "fallida"].contains(&n.resultado.as_str()) {
        return Err(ApiError::Invalido(format!(
            "Resultado desconocido: {}",
            n.resultado
        )));
    }
    let notas = notas_validas(&n.notas)?;
    if n.consumo
        .is_some_and(|c| !c.is_finite() || !(0.0..=100_000.0).contains(&c))
    {
        return Err(ApiError::Invalido("El consumo no es válido".into()));
    }
    if n.tiempo_s.is_some_and(|t| !(0..=60 * 86_400).contains(&t)) {
        return Err(ApiError::Invalido("El tiempo no es válido".into()));
    }
    let fecha = n.fecha.unwrap_or_else(ahora);
    if fecha < 946_684_800 || fecha > ahora() + 86_400 {
        return Err(ApiError::Invalido("La fecha no es válida".into()));
    }
    let placa: Option<(String, String)> = match n.placa_id {
        Some(pl) => Some(
            sqlx::query_as("SELECT nombre, estado FROM placas WHERE id = ?")
                .bind(pl)
                .fetch_optional(p)
                .await?
                .ok_or_else(|| ApiError::Invalido("Esa placa no existe".into()))?,
        ),
        None => None,
    };
    let imp = impresora_costo(p, n.impresora_id).await?;
    let mat = material_costo(p, n.material_id).await?;
    let tecnologia = imp.as_ref().map(|i| i.tecnologia.clone());
    if let (Some(t), Some(m)) = (&tecnologia, &mat) {
        let esperado = if t == "fdm" { "filamento" } else { "resina" };
        if m.tipo != esperado {
            return Err(ApiError::Invalido(format!(
                "Esa impresora usa {esperado}, no {}",
                m.tipo
            )));
        }
    }
    let perfil_version: Option<i64> = match n.perfil_id {
        Some(pf) => {
            let x: Option<(i64, i64)> =
                sqlx::query_as("SELECT impresora_id, version FROM perfiles WHERE id = ?")
                    .bind(pf)
                    .fetch_optional(p)
                    .await?;
            let (de, version) =
                x.ok_or_else(|| ApiError::Invalido("Ese perfil no existe".into()))?;
            if n.impresora_id.is_some_and(|i| i != de) {
                return Err(ApiError::Invalido("Ese perfil es de otra impresora".into()));
            }
            Some(version)
        }
        None => None,
    };
    let mut piezas: Vec<(i64, i64, String, String)> = Vec::new();
    for x in &n.piezas {
        if !(0..=10_000).contains(&x.cantidad) {
            return Err(ApiError::Invalido(
                "Cantidad de miniaturas no válida".into(),
            ));
        }
        let acabado = x.acabado.clone().unwrap_or_else(|| "sin_pintar".into());
        acabado_valido(&acabado)?;
        let ubicacion = ubicacion_valida(x.ubicacion.as_deref().unwrap_or(""))?;
        let hay: i64 = sqlx::query_scalar("SELECT count(*) FROM modelos WHERE id = ?")
            .bind(x.modelo_id)
            .fetch_one(p)
            .await?;
        if hay == 0 {
            return Err(ApiError::Invalido("Uno de los modelos no existe".into()));
        }
        if x.cantidad > 0 {
            piezas.push((x.modelo_id, x.cantidad, acabado, ubicacion));
        }
    }
    if n.resultado == "fallida" && !piezas.is_empty() {
        return Err(ApiError::Invalido(
            "Si falló toda, no salieron miniaturas: usa «parcial» si algunas sí".into(),
        ));
    }
    if let Some(pe) = n.pedido_id {
        let estado: Option<String> = sqlx::query_scalar("SELECT estado FROM pedidos WHERE id = ?")
            .bind(pe)
            .fetch_optional(p)
            .await?;
        match estado.as_deref() {
            None => return Err(ApiError::Invalido("Ese pedido no existe".into())),
            Some("entregado" | "cancelado") => {
                return Err(ApiError::Invalido("Ese pedido ya está cerrado".into()));
            }
            _ => {}
        }
    }
    let total_piezas: i64 = piezas.iter().map(|x| x.1).sum();
    let unidad = unidad_de(&n.unidad, mat.as_ref(), tecnologia.as_deref());
    let a = Ajustes::cargar(p).await?;
    let (d, unitario, _) = costear(
        &a,
        imp.as_ref(),
        mat.as_ref(),
        n.consumo,
        &unidad,
        n.tiempo_s,
        total_piezas,
    );
    let origen = match n.consumo_origen.as_deref() {
        Some("laminado") if n.consumo.is_some() => "laminado",
        _ => "capturado",
    };

    let mut tx = p.begin().await?;
    // lo que queda del material, si se lleva la cuenta
    let mut descontado = None;
    if n.descontar_material.unwrap_or(true)
        && let (Some(m), Some(c)) = (&mat, d.consumo_material)
        && let Some(q) = m.queda
    {
        let nuevo = (q - c).max(0.0);
        descontado = Some(q - nuevo);
        sqlx::query("UPDATE materiales SET queda = ? WHERE id = ?")
            .bind(nuevo)
            .bind(m.id)
            .execute(&mut *tx)
            .await?;
    }
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO impresiones (placa_id, placa_nombre, impresora_id, material_id, perfil_id, perfil_version, fecha,
            resultado, consumo, unidad, consumo_origen, tiempo_s, costo_material, costo_luz, costo_limpieza,
            costo_fallas, costo_total, piezas, material_descontado, notas, usuario, creada, pedido_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(n.placa_id)
    .bind(placa.as_ref().map(|x| x.0.clone()).unwrap_or_default())
    .bind(n.impresora_id)
    .bind(n.material_id)
    .bind(n.perfil_id)
    .bind(perfil_version)
    .bind(fecha)
    .bind(&n.resultado)
    .bind(n.consumo)
    .bind(&unidad)
    .bind(origen)
    .bind(n.tiempo_s)
    .bind(d.material)
    .bind(d.luz)
    .bind(d.limpieza)
    .bind(d.fallas)
    .bind(d.total)
    .bind(total_piezas)
    .bind(descontado)
    .bind(&notas)
    .bind(&u.nombre)
    .bind(ahora())
    .bind(n.pedido_id)
    .fetch_one(&mut *tx)
    .await?;
    for (modelo, cant, acabado, ubicacion) in &piezas {
        let e = existencia_de(&mut tx, *modelo, acabado, ubicacion).await?;
        movimiento(
            &mut tx,
            e,
            *cant,
            "impresion",
            unitario,
            Some(id),
            "",
            &u.nombre,
        )
        .await?;
    }
    let apartadas = match n.pedido_id {
        Some(pe) => super::ventas::apartar_impresas(&mut tx, pe, id, &u.nombre).await?,
        None => 0,
    };
    // la primera impresión dice si la placa funciona
    if let (Some(pl), Some((_, estado))) = (n.placa_id, &placa)
        && estado == "sin_probar"
    {
        sqlx::query("UPDATE placas SET estado = ?, actualizada = ? WHERE id = ?")
            .bind(if n.resultado == "fallida" {
                "fallida"
            } else {
                "probada"
            })
            .bind(ahora())
            .bind(pl)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(
            json!({ "id": id, "desglose": d, "costo_unitario": unitario, "piezas": total_piezas,
                    "apartadas": apartadas }),
        ),
    ))
}

/// Borra una impresión registrada por error: saca sus miniaturas de las existencias (si siguen ahí) y
/// devuelve el material que se había descontado.
async fn deshacer(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    let fila: Option<(Option<i64>, Option<f64>)> =
        sqlx::query_as("SELECT material_id, material_descontado FROM impresiones WHERE id = ?")
            .bind(id)
            .fetch_optional(p)
            .await?;
    let (material, descontado) = fila.ok_or(ApiError::NoEncontrado)?;
    let mut tx = p.begin().await?;
    let entradas: Vec<(i64, i64)> =
        sqlx::query_as("SELECT existencia_id, delta FROM movimientos WHERE impresion_id = ?")
            .bind(id)
            .fetch_all(&mut *tx)
            .await?;
    for (e, delta) in &entradas {
        let hay = cantidad(&mut tx, *e).await?;
        if hay < *delta {
            return Err(ApiError::Conflicto(format!(
                "De las {delta} miniaturas que entraron con esta impresión solo quedan {hay} (las demás se \
                 vendieron, se pintaron o se movieron): deshaz o ajusta eso primero"
            )));
        }
    }
    if let (Some(m), Some(d)) = (material, descontado) {
        sqlx::query("UPDATE materiales SET queda = queda + ? WHERE id = ? AND queda IS NOT NULL")
            .bind(d)
            .bind(m)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("DELETE FROM impresiones WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

// ---------- historial de impresiones ----------

#[derive(Debug, Default, Deserialize)]
struct FiltrosImpresiones {
    placa_id: Option<i64>,
    pedido_id: Option<i64>,
    modelo_id: Option<i64>,
    impresora_id: Option<i64>,
    resultado: Option<String>,
    pagina: Option<i64>,
    por_pagina: Option<i64>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct FilaImpresion {
    id: i64,
    placa_id: Option<i64>,
    placa: String,
    impresora_id: Option<i64>,
    impresora: Option<String>,
    material_id: Option<i64>,
    material: Option<String>,
    perfil_id: Option<i64>,
    perfil: Option<String>,
    perfil_version: Option<i64>,
    fecha: i64,
    resultado: String,
    consumo: Option<f64>,
    unidad: String,
    consumo_origen: String,
    tiempo_s: Option<i64>,
    costo_material: Option<f64>,
    costo_luz: Option<f64>,
    costo_limpieza: Option<f64>,
    costo_fallas: Option<f64>,
    costo_total: f64,
    piezas: i64,
    material_descontado: Option<f64>,
    notas: String,
    usuario: String,
    pedido_id: Option<i64>,
}

const SELECT_IMPRESION: &str = "SELECT x.id, x.placa_id, coalesce(pl.nombre, x.placa_nombre) AS placa, x.impresora_id,
        i.nombre AS impresora, x.material_id, trim(m.marca || ' ' || m.nombre) AS material, x.perfil_id,
        pf.nombre AS perfil, x.perfil_version, x.fecha, x.resultado, x.consumo, x.unidad, x.consumo_origen,
        x.tiempo_s, x.costo_material, x.costo_luz, x.costo_limpieza, x.costo_fallas, x.costo_total, x.piezas,
        x.material_descontado, x.notas, x.usuario, x.pedido_id
    FROM impresiones x LEFT JOIN placas pl ON pl.id = x.placa_id LEFT JOIN impresoras i ON i.id = x.impresora_id
    LEFT JOIN materiales m ON m.id = x.material_id LEFT JOIN perfiles pf ON pf.id = x.perfil_id";

/// Las miniaturas que entraron con cada impresión.
async fn piezas_de(
    pool: &SqlitePool,
    ids: &[i64],
) -> Result<HashMap<i64, Vec<Value>>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT mv.impresion_id, m.id, m.nombre, m.faccion, m.carpeta, e.acabado, e.ubicacion, mv.delta
         FROM movimientos mv JOIN existencias e ON e.id = mv.existencia_id JOIN modelos m ON m.id = e.modelo_id
         WHERE mv.impresion_id IN (",
    );
    let mut sep = qb.separated(", ");
    for id in ids {
        sep.push_bind(*id);
    }
    qb.push(") ORDER BY m.nombre");
    let filas: Vec<PiezaDeImpresion> = qb.build_query_as().fetch_all(pool).await?;
    let mut r: HashMap<i64, Vec<Value>> = HashMap::new();
    for (imp, mid, nombre, faccion, carpeta, acabado, ubicacion, delta) in filas {
        r.entry(imp).or_default().push(json!({
            "modelo_id": mid, "nombre": nombre, "faccion": faccion, "carpeta": carpeta,
            "acabado": acabado, "ubicacion": ubicacion, "cantidad": delta
        }));
    }
    Ok(r)
}

async fn listar_impresiones(pool: &SqlitePool, f: &FiltrosImpresiones) -> Result<Value, ApiError> {
    let por_pagina = f.por_pagina.unwrap_or(50).clamp(1, 500);
    let pagina = f.pagina.unwrap_or(1).max(1);
    let condiciones = |qb: &mut QueryBuilder<Sqlite>| {
        qb.push(" WHERE 1 = 1");
        if let Some(pl) = f.placa_id {
            qb.push(" AND x.placa_id = ").push_bind(pl);
        }
        if let Some(pe) = f.pedido_id {
            qb.push(" AND x.pedido_id = ").push_bind(pe);
        }
        if let Some(i) = f.impresora_id {
            qb.push(" AND x.impresora_id = ").push_bind(i);
        }
        if let Some(r) = f.resultado.as_deref().filter(|r| !r.is_empty()) {
            qb.push(" AND x.resultado = ").push_bind(r.to_string());
        }
        if let Some(m) = f.modelo_id {
            qb.push(
                " AND (EXISTS (SELECT 1 FROM movimientos mv JOIN existencias e ON e.id = mv.existencia_id
                   WHERE mv.impresion_id = x.id AND e.modelo_id = ",
            )
            .push_bind(m)
            .push(
                ") OR EXISTS (SELECT 1 FROM placa_modelos pm WHERE pm.placa_id = x.placa_id
                   AND pm.estado = 'confirmado' AND pm.modelo_id = ",
            )
            .push_bind(m)
            .push("))");
        }
    };
    let mut cuenta: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT count(*), coalesce(sum(x.costo_total), 0.0), coalesce(sum(x.piezas), 0) FROM impresiones x",
    );
    condiciones(&mut cuenta);
    let (total, costo, piezas): (i64, f64, i64) = cuenta.build_query_as().fetch_one(pool).await?;
    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(SELECT_IMPRESION);
    condiciones(&mut qb);
    qb.push(" ORDER BY x.fecha DESC, x.id DESC LIMIT ")
        .push_bind(por_pagina)
        .push(" OFFSET ")
        .push_bind((pagina - 1) * por_pagina);
    let filas: Vec<FilaImpresion> = qb.build_query_as().fetch_all(pool).await?;
    let mut pz = piezas_de(pool, &filas.iter().map(|x| x.id).collect::<Vec<_>>()).await?;
    let lista: Vec<Value> = filas
        .iter()
        .map(|x| {
            let mut v = como_json(x);
            v["miniaturas"] = json!(pz.remove(&x.id).unwrap_or_default());
            v
        })
        .collect();
    Ok(
        json!({ "total": total, "costo": costo, "piezas": piezas, "pagina": pagina,
               "por_pagina": por_pagina, "impresiones": lista }),
    )
}

async fn impresiones(
    _u: Usuario,
    State(st): State<AppState>,
    Query(f): Query<FiltrosImpresiones>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(listar_impresiones(&st.pool, &f).await?))
}

async fn impresion(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let x: FilaImpresion = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SELECT_IMPRESION} WHERE x.id = ?"
    )))
    .bind(id)
    .fetch_optional(&st.pool)
    .await?
    .ok_or(ApiError::NoEncontrado)?;
    let mut v = como_json(&x);
    v["miniaturas"] = json!(
        piezas_de(&st.pool, &[id])
            .await?
            .remove(&id)
            .unwrap_or_default()
    );
    Ok(Json(v))
}

// ---------- existencias ----------

#[derive(Debug, Default, Deserialize)]
struct FiltrosExistencias {
    q: Option<String>,
    juego: Option<String>,
    faccion: Option<String>,
    acabado: Option<String>,
    ubicacion: Option<String>,
    modelo_id: Option<i64>,
    /// con (por omisión: las que tienen algo) | todas | bajo_minimo
    ver: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct FilaExistencia {
    id: i64,
    modelo_id: i64,
    nombre: String,
    faccion: String,
    juego: String,
    estudio: String,
    carpeta: String,
    acabado: String,
    ubicacion: String,
    precio: Option<f64>,
    minimo: Option<i64>,
    notas: String,
    cantidad: i64,
    costo_unitario: Option<f64>,
    ultimo: Option<i64>,
    #[serde(skip)]
    portada_id: Option<i64>,
}

fn select_existencias() -> String {
    format!(
        "SELECT e.id, e.modelo_id, m.nombre, m.faccion, m.juego, m.estudio, m.carpeta, e.acabado, e.ubicacion,
                e.precio, e.minimo, e.notas, {SQL_CANTIDAD} AS cantidad, {SQL_COSTO} AS costo_unitario,
                (SELECT max(mv.fecha) FROM movimientos mv WHERE mv.existencia_id = e.id) AS ultimo,
                {} AS portada_id
         FROM existencias e JOIN modelos m ON m.id = e.modelo_id",
        crate::miniaturas::SQL_PORTADA
    )
}

pub(crate) async fn portadas(
    pool: &SqlitePool,
    ids: &[i64],
) -> Result<HashMap<i64, String>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT archivo_id, clave FROM miniaturas WHERE estado = 'ok' AND archivo_id IN (",
    );
    let mut sep = qb.separated(", ");
    for id in ids {
        sep.push_bind(*id);
    }
    qb.push(")");
    Ok(qb
        .build_query_as()
        .fetch_all(pool)
        .await?
        .into_iter()
        .collect())
}

/// Las existencias con su precio sugerido y su valor.
async fn filas_json(
    pool: &SqlitePool,
    filas: Vec<FilaExistencia>,
) -> Result<Vec<Value>, sqlx::Error> {
    let a = Ajustes::cargar(pool).await?;
    let cl = portadas(
        pool,
        &filas
            .iter()
            .filter_map(|f| f.portada_id)
            .collect::<Vec<_>>(),
    )
    .await?;
    Ok(filas
        .iter()
        .map(|f| {
            let costo = f.costo_unitario.map(|c| (c * 100.0).round() / 100.0);
            let precio = costos::precio_sugerido(&a, costo, &f.acabado);
            let vale = f.precio.or(precio.sugerido);
            let mut v = como_json(f);
            v["costo_unitario"] = json!(costo);
            v["precio_sugerido"] = json!(precio);
            v["precio_venta"] = json!(vale);
            v["valor_costo"] =
                json!(costo.map(|c| (c * f.cantidad as f64 * 100.0).round() / 100.0));
            v["valor_venta"] = json!(vale.map(|p| p * f.cantidad as f64));
            v["bajo_minimo"] = json!(f.minimo.is_some_and(|m| f.cantidad < m));
            v["portada"] = match f.portada_id.and_then(|p| cl.get(&p).map(|c| (p, c))) {
                Some((p, c)) => json!({ "id": p, "clave": c }),
                None => Value::Null,
            };
            v
        })
        .collect())
}

async fn listar_existencias(
    pool: &SqlitePool,
    f: &FiltrosExistencias,
) -> Result<Vec<Value>, ApiError> {
    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(select_existencias());
    qb.push(" WHERE 1 = 1");
    if let Some(q) = f.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        let patron = format!("%{}%", q.replace(['%', '_'], ""));
        qb.push(" AND (m.nombre LIKE ")
            .push_bind(patron.clone())
            .push(" OR m.faccion LIKE ")
            .push_bind(patron.clone())
            .push(" OR e.ubicacion LIKE ")
            .push_bind(patron)
            .push(")");
    }
    for (col, v) in [
        ("m.juego", &f.juego),
        ("m.faccion", &f.faccion),
        ("e.acabado", &f.acabado),
        ("e.ubicacion", &f.ubicacion),
    ] {
        if let Some(v) = v.as_deref().filter(|v| !v.is_empty()) {
            qb.push(format!(" AND {col} = ")).push_bind(v.to_string());
        }
    }
    if let Some(m) = f.modelo_id {
        qb.push(" AND e.modelo_id = ").push_bind(m);
    }
    match f.ver.as_deref() {
        Some("todas") => {}
        Some("bajo_minimo") => {
            qb.push(format!(
                " AND e.minimo IS NOT NULL AND {SQL_CANTIDAD} < e.minimo"
            ));
        }
        _ => {
            qb.push(format!(" AND {SQL_CANTIDAD} > 0"));
        }
    }
    qb.push(" ORDER BY m.juego, m.faccion, m.nombre, e.acabado, e.ubicacion LIMIT 5000");
    let filas: Vec<FilaExistencia> = qb.build_query_as().fetch_all(pool).await?;
    Ok(filas_json(pool, filas).await?)
}

async fn existencias(
    _u: Usuario,
    State(st): State<AppState>,
    Query(f): Query<FiltrosExistencias>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(listar_existencias(&st.pool, &f).await?)))
}

async fn resumen(_u: Usuario, State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    let todas = listar_existencias(p, &FiltrosExistencias::default()).await?;
    let suma = |k: &str| todas.iter().filter_map(|v| v[k].as_f64()).sum::<f64>();
    let mut por_lugar: Vec<(String, i64)> = Vec::new();
    for v in &todas {
        let u = v["ubicacion"].as_str().unwrap_or("").to_string();
        let n = v["cantidad"].as_i64().unwrap_or(0);
        match por_lugar.iter_mut().find(|x| x.0 == u) {
            Some(x) => x.1 += n,
            None => por_lugar.push((u, n)),
        }
    }
    // los lugares con más miniaturas primero
    por_lugar.sort_by(|x, y| y.1.cmp(&x.1).then_with(|| x.0.cmp(&y.0)));
    let bajo: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT count(*) FROM existencias e WHERE e.minimo IS NOT NULL AND {SQL_CANTIDAD} < e.minimo"
    )))
    .fetch_one(p)
    .await?;
    let hace_30 = ahora() - 30 * 86_400;
    let (impresiones, mes, costo_mes, fallidas_mes): (i64, i64, f64, i64) = sqlx::query_as(
        "SELECT count(*), coalesce(sum(fecha >= ?1), 0), coalesce(sum(CASE WHEN fecha >= ?1 THEN costo_total END), 0.0),
                coalesce(sum(fecha >= ?1 AND resultado = 'fallida'), 0)
         FROM impresiones",
    )
    .bind(hace_30)
    .fetch_one(p)
    .await?;
    let a = Ajustes::cargar(p).await?;
    let mut modelos: Vec<i64> = todas
        .iter()
        .filter_map(|v| v["modelo_id"].as_i64())
        .collect();
    modelos.sort();
    modelos.dedup();
    let modelos = modelos.len();
    Ok(Json(json!({
        "piezas": todas.iter().filter_map(|v| v["cantidad"].as_i64()).sum::<i64>(),
        "existencias": todas.len(),
        "modelos": modelos,
        "valor_costo": (suma("valor_costo") * 100.0).round() / 100.0 + 0.0,
        "valor_venta": suma("valor_venta") + 0.0,
        "sin_precio": todas.iter().filter(|v| v["precio_venta"].is_null()).count(),
        "bajo_minimo": bajo,
        "ubicaciones": por_lugar.iter().map(|(u, n)| json!({ "ubicacion": u, "piezas": n })).collect::<Vec<_>>(),
        "impresiones": impresiones,
        "impresiones_mes": mes,
        "costo_mes": costo_mes,
        "fallidas_mes": fallidas_mes,
        "ajustes_faltan": a.faltan(),
        "moneda": a.moneda,
    })))
}

#[derive(Debug, Deserialize)]
struct NuevaExistencia {
    modelo_id: i64,
    acabado: Option<String>,
    ubicacion: Option<String>,
    cantidad: i64,
    costo_unitario: Option<f64>,
    notas: Option<String>,
}

/// Miniaturas que ya tenías antes de la app (o que no salieron de una impresión registrada).
async fn crear_existencia(
    u: Usuario,
    State(st): State<AppState>,
    Json(n): Json<NuevaExistencia>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let p = &st.pool;
    let acabado = n.acabado.clone().unwrap_or_else(|| "sin_pintar".into());
    acabado_valido(&acabado)?;
    let ubicacion = ubicacion_valida(n.ubicacion.as_deref().unwrap_or(""))?;
    let notas = notas_validas(&n.notas)?;
    if !(1..=10_000).contains(&n.cantidad) {
        return Err(ApiError::Invalido(
            "La cantidad debe ser de 1 a 10,000".into(),
        ));
    }
    if n.costo_unitario
        .is_some_and(|c| !c.is_finite() || !(0.0..=1_000_000.0).contains(&c))
    {
        return Err(ApiError::Invalido("El costo no es válido".into()));
    }
    let hay: i64 = sqlx::query_scalar("SELECT count(*) FROM modelos WHERE id = ?")
        .bind(n.modelo_id)
        .fetch_one(p)
        .await?;
    if hay == 0 {
        return Err(ApiError::Invalido("Ese modelo no existe".into()));
    }
    let mut tx = p.begin().await?;
    let e = existencia_de(&mut tx, n.modelo_id, &acabado, &ubicacion).await?;
    let nota = if notas.is_empty() {
        "Existencia inicial".to_string()
    } else {
        notas
    };
    movimiento(
        &mut tx,
        e,
        n.cantidad,
        "ajuste",
        n.costo_unitario,
        None,
        &nota,
        &u.nombre,
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": e }))))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct FilaMovimiento {
    id: i64,
    delta: i64,
    motivo: String,
    costo_unitario: Option<f64>,
    impresion_id: Option<i64>,
    par_acabado: Option<String>,
    par_ubicacion: Option<String>,
    notas: String,
    usuario: String,
    fecha: i64,
}

async fn existencia(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    let fila: FilaExistencia = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{} WHERE e.id = ?",
        select_existencias()
    )))
    .bind(id)
    .fetch_optional(p)
    .await?
    .ok_or(ApiError::NoEncontrado)?;
    let movs: Vec<FilaMovimiento> = sqlx::query_as(
        "SELECT mv.id, mv.delta, mv.motivo, mv.costo_unitario, mv.impresion_id, pe.acabado AS par_acabado,
                pe.ubicacion AS par_ubicacion, mv.notas, mv.usuario, mv.fecha
         FROM movimientos mv LEFT JOIN movimientos par ON par.id = mv.par_id
         LEFT JOIN existencias pe ON pe.id = par.existencia_id
         WHERE mv.existencia_id = ? ORDER BY mv.fecha DESC, mv.id DESC",
    )
    .bind(id)
    .fetch_all(p)
    .await?;
    let mut v = filas_json(p, vec![fila]).await?.remove(0);
    v["movimientos"] = json!(movs);
    Ok(Json(v))
}

#[derive(Debug, Deserialize)]
struct CambiosExistencia {
    #[serde(default, deserialize_with = "doble_f")]
    precio: Option<Option<f64>>,
    #[serde(default, deserialize_with = "doble_i")]
    minimo: Option<Option<i64>>,
    notas: Option<String>,
}

async fn editar_existencia(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(c): Json<CambiosExistencia>,
) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    if let Some(Some(x)) = c.precio
        && (!x.is_finite() || !(0.0..=10_000_000.0).contains(&x))
    {
        return Err(ApiError::Invalido("El precio no es válido".into()));
    }
    if let Some(Some(m)) = c.minimo
        && !(0..=10_000).contains(&m)
    {
        return Err(ApiError::Invalido(
            "El mínimo debe ser de 0 a 10,000".into(),
        ));
    }
    let notas = c
        .notas
        .as_ref()
        .map(|_| notas_validas(&c.notas))
        .transpose()?;
    let mut tx = p.begin().await?;
    let r = sqlx::query("UPDATE existencias SET actualizada = ? WHERE id = ?")
        .bind(ahora())
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if r.rows_affected() == 0 {
        return Err(ApiError::NoEncontrado);
    }
    if let Some(x) = c.precio {
        sqlx::query("UPDATE existencias SET precio = ? WHERE id = ?")
            .bind(x)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(x) = c.minimo {
        sqlx::query("UPDATE existencias SET minimo = ? WHERE id = ?")
            .bind(x)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(n) = notas {
        sqlx::query("UPDATE existencias SET notas = ? WHERE id = ?")
            .bind(n)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Debug, Deserialize)]
struct PedidoMovimiento {
    delta: i64,
    /// ajuste | merma
    motivo: String,
    notas: Option<String>,
}

/// Ajuste (se contó y hay más o menos) o merma (se rompió, se perdió).
async fn mover_cantidad(
    u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(m): Json<PedidoMovimiento>,
) -> Result<Json<Value>, ApiError> {
    if !["ajuste", "merma"].contains(&m.motivo.as_str()) {
        return Err(ApiError::Invalido(format!(
            "Motivo no válido aquí: {}",
            m.motivo
        )));
    }
    if m.delta == 0 || m.delta.abs() > 10_000 {
        return Err(ApiError::Invalido("La cantidad no es válida".into()));
    }
    if m.motivo == "merma" && m.delta > 0 {
        return Err(ApiError::Invalido("Una merma resta: va en negativo".into()));
    }
    let notas = notas_validas(&m.notas)?;
    let mut tx = st.pool.begin().await?;
    let existe: i64 = sqlx::query_scalar("SELECT count(*) FROM existencias WHERE id = ?")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    if existe == 0 {
        return Err(ApiError::NoEncontrado);
    }
    let hay = cantidad(&mut tx, id).await?;
    if hay + m.delta < 0 {
        return Err(ApiError::Conflicto(format!("Solo hay {hay}")));
    }
    // una entrada por ajuste lleva el costo promedio, para no moverlo
    let costo: Option<f64> = if m.delta > 0 {
        sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT {SQL_COSTO} FROM existencias e WHERE e.id = ?"
        )))
        .bind(id)
        .fetch_one(&mut *tx)
        .await?
    } else {
        None
    };
    movimiento(
        &mut tx, id, m.delta, &m.motivo, costo, None, &notas, &u.nombre,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "cantidad": hay + m.delta })))
}

#[derive(Debug, Deserialize)]
struct PedidoCambio {
    cantidad: i64,
    acabado: Option<String>,
    ubicacion: Option<String>,
    notas: Option<String>,
}

/// Pasa miniaturas a otro acabado (se imprimaron, se pintaron) o a otro lugar.
async fn cambiar(
    u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(c): Json<PedidoCambio>,
) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    let origen: Option<(i64, String, String)> =
        sqlx::query_as("SELECT modelo_id, acabado, ubicacion FROM existencias WHERE id = ?")
            .bind(id)
            .fetch_optional(p)
            .await?;
    let (modelo, acabado, ubicacion) = origen.ok_or(ApiError::NoEncontrado)?;
    let nuevo_acabado = c.acabado.clone().unwrap_or_else(|| acabado.clone());
    acabado_valido(&nuevo_acabado)?;
    let nueva_ubicacion = match &c.ubicacion {
        Some(u) => ubicacion_valida(u)?,
        None => ubicacion.clone(),
    };
    if nuevo_acabado == acabado && nueva_ubicacion == ubicacion {
        return Err(ApiError::Invalido("Elige otro acabado u otro lugar".into()));
    }
    if !(1..=10_000).contains(&c.cantidad) {
        return Err(ApiError::Invalido("La cantidad no es válida".into()));
    }
    let notas = notas_validas(&c.notas)?;
    let motivo = if nuevo_acabado != acabado {
        "acabado"
    } else {
        "traslado"
    };
    let mut tx = p.begin().await?;
    let hay = cantidad(&mut tx, id).await?;
    if hay < c.cantidad {
        return Err(ApiError::Conflicto(format!("Solo hay {hay}")));
    }
    let costo: Option<f64> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT {SQL_COSTO} FROM existencias e WHERE e.id = ?"
    )))
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    let destino = existencia_de(&mut tx, modelo, &nuevo_acabado, &nueva_ubicacion).await?;
    let sale = movimiento(
        &mut tx,
        id,
        -c.cantidad,
        motivo,
        None,
        None,
        &notas,
        &u.nombre,
    )
    .await?;
    let entra = movimiento(
        &mut tx, destino, c.cantidad, motivo, costo, None, &notas, &u.nombre,
    )
    .await?;
    sqlx::query(
        "UPDATE movimientos SET par_id = CASE id WHEN ?1 THEN ?2 ELSE ?1 END WHERE id IN (?1, ?2)",
    )
    .bind(sale)
    .bind(entra)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "destino": destino })))
}

/// Existencias de un modelo e impresiones de sus placas.
async fn del_modelo(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    let ex = listar_existencias(
        p,
        &FiltrosExistencias {
            modelo_id: Some(id),
            ver: Some("todas".into()),
            ..Default::default()
        },
    )
    .await?;
    let imp = listar_impresiones(
        p,
        &FiltrosImpresiones {
            modelo_id: Some(id),
            por_pagina: Some(10),
            ..Default::default()
        },
    )
    .await?;
    // costo promedio de las miniaturas de este modelo que han salido de impresiones
    let costo: Option<f64> = sqlx::query_scalar(
        "SELECT sum(mv.delta * mv.costo_unitario) / sum(mv.delta) FROM movimientos mv
         JOIN existencias e ON e.id = mv.existencia_id
         WHERE e.modelo_id = ? AND mv.motivo = 'impresion' AND mv.costo_unitario IS NOT NULL",
    )
    .bind(id)
    .fetch_one(p)
    .await?;
    let a = Ajustes::cargar(p).await?;
    let costo = costo.map(|c| (c * 100.0).round() / 100.0);
    Ok(Json(json!({
        "existencias": ex,
        "impresiones": imp,
        "costo_promedio": costo,
        "precios": ACABADOS.iter().map(|ac| (ac.to_string(), costos::precio_sugerido(&a, costo, ac)))
            .collect::<HashMap<_, _>>(),
    })))
}
