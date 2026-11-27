//! Bandeja «Por catalogar» (fase 9): lo que llegó a la colección y no es de ningún modelo, con su propuesta.
//! Aprobar crea la fila (o junta los archivos con un modelo que ya está en su lugar) y la mueve con una mudanza en
//! segundo plano; «No mover» deja el grupo donde está y no lo vuelve a proponer. Exigen sesión.

use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    AppState,
    auth::Usuario,
    bandeja::{self, Aprobacion, CamposGrupo},
    error::ApiError,
    trabajos, transferencias,
};

pub fn rutas() -> Router<AppState> {
    Router::new()
        .route("/bandeja", get(lista))
        .route("/bandeja/grupo", get(grupo))
        .route("/bandeja/revisar", post(revisar))
        .route("/bandeja/aprobar", post(aprobar))
        .route("/bandeja/descartar", post(descartar))
        .route("/bandeja/volver", post(volver))
        .route("/bandeja/extraer", post(extraer))
}

#[derive(Debug, Deserialize)]
struct Ver {
    ver: Option<String>,
}

async fn lista(
    _u: Usuario,
    State(st): State<AppState>,
    Query(q): Query<Ver>,
) -> Result<Json<Value>, ApiError> {
    let b = bandeja::detectar(&st.pool).await?;
    let descartados = b.grupos.iter().filter(|g| g.descartado).count();
    let todos = q.ver.as_deref() == Some("todos");
    let grupos: Vec<&bandeja::Grupo> = b.grupos.iter().filter(|g| todos || !g.descartado).collect();
    let destino: Option<(i64, String)> = sqlx::query_as(
        "SELECT id, nombre FROM ubicaciones_frio WHERE predeterminada = 1 AND escribible = 1 LIMIT 1",
    )
    .fetch_optional(&st.pool)
    .await?;
    Ok(Json(json!({
        "entrada": bandeja::ENTRADA,
        "existe_entrada": b.existe_entrada,
        "grupos": grupos,
        "descartados": descartados,
        "destino_frio": destino.map(|(id, nombre)| json!({ "id": id, "nombre": nombre })),
    })))
}

#[derive(Debug, Deserialize)]
struct Clave {
    clave: String,
}

async fn grupo(
    _u: Usuario,
    State(st): State<AppState>,
    Query(q): Query<Clave>,
) -> Result<Json<Value>, ApiError> {
    let b = bandeja::detectar(&st.pool).await?;
    let g = b
        .grupos
        .into_iter()
        .find(|g| g.clave == q.clave)
        .ok_or(ApiError::NoEncontrado)?;
    let mut archivos = Vec::new();
    for id in &g.archivo_ids {
        let fila: Option<(i64, String, String, String, i64, Option<String>)> = sqlx::query_as(
            "SELECT a.id, a.ruta, a.nombre, a.tipo, a.bytes, mi.clave FROM archivos a
             LEFT JOIN miniaturas mi ON mi.archivo_id = a.id AND mi.estado = 'ok' WHERE a.id = ?",
        )
        .bind(id)
        .fetch_optional(&st.pool)
        .await?;
        if let Some((id, ruta, nombre, tipo, bytes, miniatura)) = fila {
            archivos.push(json!({
                "id": id, "ruta": ruta, "nombre": nombre, "tipo": tipo, "bytes": bytes, "miniatura": miniatura
            }));
        }
    }
    let (_, raiz) = transferencias::principal(&st.pool).await?;
    let mut contenidos = serde_json::Map::new();
    for z in g
        .comprimidos
        .iter()
        .filter(|z| z.nombre.to_lowercase().ends_with(".zip"))
    {
        let ruta = raiz.join(&z.ruta);
        let r = tokio::task::spawn_blocking(move || bandeja::contenido_zip(&ruta))
            .await
            .map_err(|e| ApiError::Interno(e.into()))?;
        contenidos.insert(
            z.id.to_string(),
            match r {
                Ok((n, bytes, nombres)) => {
                    json!({ "entradas": n, "bytes": bytes, "nombres": nombres })
                }
                Err(e) => json!({ "error": e }),
            },
        );
    }
    Ok(Json(json!({
        "grupo": g,
        "archivos": archivos,
        "contenidos": contenidos,
    })))
}

async fn revisar(
    _u: Usuario,
    State(st): State<AppState>,
    Json(c): Json<CamposGrupo>,
) -> Result<Json<Value>, ApiError> {
    let r = bandeja::revisar(&st.pool, &c).await?;
    Ok(Json(serde_json::to_value(r).unwrap_or(Value::Null)))
}

async fn aprobar(
    u: Usuario,
    State(st): State<AppState>,
    Json(a): Json<Aprobacion>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let r = bandeja::aprobar(&st.pool, &a, &u.nombre).await?;
    let mut v = serde_json::to_value(&r).unwrap_or(Value::Null);
    let Some(mudanza) = r.mudanza_id else {
        return Ok((StatusCode::OK, Json(v)));
    };
    let nombre = if a.campos.nombre.trim().is_empty() {
        a.campos.faccion.trim().to_string()
    } else {
        a.campos.nombre.trim().to_string()
    };
    let trabajo = trabajos::iniciar_aprobar(
        st.pool.clone(),
        st.trabajos.clone(),
        st.config.rclone.clone(),
        mudanza,
        r.transferencia_id,
        format!("{nombre} desde la bandeja"),
    )
    .await;
    let trabajo = match trabajo {
        Ok(t) => t,
        Err(e) => {
            if let Some(t) = r.transferencia_id {
                transferencias::marcar_fallo(&st.pool, t, "no se pudo empezar").await;
            }
            return Err(e);
        }
    };
    if let Some(t) = r.transferencia_id {
        sqlx::query("UPDATE transferencias SET trabajo_id = ? WHERE id = ?")
            .bind(trabajo)
            .bind(t)
            .execute(&st.pool)
            .await?;
    }
    v["trabajo_id"] = json!(trabajo);
    Ok((StatusCode::ACCEPTED, Json(v)))
}

#[derive(Debug, Deserialize)]
struct Descarte {
    clave: String,
    #[serde(default)]
    motivo: String,
}

async fn descartar(
    u: Usuario,
    State(st): State<AppState>,
    Json(d): Json<Descarte>,
) -> Result<Json<Value>, ApiError> {
    if d.clave.trim().is_empty() {
        return Err(ApiError::Invalido("Falta la clave del grupo".into()));
    }
    bandeja::descartar(&st.pool, &d.clave, &d.motivo, &u.nombre).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn volver(
    _u: Usuario,
    State(st): State<AppState>,
    Json(c): Json<Clave>,
) -> Result<Json<Value>, ApiError> {
    if !bandeja::volver(&st.pool, &c.clave).await? {
        return Err(ApiError::NoEncontrado);
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Debug, Deserialize)]
struct PedidoExtraer {
    archivo_id: i64,
}

async fn extraer(
    u: Usuario,
    State(st): State<AppState>,
    Json(p): Json<PedidoExtraer>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let ruta = bandeja::validar_extraer(&st.pool, p.archivo_id)
        .await
        .map_err(|e| ApiError::Invalido(format!("{e:#}")))?;
    let nombre = ruta.rsplit('/').next().unwrap_or(&ruta).to_string();
    let trabajo = trabajos::iniciar_extraer(
        st.pool.clone(),
        st.trabajos.clone(),
        p.archivo_id,
        u.nombre.clone(),
        format!("extraer {nombre}"),
    )
    .await?;
    Ok((StatusCode::ACCEPTED, Json(json!({ "trabajo_id": trabajo }))))
}
