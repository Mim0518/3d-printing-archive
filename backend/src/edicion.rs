//! Edición del catálogo (la app reemplaza a la hoja): validar y guardar cambios con historial, etiquetas, y
//! recalcular la ruta nueva, si el modelo está pendiente de mover y sus avisos (reglas de nombres acordadas).

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::{
    catalogo::separar_etiquetas,
    db::ahora,
    error::ApiError,
    estructura::{SUELTOS, ruta_nueva},
};

pub const REVISADO: [&str; 4] = ["", "OK", "Corregido", "No mover"];
pub const CONFIANZA: [&str; 4] = ["", "alta", "media", "baja"];
const JUEGOS: [&str; 6] = [
    "Warhammer 40K",
    "One Page Rules",
    "D&D",
    "Game Aids",
    "Rules",
    "Otros",
];
const TEMATICAS: [&str; 3] = ["Maid Marines", "FishMech", "Cartoon Marines"];
/// Capítulos y legiones con ejército propio en 40K: van como facción, no como etiqueta.
const PROPIO_SM: [&str; 4] = [
    "Blood Angels",
    "Dark Angels",
    "Space Wolves",
    "Black Templars",
];
const PROPIO_CSM: [&str; 4] = [
    "Death Guard",
    "World Eaters",
    "Emperor's Children",
    "Thousand Sons",
];

/// Cambios a un modelo; lo que viene en `None` no se toca.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct Cambios {
    pub juego: Option<String>,
    pub faccion: Option<String>,
    pub nombre: Option<String>,
    pub estudio: Option<String>,
    pub subcarpeta: Option<String>,
    pub confianza: Option<String>,
    pub notas: Option<String>,
    pub revisado: Option<String>,
    pub etiquetas: Option<Vec<String>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Campos {
    pub id: i64,
    pub carpeta: String,
    pub juego: String,
    pub faccion: String,
    pub nombre: String,
    pub estudio: String,
    pub subcarpeta: String,
    pub confianza: String,
    pub notas: String,
    pub revisado: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Cambio {
    pub campo: String,
    pub antes: String,
    pub despues: String,
    pub usuario: String,
    pub fecha: i64,
}

/// Un tramo de ruta que NTFS y ext4 aceptan y que no escapa de la carpeta.
fn segmento_valido(s: &str) -> Result<(), String> {
    if s.is_empty() {
        return Err(
            "hay un tramo vacío (dos «/» seguidas o una «/» al principio o al final)".into(),
        );
    }
    if s == "." || s == ".." {
        return Err("«.» y «..» no son nombres válidos".into());
    }
    if let Some(c) = s
        .chars()
        .find(|c| "<>:\"\\|?*".contains(*c) || c.is_control())
    {
        return Err(format!(
            "el carácter «{c}» no se permite en nombres de carpeta"
        ));
    }
    if s.ends_with(' ') || s.ends_with('.') {
        return Err("un nombre de carpeta no puede terminar en espacio o punto".into());
    }
    if s.len() > 255 {
        return Err("nombre de carpeta de más de 255 bytes".into());
    }
    Ok(())
}

pub fn validar(campo: &str, valor: &str) -> Result<(), String> {
    match campo {
        "juego" | "faccion" | "estudio" => {
            if valor.is_empty() {
                return Ok(());
            }
            if valor.contains('/') {
                return Err("no puede llevar «/»".into());
            }
            segmento_valido(valor)
        }
        "nombre" | "subcarpeta" => {
            if valor.is_empty() {
                return Ok(());
            }
            valor.split('/').try_for_each(segmento_valido)
        }
        "revisado" if !REVISADO.contains(&valor) => {
            Err("debe ser OK, Corregido, No mover o vacío".into())
        }
        "confianza" if !CONFIANZA.contains(&valor) => {
            Err("debe ser alta, media, baja o vacío".into())
        }
        _ => Ok(()),
    }
}

/// Avisos de las reglas de nombres (no impiden guardar).
pub fn avisos(m: &Campos, etiquetas: &[String]) -> Vec<String> {
    let mut a = Vec::new();
    let tiene = |e: &str| etiquetas.iter().any(|x| x == e);
    let equivalencia = etiquetas.iter().any(|e| e.starts_with("40K: "));
    if m.juego.is_empty() {
        a.push("Falta el juego".into());
    } else if !JUEGOS.contains(&m.juego.as_str()) {
        a.push(format!(
            "Juego no estándar: «{}» (los acordados: {})",
            m.juego,
            JUEGOS.join(", ")
        ));
    }
    if matches!(m.juego.as_str(), "Warhammer 40K" | "One Page Rules" | "D&D")
        && m.faccion.is_empty()
    {
        a.push("Falta la facción o el tipo".into());
    }
    if matches!(m.juego.as_str(), "Warhammer 40K" | "D&D") && m.nombre.is_empty() {
        a.push("Falta el modelo".into());
    }
    if m.juego == "One Page Rules" && !equivalencia {
        a.push("OPR sin su equivalente de 40K (etiqueta «40K: …»)".into());
    }
    if TEMATICAS.contains(&m.faccion.as_str()) {
        if !tiene("Proxy temático") {
            a.push("Línea temática sin la etiqueta «Proxy temático»".into());
        }
        if !equivalencia {
            a.push("Línea temática sin su facción oficial como etiqueta «40K: …»".into());
        }
    }
    let propios: &[&str] = match m.faccion.as_str() {
        "Space Marines" => &PROPIO_SM,
        "Chaos Space Marines" => &PROPIO_CSM,
        _ => &[],
    };
    for p in propios.iter().filter(|p| tiene(p)) {
        a.push(format!(
            "«{p}» tiene ejército propio: va como facción, no como etiqueta"
        ));
    }
    if m.nombre.starts_with("Bits ") || m.nombre.starts_with("Bits-") {
        a.push("Los bits van como «Bits/<nombre>» en Modelo".into());
    }
    for (campo, v) in [
        ("juego", &m.juego),
        ("facción", &m.faccion),
        ("modelo", &m.nombre),
        ("estudio", &m.estudio),
        ("subcarpeta", &m.subcarpeta),
    ] {
        if v.contains("  ") || v.trim() != v.as_str() {
            a.push(format!("Espacios de más en {campo}"));
        }
    }
    a
}

pub fn ruta_de(m: &Campos) -> String {
    ruta_nueva(&m.juego, &m.faccion, &m.nombre, &m.estudio, &m.subcarpeta)
}

/// No está donde dice su ruta nueva (ni en una subcarpeta de ella). Las filas «No mover» y sin juego no cuentan.
pub fn esta_pendiente(m: &Campos) -> bool {
    if m.revisado == "No mover" || m.juego.is_empty() {
        return false;
    }
    let j = ruta_de(m);
    let k = m.carpeta.strip_suffix(SUELTOS).unwrap_or(&m.carpeta);
    !(k == j || k.starts_with(&format!("{j}/")))
}

pub async fn campos(pool: &SqlitePool, id: i64) -> Result<Option<Campos>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, carpeta, juego, faccion, nombre, estudio, subcarpeta, confianza, notas, revisado
         FROM modelos WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn etiquetas_de(pool: &SqlitePool, id: i64) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT e.nombre FROM modelo_etiquetas me JOIN etiquetas e ON e.id = me.etiqueta_id
         WHERE me.modelo_id = ? ORDER BY e.nombre",
    )
    .bind(id)
    .fetch_all(pool)
    .await
}

/// Recalcula ruta nueva, pendiente y número de avisos (de `ids`, o de todos).
pub async fn recalcular(pool: &SqlitePool, ids: Option<&[i64]>) -> anyhow::Result<()> {
    let todos: Vec<Campos> = sqlx::query_as(
        "SELECT id, carpeta, juego, faccion, nombre, estudio, subcarpeta, confianza, notas, revisado FROM modelos",
    )
    .fetch_all(pool)
    .await?;
    let mut tx = pool.begin().await?;
    for m in todos
        .iter()
        .filter(|m| ids.is_none_or(|ids| ids.contains(&m.id)))
    {
        let et: Vec<String> = sqlx::query_scalar(
            "SELECT e.nombre FROM modelo_etiquetas me JOIN etiquetas e ON e.id = me.etiqueta_id
             WHERE me.modelo_id = ?",
        )
        .bind(m.id)
        .fetch_all(&mut *tx)
        .await?;
        sqlx::query("UPDATE modelos SET ruta_nueva = ?, pendiente = ?, n_avisos = ? WHERE id = ?")
            .bind(ruta_de(m))
            .bind(esta_pendiente(m) as i64)
            .bind(avisos(m, &et).len() as i64)
            .bind(m.id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Vuelve a indexar un modelo en la búsqueda.
pub async fn reindexar(pool: &SqlitePool, id: i64) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM modelos_fts WHERE rowid = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO modelos_fts (rowid, nombre, juego, faccion, estudio, subcarpeta, etiquetas, notas, carpeta)
         SELECT m.id, m.nombre, m.juego, m.faccion, m.estudio, m.subcarpeta,
                coalesce((SELECT group_concat(e.nombre, ' ') FROM modelo_etiquetas me
                          JOIN etiquetas e ON e.id = me.etiqueta_id WHERE me.modelo_id = m.id), ''),
                m.notas, m.carpeta
         FROM modelos m WHERE m.id = ?",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

async fn registrar(
    tx: &mut sqlx::SqliteConnection,
    id: i64,
    campo: &str,
    antes: &str,
    despues: &str,
    usuario: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO historial (modelo_id, campo, antes, despues, usuario, fecha) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(campo)
    .bind(antes)
    .bind(despues)
    .bind(usuario)
    .bind(ahora())
    .execute(tx)
    .await?;
    Ok(())
}

/// Guarda los cambios de un modelo con su historial. Devuelve cuántos campos cambiaron.
pub async fn aplicar(
    pool: &SqlitePool,
    id: i64,
    c: &Cambios,
    usuario: &str,
) -> Result<usize, ApiError> {
    let actual = campos(pool, id).await?.ok_or(ApiError::NoEncontrado)?;
    let pares: [(&str, &Option<String>, &str); 8] = [
        ("juego", &c.juego, &actual.juego),
        ("faccion", &c.faccion, &actual.faccion),
        ("nombre", &c.nombre, &actual.nombre),
        ("estudio", &c.estudio, &actual.estudio),
        ("subcarpeta", &c.subcarpeta, &actual.subcarpeta),
        ("confianza", &c.confianza, &actual.confianza),
        ("notas", &c.notas, &actual.notas),
        ("revisado", &c.revisado, &actual.revisado),
    ];
    let mut cambios = Vec::new();
    for (campo, nuevo, antes) in pares {
        if let Some(v) = nuevo {
            let v = v.trim();
            validar(campo, v).map_err(|e| ApiError::Invalido(format!("{campo}: {e}")))?;
            if v != antes {
                cambios.push((campo, antes.to_string(), v.to_string()));
            }
        }
    }
    let et_antes = etiquetas_de(pool, id).await?;
    let et_nuevas = c
        .etiquetas
        .as_ref()
        .map(|l| separar_etiquetas(&l.join(";")));
    let et_cambian = et_nuevas.as_ref().is_some_and(|n| {
        let mut a = et_antes.clone();
        let mut b = n.clone();
        a.sort();
        b.sort();
        a != b
    });
    if cambios.is_empty() && !et_cambian {
        return Ok(0);
    }
    let mut tx = pool.begin().await?;
    for (campo, antes, despues) in &cambios {
        let sql = match *campo {
            "juego" => "UPDATE modelos SET juego = ? WHERE id = ?",
            "faccion" => "UPDATE modelos SET faccion = ? WHERE id = ?",
            "nombre" => "UPDATE modelos SET nombre = ? WHERE id = ?",
            "estudio" => "UPDATE modelos SET estudio = ? WHERE id = ?",
            "subcarpeta" => "UPDATE modelos SET subcarpeta = ? WHERE id = ?",
            "confianza" => "UPDATE modelos SET confianza = ? WHERE id = ?",
            "notas" => "UPDATE modelos SET notas = ? WHERE id = ?",
            _ => "UPDATE modelos SET revisado = ? WHERE id = ?",
        };
        sqlx::query(sql)
            .bind(despues)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        registrar(&mut tx, id, campo, antes, despues, usuario).await?;
    }
    if let (true, Some(nuevas)) = (et_cambian, &et_nuevas) {
        sqlx::query("DELETE FROM modelo_etiquetas WHERE modelo_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        for e in nuevas {
            validar("etiqueta", e).map_err(ApiError::Invalido)?;
            let eid: i64 = sqlx::query_scalar(
                "INSERT INTO etiquetas (nombre) VALUES (?)
                 ON CONFLICT (nombre) DO UPDATE SET nombre = excluded.nombre RETURNING id",
            )
            .bind(e)
            .fetch_one(&mut *tx)
            .await?;
            sqlx::query(
                "INSERT OR IGNORE INTO modelo_etiquetas (modelo_id, etiqueta_id) VALUES (?, ?)",
            )
            .bind(id)
            .bind(eid)
            .execute(&mut *tx)
            .await?;
        }
        registrar(
            &mut tx,
            id,
            "etiquetas",
            &et_antes.join("; "),
            &nuevas.join("; "),
            usuario,
        )
        .await?;
    }
    sqlx::query("UPDATE modelos SET actualizado = ? WHERE id = ?")
        .bind(ahora())
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    recalcular(pool, Some(&[id])).await?;
    reindexar(pool, id).await?;
    Ok(cambios.len() + et_cambian as usize)
}

/// Lo mismo para varios modelos a la vez; `agregar` y `quitar` cambian etiquetas sin tocar las demás.
pub async fn aplicar_varios(
    pool: &SqlitePool,
    ids: &[i64],
    c: &Cambios,
    agregar: &[String],
    quitar: &[String],
    usuario: &str,
) -> Result<usize, ApiError> {
    if ids.is_empty() {
        return Err(ApiError::Invalido("No hay modelos seleccionados".into()));
    }
    let mut total = 0;
    for &id in ids {
        let mut c = c.clone();
        if !agregar.is_empty() || !quitar.is_empty() {
            let mut et = etiquetas_de(pool, id).await?;
            et.retain(|e| !quitar.contains(e));
            for a in agregar.iter().map(|a| a.trim()).filter(|a| !a.is_empty()) {
                if !et.iter().any(|e| e == a) {
                    et.push(a.to_string());
                }
            }
            c.etiquetas = Some(et);
        }
        total += aplicar(pool, id, &c, usuario).await?;
    }
    Ok(total)
}

/// Renombra una etiqueta, o la une con otra si el nombre nuevo ya existe; también cambia tipo y color.
pub async fn editar_etiqueta(
    pool: &SqlitePool,
    id: i64,
    nombre: Option<&str>,
    tipo: Option<&str>,
    color: Option<&str>,
    usuario: &str,
) -> Result<i64, ApiError> {
    let actual: Option<String> = sqlx::query_scalar("SELECT nombre FROM etiquetas WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    let actual = actual.ok_or(ApiError::NoEncontrado)?;
    let mut destino = id;
    let mut tx = pool.begin().await?;
    if let Some(n) = nombre
        .map(str::trim)
        .filter(|n| !n.is_empty() && *n != actual)
    {
        let otro: Option<i64> = sqlx::query_scalar("SELECT id FROM etiquetas WHERE nombre = ?")
            .bind(n)
            .fetch_optional(&mut *tx)
            .await?;
        let afectados: Vec<i64> =
            sqlx::query_scalar("SELECT modelo_id FROM modelo_etiquetas WHERE etiqueta_id = ?")
                .bind(id)
                .fetch_all(&mut *tx)
                .await?;
        match otro {
            Some(o) => {
                sqlx::query(
                    "INSERT OR IGNORE INTO modelo_etiquetas (modelo_id, etiqueta_id)
                     SELECT modelo_id, ? FROM modelo_etiquetas WHERE etiqueta_id = ?",
                )
                .bind(o)
                .bind(id)
                .execute(&mut *tx)
                .await?;
                sqlx::query("DELETE FROM etiquetas WHERE id = ?")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                destino = o;
            }
            None => {
                sqlx::query("UPDATE etiquetas SET nombre = ? WHERE id = ?")
                    .bind(n)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        for m in &afectados {
            registrar(&mut tx, *m, "etiquetas", &actual, n, usuario).await?;
        }
        tx.commit().await?;
        recalcular(pool, Some(&afectados)).await?;
        for m in afectados {
            reindexar(pool, m).await?;
        }
        tx = pool.begin().await?;
    }
    if let Some(t) = tipo {
        sqlx::query("UPDATE etiquetas SET tipo = ? WHERE id = ?")
            .bind(t.trim())
            .bind(destino)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(c) = color {
        sqlx::query("UPDATE etiquetas SET color = ? WHERE id = ?")
            .bind(c.trim())
            .bind(destino)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(destino)
}

pub async fn historial(pool: &SqlitePool, id: i64) -> Result<Vec<Cambio>, sqlx::Error> {
    sqlx::query_as(
        "SELECT campo, antes, despues, usuario, fecha FROM historial WHERE modelo_id = ? ORDER BY id DESC LIMIT 500",
    )
    .bind(id)
    .fetch_all(pool)
    .await
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn m(
        juego: &str,
        faccion: &str,
        nombre: &str,
        estudio: &str,
        sub: &str,
        carpeta: &str,
    ) -> Campos {
        Campos {
            id: 1,
            carpeta: carpeta.into(),
            juego: juego.into(),
            faccion: faccion.into(),
            nombre: nombre.into(),
            estudio: estudio.into(),
            subcarpeta: sub.into(),
            confianza: String::new(),
            notas: String::new(),
            revisado: "OK".into(),
        }
    }

    #[test]
    fn valida_nombres_de_carpeta() {
        assert!(validar("nombre", "Bits/Heads Set 1").is_ok());
        assert!(validar("nombre", "Captain: Sword").is_err());
        assert!(validar("faccion", "Space/Marines").is_err());
        assert!(validar("subcarpeta", "../fuera").is_err());
        assert!(validar("nombre", "Termina en punto.").is_err());
        assert!(validar("revisado", "Tal vez").is_err());
        assert!(validar("revisado", "No mover").is_ok());
    }

    #[test]
    fn pendiente_y_avisos() {
        let ok = m(
            "Warhammer 40K",
            "Necrons",
            "Wraiths",
            "SF",
            "",
            "Wargames/Warhammer 40K/Necrons/Wraiths (SF)",
        );
        assert!(!esta_pendiente(&ok));
        let sub = m(
            "Warhammer 40K",
            "Necrons",
            "Wraiths",
            "SF",
            "",
            "Wargames/Warhammer 40K/Necrons/Wraiths (SF)/x5",
        );
        assert!(
            !esta_pendiente(&sub),
            "una subcarpeta de su ruta cuenta como en su lugar"
        );
        let mal = m(
            "Warhammer 40K",
            "Space Marines",
            "Captain",
            "",
            "",
            "Wargames/Warhammer 40K/Necrons/Captain",
        );
        assert!(esta_pendiente(&mal));
        let mut no_mover = mal.clone();
        no_mover.revisado = "No mover".into();
        assert!(!esta_pendiente(&no_mover));

        let a = avisos(&mal, &["Blood Angels".into()]);
        assert!(a.iter().any(|x| x.contains("ejército propio")), "{a:?}");
        let opr = m("One Page Rules", "Alien Hives", "", "", "", "x");
        assert!(avisos(&opr, &[]).iter().any(|x| x.contains("OPR")));
        assert!(avisos(&opr, &["40K: Tyranids".into()]).is_empty());
        let fish = m("Warhammer 40K", "FishMech", "Koi", "Pipermakes", "", "x");
        assert_eq!(avisos(&fish, &[]).len(), 2);
        assert!(avisos(&fish, &["Proxy temático".into(), "40K: T'au Empire".into()]).is_empty());
    }
}
