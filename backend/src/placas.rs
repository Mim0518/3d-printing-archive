//! Placas: se detectan solas a partir de los archivos y se ligan a sus modelos.
//!
//! - Una placa reúne, en una misma carpeta, la escena y los laminados que se llaman igual (sin la extensión), más
//!   los autoguardados de Lychee: `<Nombre>_autosave/<Nombre>_<fecha>.lyt` van con `<Nombre>.lys`.
//! - Un 3MF cuenta como placa solo si es un proyecto de laminador (trae su configuración o sus placas).
//! - Los archivos que el usuario puso a mano en una placa no se mueven al volver a detectar.
//! - De qué modelos es: se sugiere el modelo de la carpeta (o, si la placa no está en la carpeta de un modelo, el
//!   que aparezca en su nombre) y el usuario confirma o rechaza. Lo rechazado no se vuelve a sugerir.
//! - Solo lee la colección; nunca escribe en ella.

use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
};

use serde::Serialize;
use sqlx::SqlitePool;

use crate::{
    db::ahora,
    estructura::padre,
    lectores::{self, Datos},
    miniaturas::{EXT_MALLA, clave},
};

/// Archivos que pueden ser placas. `.pwsr` es un perfil de resina de Photon Workshop, no una escena.
const SQL_CANDIDATO: &str =
    "((a.tipo IN ('escena', 'laminado') AND a.ext NOT IN ('pwsr')) OR a.ext = '3mf')";

#[derive(Debug, Default, Serialize)]
pub struct Resultado {
    /// Archivos que forman placas.
    pub archivos: u64,
    /// Archivos cuyos datos se leyeron en esta vuelta (nuevos o que cambiaron).
    pub leidos: u64,
    pub errores_lectura: u64,
    pub placas: u64,
    pub nuevas: u64,
    pub sugerencias_nuevas: u64,
    pub ejemplos_de_error: Vec<String>,
}

/// Dónde va un archivo: la carpeta y el nombre de su placa, y qué papel tiene en ella.
#[derive(Debug, PartialEq, Eq)]
pub struct Lugar {
    pub carpeta: String,
    pub nombre: String,
    pub rol: &'static str,
}

/// Sufijo de las carpetas de autoguardado de Lychee.
const AUTOGUARDADO: &str = "_autosave";

pub fn lugar(carpeta: &str, nombre: &str, tipo: &str) -> Lugar {
    let ultima = carpeta.rsplit('/').next().unwrap_or(carpeta);
    if let Some(corte) = ultima.len().checked_sub(AUTOGUARDADO.len())
        && corte > 0
        && ultima.is_char_boundary(corte)
        && ultima[corte..].eq_ignore_ascii_case(AUTOGUARDADO)
    {
        return Lugar {
            carpeta: padre(carpeta).to_string(),
            nombre: ultima[..corte].to_string(),
            rol: "autoguardado",
        };
    }
    let sin_ext = match nombre.rfind('.') {
        Some(i) if i > 0 => &nombre[..i],
        _ => nombre,
    };
    Lugar {
        carpeta: carpeta.to_string(),
        nombre: sin_ext.to_string(),
        rol: if tipo == "laminado" {
            "laminado"
        } else {
            "escena"
        },
    }
}

fn clave_placa(biblioteca: i64, l: &Lugar) -> String {
    format!(
        "{biblioteca}:{}/{}",
        l.carpeta,
        l.nombre.trim().to_lowercase()
    )
}

/// Minúsculas, sin acentos comunes ni signos; con espacios a los lados para buscar palabras completas.
pub fn normalizar(s: &str) -> String {
    let mut r = String::with_capacity(s.len() + 2);
    r.push(' ');
    let mut espacio = true;
    for c in s.chars().flat_map(char::to_lowercase) {
        let c = match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            c => c,
        };
        if c.is_alphanumeric() {
            r.push(c);
            espacio = false;
        } else if !espacio {
            r.push(' ');
            espacio = true;
        }
    }
    if !espacio {
        r.push(' ');
    }
    r
}

/// Nombre de malla para buscar la «gemela» de una escena: minúsculas, sin prefijos de laminador.
fn tallo(nombre: &str) -> String {
    let l = nombre.trim().to_lowercase();
    for p in ["lys_", "lychee_", "chitubox_", "ctb_"] {
        if let Some(r) = l.strip_prefix(p) {
            return r.to_string();
        }
    }
    l
}

type FilaCandidato = (
    i64,
    i64,
    String,
    String,
    String,
    String,
    String,
    String,
    i64,
    i64,
    i64,
    Option<i64>,
    Option<String>,
);

async fn guardar_datos(
    pool: &SqlitePool,
    leidos: &[(i64, String, &'static str, Result<Datos, String>)],
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    for (id, clave, tipo, r) in leidos {
        let (d, error) = match r {
            Ok(d) => (d.clone(), String::new()),
            Err(e) => (
                Datos {
                    // una escena o un laminado que no se pudo leer sigue siendo una placa; un 3MF, no
                    es_placa: *tipo != "malla",
                    ..Default::default()
                },
                e.clone(),
            ),
        };
        sqlx::query(
            "INSERT INTO datos_placa (archivo_id, clave, formato, es_placa, vista_previa, maquina, res_x, res_y,
                capa_mm, exposicion_s, exposicion_base_s, capas_base, capas, alto_mm, tiempo_s, volumen_ml, peso_g,
                n_placas, error)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (archivo_id) DO UPDATE SET clave = excluded.clave, formato = excluded.formato,
                es_placa = excluded.es_placa, vista_previa = excluded.vista_previa, maquina = excluded.maquina,
                res_x = excluded.res_x, res_y = excluded.res_y, capa_mm = excluded.capa_mm,
                exposicion_s = excluded.exposicion_s, exposicion_base_s = excluded.exposicion_base_s,
                capas_base = excluded.capas_base, capas = excluded.capas, alto_mm = excluded.alto_mm,
                tiempo_s = excluded.tiempo_s, volumen_ml = excluded.volumen_ml, peso_g = excluded.peso_g,
                n_placas = excluded.n_placas, error = excluded.error",
        )
        .bind(id)
        .bind(clave)
        .bind(&d.formato)
        .bind(d.es_placa)
        .bind(d.vista_previa)
        .bind(&d.maquina)
        .bind(d.res_x)
        .bind(d.res_y)
        .bind(d.capa_mm)
        .bind(d.exposicion_s)
        .bind(d.exposicion_base_s)
        .bind(d.capas_base)
        .bind(d.capas)
        .bind(d.alto_mm)
        .bind(d.tiempo_s)
        .bind(d.volumen_ml)
        .bind(d.peso_g)
        .bind(d.n_placas)
        .bind(error)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

struct Miembro {
    id: i64,
    nombre: String,
    rol: &'static str,
    modelo_id: Option<i64>,
}

struct Grupo {
    biblioteca: i64,
    lugar_carpeta: String,
    nombre: String,
    miembros: Vec<Miembro>,
}

/// Lee los datos de lo nuevo, agrupa los archivos en placas, liga los archivos y sugiere modelos.
pub async fn detectar(pool: &SqlitePool) -> anyhow::Result<Resultado> {
    let mut res = Resultado::default();
    let sql = format!(
        "SELECT a.id, a.biblioteca_id, b.ruta, a.ruta, a.carpeta, a.nombre, a.ext, a.tipo, a.inode, a.bytes,
                a.mtime_ms, a.modelo_id, d.clave
         FROM archivos a JOIN bibliotecas b ON b.id = a.biblioteca_id
         LEFT JOIN datos_placa d ON d.archivo_id = a.id
         WHERE a.estado = 'presente' AND {SQL_CANDIDATO}
         ORDER BY a.biblioteca_id, a.ruta"
    );
    let filas: Vec<FilaCandidato> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .fetch_all(pool)
        .await?;

    // 1. leer lo que cambió (en un hilo aparte: es disco)
    let por_leer: Vec<(i64, String, String, &'static str, std::path::PathBuf)> = filas
        .iter()
        .filter_map(|f| {
            let k = clave(f.8, f.9, f.10);
            (f.12.as_deref() != Some(k.as_str())).then(|| {
                let tipo = if f.7 == "malla" { "malla" } else { "placa" };
                (f.0, k, f.6.clone(), tipo, Path::new(&f.2).join(&f.3))
            })
        })
        .collect();
    if !por_leer.is_empty() {
        let leidos = tokio::task::spawn_blocking(move || {
            por_leer
                .into_iter()
                .map(|(id, k, ext, tipo, ruta)| {
                    let r = lectores::leer(&ruta, &ext);
                    (
                        id,
                        k,
                        tipo,
                        r.map_err(|e| format!("{}: {e}", ruta.display())),
                    )
                })
                .collect::<Vec<_>>()
        })
        .await?;
        res.leidos = leidos.len() as u64;
        for (_, _, _, r) in &leidos {
            if let Err(e) = r {
                res.errores_lectura += 1;
                if res.ejemplos_de_error.len() < 20 {
                    res.ejemplos_de_error.push(e.clone());
                }
            }
        }
        guardar_datos(pool, &leidos).await?;
    }

    // 2. agrupar los que sí son placas
    let es_placa: HashMap<i64, bool> =
        sqlx::query_as::<_, (i64, bool)>("SELECT archivo_id, es_placa FROM datos_placa")
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect();
    let mut grupos: BTreeMap<String, Grupo> = BTreeMap::new();
    for f in &filas {
        if !es_placa.get(&f.0).copied().unwrap_or(false) {
            continue;
        }
        let l = lugar(&f.4, &f.5, &f.7);
        let k = clave_placa(f.1, &l);
        let g = grupos.entry(k).or_insert_with(|| Grupo {
            biblioteca: f.1,
            lugar_carpeta: l.carpeta.clone(),
            nombre: l.nombre.clone(),
            miembros: Vec::new(),
        });
        // el nombre de la placa: el de su escena principal, si la hay
        if l.rol == "escena" {
            g.nombre = l.nombre.clone();
        }
        g.miembros.push(Miembro {
            id: f.0,
            nombre: l.nombre,
            rol: l.rol,
            modelo_id: f.11,
        });
    }
    res.archivos = grupos.values().map(|g| g.miembros.len() as u64).sum();

    // mallas para la vista previa de las escenas que no traen imagen
    let mallas: Vec<(i64, String, String, Option<i64>)> =
        sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT id, carpeta, nombre, modelo_id FROM archivos
             WHERE estado = 'presente' AND tipo = 'malla' AND ext IN ('{}')",
            EXT_MALLA.join("', '")
        )))
        .fetch_all(pool)
        .await?;
    let mut por_carpeta: HashMap<(String, String), i64> = HashMap::new();
    let mut por_modelo: HashMap<(i64, String), i64> = HashMap::new();
    for (id, carpeta, nombre, modelo) in mallas {
        let t = tallo(match nombre.rfind('.') {
            Some(i) if i > 0 => &nombre[..i],
            _ => &nombre,
        });
        por_carpeta.entry((carpeta, t.clone())).or_insert(id);
        if let Some(m) = modelo {
            por_modelo.entry((m, t)).or_insert(id);
        }
    }

    // 3. ligar
    let ligas: HashMap<i64, (i64, bool)> = sqlx::query_as::<_, (i64, i64, bool)>(
        "SELECT archivo_id, placa_id, auto FROM placa_archivos",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(a, p, auto)| (a, (p, auto)))
    .collect();
    let mut por_clave: HashMap<String, i64> = HashMap::new();
    for (id, k) in sqlx::query_as::<_, (i64, String)>(
        "SELECT id, clave FROM placas WHERE clave IS NOT NULL ORDER BY id DESC",
    )
    .fetch_all(pool)
    .await?
    {
        por_clave.insert(k, id); // la de id más bajo gana
    }

    let t = ahora();
    let mut tx = pool.begin().await?;
    for (k, g) in &grupos {
        let libres: Vec<&Miembro> = g
            .miembros
            .iter()
            .filter(|m| ligas.get(&m.id).is_none_or(|(_, auto)| *auto))
            .collect();
        if libres.is_empty() {
            continue; // todos los puso el usuario en otras placas
        }
        let mut cuenta: HashMap<i64, usize> = HashMap::new();
        for m in &libres {
            if let Some((p, _)) = ligas.get(&m.id) {
                *cuenta.entry(*p).or_default() += 1;
            }
        }
        let existente = cuenta
            .into_iter()
            .max_by_key(|(p, n)| (*n, -*p))
            .map(|(p, _)| p)
            .or_else(|| por_clave.get(k).copied());
        let placa = match existente {
            Some(p) => p,
            None => {
                res.nuevas += 1;
                sqlx::query_scalar(
                    "INSERT INTO placas (nombre, biblioteca_id, carpeta, clave, origen, creada, actualizada)
                     VALUES (?, ?, ?, ?, 'detectada', ?, ?) RETURNING id",
                )
                .bind(&g.nombre)
                .bind(g.biblioteca)
                .bind(&g.lugar_carpeta)
                .bind(k)
                .bind(t)
                .bind(t)
                .fetch_one(&mut *tx)
                .await?
            }
        };
        res.placas += 1;
        // la malla gemela: misma carpeta y mismo nombre; si no, mismo modelo y mismo nombre
        let propios: Vec<i64> = g.miembros.iter().map(|m| m.id).collect();
        let modelo = g.miembros.iter().find_map(|m| m.modelo_id);
        let gemela = g
            .miembros
            .iter()
            .map(|m| tallo(&m.nombre))
            .find_map(|t| {
                por_carpeta
                    .get(&(g.lugar_carpeta.clone(), t.clone()))
                    .or_else(|| modelo.and_then(|mo| por_modelo.get(&(mo, t))))
                    .copied()
            })
            .filter(|id| !propios.contains(id));
        sqlx::query(
            "UPDATE placas SET
                carpeta = CASE WHEN origen = 'detectada' THEN ? ELSE carpeta END,
                clave = CASE WHEN origen = 'detectada' THEN ? ELSE clave END,
                biblioteca_id = coalesce(biblioteca_id, ?),
                gemela_id = ?
             WHERE id = ?",
        )
        .bind(&g.lugar_carpeta)
        .bind(k)
        .bind(g.biblioteca)
        .bind(gemela)
        .bind(placa)
        .execute(&mut *tx)
        .await?;
        for m in libres {
            sqlx::query(
                "INSERT INTO placa_archivos (archivo_id, placa_id, rol, auto) VALUES (?, ?, ?, 1)
                 ON CONFLICT (archivo_id) DO UPDATE SET placa_id = excluded.placa_id, rol = excluded.rol
                 WHERE placa_archivos.auto = 1",
            )
            .bind(m.id)
            .bind(placa)
            .bind(m.rol)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;

    limpiar(pool).await?;
    res.sugerencias_nuevas = sugerir(pool).await?;
    Ok(res)
}

/// Quita las placas detectadas que se quedaron sin archivos y sin nada que el usuario haya puesto.
pub async fn limpiar(pool: &SqlitePool) -> anyhow::Result<u64> {
    let r = sqlx::query(
        "DELETE FROM placas WHERE origen = 'detectada'
           AND NOT EXISTS (SELECT 1 FROM placa_archivos pa WHERE pa.placa_id = placas.id)
           AND estado = 'sin_probar' AND favorita = 0 AND notas = '' AND sin_modelo = 0
           AND portada_archivo_id IS NULL
           AND NOT EXISTS (SELECT 1 FROM placa_modelos pm WHERE pm.placa_id = placas.id AND pm.estado <> 'sugerido')",
    )
    .execute(pool)
    .await?;
    Ok(r.rows_affected())
}

/// Pone al día las sugerencias de modelo. Devuelve cuántas sugerencias nuevas hay.
pub async fn sugerir(pool: &SqlitePool) -> anyhow::Result<u64> {
    let t = ahora();
    let mut tx = pool.begin().await?;
    // por la carpeta: el modelo de cada archivo presente de la placa
    let nuevas = sqlx::query(
        "INSERT INTO placa_modelos (placa_id, modelo_id, estado, origen, actualizado)
         SELECT DISTINCT pa.placa_id, a.modelo_id, 'sugerido', 'carpeta', ?
         FROM placa_archivos pa JOIN archivos a ON a.id = pa.archivo_id
         JOIN placas p ON p.id = pa.placa_id
         WHERE a.modelo_id IS NOT NULL AND a.estado = 'presente' AND p.sin_modelo = 0
         ON CONFLICT (placa_id, modelo_id) DO NOTHING",
    )
    .bind(t)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    // las sugerencias por carpeta que ya no se sostienen (el archivo cambió de modelo o de placa)
    sqlx::query(
        "DELETE FROM placa_modelos WHERE estado = 'sugerido' AND origen = 'carpeta' AND NOT EXISTS (
            SELECT 1 FROM placa_archivos pa JOIN archivos a ON a.id = pa.archivo_id
            WHERE pa.placa_id = placa_modelos.placa_id AND a.modelo_id = placa_modelos.modelo_id
              AND a.estado = 'presente')",
    )
    .execute(&mut *tx)
    .await?;
    // la sugerencia por nombre sobra si ya hay otra
    sqlx::query(
        "DELETE FROM placa_modelos WHERE estado = 'sugerido' AND origen = 'nombre' AND EXISTS (
            SELECT 1 FROM placa_modelos x WHERE x.placa_id = placa_modelos.placa_id AND x.origen <> 'nombre'
              AND x.estado <> 'rechazado')",
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    // por el nombre: solo para las placas que no tienen ninguna sugerencia
    let huerfanas: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT p.id, p.nombre, p.carpeta FROM placas p
         WHERE p.sin_modelo = 0 AND NOT EXISTS (SELECT 1 FROM placa_modelos pm WHERE pm.placa_id = p.id)",
    )
    .fetch_all(pool)
    .await?;
    if huerfanas.is_empty() {
        return Ok(nuevas);
    }
    let modelos: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, nombre FROM modelos WHERE length(nombre) >= 5")
            .fetch_all(pool)
            .await?;
    let modelos: Vec<(i64, String)> = modelos
        .into_iter()
        .map(|(id, n)| (id, normalizar(&n)))
        .filter(|(_, n)| n.trim().len() >= 5)
        .collect();
    let mut tx = pool.begin().await?;
    let mut por_nombre = 0;
    for (placa, nombre, carpeta) in huerfanas {
        let pajar = normalizar(&format!("{carpeta} {nombre}"));
        let hallados: Vec<&(i64, String)> =
            modelos.iter().filter(|(_, n)| pajar.contains(n)).collect();
        let Some(largo) = hallados.iter().map(|(_, n)| n.len()).max() else {
            continue;
        };
        for (m, _) in hallados
            .into_iter()
            .filter(|(_, n)| n.len() == largo)
            .take(3)
        {
            por_nombre += sqlx::query(
                "INSERT INTO placa_modelos (placa_id, modelo_id, estado, origen, actualizado)
                 VALUES (?, ?, 'sugerido', 'nombre', ?) ON CONFLICT (placa_id, modelo_id) DO NOTHING",
            )
            .bind(placa)
            .bind(m)
            .bind(t)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        }
    }
    tx.commit().await?;
    Ok(nuevas + por_nombre)
}

/// La portada de una placa: la elegida a mano; si no, la vista previa de su escena o laminado más reciente
/// (los autoguardados al final); si no, la miniatura de la malla gemela. Expresión SQL sobre el alias `p`.
pub const SQL_PORTADA: &str = "coalesce(
    (SELECT mi.archivo_id FROM miniaturas mi WHERE mi.archivo_id = p.portada_archivo_id AND mi.estado = 'ok'),
    (SELECT pa.archivo_id FROM placa_archivos pa
     JOIN archivos a ON a.id = pa.archivo_id AND a.estado = 'presente'
     JOIN miniaturas mi ON mi.archivo_id = pa.archivo_id AND mi.estado = 'ok'
     WHERE pa.placa_id = p.id ORDER BY pa.rol = 'autoguardado', a.mtime_ms DESC LIMIT 1),
    (SELECT mi.archivo_id FROM miniaturas mi WHERE mi.archivo_id = p.gemela_id AND mi.estado = 'ok'))";

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn agrupa_escenas_laminados_y_autoguardados() {
        assert_eq!(
            lugar("A/B", "Torso Main.lys", "escena"),
            Lugar {
                carpeta: "A/B".into(),
                nombre: "Torso Main".into(),
                rol: "escena"
            }
        );
        assert_eq!(
            lugar("A/B", "Torso Main.photon", "laminado").rol,
            "laminado"
        );
        // el autoguardado va con la escena de la carpeta de arriba, aunque el archivo se llame distinto
        assert_eq!(
            lugar(
                "A/B/Escuadra de Choque_autosave",
                "Grey Knights_2023-08-18_12-58-34.lyt",
                "escena"
            ),
            Lugar {
                carpeta: "A/B".into(),
                nombre: "Escuadra de Choque".into(),
                rol: "autoguardado"
            }
        );
        assert_eq!(lugar("", "_autosave", "escena").rol, "escena");
        assert_eq!(
            clave_placa(1, &lugar("A", "Torso Main.lys", "escena")),
            clave_placa(1, &lugar("A/Torso main_AUTOSAVE", "x.lyt", "escena"))
        );
    }

    #[test]
    fn normaliza_para_buscar_nombres() {
        assert_eq!(normalizar("Canoptek  Wraiths!"), " canoptek wraiths ");
        assert_eq!(normalizar("Señor Ácido"), " senor acido ");
        assert!(normalizar("necrons/Canoptek_Wraiths_plate1").contains(" canoptek wraiths "));
        assert_eq!(tallo("LYS_Arm"), "arm");
    }
}
