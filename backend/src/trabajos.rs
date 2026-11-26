//! Trabajos en segundo plano: escaneo, miniaturas, mudanzas y detección de placas. Su estado queda en la tabla
//! `trabajos`; el progreso de los que están en curso, en memoria.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use serde::Serialize;
use sqlx::SqlitePool;

use crate::{db::ahora, error::ApiError, inventario};

#[derive(Default)]
pub struct Trabajos {
    progreso: Mutex<HashMap<i64, Arc<AtomicU64>>>,
}

impl Trabajos {
    pub fn progreso(&self, id: i64) -> Option<u64> {
        self.progreso
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&id)
            .map(|a| a.load(Ordering::Relaxed))
    }

    fn registrar(&self, id: i64) -> Arc<AtomicU64> {
        let a = Arc::new(AtomicU64::new(0));
        self.progreso
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, a.clone());
        a
    }

    fn quitar(&self, id: i64) {
        self.progreso
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
    }
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Trabajo {
    pub id: i64,
    pub tipo: String,
    pub estado: String,
    pub detalle: String,
    pub resultado: String,
    pub creado: i64,
    pub terminado: Option<i64>,
}

/// Al arrancar: lo que quedó «en curso» se interrumpió con el servidor.
pub async fn marcar_interrumpidos(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE trabajos SET estado = 'fallo', detalle = 'se interrumpió (el servidor se reinició)', terminado = ?
         WHERE estado = 'en_curso'",
    )
    .bind(ahora())
    .execute(pool)
    .await?;
    // una mudanza cortada a la mitad: lo que se movió quedó registrado archivo por archivo
    sqlx::query("UPDATE mudanzas SET estado = 'con_errores' WHERE estado = 'en_curso'")
        .execute(pool)
        .await?;
    Ok(())
}

async fn alguno_en_curso(pool: &SqlitePool) -> Result<(), ApiError> {
    let corriendo: Option<(i64, String)> =
        sqlx::query_as("SELECT id, tipo FROM trabajos WHERE estado = 'en_curso' LIMIT 1")
            .fetch_optional(pool)
            .await?;
    match corriendo {
        Some((id, tipo)) => Err(ApiError::Conflicto(format!(
            "ya hay un trabajo en curso ({tipo}, trabajo {id}); espera a que termine"
        ))),
        None => Ok(()),
    }
}

/// Registra un trabajo «en curso» y lo corre en segundo plano; al terminar guarda su resultado (o el error).
/// Solo corre uno a la vez.
async fn lanzar<F, Fut, R>(
    pool: SqlitePool,
    trabajos: Arc<Trabajos>,
    tipo: &'static str,
    detalle: String,
    tarea: F,
) -> Result<i64, ApiError>
where
    F: FnOnce(SqlitePool, Arc<AtomicU64>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = anyhow::Result<R>> + Send + 'static,
    R: Serialize + Send + 'static,
{
    alguno_en_curso(&pool).await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO trabajos (tipo, estado, detalle, creado) VALUES (?, 'en_curso', ?, ?) RETURNING id",
    )
    .bind(tipo)
    .bind(&detalle)
    .bind(ahora())
    .fetch_one(&pool)
    .await?;
    let progreso = trabajos.registrar(id);
    tokio::spawn(async move {
        let r = tarea(pool.clone(), progreso).await;
        let (estado, det, resultado) = match r {
            Ok(res) => (
                "terminado",
                detalle,
                serde_json::to_string(&res).unwrap_or_default(),
            ),
            Err(e) => {
                tracing::error!("{tipo} (trabajo {id}): {e:#}");
                ("fallo", format!("{e:#}"), String::new())
            }
        };
        let _ = sqlx::query(
            "UPDATE trabajos SET estado = ?, detalle = ?, resultado = ?, terminado = ? WHERE id = ?",
        )
        .bind(estado)
        .bind(det)
        .bind(resultado)
        .bind(ahora())
        .bind(id)
        .execute(&pool)
        .await;
        trabajos.quitar(id);
    });
    Ok(id)
}

/// Ejecuta (o deshace) una mudanza en segundo plano. Al terminar, las placas se ponen al día con las rutas nuevas.
pub async fn iniciar_mudanza(
    pool: SqlitePool,
    trabajos: Arc<Trabajos>,
    mudanza: i64,
    deshacer: bool,
) -> Result<i64, ApiError> {
    let tipo = if deshacer {
        "deshacer_mudanza"
    } else {
        "mudanza"
    };
    lanzar(
        pool,
        trabajos,
        tipo,
        format!("mudanza {mudanza}"),
        move |pool, progreso| async move {
            let res = if deshacer {
                crate::mudanzas::deshacer(&pool, mudanza, progreso).await?
            } else {
                crate::mudanzas::ejecutar(&pool, mudanza, progreso).await?
            };
            crate::placas::detectar(&pool).await?;
            Ok(res)
        },
    )
    .await
}

/// Arranca el escaneo de una biblioteca en segundo plano (al final detecta las placas).
pub async fn iniciar_escaneo(
    pool: SqlitePool,
    trabajos: Arc<Trabajos>,
    biblioteca_id: i64,
) -> Result<i64, ApiError> {
    let nombre: Option<String> = sqlx::query_scalar("SELECT nombre FROM bibliotecas WHERE id = ?")
        .bind(biblioteca_id)
        .fetch_optional(&pool)
        .await?;
    let Some(nombre) = nombre else {
        return Err(ApiError::NoEncontrado);
    };
    lanzar(
        pool,
        trabajos,
        "escaneo",
        format!("biblioteca {nombre}"),
        move |pool, vistos| async move { inventario::escanear(&pool, biblioteca_id, vistos).await },
    )
    .await
}

/// Genera miniaturas en segundo plano (de todo o de los modelos indicados).
pub async fn iniciar_miniaturas(
    pool: SqlitePool,
    trabajos: Arc<Trabajos>,
    opciones: crate::miniaturas::Opciones,
    cuales: crate::miniaturas::Cuales,
    forzar: bool,
) -> Result<i64, ApiError> {
    let detalle = match (&cuales.modelo_ids, cuales.solo_placas) {
        (Some(ids), false) => format!("{} modelos", ids.len()),
        (Some(ids), true) => format!("vistas previas de placas de {} modelos", ids.len()),
        (None, true) => "vistas previas de las placas".to_string(),
        (None, false) => "toda la colección".to_string(),
    };
    lanzar(
        pool,
        trabajos,
        "miniaturas",
        detalle,
        move |pool, progreso| async move {
            crate::miniaturas::generar(&pool, opciones, cuales, forzar, progreso).await
        },
    )
    .await
}

/// Vuelve a detectar las placas (sin escanear el disco).
pub async fn iniciar_placas(pool: SqlitePool, trabajos: Arc<Trabajos>) -> Result<i64, ApiError> {
    lanzar(
        pool,
        trabajos,
        "placas",
        "detección de placas".to_string(),
        |pool, _| async move { crate::placas::detectar(&pool).await },
    )
    .await
}
