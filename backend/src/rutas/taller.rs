//! Impresoras, materiales y perfiles (fase 5): listas, fichas, edición, versiones de los perfiles, sus archivos
//! y la importación de los archivos de perfiles de la colección. Exigen sesión.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value, json};
use sqlx::{QueryBuilder, Sqlite, SqliteConnection, SqlitePool};

use crate::{
    AppState,
    auth::Usuario,
    db::ahora,
    error::ApiError,
    taller::{self, CAMPOS, SQL_MISMA_PANTALLA, Taller},
};

pub fn rutas() -> Router<AppState> {
    Router::new()
        .route("/impresoras", get(impresoras).post(crear_impresora))
        .route(
            "/impresoras/{id}",
            get(impresora)
                .patch(editar_impresora)
                .delete(borrar_impresora),
        )
        .route("/materiales", get(materiales).post(crear_material))
        .route(
            "/materiales/{id}",
            axum::routing::patch(editar_material).delete(borrar_material),
        )
        .route("/perfiles", get(perfiles).post(crear_perfil))
        .route("/perfiles/campos", get(campos))
        .route(
            "/perfiles/{id}",
            get(perfil).patch(editar_perfil).delete(borrar_perfil),
        )
        .route("/perfiles/{id}/archivos", post(ligar_archivo))
        .route(
            "/perfiles/{id}/archivos/{archivo_id}",
            delete(desligar_archivo),
        )
        .route("/archivos/buscar", get(buscar_archivos))
        .route("/taller/resumen", get(resumen))
        .route("/taller/importar", get(importables).post(importar))
}

// ---------- utilidades ----------

/// Distingue «no vino» (`None`) de «vino vacío» (`Some(None)`), para poder borrar un valor.
fn doble_f<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<f64>>, D::Error> {
    Option::<f64>::deserialize(d).map(Some)
}

fn doble_i<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<i64>>, D::Error> {
    Option::<i64>::deserialize(d).map(Some)
}

enum Valor {
    Texto(String),
    Real(Option<f64>),
    Entero(Option<i64>),
    Logico(bool),
}

/// `UPDATE tabla SET campo = …` de lo que vino, más la fecha de actualización.
async fn actualizar(
    tx: &mut SqliteConnection,
    tabla: &str,
    fecha: &str,
    id: i64,
    campos: Vec<(&'static str, Valor)>,
) -> Result<(), sqlx::Error> {
    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(format!("UPDATE {tabla} SET {fecha} = "));
    qb.push_bind(ahora());
    for (k, v) in campos {
        qb.push(format!(", {k} = "));
        match v {
            Valor::Texto(s) => qb.push_bind(s),
            Valor::Real(x) => qb.push_bind(x),
            Valor::Entero(x) => qb.push_bind(x),
            Valor::Logico(b) => qb.push_bind(b),
        };
    }
    qb.push(" WHERE id = ").push_bind(id);
    qb.build().execute(&mut *tx).await?;
    Ok(())
}

fn texto(
    c: &mut Vec<(&'static str, Valor)>,
    campo: &'static str,
    etiqueta: &str,
    v: &Option<String>,
    max: usize,
    requerido: bool,
) -> Result<(), ApiError> {
    if let Some(s) = v {
        let s = s.trim();
        if requerido && s.is_empty() {
            return Err(ApiError::Invalido(format!("Falta {etiqueta}")));
        }
        if s.chars().count() > max {
            return Err(ApiError::Invalido(format!(
                "{etiqueta} pasa de {max} letras"
            )));
        }
        c.push((campo, Valor::Texto(s.to_string())));
    }
    Ok(())
}

fn real(
    c: &mut Vec<(&'static str, Valor)>,
    campo: &'static str,
    etiqueta: &str,
    v: Option<Option<f64>>,
    min: f64,
    max: f64,
) -> Result<(), ApiError> {
    if let Some(x) = v {
        if let Some(n) = x
            && (!n.is_finite() || n < min || n > max)
        {
            return Err(ApiError::Invalido(format!(
                "{etiqueta} debe estar entre {min} y {max}"
            )));
        }
        c.push((campo, Valor::Real(x)));
    }
    Ok(())
}

fn entero(
    c: &mut Vec<(&'static str, Valor)>,
    campo: &'static str,
    etiqueta: &str,
    v: Option<Option<i64>>,
    min: i64,
    max: i64,
) -> Result<(), ApiError> {
    if let Some(x) = v {
        if let Some(n) = x
            && (n < min || n > max)
        {
            return Err(ApiError::Invalido(format!(
                "{etiqueta} debe estar entre {min} y {max}"
            )));
        }
        c.push((campo, Valor::Entero(x)));
    }
    Ok(())
}

/// Ruta relativa dentro de la biblioteca, sin `..` ni diagonales de más.
fn carpeta_valida(s: &str) -> Result<String, ApiError> {
    let s = s.trim().trim_matches('/');
    if s.chars().count() > 500
        || s.split('/')
            .any(|p| p == ".." || p == "." || (p.is_empty() && !s.is_empty()))
    {
        return Err(ApiError::Invalido(
            "La carpeta no es una ruta válida de la biblioteca".into(),
        ));
    }
    Ok(s.to_string())
}

async fn cuenta(pool: &SqlitePool, sql: &'static str, id: i64) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(sql).bind(id).fetch_one(pool).await
}

fn como_json<T: Serialize>(x: &T) -> Value {
    serde_json::to_value(x).unwrap_or(Value::Null)
}

// ---------- campos ----------

async fn campos(_u: Usuario) -> Json<Value> {
    Json(json!(CAMPOS))
}

// ---------- impresoras ----------

#[derive(Debug, Serialize, sqlx::FromRow)]
struct FilaImpresora {
    id: i64,
    nombre: String,
    tecnologia: String,
    marca: String,
    modelo: String,
    volumen_x_mm: Option<f64>,
    volumen_y_mm: Option<f64>,
    volumen_z_mm: Option<f64>,
    boquilla_mm: Option<f64>,
    res_x: Option<i64>,
    res_y: Option<i64>,
    pixel_um: Option<f64>,
    potencia_w: Option<f64>,
    carpeta: String,
    perfil_id: Option<i64>,
    perfil: Option<String>,
    en_uso: bool,
    notas: String,
    origen: Option<String>,
    n_perfiles: i64,
    n_placas: i64,
    n_laminados: i64,
}

fn select_impresoras() -> String {
    format!(
        "SELECT i.id, i.nombre, i.tecnologia, i.marca, i.modelo, i.volumen_x_mm, i.volumen_y_mm, i.volumen_z_mm,
                i.boquilla_mm, i.res_x, i.res_y, i.pixel_um, i.potencia_w, i.carpeta, i.perfil_id,
                (SELECT pf.nombre FROM perfiles pf WHERE pf.id = i.perfil_id) AS perfil,
                i.en_uso, i.notas, i.clave_origen AS origen,
                (SELECT count(*) FROM perfiles pf WHERE pf.impresora_id = i.id) AS n_perfiles,
                (SELECT count(*) FROM placas p WHERE p.impresora_id = i.id OR EXISTS (
                    SELECT 1 FROM placa_archivos pa JOIN datos_placa d ON d.archivo_id = pa.archivo_id
                    WHERE pa.placa_id = p.id AND pa.rol = 'laminado' AND {SQL_MISMA_PANTALLA})) AS n_placas,
                (SELECT count(*) FROM archivos a JOIN datos_placa d ON d.archivo_id = a.id
                 WHERE a.estado = 'presente' AND a.tipo = 'laminado' AND {SQL_MISMA_PANTALLA}) AS n_laminados
         FROM impresoras i"
    )
}

async fn impresoras(_u: Usuario, State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let filas: Vec<FilaImpresora> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{} ORDER BY i.en_uso DESC, i.tecnologia DESC, i.nombre",
        select_impresoras()
    )))
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(json!(filas)))
}

#[derive(Debug, Default, Deserialize)]
struct DatosImpresora {
    nombre: Option<String>,
    tecnologia: Option<String>,
    marca: Option<String>,
    modelo: Option<String>,
    #[serde(default, deserialize_with = "doble_f")]
    volumen_x_mm: Option<Option<f64>>,
    #[serde(default, deserialize_with = "doble_f")]
    volumen_y_mm: Option<Option<f64>>,
    #[serde(default, deserialize_with = "doble_f")]
    volumen_z_mm: Option<Option<f64>>,
    #[serde(default, deserialize_with = "doble_f")]
    boquilla_mm: Option<Option<f64>>,
    #[serde(default, deserialize_with = "doble_i")]
    res_x: Option<Option<i64>>,
    #[serde(default, deserialize_with = "doble_i")]
    res_y: Option<Option<i64>>,
    #[serde(default, deserialize_with = "doble_f")]
    pixel_um: Option<Option<f64>>,
    #[serde(default, deserialize_with = "doble_f")]
    potencia_w: Option<Option<f64>>,
    carpeta: Option<String>,
    #[serde(default, deserialize_with = "doble_i")]
    perfil_id: Option<Option<i64>>,
    en_uso: Option<bool>,
    notas: Option<String>,
}

fn campos_impresora(d: &DatosImpresora) -> Result<Vec<(&'static str, Valor)>, ApiError> {
    let mut c = Vec::new();
    texto(&mut c, "nombre", "El nombre", &d.nombre, 120, true)?;
    if let Some(t) = &d.tecnologia {
        if t != "resina" && t != "fdm" {
            return Err(ApiError::Invalido(format!("Tecnología desconocida: {t}")));
        }
        c.push(("tecnologia", Valor::Texto(t.clone())));
    }
    texto(&mut c, "marca", "La marca", &d.marca, 120, false)?;
    texto(&mut c, "modelo", "El modelo", &d.modelo, 120, false)?;
    real(
        &mut c,
        "volumen_x_mm",
        "El ancho",
        d.volumen_x_mm,
        1.0,
        2000.0,
    )?;
    real(
        &mut c,
        "volumen_y_mm",
        "El fondo",
        d.volumen_y_mm,
        1.0,
        2000.0,
    )?;
    real(
        &mut c,
        "volumen_z_mm",
        "El alto",
        d.volumen_z_mm,
        1.0,
        2000.0,
    )?;
    real(
        &mut c,
        "boquilla_mm",
        "La boquilla",
        d.boquilla_mm,
        0.1,
        2.0,
    )?;
    entero(&mut c, "res_x", "La resolución", d.res_x, 1, 100_000)?;
    entero(&mut c, "res_y", "La resolución", d.res_y, 1, 100_000)?;
    real(
        &mut c,
        "pixel_um",
        "El tamaño de pixel",
        d.pixel_um,
        1.0,
        1000.0,
    )?;
    real(
        &mut c,
        "potencia_w",
        "La potencia",
        d.potencia_w,
        1.0,
        5000.0,
    )?;
    if let Some(s) = &d.carpeta {
        c.push(("carpeta", Valor::Texto(carpeta_valida(s)?)));
    }
    if let Some(b) = d.en_uso {
        c.push(("en_uso", Valor::Logico(b)));
    }
    texto(&mut c, "notas", "Las notas", &d.notas, 5000, false)?;
    Ok(c)
}

async fn crear_impresora(
    _u: Usuario,
    State(st): State<AppState>,
    Json(d): Json<DatosImpresora>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    if d.nombre.as_deref().is_none_or(|n| n.trim().is_empty()) {
        return Err(ApiError::Invalido("Falta el nombre".into()));
    }
    if d.tecnologia.is_none() {
        return Err(ApiError::Invalido(
            "Falta la tecnología (resina o FDM)".into(),
        ));
    }
    if d.perfil_id.is_some() {
        return Err(ApiError::Invalido(
            "Una impresora nueva todavía no tiene perfiles".into(),
        ));
    }
    let c = campos_impresora(&d)?;
    let t = ahora();
    let mut tx = st.pool.begin().await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO impresoras (nombre, tecnologia, creada, actualizada) VALUES ('', 'resina', ?, ?) RETURNING id",
    )
    .bind(t)
    .bind(t)
    .fetch_one(&mut *tx)
    .await?;
    actualizar(&mut tx, "impresoras", "actualizada", id, c).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

async fn impresora(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    let fila: FilaImpresora = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{} WHERE i.id = ?",
        select_impresoras()
    )))
    .bind(id)
    .fetch_optional(p)
    .await?
    .ok_or(ApiError::NoEncontrado)?;
    let perfiles = lista_perfiles(p, Some(id), None).await?;
    // manuales, mejoras y calibraciones: lo que hay en su carpeta
    let documentos: Vec<Documento> = if fila.carpeta.is_empty() {
        Vec::new()
    } else {
        let patron = format!(
            "{}/%",
            fila.carpeta
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        sqlx::query_as(
            "SELECT a.id, a.ruta, a.carpeta, a.nombre, a.ext, a.tipo, a.bytes, a.mtime_ms, mi.clave AS miniatura
             FROM archivos a JOIN bibliotecas b ON b.id = a.biblioteca_id AND b.tipo = 'principal'
             LEFT JOIN miniaturas mi ON mi.archivo_id = a.id AND mi.estado = 'ok'
             WHERE a.estado = 'presente' AND (a.carpeta = ? OR a.carpeta LIKE ? ESCAPE '\\')
             ORDER BY a.carpeta, a.nombre LIMIT 1000",
        )
        .bind(&fila.carpeta)
        .bind(patron)
        .fetch_all(p)
        .await?
    };
    let mut v = como_json(&fila);
    v["perfiles"] = json!(perfiles);
    v["documentos"] = json!(documentos);
    Ok(Json(v))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct Documento {
    id: i64,
    ruta: String,
    carpeta: String,
    nombre: String,
    ext: String,
    tipo: String,
    bytes: i64,
    mtime_ms: i64,
    miniatura: Option<String>,
}

async fn editar_impresora(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(d): Json<DatosImpresora>,
) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    let actual: Option<String> =
        sqlx::query_scalar("SELECT tecnologia FROM impresoras WHERE id = ?")
            .bind(id)
            .fetch_optional(p)
            .await?;
    let actual = actual.ok_or(ApiError::NoEncontrado)?;
    let mut c = campos_impresora(&d)?;
    if let Some(t) = &d.tecnologia
        && *t != actual
        && cuenta(
            p,
            "SELECT count(*) FROM perfiles WHERE impresora_id = ?",
            id,
        )
        .await?
            > 0
    {
        return Err(ApiError::Conflicto(
            "No se puede cambiar la tecnología de una impresora con perfiles".into(),
        ));
    }
    if let Some(perfil) = d.perfil_id {
        if let Some(pf) = perfil {
            let suyo: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM perfiles WHERE id = ? AND impresora_id = ?",
            )
            .bind(pf)
            .bind(id)
            .fetch_one(p)
            .await?;
            if suyo == 0 {
                return Err(ApiError::Invalido(
                    "Ese perfil no es de esta impresora".into(),
                ));
            }
        }
        c.push(("perfil_id", Valor::Entero(perfil)));
    }
    let mut tx = p.begin().await?;
    actualizar(&mut tx, "impresoras", "actualizada", id, c).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

async fn borrar_impresora(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    let n = cuenta(
        p,
        "SELECT count(*) FROM perfiles WHERE impresora_id = ?",
        id,
    )
    .await?;
    if n > 0 {
        return Err(ApiError::Conflicto(format!(
            "Tiene {n} perfiles: bórralos primero, o márcala como que ya no la usas"
        )));
    }
    let r = sqlx::query("DELETE FROM impresoras WHERE id = ?")
        .bind(id)
        .execute(p)
        .await?;
    if r.rows_affected() == 0 {
        return Err(ApiError::NoEncontrado);
    }
    Ok(Json(json!({ "ok": true })))
}

// ---------- materiales ----------

#[derive(Debug, Serialize, sqlx::FromRow)]
struct FilaMaterial {
    id: i64,
    tipo: String,
    marca: String,
    nombre: String,
    subtipo: String,
    color: String,
    color_hex: String,
    presentacion: Option<f64>,
    precio: Option<f64>,
    precio_unidad: Option<f64>,
    densidad: Option<f64>,
    diametro_mm: Option<f64>,
    queda: Option<f64>,
    en_uso: bool,
    notas: String,
    origen: Option<String>,
    n_perfiles: i64,
    n_placas: i64,
}

async fn materiales(_u: Usuario, State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let filas: Vec<FilaMaterial> = sqlx::query_as(
        "SELECT m.id, m.tipo, m.marca, m.nombre, m.subtipo, m.color, m.color_hex, m.presentacion, m.precio,
                CASE WHEN m.presentacion > 0 THEN m.precio / m.presentacion END AS precio_unidad,
                m.densidad, m.diametro_mm, m.queda, m.en_uso, m.notas, m.clave_origen AS origen,
                (SELECT count(*) FROM perfiles pf WHERE pf.material_id = m.id) AS n_perfiles,
                (SELECT count(*) FROM placas p WHERE p.material_id = m.id) AS n_placas
         FROM materiales m ORDER BY m.en_uso DESC, m.tipo DESC, m.marca, m.nombre",
    )
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(json!(filas)))
}

#[derive(Debug, Default, Deserialize)]
struct DatosMaterial {
    tipo: Option<String>,
    marca: Option<String>,
    nombre: Option<String>,
    subtipo: Option<String>,
    color: Option<String>,
    color_hex: Option<String>,
    #[serde(default, deserialize_with = "doble_f")]
    presentacion: Option<Option<f64>>,
    #[serde(default, deserialize_with = "doble_f")]
    precio: Option<Option<f64>>,
    #[serde(default, deserialize_with = "doble_f")]
    densidad: Option<Option<f64>>,
    #[serde(default, deserialize_with = "doble_f")]
    diametro_mm: Option<Option<f64>>,
    #[serde(default, deserialize_with = "doble_f")]
    queda: Option<Option<f64>>,
    en_uso: Option<bool>,
    notas: Option<String>,
}

fn campos_material(d: &DatosMaterial) -> Result<Vec<(&'static str, Valor)>, ApiError> {
    let mut c = Vec::new();
    if let Some(t) = &d.tipo {
        if t != "resina" && t != "filamento" {
            return Err(ApiError::Invalido(format!(
                "Tipo de material desconocido: {t}"
            )));
        }
        c.push(("tipo", Valor::Texto(t.clone())));
    }
    texto(&mut c, "marca", "La marca", &d.marca, 120, false)?;
    texto(&mut c, "nombre", "El nombre", &d.nombre, 120, true)?;
    texto(&mut c, "subtipo", "El tipo", &d.subtipo, 60, false)?;
    texto(&mut c, "color", "El color", &d.color, 60, false)?;
    if let Some(h) = &d.color_hex {
        let h = h.trim().to_lowercase();
        if !h.is_empty()
            && !(h.len() == 7
                && h.starts_with('#')
                && h[1..].chars().all(|c| c.is_ascii_hexdigit()))
        {
            return Err(ApiError::Invalido("El color debe ser como #a1b2c3".into()));
        }
        c.push(("color_hex", Valor::Texto(h)));
    }
    real(
        &mut c,
        "presentacion",
        "La presentación",
        d.presentacion,
        0.001,
        1_000_000.0,
    )?;
    real(&mut c, "precio", "El precio", d.precio, 0.0, 10_000_000.0)?;
    real(&mut c, "densidad", "La densidad", d.densidad, 0.3, 5.0)?;
    real(
        &mut c,
        "diametro_mm",
        "El diámetro",
        d.diametro_mm,
        0.5,
        5.0,
    )?;
    real(&mut c, "queda", "Lo que queda", d.queda, 0.0, 1_000_000.0)?;
    if let Some(b) = d.en_uso {
        c.push(("en_uso", Valor::Logico(b)));
    }
    texto(&mut c, "notas", "Las notas", &d.notas, 5000, false)?;
    Ok(c)
}

async fn crear_material(
    _u: Usuario,
    State(st): State<AppState>,
    Json(d): Json<DatosMaterial>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    if d.nombre.as_deref().is_none_or(|n| n.trim().is_empty()) {
        return Err(ApiError::Invalido("Falta el nombre".into()));
    }
    if d.tipo.is_none() {
        return Err(ApiError::Invalido(
            "Falta el tipo (resina o filamento)".into(),
        ));
    }
    let c = campos_material(&d)?;
    let t = ahora();
    let mut tx = st.pool.begin().await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO materiales (tipo, nombre, creado, actualizado) VALUES ('resina', '', ?, ?) RETURNING id",
    )
    .bind(t)
    .bind(t)
    .fetch_one(&mut *tx)
    .await?;
    actualizar(&mut tx, "materiales", "actualizado", id, c).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

async fn editar_material(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(d): Json<DatosMaterial>,
) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    let actual: Option<String> = sqlx::query_scalar("SELECT tipo FROM materiales WHERE id = ?")
        .bind(id)
        .fetch_optional(p)
        .await?;
    let actual = actual.ok_or(ApiError::NoEncontrado)?;
    let c = campos_material(&d)?;
    if let Some(t) = &d.tipo
        && *t != actual
        && cuenta(
            p,
            "SELECT (SELECT count(*) FROM perfiles WHERE material_id = ?1)
                  + (SELECT count(*) FROM placas WHERE material_id = ?1)",
            id,
        )
        .await?
            > 0
    {
        return Err(ApiError::Conflicto(
            "No se puede cambiar el tipo de un material que usan perfiles o placas".into(),
        ));
    }
    let mut tx = p.begin().await?;
    actualizar(&mut tx, "materiales", "actualizado", id, c).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

async fn borrar_material(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    let n = cuenta(
        p,
        "SELECT (SELECT count(*) FROM perfiles WHERE material_id = ?1)
              + (SELECT count(*) FROM placas WHERE material_id = ?1)",
        id,
    )
    .await?;
    if n > 0 {
        return Err(ApiError::Conflicto(
            "Lo usan perfiles o placas: márcalo como que ya no lo usas".into(),
        ));
    }
    let r = sqlx::query("DELETE FROM materiales WHERE id = ?")
        .bind(id)
        .execute(p)
        .await?;
    if r.rows_affected() == 0 {
        return Err(ApiError::NoEncontrado);
    }
    Ok(Json(json!({ "ok": true })))
}

// ---------- perfiles ----------

#[derive(Debug, Serialize, sqlx::FromRow)]
struct FilaPerfil {
    id: i64,
    nombre: String,
    impresora_id: i64,
    impresora: String,
    tecnologia: String,
    material_id: Option<i64>,
    material: Option<String>,
    capa_mm: Option<f64>,
    #[serde(skip)]
    parametros: String,
    version: i64,
    en_uso: bool,
    predeterminado: bool,
    notas: String,
    origen: String,
    n_archivos: i64,
    creado: i64,
    actualizado: i64,
}

fn perfil_json(f: &FilaPerfil) -> Value {
    let mut v = como_json(f);
    v["parametros"] = serde_json::from_str(&f.parametros).unwrap_or(json!({}));
    v
}

const SELECT_PERFIL: &str = "SELECT pf.id, pf.nombre, pf.impresora_id, i.nombre AS impresora, i.tecnologia,
        pf.material_id, trim(m.marca || ' ' || m.nombre) AS material, pf.capa_mm, pf.parametros, pf.version,
        pf.en_uso, coalesce(i.perfil_id = pf.id, 0) AS predeterminado, pf.notas, pf.origen,
        (SELECT count(*) FROM perfil_archivos x WHERE x.perfil_id = pf.id) AS n_archivos, pf.creado, pf.actualizado
    FROM perfiles pf JOIN impresoras i ON i.id = pf.impresora_id LEFT JOIN materiales m ON m.id = pf.material_id";

async fn lista_perfiles(
    pool: &SqlitePool,
    impresora: Option<i64>,
    material: Option<i64>,
) -> Result<Vec<Value>, sqlx::Error> {
    let filas: Vec<FilaPerfil> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SELECT_PERFIL} WHERE (?1 IS NULL OR pf.impresora_id = ?1) AND (?2 IS NULL OR pf.material_id = ?2)
         ORDER BY i.en_uso DESC, i.nombre, pf.en_uso DESC, pf.nombre"
    )))
    .bind(impresora)
    .bind(material)
    .fetch_all(pool)
    .await?;
    Ok(filas.iter().map(perfil_json).collect())
}

#[derive(Debug, Deserialize)]
struct FiltrosPerfiles {
    impresora_id: Option<i64>,
    material_id: Option<i64>,
}

async fn perfiles(
    _u: Usuario,
    State(st): State<AppState>,
    Query(f): Query<FiltrosPerfiles>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(
        lista_perfiles(&st.pool, f.impresora_id, f.material_id).await?
    )))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct ArchivoPerfil {
    id: i64,
    ruta: String,
    nombre: String,
    ext: String,
    tipo: String,
    bytes: i64,
    mtime_ms: i64,
    estado: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct VersionPerfil {
    version: i64,
    material: Option<String>,
    capa_mm: Option<f64>,
    #[serde(skip)]
    parametros: String,
    desde: i64,
    hasta: i64,
    usuario: String,
}

#[derive(Debug, sqlx::FromRow)]
struct LaminadoDe {
    placa_id: i64,
    placa: String,
    carpeta: String,
    elegido: bool,
    capa_mm: Option<f64>,
    exposicion_s: Option<f64>,
    exposicion_base_s: Option<f64>,
}

async fn perfil(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    let fila: FilaPerfil = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SELECT_PERFIL} WHERE pf.id = ?"
    )))
    .bind(id)
    .fetch_optional(p)
    .await?
    .ok_or(ApiError::NoEncontrado)?;
    let archivos: Vec<ArchivoPerfil> = sqlx::query_as(
        "SELECT a.id, a.ruta, a.nombre, a.ext, a.tipo, a.bytes, a.mtime_ms, a.estado
         FROM perfil_archivos x JOIN archivos a ON a.id = x.archivo_id WHERE x.perfil_id = ? ORDER BY a.ruta",
    )
    .bind(id)
    .fetch_all(p)
    .await?;
    let versiones: Vec<VersionPerfil> = sqlx::query_as(
        "SELECT v.version, trim(m.marca || ' ' || m.nombre) AS material, v.capa_mm, v.parametros, v.desde, v.hasta,
                v.usuario
         FROM perfil_versiones v LEFT JOIN materiales m ON m.id = v.material_id
         WHERE v.perfil_id = ? ORDER BY v.version DESC",
    )
    .bind(id)
    .fetch_all(p)
    .await?;
    // placas que se imprimen con este perfil: elegido a mano, o con un laminado de su impresora con la misma
    // capa y exposición
    let candidatos: Vec<LaminadoDe> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT p.id AS placa_id, p.nombre AS placa, p.carpeta, p.perfil_id IS ?1 AS elegido, d.capa_mm,
                d.exposicion_s, d.exposicion_base_s
         FROM placas p
         LEFT JOIN placa_archivos pa ON pa.placa_id = p.id AND pa.rol = 'laminado'
         LEFT JOIN datos_placa d ON d.archivo_id = pa.archivo_id
         LEFT JOIN impresoras i ON i.id = ?2
         WHERE p.perfil_id = ?1 OR (p.perfil_id IS NULL AND {SQL_MISMA_PANTALLA})"
    )))
    .bind(id)
    .bind(fila.impresora_id)
    .fetch_all(p)
    .await?;
    let taller = Taller::cargar(p).await?;
    let mut placas: Vec<Value> = Vec::new();
    for c in candidatos {
        let es = c.elegido
            || taller
                .perfil_de(
                    fila.impresora_id,
                    c.capa_mm,
                    c.exposicion_s,
                    c.exposicion_base_s,
                )
                .is_some_and(|x| x.id == id);
        if es && !placas.iter().any(|x| x["id"] == c.placa_id) {
            placas.push(
                json!({ "id": c.placa_id, "nombre": c.placa, "carpeta": c.carpeta,
                                "por": if c.elegido { "elegido" } else { "laminado" } }),
            );
        }
    }
    let mut v = perfil_json(&fila);
    v["archivos"] = json!(archivos);
    v["versiones"] = json!(
        versiones
            .iter()
            .map(|x| {
                let mut y = como_json(x);
                y["parametros"] = serde_json::from_str(&x.parametros).unwrap_or(json!({}));
                y
            })
            .collect::<Vec<_>>()
    );
    v["placas"] = json!(placas);
    Ok(Json(v))
}

/// impresora, material, capa, parámetros, versión y fecha de creación
type PerfilActual = (i64, Option<i64>, Option<f64>, String, i64, i64);

#[derive(Debug, Default, Deserialize)]
struct DatosPerfil {
    nombre: Option<String>,
    impresora_id: Option<i64>,
    #[serde(default, deserialize_with = "doble_i")]
    material_id: Option<Option<i64>>,
    #[serde(default, deserialize_with = "doble_f")]
    capa_mm: Option<Option<f64>>,
    parametros: Option<Map<String, Value>>,
    en_uso: Option<bool>,
    notas: Option<String>,
    /// el que usa normalmente su impresora
    predeterminado: Option<bool>,
}

async fn tecnologia_de(pool: &SqlitePool, impresora: i64) -> Result<String, ApiError> {
    let t: Option<String> = sqlx::query_scalar("SELECT tecnologia FROM impresoras WHERE id = ?")
        .bind(impresora)
        .fetch_optional(pool)
        .await?;
    t.ok_or_else(|| ApiError::Invalido("Esa impresora no existe".into()))
}

async fn revisar_material(
    pool: &SqlitePool,
    material: i64,
    tecnologia: &str,
) -> Result<(), ApiError> {
    let t: Option<String> = sqlx::query_scalar("SELECT tipo FROM materiales WHERE id = ?")
        .bind(material)
        .fetch_optional(pool)
        .await?;
    let t = t.ok_or_else(|| ApiError::Invalido("Ese material no existe".into()))?;
    let esperado = if tecnologia == "fdm" {
        "filamento"
    } else {
        "resina"
    };
    if t != esperado {
        return Err(ApiError::Invalido(format!(
            "Una impresora de {} usa {esperado}, no {t}",
            taller::nombre_tecnologia(tecnologia)
        )));
    }
    Ok(())
}

async fn poner_predeterminado(
    tx: &mut SqliteConnection,
    perfil: i64,
    impresora: i64,
    si: bool,
) -> Result<(), sqlx::Error> {
    if si {
        sqlx::query("UPDATE impresoras SET perfil_id = ? WHERE id = ?")
            .bind(perfil)
            .bind(impresora)
            .execute(&mut *tx)
            .await?;
    } else {
        sqlx::query("UPDATE impresoras SET perfil_id = NULL WHERE id = ? AND perfil_id = ?")
            .bind(impresora)
            .bind(perfil)
            .execute(&mut *tx)
            .await?;
    }
    Ok(())
}

async fn crear_perfil(
    _u: Usuario,
    State(st): State<AppState>,
    Json(d): Json<DatosPerfil>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let p = &st.pool;
    let impresora = d
        .impresora_id
        .ok_or_else(|| ApiError::Invalido("Falta la impresora".into()))?;
    let tecnologia = tecnologia_de(p, impresora).await?;
    if d.nombre.as_deref().is_none_or(|n| n.trim().is_empty()) {
        return Err(ApiError::Invalido("Falta el nombre".into()));
    }
    let mut c = Vec::new();
    texto(&mut c, "nombre", "El nombre", &d.nombre, 120, true)?;
    texto(&mut c, "notas", "Las notas", &d.notas, 5000, false)?;
    real(
        &mut c,
        "capa_mm",
        "La altura de capa",
        d.capa_mm,
        0.005,
        1.0,
    )?;
    if let Some(Some(m)) = d.material_id {
        revisar_material(p, m, &tecnologia).await?;
        c.push(("material_id", Valor::Entero(Some(m))));
    }
    let parametros =
        taller::validar_parametros(&tecnologia, &d.parametros.clone().unwrap_or_default())
            .map_err(ApiError::Invalido)?;
    c.push((
        "parametros",
        Valor::Texto(Value::Object(parametros).to_string()),
    ));
    if let Some(b) = d.en_uso {
        c.push(("en_uso", Valor::Logico(b)));
    }
    let t = ahora();
    let mut tx = p.begin().await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO perfiles (nombre, impresora_id, creado, actualizado) VALUES ('', ?, ?, ?) RETURNING id",
    )
    .bind(impresora)
    .bind(t)
    .bind(t)
    .fetch_one(&mut *tx)
    .await?;
    actualizar(&mut tx, "perfiles", "actualizado", id, c).await?;
    if let Some(si) = d.predeterminado {
        poner_predeterminado(&mut tx, id, impresora, si).await?;
    }
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

/// Edita un perfil. Si cambian sus valores (material, capa o parámetros), la versión anterior queda en el
/// historial y el número de versión sube.
async fn editar_perfil(
    u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(d): Json<DatosPerfil>,
) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    let actual: Option<PerfilActual> = sqlx::query_as(
        "SELECT impresora_id, material_id, capa_mm, parametros, version, creado FROM perfiles WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(p)
    .await?;
    let (imp_actual, mat_actual, capa_actual, par_actual, version, creado) =
        actual.ok_or(ApiError::NoEncontrado)?;
    let impresora = d.impresora_id.unwrap_or(imp_actual);
    let tecnologia = tecnologia_de(p, impresora).await?;
    let mut c = Vec::new();
    texto(&mut c, "nombre", "El nombre", &d.nombre, 120, true)?;
    texto(&mut c, "notas", "Las notas", &d.notas, 5000, false)?;
    real(
        &mut c,
        "capa_mm",
        "La altura de capa",
        d.capa_mm,
        0.005,
        1.0,
    )?;
    if let Some(b) = d.en_uso {
        c.push(("en_uso", Valor::Logico(b)));
    }
    let material = d.material_id.unwrap_or(mat_actual);
    if let Some(m) = material
        && (d.material_id.is_some() || impresora != imp_actual)
    {
        revisar_material(p, m, &tecnologia).await?;
    }
    if d.material_id.is_some() {
        c.push(("material_id", Valor::Entero(material)));
    }
    let par_viejos: Map<String, Value> = serde_json::from_str(&par_actual).unwrap_or_default();
    let parametros = match (&d.parametros, impresora != imp_actual) {
        (Some(nuevos), _) => Some(taller::validar_parametros(&tecnologia, nuevos)),
        // otra impresora: los valores de antes deben servirle
        (None, true) => Some(taller::validar_parametros(&tecnologia, &par_viejos)),
        (None, false) => None,
    }
    .transpose()
    .map_err(ApiError::Invalido)?;
    if let Some(pm) = &parametros {
        c.push((
            "parametros",
            Valor::Texto(Value::Object(pm.clone()).to_string()),
        ));
    }
    if impresora != imp_actual {
        c.push(("impresora_id", Valor::Entero(Some(impresora))));
    }
    let capa = d.capa_mm.unwrap_or(capa_actual);
    let cambia = material != mat_actual
        || capa != capa_actual
        || parametros.as_ref().is_some_and(|pm| *pm != par_viejos);
    let mut tx = p.begin().await?;
    if cambia {
        let desde: i64 = sqlx::query_scalar(
            "SELECT coalesce((SELECT max(hasta) FROM perfil_versiones WHERE perfil_id = ?), ?)",
        )
        .bind(id)
        .bind(creado)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO perfil_versiones (perfil_id, version, material_id, capa_mm, parametros, desde, hasta, usuario)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(version)
        .bind(mat_actual)
        .bind(capa_actual)
        .bind(&par_actual)
        .bind(desde)
        .bind(ahora())
        .bind(&u.nombre)
        .execute(&mut *tx)
        .await?;
        c.push(("version", Valor::Entero(Some(version + 1))));
    }
    if impresora != imp_actual {
        // deja de ser el normal de la impresora de antes
        poner_predeterminado(&mut tx, id, imp_actual, false).await?;
    }
    actualizar(&mut tx, "perfiles", "actualizado", id, c).await?;
    if let Some(si) = d.predeterminado {
        poner_predeterminado(&mut tx, id, impresora, si).await?;
    }
    tx.commit().await?;
    Ok(Json(
        json!({ "ok": true, "version": if cambia { version + 1 } else { version } }),
    ))
}

async fn borrar_perfil(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let r = sqlx::query("DELETE FROM perfiles WHERE id = ?")
        .bind(id)
        .execute(&st.pool)
        .await?;
    if r.rows_affected() == 0 {
        return Err(ApiError::NoEncontrado);
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Debug, Deserialize)]
struct PedidoArchivo {
    archivo_id: i64,
}

async fn ligar_archivo(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(pa): Json<PedidoArchivo>,
) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    if cuenta(p, "SELECT count(*) FROM perfiles WHERE id = ?", id).await? == 0 {
        return Err(ApiError::NoEncontrado);
    }
    if cuenta(
        p,
        "SELECT count(*) FROM archivos WHERE id = ?",
        pa.archivo_id,
    )
    .await?
        == 0
    {
        return Err(ApiError::Invalido("Ese archivo no existe".into()));
    }
    sqlx::query("INSERT OR IGNORE INTO perfil_archivos (perfil_id, archivo_id) VALUES (?, ?)")
        .bind(id)
        .bind(pa.archivo_id)
        .execute(p)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

async fn desligar_archivo(
    _u: Usuario,
    State(st): State<AppState>,
    Path((id, archivo)): Path<(i64, i64)>,
) -> Result<Json<Value>, ApiError> {
    sqlx::query("DELETE FROM perfil_archivos WHERE perfil_id = ? AND archivo_id = ?")
        .bind(id)
        .bind(archivo)
        .execute(&st.pool)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Debug, Deserialize)]
struct Busqueda {
    q: Option<String>,
    tipo: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct Encontrado {
    id: i64,
    ruta: String,
    nombre: String,
    ext: String,
    tipo: String,
    bytes: i64,
}

/// Busca archivos de la biblioteca por palabras de su ruta (para ligarlos a un perfil).
async fn buscar_archivos(
    _u: Usuario,
    State(st): State<AppState>,
    Query(b): Query<Busqueda>,
) -> Result<Json<Value>, ApiError> {
    let palabras: Vec<String> =
        b.q.unwrap_or_default()
            .split_whitespace()
            .map(|p| format!("%{}%", p.replace(['%', '_'], "")))
            .filter(|p| p.len() > 2)
            .take(6)
            .collect();
    if palabras.is_empty() {
        return Ok(Json(json!([])));
    }
    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT a.id, a.ruta, a.nombre, a.ext, a.tipo, a.bytes FROM archivos a WHERE a.estado = 'presente'",
    );
    for p in palabras {
        qb.push(" AND a.ruta LIKE ").push_bind(p);
    }
    if let Some(t) = b.tipo.as_deref().filter(|t| !t.is_empty()) {
        qb.push(" AND a.tipo = ").push_bind(t.to_string());
    }
    qb.push(" ORDER BY a.tipo = 'perfil' DESC, a.ruta LIMIT 30");
    let filas: Vec<Encontrado> = qb.build_query_as().fetch_all(&st.pool).await?;
    Ok(Json(json!(filas)))
}

// ---------- resumen e importación ----------

async fn resumen(_u: Usuario, State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    let (impresoras, materiales, perfiles, perfiles_en_uso): (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM impresoras), (SELECT count(*) FROM materiales),
                (SELECT count(*) FROM perfiles), (SELECT count(*) FROM perfiles WHERE en_uso = 1)",
    )
    .fetch_one(p)
    .await?;
    let sin: Vec<(Option<String>, i64, i64, i64)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT max(nullif(nullif(d.maquina, ''), 'Custom')), d.res_x, d.res_y, count(*) FROM archivos a
         JOIN datos_placa d ON d.archivo_id = a.id
         WHERE a.estado = 'presente' AND a.tipo = 'laminado' AND d.res_x IS NOT NULL AND d.res_y IS NOT NULL
           AND NOT EXISTS (SELECT 1 FROM impresoras i WHERE {SQL_MISMA_PANTALLA})
         GROUP BY min(d.res_x, d.res_y), max(d.res_x, d.res_y) ORDER BY count(*) DESC"
    )))
    .fetch_all(p)
    .await?;
    Ok(Json(json!({
        "impresoras": impresoras,
        "materiales": materiales,
        "perfiles": perfiles,
        "perfiles_en_uso": perfiles_en_uso,
        "laminados_sin_impresora": sin.iter().map(|(m, x, y, n)| json!({
            "maquina": m, "res_x": x, "res_y": y, "laminados": n
        })).collect::<Vec<_>>(),
    })))
}

async fn importables(_u: Usuario, State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(taller::propuestas(&st.pool).await?)))
}

#[derive(Debug, Deserialize)]
struct PedidoImportar {
    claves: Vec<String>,
}

async fn importar(
    _u: Usuario,
    State(st): State<AppState>,
    Json(pi): Json<PedidoImportar>,
) -> Result<Json<Value>, ApiError> {
    if pi.claves.is_empty() {
        return Err(ApiError::Invalido("No elegiste nada para importar".into()));
    }
    let r = taller::importar(&st.pool, &pi.claves).await?;
    Ok(Json(json!(r)))
}
