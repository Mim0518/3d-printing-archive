//! Almacenamiento frío (fase 8): ubicaciones, estado de cada modelo, mandar y traer, quitar la copia local y la
//! papelera de la app. Exigen sesión. Lo que mueve o sube archivos corre en segundo plano (un trabajo a la vez).

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    AppState,
    almacen::{self, MARCA},
    auth::Usuario,
    error::ApiError,
    trabajos,
    transferencias::{self, FRASE_VACIAR, Ubicacion},
};

pub fn rutas() -> Router<AppState> {
    Router::new()
        .route("/frio/resumen", get(resumen))
        .route("/frio/ubicaciones", get(ubicaciones).post(crear_ubicacion))
        .route(
            "/frio/ubicaciones/{id}",
            axum::routing::patch(editar_ubicacion),
        )
        .route("/frio/ubicaciones/{id}/probar", post(probar))
        .route("/frio/ubicaciones/{id}/listar", post(listar))
        .route("/frio/modelos/{id}", get(estado_modelo))
        .route("/frio/modelos/{id}/plan", get(plan))
        .route("/frio/modelos/{id}/subir", post(subir))
        .route("/frio/modelos/{id}/quitar", post(quitar))
        .route("/frio/modelos/{id}/traer", post(traer))
        .route("/frio/transferencias", get(lista_transferencias))
        .route("/papelera", get(papelera))
        .route("/papelera/{id}", get(lote))
        .route("/papelera/{id}/restaurar", post(restaurar))
        .route("/papelera/{id}/vaciar", post(vaciar))
}

fn como_json<T: Serialize>(x: &T) -> Value {
    serde_json::to_value(x).unwrap_or(Value::Null)
}

async fn ubicacion(st: &AppState, id: i64) -> Result<Ubicacion, ApiError> {
    Ubicacion::cargar(&st.pool, id)
        .await?
        .ok_or(ApiError::NoEncontrado)
}

async fn modelo_existe(st: &AppState, id: i64) -> Result<String, ApiError> {
    let x: Option<(String, String, String)> =
        sqlx::query_as("SELECT nombre, faccion, carpeta FROM modelos WHERE id = ?")
            .bind(id)
            .fetch_optional(&st.pool)
            .await?;
    let (n, f, c) = x.ok_or(ApiError::NoEncontrado)?;
    Ok(if !n.is_empty() {
        n
    } else if !f.is_empty() {
        f
    } else {
        c
    })
}

// ---------- ubicaciones ----------

#[derive(Debug, Serialize, sqlx::FromRow)]
struct FilaUbicacion {
    id: i64,
    nombre: String,
    tipo: String,
    remoto: String,
    ruta_base: String,
    escribible: bool,
    predeterminada: bool,
    ultimo_listado: Option<i64>,
    copias: i64,
    bytes: i64,
    verificadas: i64,
    faltan: i64,
    modelos: i64,
}

const SELECT_UBICACION: &str = "SELECT u.id, u.nombre, u.tipo, u.remoto, u.ruta_base, u.escribible, u.predeterminada,
        u.ultimo_listado,
        (SELECT count(*) FROM copias_frio c WHERE c.ubicacion_id = u.id AND c.estado = 'presente') AS copias,
        (SELECT coalesce(sum(c.bytes), 0) FROM copias_frio c WHERE c.ubicacion_id = u.id AND c.estado = 'presente')
            AS bytes,
        (SELECT count(*) FROM copias_frio c WHERE c.ubicacion_id = u.id AND c.estado = 'presente'
           AND c.verificado IS NOT NULL) AS verificadas,
        (SELECT count(*) FROM copias_frio c WHERE c.ubicacion_id = u.id AND c.estado = 'falta') AS faltan,
        (SELECT count(DISTINCT c.modelo_id) FROM copias_frio c WHERE c.ubicacion_id = u.id AND c.estado = 'presente')
            AS modelos
    FROM ubicaciones_frio u";

async fn ubicaciones(_u: Usuario, State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let filas: Vec<FilaUbicacion> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SELECT_UBICACION} ORDER BY u.predeterminada DESC, u.nombre"
    )))
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(json!(filas)))
}

#[derive(Debug, Deserialize)]
struct NuevaUbicacion {
    nombre: String,
    /// drive | disco_externo
    tipo: String,
    remoto: Option<String>,
    ruta_base: Option<String>,
    escribible: Option<bool>,
    /// disco externo sin marca: se permite escribir el archivo marca en su raíz
    #[serde(default)]
    crear_marca: bool,
}

async fn crear_ubicacion(
    _u: Usuario,
    State(st): State<AppState>,
    Json(n): Json<NuevaUbicacion>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let nombre = n.nombre.trim().to_string();
    if nombre.is_empty() || nombre.chars().count() > 80 {
        return Err(ApiError::Invalido("El nombre va de 1 a 80 letras".into()));
    }
    let (remoto, ruta_base, marca) = match n.tipo.as_str() {
        "drive" => {
            let r = n.remoto.as_deref().unwrap_or("").trim().to_string();
            if !almacen::remoto_valido(&r) {
                return Err(ApiError::Invalido(
                    "El remoto va como «Nombre:carpeta», con un remoto de rclone ya configurado"
                        .into(),
                ));
            }
            (r, String::new(), String::new())
        }
        "disco_externo" => {
            let base = n
                .ruta_base
                .as_deref()
                .unwrap_or("")
                .trim()
                .trim_end_matches('/')
                .to_string();
            let p = std::path::Path::new(&base);
            if !p.is_absolute() || !p.is_dir() {
                return Err(ApiError::Invalido(format!(
                    "«{base}» no es una carpeta del mini PC (¿está conectado el disco?)"
                )));
            }
            let archivo = p.join(MARCA);
            let marca = match std::fs::read_to_string(&archivo) {
                Ok(m) if !m.trim().is_empty() => m.trim().to_string(),
                _ => {
                    if !n.crear_marca {
                        return Err(ApiError::Conflicto(format!(
                            "Para reconocer este disco se escribe un archivo {MARCA} en {base}: confírmalo"
                        )));
                    }
                    let m = hex::encode(rand::random::<[u8; 12]>());
                    std::fs::write(&archivo, &m).map_err(|e| {
                        ApiError::Invalido(format!("No se pudo escribir en el disco: {e}"))
                    })?;
                    m
                }
            };
            (String::new(), base, marca)
        }
        t => return Err(ApiError::Invalido(format!("Tipo desconocido: {t}"))),
    };
    let r = sqlx::query_scalar::<_, i64>(
        "INSERT INTO ubicaciones_frio (nombre, tipo, remoto, ruta_base, marca, escribible) VALUES (?, ?, ?, ?, ?, ?)
         RETURNING id",
    )
    .bind(&nombre)
    .bind(&n.tipo)
    .bind(&remoto)
    .bind(&ruta_base)
    .bind(&marca)
    .bind(n.escribible.unwrap_or(true))
    .fetch_one(&st.pool)
    .await;
    match r {
        Ok(id) => Ok((StatusCode::CREATED, Json(json!({ "id": id })))),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Err(ApiError::Conflicto(
            "Ya hay una ubicación con ese nombre".into(),
        )),
        Err(e) => Err(e.into()),
    }
}

#[derive(Debug, Deserialize)]
struct CambiosUbicacion {
    nombre: Option<String>,
    escribible: Option<bool>,
    predeterminada: Option<bool>,
}

async fn editar_ubicacion(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(c): Json<CambiosUbicacion>,
) -> Result<Json<Value>, ApiError> {
    ubicacion(&st, id).await?;
    let mut tx = st.pool.begin().await?;
    if let Some(n) = &c.nombre {
        let n = n.trim();
        if n.is_empty() || n.chars().count() > 80 {
            return Err(ApiError::Invalido("El nombre va de 1 a 80 letras".into()));
        }
        sqlx::query("UPDATE ubicaciones_frio SET nombre = ? WHERE id = ?")
            .bind(n)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(e) = c.escribible {
        sqlx::query("UPDATE ubicaciones_frio SET escribible = ? WHERE id = ?")
            .bind(e)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if c.predeterminada == Some(true) {
        sqlx::query("UPDATE ubicaciones_frio SET predeterminada = (id = ?)")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

async fn probar(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let u = ubicacion(&st, id).await?;
    let rclone = st.config.rclone.clone();
    let r = tokio::task::spawn_blocking(move || {
        let a = u.almacen(&rclone).map_err(|e| e.to_string())?;
        a.probar(&u.marca)
    })
    .await
    .map_err(|e| ApiError::Interno(e.into()))?;
    Ok(Json(match r {
        Ok(()) => json!({ "ok": true }),
        Err(e) => json!({ "ok": false, "error": e }),
    }))
}

async fn listar(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let u = ubicacion(&st, id).await?;
    let t = trabajos::iniciar_listar(
        st.pool.clone(),
        st.trabajos.clone(),
        st.config.rclone.clone(),
        id,
        format!("listar {}", u.nombre),
    )
    .await?;
    Ok((StatusCode::ACCEPTED, Json(json!({ "trabajo_id": t }))))
}

// ---------- estado de un modelo ----------

#[derive(Debug, Serialize, sqlx::FromRow)]
struct CopiaModelo {
    id: i64,
    ubicacion_id: i64,
    ruta: String,
    bytes: i64,
    verificado: Option<i64>,
    estado: String,
    origen: String,
    en_disco: bool,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct FilaTransferencia {
    id: i64,
    modelo_id: Option<i64>,
    modelo: Option<String>,
    ubicacion_id: i64,
    ubicacion: String,
    direccion: String,
    estado: String,
    quitar_local: bool,
    archivos: i64,
    bytes: i64,
    verificados: i64,
    lote_id: Option<i64>,
    trabajo_id: Option<i64>,
    #[serde(skip)]
    resultado: String,
    usuario: String,
    creada: i64,
    terminada: Option<i64>,
}

fn transferencia_json(t: &FilaTransferencia) -> Value {
    let mut v = como_json(t);
    v["resultado"] = serde_json::from_str(&t.resultado).unwrap_or(Value::Null);
    v
}

const SELECT_TRANSFERENCIA: &str = "SELECT t.id, t.modelo_id, coalesce(nullif(m.nombre, ''), m.faccion) AS modelo,
        t.ubicacion_id, u.nombre AS ubicacion, t.direccion, t.estado, t.quitar_local, t.archivos, t.bytes,
        t.verificados, t.lote_id, t.trabajo_id, t.resultado, t.usuario, t.creada, t.terminada
    FROM transferencias t JOIN ubicaciones_frio u ON u.id = t.ubicacion_id LEFT JOIN modelos m ON m.id = t.modelo_id";

/// Dónde está cada cosa de un modelo: en el disco, en cada ubicación, y qué se puede hacer.
async fn estado_modelo(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    modelo_existe(&st, id).await?;
    let p = &st.pool;
    let (archivos, bytes, compartidos): (i64, i64, i64) = sqlx::query_as(
        "SELECT count(*), coalesce(sum(bytes), 0), coalesce(sum(nlink > 1), 0) FROM archivos
         WHERE modelo_id = ? AND estado = 'presente'",
    )
    .bind(id)
    .fetch_one(p)
    .await?;
    let copias: Vec<CopiaModelo> = sqlx::query_as(
        "SELECT c.id, c.ubicacion_id, c.ruta, c.bytes, c.verificado, c.estado, c.origen,
                EXISTS (SELECT 1 FROM archivos a WHERE a.estado = 'presente' AND a.ruta = c.ruta) AS en_disco
         FROM copias_frio c WHERE c.modelo_id = ? ORDER BY c.ubicacion_id, c.ruta LIMIT 2000",
    )
    .bind(id)
    .fetch_all(p)
    .await?;
    let ubis: Vec<FilaUbicacion> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SELECT_UBICACION} ORDER BY u.predeterminada DESC, u.nombre"
    )))
    .fetch_all(p)
    .await?;
    let por_ubicacion: Vec<Value> = ubis
        .iter()
        .filter_map(|u| {
            let suyas: Vec<&CopiaModelo> = copias
                .iter()
                .filter(|c| c.ubicacion_id == u.id && c.estado == "presente")
                .collect();
            (!suyas.is_empty()).then(|| {
                json!({
                    "ubicacion_id": u.id, "ubicacion": u.nombre, "tipo": u.tipo,
                    "copias": suyas.len(),
                    "bytes": suyas.iter().map(|c| c.bytes).sum::<i64>(),
                    "verificadas": suyas.iter().filter(|c| c.verificado.is_some()).count(),
                    "comprimidos": suyas.iter().filter(|c| {
                        let r = c.ruta.to_lowercase();
                        [".zip", ".rar", ".7z"].iter().any(|e| r.ends_with(e))
                    }).count(),
                    "solo_en_frio": suyas.iter().filter(|c| !c.en_disco).count(),
                })
            })
        })
        .collect();
    let en_frio = !por_ubicacion.is_empty();
    let estado = match (archivos > 0, en_frio) {
        (true, true) => {
            if copias.iter().any(|c| c.estado == "presente" && !c.en_disco) {
                "parcial"
            } else {
                "ambos"
            }
        }
        (true, false) => "caliente",
        (false, true) => "frio",
        (false, false) => "sin_archivos",
    };
    let trans: Vec<FilaTransferencia> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SELECT_TRANSFERENCIA} WHERE t.modelo_id = ? ORDER BY t.id DESC LIMIT 10"
    )))
    .bind(id)
    .fetch_all(p)
    .await?;
    let lotes: Vec<(i64, String, i64, i64, i64)> = sqlx::query_as(
        "SELECT id, motivo, archivos, bytes, creado FROM papelera_lotes
         WHERE modelo_id = ? AND estado = 'en_papelera' ORDER BY id DESC",
    )
    .bind(id)
    .fetch_all(p)
    .await?;
    Ok(Json(json!({
        "estado": estado,
        "disco": { "archivos": archivos, "bytes": bytes, "compartidos": compartidos },
        "ubicaciones": por_ubicacion,
        "copias": copias,
        "transferencias": trans.iter().map(transferencia_json).collect::<Vec<_>>(),
        "papelera": lotes.iter().map(|(i, m, a, b, c)| json!({
            "id": i, "motivo": m, "archivos": a, "bytes": b, "creado": c
        })).collect::<Vec<_>>(),
        "destinos": ubis.iter().filter(|u| u.escribible).map(|u| json!({
            "id": u.id, "nombre": u.nombre, "predeterminada": u.predeterminada
        })).collect::<Vec<_>>(),
    })))
}

#[derive(Debug, Deserialize)]
struct ConUbicacion {
    ubicacion_id: Option<i64>,
}

async fn destino(st: &AppState, id: Option<i64>) -> Result<Ubicacion, ApiError> {
    let id = match id {
        Some(i) => i,
        None => {
            sqlx::query_scalar("SELECT id FROM ubicaciones_frio WHERE predeterminada = 1 LIMIT 1")
                .fetch_optional(&st.pool)
                .await?
                .ok_or_else(|| {
                    ApiError::Invalido("No hay una ubicación predeterminada: elige una".into())
                })?
        }
    };
    ubicacion(st, id).await
}

async fn plan(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Query(q): Query<ConUbicacion>,
) -> Result<Json<Value>, ApiError> {
    modelo_existe(&st, id).await?;
    let u = destino(&st, q.ubicacion_id).await?;
    Ok(Json(como_json(
        &transferencias::plan_subir(&st.pool, id, &u).await?,
    )))
}

#[derive(Debug, Deserialize)]
struct PedidoSubir {
    ubicacion_id: Option<i64>,
    #[serde(default)]
    quitar_local: bool,
}

async fn subir(
    usuario: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(p): Json<PedidoSubir>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let nombre = modelo_existe(&st, id).await?;
    let u = destino(&st, p.ubicacion_id).await?;
    if !u.escribible {
        return Err(ApiError::Invalido(format!(
            "«{}» no está marcada para recibir copias nuevas",
            u.nombre
        )));
    }
    let hay: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM archivos WHERE modelo_id = ? AND estado = 'presente'",
    )
    .bind(id)
    .fetch_one(&st.pool)
    .await?;
    if hay == 0 {
        return Err(ApiError::Invalido(
            "El modelo no tiene archivos en el disco".into(),
        ));
    }
    let t = transferencias::nueva_transferencia(
        &st.pool,
        id,
        u.id,
        "subir",
        p.quitar_local,
        &usuario.nombre,
    )
    .await?;
    let trabajo = trabajos::iniciar_subir(
        st.pool.clone(),
        st.trabajos.clone(),
        st.config.rclone.clone(),
        t,
        format!("{nombre} → {}", u.nombre),
    )
    .await;
    let trabajo = match trabajo {
        Ok(x) => x,
        Err(e) => {
            transferencias::marcar_fallo(&st.pool, t, "no se pudo empezar").await;
            return Err(e);
        }
    };
    sqlx::query("UPDATE transferencias SET trabajo_id = ? WHERE id = ?")
        .bind(trabajo)
        .bind(t)
        .execute(&st.pool)
        .await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({ "transferencia_id": t, "trabajo_id": trabajo })),
    ))
}

#[derive(Debug, Deserialize)]
struct PedidoQuitar {
    ubicacion_id: i64,
}

async fn quitar(
    usuario: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(p): Json<PedidoQuitar>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let nombre = modelo_existe(&st, id).await?;
    let u = ubicacion(&st, p.ubicacion_id).await?;
    let trabajo = trabajos::iniciar_quitar(
        st.pool.clone(),
        st.trabajos.clone(),
        st.config.rclone.clone(),
        id,
        u.id,
        usuario.nombre.clone(),
        format!("quitar {nombre} del disco (copia en {})", u.nombre),
    )
    .await?;
    Ok((StatusCode::ACCEPTED, Json(json!({ "trabajo_id": trabajo }))))
}

#[derive(Debug, Deserialize)]
struct PedidoTraer {
    ubicacion_id: i64,
    copia_ids: Option<Vec<i64>>,
}

async fn traer(
    usuario: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(p): Json<PedidoTraer>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let nombre = modelo_existe(&st, id).await?;
    let u = ubicacion(&st, p.ubicacion_id).await?;
    if p.copia_ids.as_ref().is_some_and(|c| c.is_empty()) {
        return Err(ApiError::Invalido("No elegiste nada para traer".into()));
    }
    let t =
        transferencias::nueva_transferencia(&st.pool, id, u.id, "traer", false, &usuario.nombre)
            .await?;
    let trabajo = trabajos::iniciar_traer(
        st.pool.clone(),
        st.trabajos.clone(),
        st.config.rclone.clone(),
        t,
        p.copia_ids.clone(),
        format!("{nombre} ← {}", u.nombre),
    )
    .await;
    let trabajo = match trabajo {
        Ok(x) => x,
        Err(e) => {
            transferencias::marcar_fallo(&st.pool, t, "no se pudo empezar").await;
            return Err(e);
        }
    };
    sqlx::query("UPDATE transferencias SET trabajo_id = ? WHERE id = ?")
        .bind(trabajo)
        .bind(t)
        .execute(&st.pool)
        .await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({ "transferencia_id": t, "trabajo_id": trabajo })),
    ))
}

#[derive(Debug, Default, Deserialize)]
struct FiltrosTransferencias {
    modelo_id: Option<i64>,
}

async fn lista_transferencias(
    _u: Usuario,
    State(st): State<AppState>,
    Query(f): Query<FiltrosTransferencias>,
) -> Result<Json<Value>, ApiError> {
    let filas: Vec<FilaTransferencia> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SELECT_TRANSFERENCIA} WHERE (?1 IS NULL OR t.modelo_id = ?1) ORDER BY t.id DESC LIMIT 200"
    )))
    .bind(f.modelo_id)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(json!(
        filas.iter().map(transferencia_json).collect::<Vec<_>>()
    )))
}

// ---------- resumen ----------

async fn resumen(_u: Usuario, State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let p = &st.pool;
    let (solo_frio, ambos): (i64, i64) = sqlx::query_as(
        "SELECT coalesce(sum(m.n_archivos_presentes = 0), 0), coalesce(sum(m.n_archivos_presentes > 0), 0) FROM (
            SELECT mo.id, (SELECT count(*) FROM archivos a WHERE a.modelo_id = mo.id AND a.estado = 'presente')
                   AS n_archivos_presentes
            FROM modelos mo
            WHERE EXISTS (SELECT 1 FROM copias_frio c WHERE c.modelo_id = mo.id AND c.estado = 'presente')
         ) m",
    )
    .fetch_one(p)
    .await?;
    let (lotes, archivos, bytes): (i64, i64, i64) = sqlx::query_as(
        "SELECT count(*), coalesce(sum(archivos), 0), coalesce(sum(bytes), 0) FROM papelera_lotes
         WHERE estado = 'en_papelera'",
    )
    .fetch_one(p)
    .await?;
    let en_curso: Option<(i64, String, String)> = sqlx::query_as(
        "SELECT id, tipo, detalle FROM trabajos WHERE estado = 'en_curso' AND tipo LIKE 'frio_%' LIMIT 1",
    )
    .fetch_optional(p)
    .await?;
    Ok(Json(json!({
        "modelos_solo_en_frio": solo_frio,
        "modelos_en_los_dos": ambos,
        "papelera": { "lotes": lotes, "archivos": archivos, "bytes": bytes },
        "en_curso": en_curso.map(|(i, t, d)| json!({ "id": i, "tipo": t, "detalle": d })),
        "frase_vaciar": FRASE_VACIAR,
    })))
}

// ---------- papelera ----------

#[derive(Debug, Serialize, sqlx::FromRow)]
struct FilaLote {
    id: i64,
    motivo: String,
    modelo_id: Option<i64>,
    modelo: Option<String>,
    ubicacion_id: Option<i64>,
    ubicacion: Option<String>,
    estado: String,
    archivos: i64,
    bytes: i64,
    compartidos: i64,
    usuario: String,
    creado: i64,
    cerrado: Option<i64>,
}

const SELECT_LOTE: &str = "SELECT l.id, l.motivo, l.modelo_id, coalesce(nullif(m.nombre, ''), m.faccion) AS modelo,
        l.ubicacion_id, u.nombre AS ubicacion, l.estado, l.archivos, l.bytes,
        (SELECT coalesce(sum(pa.bytes), 0) FROM papelera_archivos pa WHERE pa.lote_id = l.id AND pa.nlink > 1)
            AS compartidos,
        l.usuario, l.creado, l.cerrado
    FROM papelera_lotes l LEFT JOIN modelos m ON m.id = l.modelo_id LEFT JOIN ubicaciones_frio u ON u.id = l.ubicacion_id";

#[derive(Debug, Default, Deserialize)]
struct FiltrosPapelera {
    /// en_papelera (por omisión) | todos
    ver: Option<String>,
}

async fn papelera(
    _u: Usuario,
    State(st): State<AppState>,
    Query(f): Query<FiltrosPapelera>,
) -> Result<Json<Value>, ApiError> {
    let todos = f.ver.as_deref() == Some("todos");
    let filas: Vec<FilaLote> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SELECT_LOTE} WHERE ?1 OR l.estado = 'en_papelera' ORDER BY l.id DESC LIMIT 300"
    )))
    .bind(todos)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        json!({ "lotes": filas, "frase_vaciar": FRASE_VACIAR }),
    ))
}

async fn lote(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let fila: FilaLote =
        sqlx::query_as(sqlx::AssertSqlSafe(format!("{SELECT_LOTE} WHERE l.id = ?")))
            .bind(id)
            .fetch_optional(&st.pool)
            .await?
            .ok_or(ApiError::NoEncontrado)?;
    let archivos: Vec<(String, i64, String, i64, String)> = sqlx::query_as(
        "SELECT ruta, bytes, md5, nlink, estado FROM papelera_archivos WHERE lote_id = ? ORDER BY ruta LIMIT 5000",
    )
    .bind(id)
    .fetch_all(&st.pool)
    .await?;
    let mut v = como_json(&fila);
    v["lista"] = json!(
        archivos
            .iter()
            .map(|(r, b, m, n, e)| json!({ "ruta": r, "bytes": b, "md5": m, "nlink": n, "estado": e }))
            .collect::<Vec<_>>()
    );
    Ok(Json(v))
}

async fn ninguno_en_curso(st: &AppState) -> Result<(), ApiError> {
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM trabajos WHERE estado = 'en_curso'")
        .fetch_one(&st.pool)
        .await?;
    if n > 0 {
        return Err(ApiError::Conflicto(
            "Hay un trabajo en curso: espera a que termine".into(),
        ));
    }
    Ok(())
}

async fn restaurar(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    ninguno_en_curso(&st).await?;
    let r = transferencias::restaurar(&st.pool, id)
        .await
        .map_err(|e| ApiError::Conflicto(format!("{e:#}")))?;
    Ok(Json(como_json(&r)))
}

#[derive(Debug, Deserialize)]
struct PedidoVaciar {
    frase: String,
}

async fn vaciar(
    _u: Usuario,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(p): Json<PedidoVaciar>,
) -> Result<Json<Value>, ApiError> {
    ninguno_en_curso(&st).await?;
    if p.frase.trim() != FRASE_VACIAR {
        return Err(ApiError::Invalido(format!(
            "Para borrar para siempre escribe «{FRASE_VACIAR}»"
        )));
    }
    let r = transferencias::vaciar(&st.pool, &st.config.rclone, id, &p.frase)
        .await
        .map_err(|e| ApiError::Conflicto(format!("{e:#}")))?;
    Ok(Json(como_json(&r)))
}
