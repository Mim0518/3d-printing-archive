//! Fase 8: mandar modelos a frío y traerlos de regreso, con verificación por MD5, y la papelera de la app.
//!
//! Reglas (las de la limpieza, sección 9 de la especificación):
//! - una copia local solo se quita si hay otra verificada por MD5 en el destino, comprobada justo antes;
//! - quitar es mover a la papelera de la app (en el mismo disco, con rename: los enlaces duros se respetan);
//! - vaciar la papelera borra para siempre: pide una frase de confirmación y vuelve a comprobar la copia en frío;
//! - traer nunca sobrescribe: lo que ya está igual se omite y lo que choca se queda aparte y se avisa.

use std::{
    collections::{HashMap, HashSet},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use anyhow::{Context, anyhow, bail};
use serde::Serialize;
use serde_json::json;
use sqlx::SqlitePool;

use crate::{
    almacen::{self, Almacen, ArchivoFrio, dentro},
    db::ahora,
};

pub const PAPELERA: &str = ".archivo-papelera";
pub const ENTRANTE: &str = ".archivo-entrante";
/// Lo que hay que escribir para vaciar la papelera.
pub const FRASE_VACIAR: &str = "borrar para siempre";

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct Ubicacion {
    pub id: i64,
    pub nombre: String,
    pub tipo: String,
    pub remoto: String,
    pub ruta_base: String,
    #[serde(skip)]
    pub marca: String,
    pub escribible: bool,
}

impl Ubicacion {
    pub async fn cargar(pool: &SqlitePool, id: i64) -> anyhow::Result<Option<Self>> {
        Ok(sqlx::query_as(
            "SELECT id, nombre, tipo, remoto, ruta_base, marca, escribible FROM ubicaciones_frio WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(pool)
        .await?)
    }

    pub fn almacen(&self, rclone: &str) -> anyhow::Result<Almacen> {
        if self.tipo == "drive" {
            if !almacen::remoto_valido(&self.remoto) {
                bail!("el remoto «{}» no es válido", self.remoto);
            }
            Ok(Almacen::Rclone {
                programa: rclone.to_string(),
                remoto: self.remoto.clone(),
            })
        } else {
            Ok(Almacen::Carpeta {
                base: PathBuf::from(&self.ruta_base),
            })
        }
    }

    /// El almacén, comprobando que se pueda usar (el disco conectado y que sea el mismo).
    pub fn abrir(&self, rclone: &str) -> anyhow::Result<Almacen> {
        let a = self.almacen(rclone)?;
        if self.tipo != "drive" {
            a.probar(&self.marca).map_err(|e| anyhow!(e))?;
        }
        Ok(a)
    }
}

/// La biblioteca principal: (id, raíz).
pub async fn principal(pool: &SqlitePool) -> anyhow::Result<(i64, PathBuf)> {
    let (id, ruta): (i64, String) =
        sqlx::query_as("SELECT id, ruta FROM bibliotecas WHERE tipo = 'principal' LIMIT 1")
            .fetch_optional(pool)
            .await?
            .context("no hay biblioteca principal (ARCHIVO_BIBLIOTECA)")?;
    Ok((id, PathBuf::from(ruta)))
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Local {
    pub id: i64,
    pub ruta: String,
    pub bytes: i64,
    pub clave: String,
    pub nlink: i64,
}

/// Los archivos del modelo que están en el disco.
pub async fn locales(pool: &SqlitePool, modelo: i64) -> anyhow::Result<Vec<Local>> {
    Ok(sqlx::query_as(
        "SELECT a.id, a.ruta, a.bytes, printf('%x-%x-%x', a.inode, a.bytes, a.mtime_ms) AS clave, a.nlink
         FROM archivos a JOIN bibliotecas b ON b.id = a.biblioteca_id AND b.tipo = 'principal'
         WHERE a.modelo_id = ? AND a.estado = 'presente' ORDER BY a.ruta",
    )
    .bind(modelo)
    .fetch_all(pool)
    .await?)
}

/// La clave de un archivo como la guarda el escaneo (inode-tamaño-fecha en hex).
fn clave_actual(ruta: &Path) -> std::io::Result<String> {
    let m = std::fs::symlink_metadata(ruta)?;
    let mtime_ms = m.mtime() * 1000 + m.mtime_nsec() / 1_000_000;
    Ok(format!(
        "{:x}-{:x}-{:x}",
        m.ino() as i64,
        m.len() as i64,
        mtime_ms
    ))
}

/// La carpeta común de unas rutas («a/b/x», «a/b/c/y» → «a/b»).
pub fn carpeta_comun<'a>(rutas: impl IntoIterator<Item = &'a str>) -> String {
    let mut comun: Option<Vec<&str>> = None;
    for r in rutas {
        let partes: Vec<&str> = r.split('/').collect();
        let carpetas = &partes[..partes.len().saturating_sub(1)];
        comun = Some(match comun {
            None => carpetas.to_vec(),
            Some(c) => c
                .iter()
                .zip(carpetas)
                .take_while(|(a, b)| a == b)
                .map(|(a, _)| *a)
                .collect(),
        });
    }
    comun.unwrap_or_default().join("/")
}

/// MD5 de los archivos locales: de la caché si no cambiaron; si no, se calculan. Un archivo que cambió desde el
/// último escaneo es un error (hay que escanear antes).
pub async fn md5_locales(
    pool: &SqlitePool,
    raiz: &Path,
    lista: &[Local],
    progreso: &Arc<AtomicU64>,
) -> anyhow::Result<(HashMap<i64, String>, Vec<String>)> {
    let guardados: Vec<(i64, String, String)> =
        sqlx::query_as("SELECT archivo_id, clave, md5 FROM hashes")
            .fetch_all(pool)
            .await?;
    let guardados: HashMap<i64, (String, String)> =
        guardados.into_iter().map(|(i, c, m)| (i, (c, m))).collect();
    let mut md5 = HashMap::new();
    let mut por_calcular = Vec::new();
    for l in lista {
        match guardados.get(&l.id) {
            Some((c, m)) if *c == l.clave => {
                md5.insert(l.id, m.clone());
                progreso.fetch_add(1, Ordering::Relaxed);
            }
            _ => por_calcular.push(l.clone()),
        }
    }
    let raiz = raiz.to_path_buf();
    let p = progreso.clone();
    let calculados = tokio::task::spawn_blocking(move || {
        por_calcular
            .into_iter()
            .map(|l| {
                let abs = dentro(&raiz, &l.ruta).map_err(|e| anyhow!(e))?;
                let r = match clave_actual(&abs) {
                    Ok(c) if c == l.clave => almacen::md5_de(&abs)
                        .map(|m| (l.id, l.clave.clone(), m))
                        .map_err(|e| format!("{}: {e}", l.ruta)),
                    Ok(_) => Err(format!(
                        "{}: cambió desde el último escaneo (escanea primero)",
                        l.ruta
                    )),
                    Err(e) => Err(format!("{}: {e}", l.ruta)),
                };
                p.fetch_add(1, Ordering::Relaxed);
                Ok::<_, anyhow::Error>(r)
            })
            .collect::<anyhow::Result<Vec<_>>>()
    })
    .await??;
    let mut errores = Vec::new();
    let t = ahora();
    let mut tx = pool.begin().await?;
    for r in calculados {
        match r {
            Ok((id, clave, m)) => {
                sqlx::query(
                    "INSERT INTO hashes (archivo_id, clave, md5, calculado) VALUES (?, ?, ?, ?)
                     ON CONFLICT (archivo_id) DO UPDATE SET clave = excluded.clave, md5 = excluded.md5,
                        calculado = excluded.calculado",
                )
                .bind(id)
                .bind(&clave)
                .bind(&m)
                .bind(t)
                .execute(&mut *tx)
                .await?;
                md5.insert(id, m);
            }
            Err(e) => errores.push(e),
        }
    }
    tx.commit().await?;
    Ok((md5, errores))
}

async fn listar(almacen: &Almacen, prefijo: &str) -> anyhow::Result<HashMap<String, ArchivoFrio>> {
    let a = almacen.clone();
    let p = prefijo.to_string();
    let lista = tokio::task::spawn_blocking(move || a.listar(&p))
        .await?
        .map_err(|e| anyhow!(e))?;
    Ok(lista.into_iter().map(|x| (x.ruta.clone(), x)).collect())
}

// ---------- plan ----------

#[derive(Debug, Serialize)]
pub struct PlanSubir {
    pub modelo_id: i64,
    pub ubicacion_id: i64,
    pub ubicacion: String,
    pub destino: String,
    pub archivos: usize,
    pub bytes: i64,
    /// archivos que ya tienen una copia verificada en esa ubicación (con su misma ruta y tamaño)
    pub ya_en_frio: usize,
    /// archivos con otro nombre en el disco (enlaces duros): al vaciar la papelera su espacio no se libera
    pub compartidos: usize,
    pub bytes_compartidos: i64,
    pub avisos: Vec<String>,
}

pub async fn plan_subir(
    pool: &SqlitePool,
    modelo: i64,
    u: &Ubicacion,
) -> anyhow::Result<PlanSubir> {
    let lista = locales(pool, modelo).await?;
    let ya: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM archivos a JOIN copias_frio c ON c.ruta = a.ruta AND c.bytes = a.bytes
         WHERE a.modelo_id = ? AND a.estado = 'presente' AND c.ubicacion_id = ? AND c.verificado IS NOT NULL
           AND c.estado = 'presente'",
    )
    .bind(modelo)
    .bind(u.id)
    .fetch_one(pool)
    .await?;
    let prefijo = carpeta_comun(lista.iter().map(|l| l.ruta.as_str()));
    let destino = if u.tipo == "drive" {
        format!("{}/{prefijo}", u.remoto.trim_end_matches('/'))
    } else {
        format!("{}/{prefijo}", u.ruta_base.trim_end_matches('/'))
    };
    let compartidos: Vec<&Local> = lista.iter().filter(|l| l.nlink > 1).collect();
    let mut avisos = Vec::new();
    if lista.is_empty() {
        avisos.push("No tiene archivos en el disco.".into());
    }
    if !u.escribible {
        avisos.push(format!(
            "«{}» no está marcada para recibir copias nuevas.",
            u.nombre
        ));
    }
    Ok(PlanSubir {
        modelo_id: modelo,
        ubicacion_id: u.id,
        ubicacion: u.nombre.clone(),
        destino,
        archivos: lista.len(),
        bytes: lista.iter().map(|l| l.bytes).sum(),
        ya_en_frio: ya as usize,
        compartidos: compartidos.len(),
        bytes_compartidos: compartidos.iter().map(|l| l.bytes).sum(),
        avisos,
    })
}

// ---------- registro de transferencias ----------

pub async fn nueva_transferencia(
    pool: &SqlitePool,
    modelo: i64,
    ubicacion: i64,
    direccion: &str,
    quitar_local: bool,
    usuario: &str,
) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO transferencias (modelo_id, ubicacion_id, direccion, quitar_local, usuario, creada)
         VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(modelo)
    .bind(ubicacion)
    .bind(direccion)
    .bind(quitar_local)
    .bind(usuario)
    .bind(ahora())
    .fetch_one(pool)
    .await?)
}

#[allow(clippy::too_many_arguments)]
async fn cerrar_transferencia(
    pool: &SqlitePool,
    id: i64,
    estado: &str,
    archivos: usize,
    bytes: i64,
    verificados: usize,
    lote: Option<i64>,
    resultado: &impl Serialize,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE transferencias SET estado = ?, archivos = ?, bytes = ?, verificados = ?, lote_id = ?, resultado = ?,
            terminada = ? WHERE id = ?",
    )
    .bind(estado)
    .bind(archivos as i64)
    .bind(bytes)
    .bind(verificados as i64)
    .bind(lote)
    .bind(serde_json::to_string(resultado)?)
    .bind(ahora())
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Si el trabajo falla a medias, la transferencia no se queda «en curso».
pub async fn marcar_fallo(pool: &SqlitePool, id: i64, error: &str) {
    sqlx::query(
        "UPDATE transferencias SET estado = 'fallo', resultado = ?, terminada = ? WHERE id = ? AND estado = 'en_curso'",
    )
    .bind(json!({ "error": error }).to_string())
    .bind(ahora())
    .bind(id)
    .execute(pool)
    .await
    .ok();
}

// ---------- subir ----------

#[derive(Debug, Default, Serialize)]
pub struct ResultadoSubir {
    pub transferencia_id: i64,
    pub archivos: usize,
    pub subidos: usize,
    pub ya_estaban: usize,
    pub verificados: usize,
    pub errores: Vec<String>,
    pub lote_papelera: Option<i64>,
    pub a_papelera: usize,
    pub no_quitados: Vec<String>,
}

/// Manda los archivos de un modelo a una ubicación y comprueba cada uno por MD5. Si se pidió y todo cuadró,
/// quita la copia local a la papelera.
pub async fn subir(
    pool: &SqlitePool,
    rclone: &str,
    transferencia: i64,
    progreso: Arc<AtomicU64>,
) -> anyhow::Result<ResultadoSubir> {
    let (modelo, ubicacion, quitar, usuario): (i64, i64, bool, String) = sqlx::query_as(
        "SELECT modelo_id, ubicacion_id, quitar_local, usuario FROM transferencias WHERE id = ?",
    )
    .bind(transferencia)
    .fetch_one(pool)
    .await?;
    let r = subir_interno(
        pool,
        rclone,
        transferencia,
        modelo,
        ubicacion,
        quitar,
        &usuario,
        progreso,
    )
    .await;
    if let Err(e) = &r {
        marcar_fallo(pool, transferencia, &format!("{e:#}")).await;
    }
    r
}

#[allow(clippy::too_many_arguments)]
async fn subir_interno(
    pool: &SqlitePool,
    rclone: &str,
    transferencia: i64,
    modelo: i64,
    ubicacion: i64,
    quitar: bool,
    usuario: &str,
    progreso: Arc<AtomicU64>,
) -> anyhow::Result<ResultadoSubir> {
    let u = Ubicacion::cargar(pool, ubicacion)
        .await?
        .context("no existe esa ubicación")?;
    let alm = u.abrir(rclone)?;
    let (biblioteca, raiz) = principal(pool).await?;
    let lista = locales(pool, modelo).await?;
    if lista.is_empty() {
        bail!("el modelo no tiene archivos en el disco");
    }
    let mut res = ResultadoSubir {
        transferencia_id: transferencia,
        archivos: lista.len(),
        ..Default::default()
    };
    let (md5, errores_md5) = md5_locales(pool, &raiz, &lista, &progreso).await?;
    res.errores.extend(errores_md5);
    let prefijo = carpeta_comun(lista.iter().map(|l| l.ruta.as_str()));
    let antes = listar(&alm, &prefijo).await?;
    let pendientes: Vec<String> = lista
        .iter()
        .filter(|l| md5.contains_key(&l.id))
        .filter(|l| antes.get(&l.ruta).and_then(|x| x.md5.as_ref()) != md5.get(&l.id))
        .map(|l| l.ruta.clone())
        .collect();
    res.ya_estaban = md5.len() - pendientes.len();
    res.subidos = pendientes.len();
    progreso.store(0, Ordering::Relaxed);
    {
        let a = alm.clone();
        let r = raiz.clone();
        let p = progreso.clone();
        let pend = pendientes.clone();
        tokio::task::spawn_blocking(move || a.subir(&r, &pend, &p))
            .await?
            .map_err(|e| anyhow!(e))?;
    }
    // la verificación: lo que dice el destino ahora
    let despues = listar(&alm, &prefijo).await?;
    let t = ahora();
    let mut verificados: Vec<(&Local, String)> = Vec::new();
    for l in &lista {
        let Some(m) = md5.get(&l.id) else { continue };
        match despues.get(&l.ruta) {
            Some(x) if x.md5.as_deref() == Some(m.as_str()) && x.bytes == l.bytes => {
                verificados.push((l, m.clone()))
            }
            Some(x) if x.md5.is_none() => res
                .errores
                .push(format!("{}: el destino no da su MD5", l.ruta)),
            Some(_) => res
                .errores
                .push(format!("{}: el MD5 en el destino no coincide", l.ruta)),
            None => res.errores.push(format!("{}: no llegó al destino", l.ruta)),
        }
    }
    res.verificados = verificados.len();
    let mut tx = pool.begin().await?;
    for (l, m) in &verificados {
        sqlx::query(
            "INSERT INTO copias_frio (ubicacion_id, ruta, bytes, md5, ruta_original, modelo_id, archivo_id, verificado,
                                      estado, visto_en, origen)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'presente', ?, 'app')
             ON CONFLICT (ubicacion_id, ruta) DO UPDATE SET bytes = excluded.bytes, md5 = excluded.md5,
                modelo_id = excluded.modelo_id, archivo_id = excluded.archivo_id, verificado = excluded.verificado,
                estado = 'presente', visto_en = excluded.visto_en,
                ruta_original = CASE WHEN copias_frio.ruta_original = '' THEN excluded.ruta_original
                                     ELSE copias_frio.ruta_original END",
        )
        .bind(ubicacion)
        .bind(&l.ruta)
        .bind(l.bytes)
        .bind(m)
        .bind(&l.ruta)
        .bind(modelo)
        .bind(l.id)
        .bind(t)
        .bind(t)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    let mut estado = if res.errores.is_empty() {
        "terminada"
    } else {
        "con_errores"
    };
    if quitar {
        if res.errores.is_empty() {
            let motivo = format!("Mandado a frío ({})", u.nombre);
            let (lote, movidos, no) = a_papelera(
                pool,
                biblioteca,
                &raiz,
                modelo,
                Some(ubicacion),
                &verificados,
                &motivo,
                usuario,
            )
            .await?;
            res.lote_papelera = Some(lote);
            res.a_papelera = movidos;
            res.no_quitados = no;
            if !res.no_quitados.is_empty() {
                estado = "con_errores";
            }
        } else {
            res.no_quitados
                .push("Hubo errores al verificar: no se quitó nada del disco.".into());
        }
    }
    crate::catalogo::actualizar_estadisticas(pool).await?;
    cerrar_transferencia(
        pool,
        transferencia,
        estado,
        res.archivos,
        lista.iter().map(|l| l.bytes).sum(),
        res.verificados,
        res.lote_papelera,
        &res,
    )
    .await?;
    Ok(res)
}

// ---------- quitar la copia local ----------

#[derive(Debug, Default, Serialize)]
pub struct ResultadoQuitar {
    pub lote_papelera: Option<i64>,
    pub a_papelera: usize,
    pub sin_copia: usize,
    pub no_quitados: Vec<String>,
}

/// Quita del disco (a la papelera) los archivos del modelo que tienen una copia en esa ubicación con su mismo
/// MD5, comprobada en el destino en este momento.
pub async fn quitar_local(
    pool: &SqlitePool,
    rclone: &str,
    modelo: i64,
    ubicacion: i64,
    usuario: &str,
    progreso: Arc<AtomicU64>,
) -> anyhow::Result<ResultadoQuitar> {
    let u = Ubicacion::cargar(pool, ubicacion)
        .await?
        .context("no existe esa ubicación")?;
    let alm = u.abrir(rclone)?;
    let (biblioteca, raiz) = principal(pool).await?;
    let lista = locales(pool, modelo).await?;
    if lista.is_empty() {
        bail!("el modelo no tiene archivos en el disco");
    }
    let (md5, mut errores) = md5_locales(pool, &raiz, &lista, &progreso).await?;
    let prefijo = carpeta_comun(lista.iter().map(|l| l.ruta.as_str()));
    let remoto = listar(&alm, &prefijo).await?;
    let con_copia: Vec<(&Local, String)> = lista
        .iter()
        .filter_map(|l| {
            let m = md5.get(&l.id)?;
            let x = remoto.get(&l.ruta)?;
            (x.md5.as_deref() == Some(m.as_str()) && x.bytes == l.bytes).then(|| (l, m.clone()))
        })
        .collect();
    let mut res = ResultadoQuitar {
        sin_copia: lista.len() - con_copia.len(),
        ..Default::default()
    };
    if !con_copia.is_empty() {
        let motivo = format!("Quitado del disco: la copia está en {}", u.nombre);
        let (lote, movidos, no) = a_papelera(
            pool,
            biblioteca,
            &raiz,
            modelo,
            Some(ubicacion),
            &con_copia,
            &motivo,
            usuario,
        )
        .await?;
        res.lote_papelera = Some(lote);
        res.a_papelera = movidos;
        errores.extend(no);
    }
    res.no_quitados = errores;
    crate::catalogo::actualizar_estadisticas(pool).await?;
    Ok(res)
}

// ---------- papelera ----------

fn ruta_papelera(raiz: &Path, lote: i64) -> PathBuf {
    raiz.join(PAPELERA).join(lote.to_string())
}

/// Quita las carpetas vacías desde `dir` hacia arriba, sin tocar `tope` ni lo que está fuera de él.
fn quitar_vacias(dir: &Path, tope: &Path) {
    let mut d = dir.to_path_buf();
    while d.starts_with(tope) && d != tope {
        if std::fs::remove_dir(&d).is_err() {
            break;
        }
        match d.parent() {
            Some(p) => d = p.to_path_buf(),
            None => break,
        }
    }
}

/// Mueve esos archivos a un lote nuevo de la papelera. Devuelve (lote, movidos, los que no se pudieron).
#[allow(clippy::too_many_arguments)]
async fn a_papelera(
    pool: &SqlitePool,
    biblioteca: i64,
    raiz: &Path,
    modelo: i64,
    ubicacion: Option<i64>,
    archivos: &[(&Local, String)],
    motivo: &str,
    usuario: &str,
) -> anyhow::Result<(i64, usize, Vec<String>)> {
    let lote: i64 = sqlx::query_scalar(
        "INSERT INTO papelera_lotes (biblioteca_id, motivo, modelo_id, ubicacion_id, usuario, creado)
         VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(biblioteca)
    .bind(motivo)
    .bind(modelo)
    .bind(ubicacion)
    .bind(usuario)
    .bind(ahora())
    .fetch_one(pool)
    .await?;
    let carpeta_modelo: String = sqlx::query_scalar("SELECT carpeta FROM modelos WHERE id = ?")
        .bind(modelo)
        .fetch_one(pool)
        .await?;
    let destino = ruta_papelera(raiz, lote);
    let raiz_b = raiz.to_path_buf();
    let lista: Vec<(Local, String)> = archivos
        .iter()
        .map(|(l, m)| ((*l).clone(), m.clone()))
        .collect();
    let tope = raiz.join(&carpeta_modelo);
    // los movimientos en el disco, uno por uno (si uno falla, los demás siguen)
    let hechos = tokio::task::spawn_blocking(move || {
        let mut out = Vec::new();
        for (l, m) in lista {
            let r = (|| -> Result<(), String> {
                let origen = dentro(&raiz_b, &l.ruta)?;
                let fin = dentro(&destino, &l.ruta)?;
                let c = clave_actual(&origen).map_err(|e| e.to_string())?;
                if c != l.clave {
                    return Err("cambió desde que se verificó".into());
                }
                if fin.exists() {
                    return Err("ya hay algo con ese nombre en la papelera".into());
                }
                std::fs::create_dir_all(fin.parent().expect("tiene carpeta"))
                    .map_err(|e| e.to_string())?;
                std::fs::rename(&origen, &fin).map_err(|e| e.to_string())?;
                if let Some(p) = origen.parent() {
                    quitar_vacias(p, &tope);
                }
                Ok(())
            })();
            out.push((l, m, r));
        }
        out
    })
    .await?;
    let mut movidos = 0;
    let mut bytes = 0;
    let mut no = Vec::new();
    let mut tx = pool.begin().await?;
    for (l, m, r) in hechos {
        match r {
            Ok(()) => {
                sqlx::query(
                    "INSERT INTO papelera_archivos (lote_id, archivo_id, ruta, bytes, md5, nlink) VALUES (?, ?, ?, ?, ?, ?)",
                )
                .bind(lote)
                .bind(l.id)
                .bind(&l.ruta)
                .bind(l.bytes)
                .bind(&m)
                .bind(l.nlink)
                .execute(&mut *tx)
                .await?;
                sqlx::query("UPDATE archivos SET estado = 'falta' WHERE id = ?")
                    .bind(l.id)
                    .execute(&mut *tx)
                    .await?;
                movidos += 1;
                bytes += l.bytes;
            }
            Err(e) => no.push(format!("{}: {e}", l.ruta)),
        }
    }
    sqlx::query("UPDATE papelera_lotes SET archivos = ?, bytes = ? WHERE id = ?")
        .bind(movidos as i64)
        .bind(bytes)
        .bind(lote)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((lote, movidos, no))
}

#[derive(Debug, Default, Serialize)]
pub struct ResultadoRestaurar {
    pub restaurados: usize,
    pub conflictos: Vec<String>,
}

/// Regresa a su lugar lo que está en un lote de la papelera. Lo que ya tiene algo en su lugar se queda y se
/// avisa (nunca se sobrescribe).
pub async fn restaurar(pool: &SqlitePool, lote: i64) -> anyhow::Result<ResultadoRestaurar> {
    let (estado, biblioteca): (String, i64) =
        sqlx::query_as("SELECT estado, biblioteca_id FROM papelera_lotes WHERE id = ?")
            .bind(lote)
            .fetch_optional(pool)
            .await?
            .context("no existe ese lote")?;
    if estado != "en_papelera" {
        bail!("ese lote ya no está en la papelera");
    }
    let raiz: String = sqlx::query_scalar("SELECT ruta FROM bibliotecas WHERE id = ?")
        .bind(biblioteca)
        .fetch_one(pool)
        .await?;
    let raiz = PathBuf::from(raiz);
    let archivos: Vec<(String, Option<i64>)> = sqlx::query_as(
        "SELECT ruta, archivo_id FROM papelera_archivos WHERE lote_id = ? AND estado IN ('en_papelera', 'conflicto')",
    )
    .bind(lote)
    .fetch_all(pool)
    .await?;
    let base = ruta_papelera(&raiz, lote);
    let r2 = raiz.clone();
    let b2 = base.clone();
    let hechos = tokio::task::spawn_blocking(move || {
        archivos
            .into_iter()
            .map(|(ruta, id)| {
                let r = (|| -> Result<(), String> {
                    let origen = dentro(&b2, &ruta)?;
                    let fin = dentro(&r2, &ruta)?;
                    if fin.exists() {
                        return Err("ya hay un archivo en su lugar".into());
                    }
                    std::fs::create_dir_all(fin.parent().expect("tiene carpeta"))
                        .map_err(|e| e.to_string())?;
                    std::fs::rename(&origen, &fin).map_err(|e| e.to_string())?;
                    if let Some(p) = origen.parent() {
                        quitar_vacias(p, &b2);
                    }
                    Ok(())
                })();
                (ruta, id, r)
            })
            .collect::<Vec<_>>()
    })
    .await?;
    let mut res = ResultadoRestaurar::default();
    let mut tx = pool.begin().await?;
    for (ruta, id, r) in hechos {
        let est = match &r {
            Ok(()) => "restaurado",
            Err(_) => "conflicto",
        };
        sqlx::query("UPDATE papelera_archivos SET estado = ? WHERE lote_id = ? AND ruta = ?")
            .bind(est)
            .bind(lote)
            .bind(&ruta)
            .execute(&mut *tx)
            .await?;
        match r {
            Ok(()) => {
                if let Some(id) = id {
                    sqlx::query(
                        "UPDATE archivos SET estado = 'presente' WHERE id = ? AND ruta = ?",
                    )
                    .bind(id)
                    .bind(&ruta)
                    .execute(&mut *tx)
                    .await?;
                }
                res.restaurados += 1;
            }
            Err(e) => res.conflictos.push(format!("{ruta}: {e}")),
        }
    }
    if res.conflictos.is_empty() {
        sqlx::query("UPDATE papelera_lotes SET estado = 'restaurado', cerrado = ? WHERE id = ?")
            .bind(ahora())
            .bind(lote)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    std::fs::remove_dir(&base).ok();
    std::fs::remove_dir(raiz.join(PAPELERA)).ok();
    crate::catalogo::actualizar_estadisticas(pool).await?;
    Ok(res)
}

#[derive(Debug, Default, Serialize)]
pub struct ResultadoVaciar {
    pub borrados: usize,
    pub bytes: i64,
    /// de esos bytes, los que siguen en el disco porque el archivo tiene otro nombre (enlace duro)
    pub bytes_compartidos: i64,
}

/// Borra para siempre un lote de la papelera, después de comprobar otra vez que cada archivo sigue en frío con
/// su mismo MD5.
pub async fn vaciar(
    pool: &SqlitePool,
    rclone: &str,
    lote: i64,
    frase: &str,
) -> anyhow::Result<ResultadoVaciar> {
    if frase.trim() != FRASE_VACIAR {
        bail!("Para vaciar escribe «{FRASE_VACIAR}»");
    }
    let (estado, biblioteca, ubicacion): (String, i64, Option<i64>) = sqlx::query_as(
        "SELECT estado, biblioteca_id, ubicacion_id FROM papelera_lotes WHERE id = ?",
    )
    .bind(lote)
    .fetch_optional(pool)
    .await?
    .context("no existe ese lote")?;
    if estado != "en_papelera" {
        bail!("ese lote ya no está en la papelera");
    }
    let archivos: Vec<(String, i64, String, i64)> = sqlx::query_as(
        "SELECT ruta, bytes, md5, nlink FROM papelera_archivos WHERE lote_id = ? AND estado = 'en_papelera'",
    )
    .bind(lote)
    .fetch_all(pool)
    .await?;
    let ubicacion = ubicacion.context("este lote no tiene copia en frío: no se borra")?;
    let u = Ubicacion::cargar(pool, ubicacion)
        .await?
        .context("ya no existe la ubicación de la copia")?;
    let alm = u.abrir(rclone)?;
    let prefijo = carpeta_comun(archivos.iter().map(|a| a.0.as_str()));
    let remoto = listar(&alm, &prefijo).await?;
    let faltan: Vec<&String> = archivos
        .iter()
        .filter(|(r, b, m, _)| {
            !remoto.get(r).is_some_and(|x| {
                x.md5.as_deref() == Some(m.as_str()) && x.bytes == *b && !m.is_empty()
            })
        })
        .map(|a| &a.0)
        .collect();
    if !faltan.is_empty() {
        bail!(
            "No se borra nada: {} archivos ya no están en {} con su mismo MD5 (por ejemplo {})",
            faltan.len(),
            u.nombre,
            faltan[0]
        );
    }
    let raiz: String = sqlx::query_scalar("SELECT ruta FROM bibliotecas WHERE id = ?")
        .bind(biblioteca)
        .fetch_one(pool)
        .await?;
    let base = ruta_papelera(Path::new(&raiz), lote);
    let lista = archivos.clone();
    let b2 = base.clone();
    let hechos = tokio::task::spawn_blocking(move || {
        lista
            .into_iter()
            .map(|(ruta, _, _, _)| {
                let r = dentro(&b2, &ruta)
                    .and_then(|p| std::fs::remove_file(&p).map_err(|e| e.to_string()));
                (ruta, r)
            })
            .collect::<Vec<_>>()
    })
    .await?;
    let mut res = ResultadoVaciar::default();
    let mut tx = pool.begin().await?;
    for ((ruta, r), (_, bytes, _, nlink)) in hechos.into_iter().zip(&archivos) {
        if r.is_ok() {
            sqlx::query(
                "UPDATE papelera_archivos SET estado = 'borrado' WHERE lote_id = ? AND ruta = ?",
            )
            .bind(lote)
            .bind(&ruta)
            .execute(&mut *tx)
            .await?;
            res.borrados += 1;
            res.bytes += bytes;
            if *nlink > 1 {
                res.bytes_compartidos += bytes;
            }
        }
    }
    sqlx::query("UPDATE papelera_lotes SET estado = 'vaciado', cerrado = ? WHERE id = ?")
        .bind(ahora())
        .bind(lote)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    std::fs::remove_dir_all(&base).ok();
    // la papelera sin lotes no deja carpeta
    std::fs::remove_dir(Path::new(&raiz).join(PAPELERA)).ok();
    Ok(res)
}

// ---------- traer ----------

#[derive(Debug, Clone, sqlx::FromRow)]
struct CopiaTraer {
    id: i64,
    ruta: String,
    bytes: i64,
    md5: String,
    origen: String,
}

#[derive(Debug, Default, Serialize)]
pub struct ResultadoTraer {
    pub transferencia_id: i64,
    pub copias: usize,
    pub traidos: usize,
    pub ya_estaban: usize,
    pub extraidos: usize,
    pub conflictos: Vec<String>,
    pub errores: Vec<String>,
    pub avisos: Vec<String>,
    /// dónde se quedó lo que no se pudo poner en su lugar
    pub apartado_en: Option<String>,
}

/// A dónde va una copia: la ruta de la colección si es de la app y sigue dentro del modelo; si no, la carpeta
/// del modelo más su ruta relativa a la carpeta común de las copias de ese modelo.
fn destino_de(c: &CopiaTraer, carpeta: &str, comun: &str) -> String {
    if c.origen == "app" && c.ruta.starts_with(&format!("{carpeta}/")) {
        return c.ruta.clone();
    }
    let rel = c
        .ruta
        .strip_prefix(comun)
        .map(|r| r.trim_start_matches('/'))
        .filter(|r| !r.is_empty())
        .unwrap_or(&c.ruta);
    format!("{carpeta}/{rel}")
}

fn es_comprimido(ruta: &str) -> Option<&'static str> {
    let l = ruta.to_lowercase();
    if l.ends_with(".zip") {
        Some("zip")
    } else if [".rar", ".7z", ".gz", ".tgz", ".tar", ".xz"]
        .iter()
        .any(|e| l.ends_with(e))
    {
        Some("otro")
    } else {
        None
    }
}

/// Extrae un zip comprobando el CRC de cada entrada (al leerla completa). No deja salir de `destino`.
fn extraer_zip(zip: &Path, destino: &Path) -> Result<usize, String> {
    let f = std::fs::File::open(zip).map_err(|e| e.to_string())?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| format!("no es un zip válido: {e}"))?;
    let mut n = 0;
    for i in 0..z.len() {
        let mut e = z.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = e.enclosed_name() else {
            return Err(format!(
                "entrada con ruta peligrosa: {}",
                e.name().map(|n| n.to_string()).unwrap_or_default()
            ));
        };
        let fin = destino.join(rel);
        if e.is_dir() {
            std::fs::create_dir_all(&fin).map_err(|e| e.to_string())?;
            continue;
        }
        std::fs::create_dir_all(fin.parent().expect("tiene carpeta")).map_err(|e| e.to_string())?;
        let mut out = std::fs::File::create(&fin).map_err(|e| e.to_string())?;
        // io::copy lee hasta el final: el zip comprueba el CRC ahí y falla si no cuadra
        std::io::copy(&mut e, &mut out).map_err(|err| format!("{}: {err}", fin.display()))?;
        n += 1;
    }
    Ok(n)
}

/// Trae de una ubicación las copias de un modelo (todas o las elegidas) a su carpeta, comprobando el MD5 de cada
/// una (y el CRC de lo que viene en un zip), y vuelve a escanear.
pub async fn traer(
    pool: &SqlitePool,
    rclone: &str,
    transferencia: i64,
    copia_ids: Option<Vec<i64>>,
    progreso: Arc<AtomicU64>,
) -> anyhow::Result<ResultadoTraer> {
    let (modelo, ubicacion): (i64, i64) =
        sqlx::query_as("SELECT modelo_id, ubicacion_id FROM transferencias WHERE id = ?")
            .bind(transferencia)
            .fetch_one(pool)
            .await?;
    let r = traer_interno(
        pool,
        rclone,
        transferencia,
        modelo,
        ubicacion,
        copia_ids,
        progreso,
    )
    .await;
    if let Err(e) = &r {
        marcar_fallo(pool, transferencia, &format!("{e:#}")).await;
    }
    r
}

async fn traer_interno(
    pool: &SqlitePool,
    rclone: &str,
    transferencia: i64,
    modelo: i64,
    ubicacion: i64,
    copia_ids: Option<Vec<i64>>,
    progreso: Arc<AtomicU64>,
) -> anyhow::Result<ResultadoTraer> {
    let u = Ubicacion::cargar(pool, ubicacion)
        .await?
        .context("no existe esa ubicación")?;
    let alm = u.abrir(rclone)?;
    let (biblioteca, raiz) = principal(pool).await?;
    let carpeta: String = sqlx::query_scalar("SELECT carpeta FROM modelos WHERE id = ?")
        .bind(modelo)
        .fetch_one(pool)
        .await?;
    let todas: Vec<CopiaTraer> = sqlx::query_as(
        "SELECT id, ruta, bytes, md5, origen FROM copias_frio
         WHERE modelo_id = ? AND ubicacion_id = ? AND estado = 'presente' ORDER BY ruta",
    )
    .bind(modelo)
    .bind(ubicacion)
    .fetch_all(pool)
    .await?;
    let comun = carpeta_comun(todas.iter().map(|c| c.ruta.as_str()));
    let elegidas: Vec<CopiaTraer> = match &copia_ids {
        Some(ids) => {
            let ids: HashSet<i64> = ids.iter().copied().collect();
            todas
                .iter()
                .filter(|c| ids.contains(&c.id))
                .cloned()
                .collect()
        }
        None => todas.clone(),
    };
    if elegidas.is_empty() {
        bail!("no hay copias de este modelo en {} para traer", u.nombre);
    }
    let mut res = ResultadoTraer {
        transferencia_id: transferencia,
        copias: elegidas.len(),
        ..Default::default()
    };
    let staging_rel = format!("{ENTRANTE}/{transferencia}");
    let staging = raiz.join(&staging_rel);
    std::fs::create_dir_all(&staging)?;
    {
        let a = alm.clone();
        let rutas: Vec<String> = elegidas.iter().map(|c| c.ruta.clone()).collect();
        let s = staging.clone();
        let p = progreso.clone();
        tokio::task::spawn_blocking(move || a.bajar(&rutas, &s, &p))
            .await?
            .map_err(|e| anyhow!(e))?;
    }
    let raiz_b = raiz.clone();
    let carpeta_b = carpeta.clone();
    let lista = elegidas.clone();
    let s = staging.clone();
    // verificar y poner en su lugar, sin sobrescribir
    let hechos = tokio::task::spawn_blocking(move || {
        let mut out: Vec<(String, Result<&'static str, String>, usize)> = Vec::new();
        for c in &lista {
            let r = (|| -> Result<(&'static str, usize), String> {
                let bajado = dentro(&s, &c.ruta)?;
                let m = almacen::md5_de(&bajado).map_err(|e| format!("no llegó: {e}"))?;
                if !c.md5.is_empty() && m != c.md5 {
                    return Err("el MD5 de lo que llegó no coincide con el registrado".into());
                }
                let destino_rel = destino_de(c, &carpeta_b, &comun);
                match es_comprimido(&c.ruta) {
                    Some("zip") => {
                        let tallo = Path::new(&c.ruta)
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| "comprimido".into());
                        let padre = crate::estructura::padre(&destino_rel).to_string();
                        let fin_rel = if padre.is_empty() {
                            tallo.clone()
                        } else {
                            format!("{padre}/{tallo}")
                        };
                        let fin = dentro(&raiz_b, &fin_rel)?;
                        if fin.exists() {
                            return Err(format!("ya existe la carpeta {fin_rel}"));
                        }
                        let tmp = bajado.with_extension("extraido");
                        let n = extraer_zip(&bajado, &tmp)?;
                        std::fs::create_dir_all(fin.parent().expect("tiene carpeta"))
                            .map_err(|e| e.to_string())?;
                        std::fs::rename(&tmp, &fin).map_err(|e| e.to_string())?;
                        std::fs::remove_file(&bajado).ok();
                        Ok(("extraido", n))
                    }
                    _ => {
                        let fin = dentro(&raiz_b, &destino_rel)?;
                        if fin.exists() {
                            let igual = almacen::md5_de(&fin).map(|x| x == m).unwrap_or(false);
                            if igual {
                                std::fs::remove_file(&bajado).ok();
                                return Ok(("ya_estaba", 0));
                            }
                            return Err(format!("ya hay otro archivo en {destino_rel}"));
                        }
                        std::fs::create_dir_all(fin.parent().expect("tiene carpeta"))
                            .map_err(|e| e.to_string())?;
                        std::fs::rename(&bajado, &fin).map_err(|e| e.to_string())?;
                        Ok(("traido", 0))
                    }
                }
            })();
            match r {
                Ok((que, n)) => out.push((c.ruta.clone(), Ok(que), n)),
                Err(e) => out.push((c.ruta.clone(), Err(e), 0)),
            }
        }
        out
    })
    .await?;
    for (ruta, r, n) in hechos {
        match r {
            Ok("traido") => res.traidos += 1,
            Ok("ya_estaba") => res.ya_estaban += 1,
            Ok(_) => {
                res.traidos += 1;
                res.extraidos += n;
            }
            Err(e) if e.contains("ya hay") || e.contains("ya existe") => {
                res.conflictos.push(format!("{ruta}: {e}"))
            }
            Err(e) => res.errores.push(format!("{ruta}: {e}")),
        }
    }
    if elegidas
        .iter()
        .any(|c| es_comprimido(&c.ruta) == Some("otro"))
    {
        res.avisos
            .push("Los .rar, .7z y similares se traen tal cual, sin extraer.".into());
    }
    if elegidas.iter().any(|c| c.md5.is_empty()) {
        res.avisos.push(
            "Algunas copias no tenían MD5 registrado: se trajeron sin poder compararlas.".into(),
        );
    }
    // lo que se quedó en la carpeta de entrada (conflictos o errores)
    let quedan = walkdir::WalkDir::new(&staging)
        .into_iter()
        .filter_map(Result::ok)
        .any(|e| e.file_type().is_file());
    if quedan {
        res.apartado_en = Some(staging_rel);
    } else {
        std::fs::remove_dir_all(&staging).ok();
        std::fs::remove_dir(raiz.join(ENTRANTE)).ok();
    }
    // el escaneo registra lo que llegó
    crate::inventario::escanear(pool, biblioteca, Arc::new(AtomicU64::new(0))).await?;
    sqlx::query(
        "UPDATE copias_frio SET archivo_id = (SELECT a.id FROM archivos a WHERE a.biblioteca_id = ?
                                              AND a.estado = 'presente' AND a.ruta = copias_frio.ruta)
         WHERE modelo_id = ? AND ubicacion_id = ? AND archivo_id IS NULL",
    )
    .bind(biblioteca)
    .bind(modelo)
    .bind(ubicacion)
    .execute(pool)
    .await?;
    crate::catalogo::actualizar_estadisticas(pool).await?;
    let estado = if res.errores.is_empty() && res.conflictos.is_empty() {
        "terminada"
    } else {
        "con_errores"
    };
    cerrar_transferencia(
        pool,
        transferencia,
        estado,
        res.copias,
        elegidas.iter().map(|c| c.bytes).sum(),
        res.traidos + res.ya_estaban,
        None,
        &res,
    )
    .await?;
    Ok(res)
}

// ---------- listar una ubicación ----------

#[derive(Debug, Default, Serialize)]
pub struct ResultadoListar {
    pub archivos: usize,
    pub nuevos: usize,
    pub ya_no_estan: u64,
    pub ligados: usize,
}

/// Lee todo lo que hay en una ubicación (con su MD5) y pone al día sus copias: las nuevas se ligan al modelo
/// cuya carpeta las contiene; las que ya no aparecen quedan como «falta».
pub async fn listar_ubicacion(
    pool: &SqlitePool,
    rclone: &str,
    ubicacion: i64,
) -> anyhow::Result<ResultadoListar> {
    let u = Ubicacion::cargar(pool, ubicacion)
        .await?
        .context("no existe esa ubicación")?;
    let alm = u.abrir(rclone)?;
    let todo = listar(&alm, "").await?;
    let modelos: HashMap<String, i64> =
        sqlx::query_as::<_, (String, i64)>("SELECT carpeta, id FROM modelos")
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect();
    let existentes: HashSet<String> =
        sqlx::query_scalar::<_, String>("SELECT ruta FROM copias_frio WHERE ubicacion_id = ?")
            .bind(ubicacion)
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect();
    let t = ahora();
    let mut res = ResultadoListar {
        archivos: todo.len(),
        ..Default::default()
    };
    let mut tx = pool.begin().await?;
    for x in todo.values() {
        // el modelo: la carpeta más larga de la colección que contiene esta ruta
        let mut modelo = None;
        let mut p = x.ruta.as_str();
        while let Some(i) = p.rfind('/') {
            p = &p[..i];
            if let Some(m) = modelos.get(p) {
                modelo = Some(*m);
                break;
            }
        }
        if !existentes.contains(&x.ruta) {
            res.nuevos += 1;
            if modelo.is_some() {
                res.ligados += 1;
            }
        }
        sqlx::query(
            "INSERT INTO copias_frio (ubicacion_id, ruta, bytes, md5, modelo_id, estado, visto_en, origen)
             VALUES (?, ?, ?, ?, ?, 'presente', ?, 'listado')
             ON CONFLICT (ubicacion_id, ruta) DO UPDATE SET bytes = excluded.bytes,
                md5 = CASE WHEN excluded.md5 <> '' THEN excluded.md5 ELSE copias_frio.md5 END,
                modelo_id = coalesce(copias_frio.modelo_id, excluded.modelo_id),
                estado = 'presente', visto_en = excluded.visto_en",
        )
        .bind(ubicacion)
        .bind(&x.ruta)
        .bind(x.bytes)
        .bind(x.md5.clone().unwrap_or_default())
        .bind(modelo)
        .bind(t)
        .execute(&mut *tx)
        .await?;
    }
    // las que estaban y ya no aparecen
    let presentes: Vec<String> = sqlx::query_scalar(
        "SELECT ruta FROM copias_frio WHERE ubicacion_id = ? AND estado = 'presente'",
    )
    .bind(ubicacion)
    .fetch_all(&mut *tx)
    .await?;
    for ruta in presentes.iter().filter(|r| !todo.contains_key(*r)) {
        res.ya_no_estan += sqlx::query(
            "UPDATE copias_frio SET estado = 'falta' WHERE ubicacion_id = ? AND ruta = ?",
        )
        .bind(ubicacion)
        .bind(ruta)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    }
    sqlx::query("UPDATE ubicaciones_frio SET ultimo_listado = ? WHERE id = ?")
        .bind(t)
        .bind(ubicacion)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    crate::catalogo::actualizar_estadisticas(pool).await?;
    Ok(res)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn carpetas_comunes_y_destinos() {
        assert_eq!(carpeta_comun(["a/b/x.stl", "a/b/c/y.stl"]), "a/b");
        assert_eq!(carpeta_comun(["a/x.stl"]), "a");
        assert_eq!(carpeta_comun(["x.stl", "a/y.stl"]), "");
        let c = |ruta: &str, origen: &str| CopiaTraer {
            id: 1,
            ruta: ruta.into(),
            bytes: 1,
            md5: String::new(),
            origen: origen.into(),
        };
        // de la app y dentro del modelo: a su misma ruta
        assert_eq!(
            destino_de(&c("W/M/Sup/a.stl", "app"), "W/M", "W/M"),
            "W/M/Sup/a.stl"
        );
        // importada con otra ruta (de antes de la reorganización): a la carpeta del modelo
        assert_eq!(
            destino_de(
                &c("Viejo/Studio/M/Sup/a.lys", "importada"),
                "W/M",
                "Viejo/Studio/M"
            ),
            "W/M/Sup/a.lys"
        );
        assert_eq!(es_comprimido("a/b.ZIP"), Some("zip"));
        assert_eq!(es_comprimido("a/b.rar"), Some("otro"));
        assert_eq!(es_comprimido("a/b.stl"), None);
    }
}
