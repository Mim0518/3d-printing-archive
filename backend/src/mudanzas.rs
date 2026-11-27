//! Mudanzas: mover la carpeta de un modelo a su ruta nueva (cuando se cambiaron sus campos en el catálogo).
//!
//! Igual que el ejecutor de la fase 8 (`mover_fase8.py`):
//! - primero se arma un plan archivo por archivo y se revisan choques (también por mayúsculas), cadenas y nombres
//!   que NTFS no acepta;
//! - al ejecutar se vuelve a revisar todo contra el disco (tamaño e inode de cada origen, destino libre); si algo
//!   no cuadra, no se toca nada;
//! - cada archivo se mueve con un enlace duro al destino y luego se quita el nombre viejo: nunca se sobrescribe y no
//!   se copia ni un byte;
//! - se registra archivo por archivo (por tandas) y la última mudanza se puede deshacer.
//!
//! Solo se mueven los archivos que están dentro de la carpeta de la fila; lo suelto en la carpeta del modelo (arriba
//! de una subcarpeta) se queda y el plan lo dice.

use std::{
    collections::{HashMap, HashSet},
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::{
    catalogo,
    db::ahora,
    edicion::{self, Campos},
    error::ApiError,
    estructura::{SUELTOS, padre},
};

/// «X:\Hobby\3DP\» para estimar la ruta completa en Windows.
const RAIZ_WIN: usize = 13;
const TANDA: usize = 200;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModeloPlan {
    pub id: i64,
    pub nombre: String,
    pub de: String,
    pub a: String,
    pub archivos: u64,
    pub bytes: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ResumenPlan {
    pub id: i64,
    pub modelos: Vec<ModeloPlan>,
    pub archivos: u64,
    pub bytes: i64,
    /// Si hay alguno, la mudanza no se puede ejecutar.
    pub conflictos: Vec<String>,
    pub avisos: Vec<String>,
    /// Archivos sueltos en la carpeta del modelo, fuera de la carpeta de la fila: no se mueven.
    pub se_quedan: u64,
    /// Destinos de más de 260 caracteres en Windows (no impide mover).
    pub rutas_largas: u64,
}

#[derive(Debug, Serialize, Default)]
pub struct ResumenEjecucion {
    pub movidos: u64,
    pub errores: Vec<String>,
    pub carpetas_quitadas: u64,
}

struct Item {
    archivo_id: i64,
    modelo_id: i64,
    origen: String,
    destino: String,
    bytes: i64,
    inode: i64,
}

fn relativa_segura(r: &str) -> bool {
    !r.is_empty()
        && !r.starts_with('/')
        && r.split('/').all(|s| !s.is_empty() && s != "." && s != "..")
}

async fn raiz_principal(pool: &SqlitePool) -> Result<(i64, PathBuf), ApiError> {
    let fila: Option<(i64, String)> = sqlx::query_as(
        "SELECT id, ruta FROM bibliotecas WHERE tipo = 'principal' ORDER BY id LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;
    let (id, ruta) =
        fila.ok_or_else(|| ApiError::Invalido("No hay biblioteca principal configurada".into()))?;
    Ok((id, PathBuf::from(ruta)))
}

/// ¿Un modelo que va a `destino` choca con la carpeta `o` de otro? Una fila de sueltos solo ocupa los archivos
/// sueltos de su carpeta: se puede mover algo a una subcarpeta suya, pero no a esa misma carpeta ni a una de arriba.
pub fn choca(o: &str, sueltos: bool, destino: &str) -> bool {
    o == destino
        || o.starts_with(&format!("{destino}/"))
        || (!sueltos && destino.starts_with(&format!("{o}/")))
}

/// Arma el plan para los modelos indicados (o para todos los pendientes ya revisados) y lo guarda.
pub async fn armar(
    pool: &SqlitePool,
    ids: Option<&[i64]>,
    usuario: &str,
) -> Result<ResumenPlan, ApiError> {
    let (bib, _) = raiz_principal(pool).await?;
    let todos: Vec<Campos> = sqlx::query_as(
        "SELECT id, carpeta, juego, faccion, nombre, estudio, subcarpeta, confianza, notas, revisado FROM modelos",
    )
    .fetch_all(pool)
    .await?;
    let candidatos: Vec<&Campos> = todos
        .iter()
        .filter(|m| edicion::esta_pendiente(m))
        .filter(|m| match ids {
            Some(ids) => ids.contains(&m.id),
            None => m.revisado == "OK" || m.revisado == "Corregido",
        })
        .collect();
    if candidatos.is_empty() {
        return Err(ApiError::Invalido(
            "No hay modelos pendientes de mover (revisados con OK o Corregido)".into(),
        ));
    }
    let moviendo: HashSet<i64> = candidatos.iter().map(|m| m.id).collect();

    let archivos: Vec<(i64, String, i64, i64, Option<i64>)> = sqlx::query_as(
        "SELECT id, ruta, bytes, inode, modelo_id FROM archivos WHERE biblioteca_id = ? AND estado = 'presente'",
    )
    .bind(bib)
    .fetch_all(pool)
    .await?;
    let rutas: HashSet<&str> = archivos.iter().map(|a| a.1.as_str()).collect();

    let mut res = ResumenPlan::default();
    let mut items: Vec<Item> = Vec::new();
    // carpetas que quedan ocupadas: las de los modelos que no se mueven y los destinos de los que sí
    // (carpeta, nombre, ¿solo los sueltos de la carpeta?)
    let mut ocupadas: Vec<(String, String, bool)> = todos
        .iter()
        .filter(|o| !moviendo.contains(&o.id))
        .map(|o| {
            (
                o.carpeta
                    .strip_suffix(SUELTOS)
                    .unwrap_or(&o.carpeta)
                    .to_string(),
                o.nombre.clone(),
                o.carpeta.ends_with(SUELTOS),
            )
        })
        .collect();
    for m in &candidatos {
        let j = edicion::ruta_de(m);
        let k = m.carpeta.as_str();
        let nombre = if m.nombre.is_empty() {
            m.faccion.clone()
        } else {
            m.nombre.clone()
        };
        if let Err(e) = j.split('/').try_for_each(|s| edicion::validar("nombre", s)) {
            res.conflictos
                .push(format!("«{nombre}»: la ruta nueva no es válida ({e})"));
            continue;
        }
        for (o, onombre, sueltos) in &ocupadas {
            if choca(o, *sueltos, &j) {
                res.conflictos.push(format!(
                    "«{nombre}» iría a «{j}», que choca con la carpeta de «{onombre}» ({o})"
                ));
            }
        }
        ocupadas.push((j.clone(), nombre.clone(), false));
        let mut mp = ModeloPlan {
            id: m.id,
            nombre: nombre.clone(),
            de: k.to_string(),
            a: j.clone(),
            archivos: 0,
            bytes: 0,
        };
        for (aid, ruta, bytes, inode, _) in archivos.iter().filter(|a| a.4 == Some(m.id)) {
            let destino = if let Some(d) = k.strip_suffix(SUELTOS) {
                (padre(ruta) == d)
                    .then(|| format!("{j}/{}", ruta.rsplit('/').next().unwrap_or(ruta)))
            } else if rutas.contains(k) {
                (ruta == k).then(|| format!("{j}/{}", ruta.rsplit('/').next().unwrap_or(ruta)))
            } else {
                ruta.strip_prefix(&format!("{k}/"))
                    .map(|rel| format!("{j}/{rel}"))
            };
            let Some(destino) = destino else {
                res.se_quedan += 1;
                continue;
            };
            mp.archivos += 1;
            mp.bytes += bytes;
            items.push(Item {
                archivo_id: *aid,
                modelo_id: m.id,
                origen: ruta.clone(),
                destino,
                bytes: *bytes,
                inode: *inode,
            });
        }
        res.archivos += mp.archivos;
        res.bytes += mp.bytes;
        res.modelos.push(mp);
    }

    choques(&items, &archivos, &mut res);
    guardar(pool, &mut res, &items, true, usuario).await?;
    Ok(res)
}

/// Choques archivo por archivo (también por mayúsculas), rutas largas y avisos.
fn choques(
    items: &[Item],
    archivos: &[(i64, String, i64, i64, Option<i64>)],
    res: &mut ResumenPlan,
) {
    let origenes: HashSet<String> = items.iter().map(|i| i.origen.to_lowercase()).collect();
    let existentes: HashSet<String> = archivos.iter().map(|a| a.1.to_lowercase()).collect();
    let mut vistos: HashMap<String, &str> = HashMap::new();
    for i in items {
        let d = i.destino.to_lowercase();
        if !relativa_segura(&i.destino) || !relativa_segura(&i.origen) {
            res.conflictos
                .push(format!("ruta no válida: {}", i.destino));
        }
        if existentes.contains(&d) && !origenes.contains(&d) {
            res.conflictos
                .push(format!("ya existe un archivo en {}", i.destino));
        } else if origenes.contains(&d) && d != i.origen.to_lowercase() {
            res.conflictos.push(format!(
                "{} es el origen de otro archivo que también se mueve",
                i.destino
            ));
        }
        if let Some(otro) = vistos.insert(d.clone(), &i.origen) {
            res.conflictos.push(format!(
                "{} y {} irían al mismo lugar: {}",
                otro, i.origen, i.destino
            ));
        }
        let mut a = padre(&d);
        while !a.is_empty() {
            if existentes.contains(a) {
                res.conflictos
                    .push(format!("hay un archivo donde iría la carpeta {a}"));
                break;
            }
            a = padre(a);
        }
        if RAIZ_WIN + i.destino.chars().count() > 260 {
            res.rutas_largas += 1;
        }
    }
    if res.rutas_largas > 0 {
        res.avisos.push(format!(
            "{} archivos quedarían con rutas de más de 260 caracteres en Windows",
            res.rutas_largas
        ));
    }
    if res.se_quedan > 0 {
        res.avisos.push(format!(
            "{} archivos sueltos en la carpeta del modelo (fuera de la subcarpeta de la fila) se quedan donde están",
            res.se_quedan
        ));
    }
    res.conflictos.sort();
    res.conflictos.dedup();
}

/// Guarda el plan (y descarta el anterior). Con `cambian`, los modelos del plan cambian de carpeta al ejecutarlo.
async fn guardar(
    pool: &SqlitePool,
    res: &mut ResumenPlan,
    items: &[Item],
    cambian: bool,
    usuario: &str,
) -> Result<(), ApiError> {
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE mudanzas SET estado = 'descartada' WHERE estado = 'plan'")
        .execute(&mut *tx)
        .await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO mudanzas (estado, usuario, creada) VALUES ('plan', ?, ?) RETURNING id",
    )
    .bind(usuario)
    .bind(ahora())
    .fetch_one(&mut *tx)
    .await?;
    for m in res.modelos.iter().filter(|_| cambian) {
        sqlx::query(
            "INSERT INTO mudanza_modelos (mudanza_id, modelo_id, carpeta_antes, carpeta_despues) VALUES (?, ?, ?, ?)",
        )
        .bind(id)
        .bind(m.id)
        .bind(&m.de)
        .bind(&m.a)
        .execute(&mut *tx)
        .await?;
    }
    for i in items {
        sqlx::query(
            "INSERT INTO mudanza_archivos (mudanza_id, archivo_id, modelo_id, origen, destino, bytes, inode)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(i.archivo_id)
        .bind(i.modelo_id)
        .bind(&i.origen)
        .bind(&i.destino)
        .bind(i.bytes)
        .bind(i.inode)
        .execute(&mut *tx)
        .await?;
    }
    res.id = id;
    sqlx::query("UPDATE mudanzas SET resumen = ? WHERE id = ?")
        .bind(serde_json::to_string(&res).unwrap_or_default())
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Plan para juntar archivos que no son de ningún modelo (un grupo de la bandeja, `clave`) con un modelo que ya
/// está en su lugar: van a `destino` (la carpeta del modelo o una subcarpeta suya) y el modelo no cambia de carpeta.
pub async fn armar_juntar(
    pool: &SqlitePool,
    modelo: i64,
    archivo_ids: &[i64],
    clave: &str,
    destino: &str,
    usuario: &str,
) -> Result<ResumenPlan, ApiError> {
    let (bib, _) = raiz_principal(pool).await?;
    let m = edicion::campos(pool, modelo)
        .await?
        .ok_or(ApiError::NoEncontrado)?;
    let archivos: Vec<(i64, String, i64, i64, Option<i64>)> = sqlx::query_as(
        "SELECT id, ruta, bytes, inode, modelo_id FROM archivos WHERE biblioteca_id = ? AND estado = 'presente'",
    )
    .bind(bib)
    .fetch_all(pool)
    .await?;
    let rutas: HashSet<&str> = archivos.iter().map(|a| a.1.as_str()).collect();
    let base = m.carpeta.as_str();
    if base.ends_with(SUELTOS) || rutas.contains(base) || edicion::esta_pendiente(&m) {
        return Err(ApiError::Invalido(format!(
            "«{}» no tiene una carpeta propia en su lugar; no se le pueden juntar archivos",
            m.nombre
        )));
    }
    if destino != base && !destino.starts_with(&format!("{base}/")) {
        return Err(ApiError::Invalido(format!(
            "«{destino}» no está dentro de la carpeta de «{}»",
            m.nombre
        )));
    }
    let mut res = ResumenPlan::default();
    let nombre = if m.nombre.is_empty() {
        m.faccion.clone()
    } else {
        m.nombre.clone()
    };
    if let Err(e) = destino
        .split('/')
        .try_for_each(|s| edicion::validar("nombre", s))
    {
        res.conflictos
            .push(format!("la carpeta destino no es válida ({e})"));
    }
    let otros: Vec<(String, String)> =
        sqlx::query_as("SELECT carpeta, nombre FROM modelos WHERE id <> ?")
            .bind(modelo)
            .fetch_all(pool)
            .await?;
    for (o, onombre) in &otros {
        let sueltos = o.ends_with(SUELTOS);
        let o = o.strip_suffix(SUELTOS).unwrap_or(o);
        // un modelo que ya contiene al de destino no estorba
        if base.starts_with(&format!("{o}/")) {
            continue;
        }
        if choca(o, sueltos, destino) {
            res.conflictos.push(format!(
                "«{destino}» choca con la carpeta de «{onombre}» ({o})"
            ));
        }
    }
    let ids: HashSet<i64> = archivo_ids.iter().copied().collect();
    let del_grupo: Vec<&(i64, String, i64, i64, Option<i64>)> = archivos
        .iter()
        .filter(|a| ids.contains(&a.0) && a.4.is_none())
        .collect();
    if del_grupo.len() != ids.len() {
        return Err(ApiError::Conflicto(
            "los archivos del grupo cambiaron; vuelve a cargar la bandeja".into(),
        ));
    }
    let mut items = Vec::new();
    let mut mp = ModeloPlan {
        id: m.id,
        nombre,
        de: clave.to_string(),
        a: destino.to_string(),
        archivos: 0,
        bytes: 0,
    };
    let k = clave.strip_suffix(SUELTOS).unwrap_or(clave);
    for (aid, ruta, bytes, inode, _) in del_grupo {
        let nombre_archivo = ruta.rsplit('/').next().unwrap_or(ruta);
        let rel = if clave.ends_with(SUELTOS) || ruta == clave {
            nombre_archivo
        } else {
            ruta.strip_prefix(&format!("{k}/"))
                .unwrap_or(nombre_archivo)
        };
        mp.archivos += 1;
        mp.bytes += bytes;
        items.push(Item {
            archivo_id: *aid,
            modelo_id: m.id,
            origen: ruta.clone(),
            destino: format!("{destino}/{rel}"),
            bytes: *bytes,
            inode: *inode,
        });
    }
    res.archivos = mp.archivos;
    res.bytes = mp.bytes;
    res.modelos.push(mp);
    choques(&items, &archivos, &mut res);
    guardar(pool, &mut res, &items, false, usuario).await?;
    Ok(res)
}

#[derive(sqlx::FromRow, Clone)]
struct FilaItem {
    id: i64,
    archivo_id: Option<i64>,
    origen: String,
    destino: String,
    bytes: i64,
    inode: i64,
}

/// Problemas que impiden mover un archivo (revisión contra el disco antes de tocar nada).
fn revisar_item(
    raiz: &Path,
    origen: &str,
    destino: &str,
    bytes: i64,
    inode: i64,
) -> Option<String> {
    let o = raiz.join(origen);
    let d = raiz.join(destino);
    match fs::symlink_metadata(&o) {
        Ok(m) if m.is_file() && m.ino() as i64 == inode && m.len() as i64 == bytes => {}
        Ok(_) => {
            return Some(format!(
                "{origen}: cambió desde el escaneo (tamaño o inode distinto)"
            ));
        }
        Err(_) => return Some(format!("{origen}: ya no está")),
    }
    if fs::symlink_metadata(&d).is_ok() {
        return Some(format!("{destino}: ya existe"));
    }
    // choque por mayúsculas con lo que ya hay en la carpeta destino
    if let (Some(dir), Some(nombre)) = (d.parent(), d.file_name())
        && let Ok(rd) = fs::read_dir(dir)
    {
        let n = nombre.to_string_lossy().to_lowercase();
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().to_lowercase() == n {
                return Some(format!(
                    "{destino}: ya hay «{}» (solo cambia en mayúsculas)",
                    e.file_name().to_string_lossy()
                ));
            }
        }
    }
    None
}

/// Mueve con enlace duro + quitar el nombre viejo. Nunca sobrescribe.
fn mover(raiz: &Path, de: &str, a: &str) -> Result<(), String> {
    let o = raiz.join(de);
    let d = raiz.join(a);
    if let Some(p) = d.parent() {
        fs::create_dir_all(p).map_err(|e| format!("no se pudo crear la carpeta: {e}"))?;
    }
    fs::hard_link(&o, &d).map_err(|e| format!("no se pudo enlazar: {e}"))?;
    fs::remove_file(&o)
        .map_err(|e| format!("quedó con los dos nombres; no se pudo quitar el viejo: {e}"))
}

/// Quita las carpetas que quedaron vacías, subiendo hasta el primer nivel (Wargames, DND… nunca se quitan).
fn quitar_vacias(raiz: &Path, carpetas: &HashSet<String>) -> u64 {
    let mut todas: HashSet<String> = HashSet::new();
    for c in carpetas {
        let mut a = c.as_str();
        while a.contains('/') {
            todas.insert(a.to_string());
            a = padre(a);
        }
    }
    let mut orden: Vec<&String> = todas.iter().collect();
    orden.sort_by_key(|c| std::cmp::Reverse(c.matches('/').count()));
    let mut n = 0;
    for c in orden {
        if fs::remove_dir(raiz.join(c)).is_ok() {
            n += 1;
        }
    }
    n
}

/// Revisa el plan contra el catálogo y el disco. Devuelve la lista de problemas (vacía = se puede ejecutar).
async fn revisar(pool: &SqlitePool, raiz: &Path, mudanza: i64) -> Result<Vec<String>, ApiError> {
    let mut problemas = Vec::new();
    let modelos: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT modelo_id, carpeta_antes, carpeta_despues FROM mudanza_modelos WHERE mudanza_id = ?",
    )
    .bind(mudanza)
    .fetch_all(pool)
    .await?;
    for (id, antes, despues) in &modelos {
        let actual = edicion::campos(pool, *id).await?;
        match actual {
            Some(m) if m.carpeta == *antes && edicion::ruta_de(&m) == *despues => {}
            _ => problemas.push(format!(
                "el modelo {id} cambió desde que se armó el plan; vuelve a armarlo"
            )),
        }
    }
    let items: Vec<FilaItem> = sqlx::query_as(
        "SELECT id, archivo_id, origen, destino, bytes, inode FROM mudanza_archivos WHERE mudanza_id = ? ORDER BY id",
    )
    .bind(mudanza)
    .fetch_all(pool)
    .await?;
    let raiz = raiz.to_path_buf();
    let mut p2 = tokio::task::spawn_blocking(move || {
        items
            .iter()
            .filter_map(|i| revisar_item(&raiz, &i.origen, &i.destino, i.bytes, i.inode))
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| ApiError::Interno(e.into()))?;
    problemas.append(&mut p2);
    Ok(problemas)
}

pub async fn ejecutar(
    pool: &SqlitePool,
    mudanza: i64,
    progreso: Arc<AtomicU64>,
) -> anyhow::Result<ResumenEjecucion> {
    let (bib, raiz) = raiz_principal(pool)
        .await
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let (estado, resumen): (String, String) =
        sqlx::query_as("SELECT estado, resumen FROM mudanzas WHERE id = ?")
            .bind(mudanza)
            .fetch_one(pool)
            .await?;
    anyhow::ensure!(
        estado == "plan",
        "la mudanza ya no es un plan (está «{estado}»)"
    );
    let plan: ResumenPlan = serde_json::from_str(&resumen).unwrap_or_default();
    anyhow::ensure!(
        plan.conflictos.is_empty(),
        "el plan tiene conflictos; corrígelos y vuelve a armarlo"
    );
    let problemas = revisar(pool, &raiz, mudanza)
        .await
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    if !problemas.is_empty() {
        anyhow::bail!(
            "{} problemas; no se movió nada: {}",
            problemas.len(),
            problemas
                .iter()
                .take(10)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
    sqlx::query("UPDATE mudanzas SET estado = 'en_curso' WHERE id = ?")
        .bind(mudanza)
        .execute(pool)
        .await?;
    let items: Vec<FilaItem> = sqlx::query_as(
        "SELECT id, archivo_id, origen, destino, bytes, inode FROM mudanza_archivos WHERE mudanza_id = ? ORDER BY id",
    )
    .bind(mudanza)
    .fetch_all(pool)
    .await?;
    let mut res = ResumenEjecucion::default();
    let mut origenes: HashSet<String> = HashSet::new();
    for tanda in items.chunks(TANDA) {
        let tanda = tanda.to_vec();
        let r2 = raiz.clone();
        let resultados = tokio::task::spawn_blocking(move || {
            tanda
                .into_iter()
                .map(|i| {
                    let r = mover(&r2, &i.origen, &i.destino);
                    (i, r)
                })
                .collect::<Vec<_>>()
        })
        .await?;
        let mut tx = pool.begin().await?;
        for (i, r) in resultados {
            match r {
                Ok(()) => {
                    res.movidos += 1;
                    origenes.insert(padre(&i.origen).to_string());
                    sqlx::query("DELETE FROM archivos WHERE biblioteca_id = ? AND ruta = ? AND estado = 'falta'")
                        .bind(bib)
                        .bind(&i.destino)
                        .execute(&mut *tx)
                        .await?;
                    if let Some(aid) = i.archivo_id {
                        sqlx::query(
                            "UPDATE archivos SET ruta = ?, carpeta = ?, nombre = ? WHERE id = ?",
                        )
                        .bind(&i.destino)
                        .bind(padre(&i.destino))
                        .bind(i.destino.rsplit('/').next().unwrap_or(&i.destino))
                        .bind(aid)
                        .execute(&mut *tx)
                        .await?;
                    }
                    sqlx::query("UPDATE mudanza_archivos SET estado = 'movido' WHERE id = ?")
                        .bind(i.id)
                        .execute(&mut *tx)
                        .await?;
                }
                Err(e) => {
                    res.errores.push(format!("{}: {e}", i.origen));
                    sqlx::query(
                        "UPDATE mudanza_archivos SET estado = 'error', mensaje = ? WHERE id = ?",
                    )
                    .bind(&e)
                    .bind(i.id)
                    .execute(&mut *tx)
                    .await?;
                }
            }
            progreso.fetch_add(1, Ordering::Relaxed);
        }
        tx.commit().await?;
    }
    let r2 = raiz.clone();
    res.carpetas_quitadas =
        tokio::task::spawn_blocking(move || quitar_vacias(&r2, &origenes)).await?;

    // los modelos que se movieron completos cambian de carpeta
    let modelos: Vec<(i64, String, String, i64)> = sqlx::query_as(
        "SELECT mm.modelo_id, mm.carpeta_antes, mm.carpeta_despues,
                (SELECT count(*) FROM mudanza_archivos ma
                 WHERE ma.mudanza_id = mm.mudanza_id AND ma.modelo_id = mm.modelo_id AND ma.estado <> 'movido')
         FROM mudanza_modelos mm WHERE mm.mudanza_id = ?",
    )
    .bind(mudanza)
    .fetch_all(pool)
    .await?;
    let usuario: String = sqlx::query_scalar("SELECT usuario FROM mudanzas WHERE id = ?")
        .bind(mudanza)
        .fetch_one(pool)
        .await?;
    let mut tx = pool.begin().await?;
    for (id, antes, despues, fallidos) in &modelos {
        if *fallidos == 0 {
            sqlx::query("UPDATE modelos SET carpeta = ?, actualizado = ? WHERE id = ?")
                .bind(despues)
                .bind(ahora())
                .bind(id)
                .execute(&mut *tx)
                .await?;
            sqlx::query(
                "INSERT INTO historial (modelo_id, campo, antes, despues, usuario, fecha) VALUES (?, 'carpeta', ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(antes)
            .bind(despues)
            .bind(&usuario)
            .bind(ahora())
            .execute(&mut *tx)
            .await?;
        }
    }
    sqlx::query("UPDATE mudanzas SET estado = ?, ejecutada = ? WHERE id = ?")
        .bind(if res.errores.is_empty() {
            "ejecutada"
        } else {
            "con_errores"
        })
        .bind(ahora())
        .bind(mudanza)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    rehacer_indices(pool).await?;
    Ok(res)
}

async fn rehacer_indices(pool: &SqlitePool) -> anyhow::Result<()> {
    catalogo::ligar_archivos(pool).await?;
    catalogo::actualizar_estadisticas(pool).await?;
    edicion::recalcular(pool, None).await?;
    catalogo::reconstruir_busqueda(pool).await?;
    Ok(())
}

/// La mudanza que se puede deshacer: la última ejecutada que no se ha deshecho.
pub async fn deshacible(pool: &SqlitePool) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM mudanzas WHERE estado IN ('ejecutada', 'con_errores') ORDER BY ejecutada DESC, id DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
}

pub async fn deshacer(
    pool: &SqlitePool,
    mudanza: i64,
    progreso: Arc<AtomicU64>,
) -> anyhow::Result<ResumenEjecucion> {
    let (_, raiz) = raiz_principal(pool)
        .await
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    anyhow::ensure!(
        deshacible(pool).await? == Some(mudanza),
        "solo se puede deshacer la última mudanza ejecutada"
    );
    let items: Vec<FilaItem> = sqlx::query_as(
        "SELECT id, archivo_id, origen, destino, bytes, inode FROM mudanza_archivos
         WHERE mudanza_id = ? AND estado = 'movido' ORDER BY id DESC",
    )
    .bind(mudanza)
    .fetch_all(pool)
    .await?;
    // al revés: el destino debe ser el mismo archivo y el origen debe estar libre
    let r2 = raiz.clone();
    let i2 = items.clone();
    let problemas: Vec<String> = tokio::task::spawn_blocking(move || {
        i2.iter()
            .filter_map(|i| revisar_item(&r2, &i.destino, &i.origen, i.bytes, i.inode))
            .collect()
    })
    .await?;
    if !problemas.is_empty() {
        anyhow::bail!(
            "{} problemas; no se deshizo nada: {}",
            problemas.len(),
            problemas
                .iter()
                .take(10)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
    let mut res = ResumenEjecucion::default();
    let mut destinos: HashSet<String> = HashSet::new();
    for tanda in items.chunks(TANDA) {
        let tanda = tanda.to_vec();
        let r2 = raiz.clone();
        let resultados = tokio::task::spawn_blocking(move || {
            tanda
                .into_iter()
                .map(|i| {
                    let r = mover(&r2, &i.destino, &i.origen);
                    (i, r)
                })
                .collect::<Vec<_>>()
        })
        .await?;
        let mut tx = pool.begin().await?;
        for (i, r) in resultados {
            match r {
                Ok(()) => {
                    res.movidos += 1;
                    destinos.insert(padre(&i.destino).to_string());
                    if let Some(aid) = i.archivo_id {
                        sqlx::query(
                            "UPDATE archivos SET ruta = ?, carpeta = ?, nombre = ? WHERE id = ?",
                        )
                        .bind(&i.origen)
                        .bind(padre(&i.origen))
                        .bind(i.origen.rsplit('/').next().unwrap_or(&i.origen))
                        .bind(aid)
                        .execute(&mut *tx)
                        .await?;
                    }
                    sqlx::query("UPDATE mudanza_archivos SET estado = 'deshecho' WHERE id = ?")
                        .bind(i.id)
                        .execute(&mut *tx)
                        .await?;
                }
                Err(e) => res.errores.push(format!("{}: {e}", i.destino)),
            }
            progreso.fetch_add(1, Ordering::Relaxed);
        }
        tx.commit().await?;
    }
    let r2 = raiz.clone();
    res.carpetas_quitadas =
        tokio::task::spawn_blocking(move || quitar_vacias(&r2, &destinos)).await?;
    let usuario: String = sqlx::query_scalar("SELECT usuario FROM mudanzas WHERE id = ?")
        .bind(mudanza)
        .fetch_one(pool)
        .await?;
    let modelos: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT modelo_id, carpeta_antes, carpeta_despues FROM mudanza_modelos WHERE mudanza_id = ?",
    )
    .bind(mudanza)
    .fetch_all(pool)
    .await?;
    let mut tx = pool.begin().await?;
    for (id, antes, despues) in &modelos {
        let r = sqlx::query(
            "UPDATE modelos SET carpeta = ?, actualizado = ? WHERE id = ? AND carpeta = ?",
        )
        .bind(antes)
        .bind(ahora())
        .bind(id)
        .bind(despues)
        .execute(&mut *tx)
        .await?;
        if r.rows_affected() > 0 {
            sqlx::query(
                "INSERT INTO historial (modelo_id, campo, antes, despues, usuario, fecha) VALUES (?, 'carpeta', ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(despues)
            .bind(antes)
            .bind(&usuario)
            .bind(ahora())
            .execute(&mut *tx)
            .await?;
        }
    }
    sqlx::query("UPDATE mudanzas SET estado = 'deshecha', deshecha = ? WHERE id = ?")
        .bind(ahora())
        .bind(mudanza)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    rehacer_indices(pool).await?;
    Ok(res)
}
