//! Fase 5: impresoras, materiales y perfiles.
//!
//! - Los campos de cada tecnología (resina y FDM) y su validación.
//! - Cómo se reconoce la impresora de un laminado (por su pantalla) y su perfil (por capa y exposición).
//! - La importación de los archivos de perfiles de la colección: escenas y perfiles de Photon Workshop
//!   (`.pwscene`, `.pwsr`), paquetes de Orca (`.orca_printer` o la carpeta con `bundle_structure.json`) y
//!   perfiles de resina de Lychee (`.lyr`, que vienen cifrados: solo se ligan).

use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use serde::Serialize;
use serde_json::{Map, Value, json};
use sqlx::SqlitePool;

use crate::{db::ahora, estructura::padre};

// ---------- campos de los perfiles ----------

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Campo {
    pub clave: &'static str,
    pub etiqueta: &'static str,
    pub unidad: &'static str,
    /// resina | fdm
    pub tecnologia: &'static str,
    /// numero | entero | texto
    pub tipo: &'static str,
    pub min: f64,
    /// En los de texto, el largo máximo.
    pub max: f64,
}

const fn campo(
    clave: &'static str,
    etiqueta: &'static str,
    unidad: &'static str,
    tecnologia: &'static str,
    tipo: &'static str,
    min: f64,
    max: f64,
) -> Campo {
    Campo {
        clave,
        etiqueta,
        unidad,
        tecnologia,
        tipo,
        min,
        max,
    }
}

pub const CAMPOS: &[Campo] = &[
    campo(
        "exposicion_s",
        "Exposición",
        "s",
        "resina",
        "numero",
        0.05,
        300.0,
    ),
    campo(
        "exposicion_base_s",
        "Exposición de base",
        "s",
        "resina",
        "numero",
        0.5,
        600.0,
    ),
    campo(
        "capas_base",
        "Capas de base",
        "",
        "resina",
        "entero",
        0.0,
        200.0,
    ),
    campo(
        "capas_transicion",
        "Capas de transición",
        "",
        "resina",
        "entero",
        0.0,
        200.0,
    ),
    campo("alzado_mm", "Alzado", "mm", "resina", "numero", 0.0, 50.0),
    campo(
        "velocidad_alzado_mm_s",
        "Velocidad de alzado",
        "mm/s",
        "resina",
        "numero",
        0.0,
        50.0,
    ),
    campo(
        "velocidad_bajada_mm_s",
        "Velocidad de bajada",
        "mm/s",
        "resina",
        "numero",
        0.0,
        50.0,
    ),
    campo(
        "espera_s",
        "Espera con la luz apagada",
        "s",
        "resina",
        "numero",
        0.0,
        120.0,
    ),
    campo(
        "antialiasing",
        "Antialiasing",
        "",
        "resina",
        "entero",
        0.0,
        16.0,
    ),
    campo("boquilla_mm", "Boquilla", "mm", "fdm", "numero", 0.1, 2.0),
    campo(
        "temp_boquilla_c",
        "Temperatura de la boquilla",
        "°C",
        "fdm",
        "entero",
        100.0,
        500.0,
    ),
    campo(
        "temp_boquilla_inicial_c",
        "Boquilla en la primera capa",
        "°C",
        "fdm",
        "entero",
        100.0,
        500.0,
    ),
    campo(
        "temp_cama_c",
        "Temperatura de la cama",
        "°C",
        "fdm",
        "entero",
        0.0,
        150.0,
    ),
    campo(
        "temp_cama_inicial_c",
        "Cama en la primera capa",
        "°C",
        "fdm",
        "entero",
        0.0,
        150.0,
    ),
    campo(
        "ventilador_pct",
        "Ventilador",
        "%",
        "fdm",
        "entero",
        0.0,
        100.0,
    ),
    campo(
        "velocidad_mm_s",
        "Velocidad",
        "mm/s",
        "fdm",
        "numero",
        1.0,
        1000.0,
    ),
    campo(
        "flujo_max_mm3_s",
        "Flujo volumétrico máximo",
        "mm³/s",
        "fdm",
        "numero",
        0.0,
        200.0,
    ),
    campo(
        "relacion_flujo",
        "Relación de flujo",
        "",
        "fdm",
        "numero",
        0.5,
        1.5,
    ),
    campo(
        "avance_presion",
        "Pressure advance",
        "",
        "fdm",
        "numero",
        0.0,
        2.0,
    ),
    campo(
        "retraccion_mm",
        "Retracción",
        "mm",
        "fdm",
        "numero",
        0.0,
        20.0,
    ),
    campo("paredes", "Paredes", "", "fdm", "entero", 0.0, 50.0),
    campo("relleno_pct", "Relleno", "%", "fdm", "numero", 0.0, 100.0),
    campo(
        "patron_relleno",
        "Patrón de relleno",
        "",
        "fdm",
        "texto",
        0.0,
        60.0,
    ),
];

pub fn nombre_tecnologia(t: &str) -> &'static str {
    if t == "fdm" { "FDM" } else { "resina" }
}

/// Revisa los parámetros de un perfil: solo campos de su tecnología y dentro de rango. `null` o texto vacío
/// quitan el campo.
pub fn validar_parametros(
    tecnologia: &str,
    p: &Map<String, Value>,
) -> Result<Map<String, Value>, String> {
    let mut limpio = Map::new();
    for (k, v) in p {
        let Some(c) = CAMPOS.iter().find(|c| c.clave == k) else {
            return Err(format!("Campo desconocido: {k}"));
        };
        if c.tecnologia != tecnologia {
            return Err(format!(
                "«{}» no es un campo de {}",
                c.etiqueta,
                nombre_tecnologia(tecnologia)
            ));
        }
        match v {
            Value::Null => continue,
            Value::String(s) if s.trim().is_empty() => continue,
            _ => {}
        }
        if c.tipo == "texto" {
            let s = v
                .as_str()
                .ok_or_else(|| format!("«{}» debe ser texto", c.etiqueta))?
                .trim();
            if s.chars().count() as f64 > c.max {
                return Err(format!("«{}» pasa de {} letras", c.etiqueta, c.max));
            }
            limpio.insert(k.clone(), json!(s));
            continue;
        }
        let n = v
            .as_f64()
            .ok_or_else(|| format!("«{}» debe ser un número", c.etiqueta))?;
        if !n.is_finite() || n < c.min || n > c.max {
            return Err(format!(
                "«{}» debe estar entre {} y {}{}",
                c.etiqueta,
                c.min,
                c.max,
                if c.unidad.is_empty() {
                    String::new()
                } else {
                    format!(" {}", c.unidad)
                }
            ));
        }
        if c.tipo == "entero" {
            if n.fract() != 0.0 {
                return Err(format!("«{}» debe ser un número entero", c.etiqueta));
            }
            limpio.insert(k.clone(), json!(n as i64));
        } else {
            limpio.insert(k.clone(), json!(n));
        }
    }
    Ok(limpio)
}

// ---------- reconocer impresora y perfil de un laminado ----------

/// Condición SQL: el laminado `d` (datos_placa) es de la impresora `i`, porque tiene su misma pantalla.
pub const SQL_MISMA_PANTALLA: &str =
    "(i.tecnologia = 'resina' AND ((d.res_x = i.res_x AND d.res_y = i.res_y)
     OR (d.res_x = i.res_y AND d.res_y = i.res_x)))";

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ImpresoraMin {
    pub id: i64,
    pub nombre: String,
    pub tecnologia: String,
    #[serde(skip)]
    pub res_x: Option<i64>,
    #[serde(skip)]
    pub res_y: Option<i64>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PerfilMin {
    pub id: i64,
    pub nombre: String,
    #[serde(skip)]
    pub impresora_id: i64,
    #[serde(skip)]
    pub material_id: Option<i64>,
    #[serde(skip)]
    pub capa_mm: Option<f64>,
    #[serde(skip)]
    pub exposicion_s: Option<f64>,
    #[serde(skip)]
    pub exposicion_base_s: Option<f64>,
}

/// Impresoras y perfiles en memoria (son pocos), para reconocer los laminados de una lista de placas.
#[derive(Debug, Default)]
pub struct Taller {
    pub impresoras: Vec<ImpresoraMin>,
    pub perfiles: Vec<PerfilMin>,
    pub materiales: HashMap<i64, String>,
}

fn cerca(a: Option<f64>, b: Option<f64>, tolerancia: f64) -> bool {
    matches!((a, b), (Some(a), Some(b)) if (a - b).abs() <= tolerancia)
}

impl Taller {
    pub async fn cargar(pool: &SqlitePool) -> Result<Self, sqlx::Error> {
        let impresoras = sqlx::query_as(
            "SELECT id, nombre, tecnologia, res_x, res_y FROM impresoras ORDER BY en_uso DESC, id",
        )
        .fetch_all(pool)
        .await?;
        let perfiles = sqlx::query_as(
            "SELECT id, nombre, impresora_id, material_id, capa_mm,
                    CAST(json_extract(parametros, '$.exposicion_s') AS REAL) AS exposicion_s,
                    CAST(json_extract(parametros, '$.exposicion_base_s') AS REAL) AS exposicion_base_s
             FROM perfiles ORDER BY en_uso DESC, id",
        )
        .fetch_all(pool)
        .await?;
        let materiales = sqlx::query_as("SELECT id, trim(marca || ' ' || nombre) FROM materiales")
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect();
        Ok(Self {
            impresoras,
            perfiles,
            materiales,
        })
    }

    pub fn impresora(&self, id: i64) -> Option<&ImpresoraMin> {
        self.impresoras.iter().find(|i| i.id == id)
    }

    pub fn perfil(&self, id: i64) -> Option<&PerfilMin> {
        self.perfiles.iter().find(|p| p.id == id)
    }

    /// La impresora de resina con la misma pantalla que el laminado (en cualquier orientación).
    pub fn impresora_de(&self, res_x: Option<i64>, res_y: Option<i64>) -> Option<&ImpresoraMin> {
        let (x, y) = (res_x?, res_y?);
        self.impresoras.iter().find(|i| {
            i.tecnologia == "resina"
                && matches!((i.res_x, i.res_y), (Some(a), Some(b)) if (a, b) == (x, y) || (a, b) == (y, x))
        })
    }

    /// El perfil de esa impresora con la misma altura de capa y exposición (y de base, si las dos la tienen).
    pub fn perfil_de(
        &self,
        impresora: i64,
        capa: Option<f64>,
        exposicion: Option<f64>,
        base: Option<f64>,
    ) -> Option<&PerfilMin> {
        self.perfiles.iter().find(|p| {
            p.impresora_id == impresora
                && cerca(p.capa_mm, capa, 0.0015)
                && cerca(p.exposicion_s, exposicion, 0.02)
                && (p.exposicion_base_s.is_none()
                    || base.is_none()
                    || cerca(p.exposicion_base_s, base, 0.2))
        })
    }
}

// ---------- importación ----------

/// Algo que se puede importar: una impresora, un material o un perfil, con los archivos de donde salió.
#[derive(Debug, Clone, Serialize)]
pub struct Propuesta {
    /// Se guarda en `clave_origen`, para no importar lo mismo dos veces.
    pub clave: String,
    /// impresora | material | perfil
    pub clase: &'static str,
    pub nombre: String,
    /// Resumen para leer: tecnología, capa y exposición, precio…
    pub detalle: String,
    /// Columnas a guardar (en los perfiles, también `parametros`).
    pub datos: Map<String, Value>,
    /// Perfiles: clave de su impresora y de su material.
    pub impresora: Option<String>,
    pub material: Option<String>,
    /// Perfiles: archivos del laminador que se le ligan.
    pub archivos: Vec<i64>,
    /// De dónde salió (rutas de la biblioteca).
    pub fuentes: Vec<String>,
    /// Perfil activo en su laminador: queda como el normal de la impresora.
    pub predeterminado: bool,
    /// Marcada para importar al abrir la página.
    pub marcada: bool,
    pub importable: bool,
    pub avisos: Vec<String>,
    /// Si ya está en la app, su id.
    pub ya: Option<i64>,
}

impl Propuesta {
    fn nueva(clase: &'static str, clave: String, nombre: String) -> Self {
        Self {
            clave,
            clase,
            nombre,
            detalle: String::new(),
            datos: Map::new(),
            impresora: None,
            material: None,
            archivos: Vec::new(),
            fuentes: Vec::new(),
            predeterminado: false,
            marcada: true,
            importable: true,
            avisos: Vec::new(),
            ya: None,
        }
    }

    fn fuente(&mut self, ruta: &str) {
        if !self.fuentes.iter().any(|f| f == ruta) {
            self.fuentes.push(ruta.to_string());
        }
    }

    fn archivo(&mut self, id: i64) {
        if !self.archivos.contains(&id) {
            self.archivos.push(id);
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct Fuente {
    id: i64,
    base: String,
    ruta: String,
    carpeta: String,
    nombre: String,
    ext: String,
    mtime_ms: i64,
}

impl Fuente {
    fn abs(&self) -> PathBuf {
        Path::new(&self.base).join(&self.ruta)
    }
}

/// Minúsculas y sin acentos, solo letras y números separados por un espacio.
pub fn normal(s: &str) -> String {
    crate::placas::normalizar(s).trim().to_string()
}

/// Capa, exposición, exposición de base y capas de base: lo que distingue un perfil de resina.
fn mismo_perfil_resina(
    datos: &Map<String, Value>,
    capa: Option<f64>,
    m: &Map<String, Value>,
) -> bool {
    let p = datos.get("parametros").cloned().unwrap_or(Value::Null);
    datos.get("capa_mm").and_then(Value::as_f64) == capa
        && ["exposicion_s", "exposicion_base_s", "capas_base"]
            .iter()
            .all(|k| p.get(k).and_then(Value::as_f64) == m.get(*k).and_then(Value::as_f64))
}

/// El primer número del nombre que parece una boquilla: «Ender-3 V3 KE 0.4 nozzle» → 0.4.
fn boquilla_del_nombre(s: &str) -> Option<f64> {
    s.split_whitespace()
        .filter_map(|t| t.parse::<f64>().ok())
        .find(|x| (0.1..=2.0).contains(x))
}

/// Solo letras y números, sin espacios: «Ender3v3KE» y «Ender-3 V3 KE» quedan iguales.
fn compacto(s: &str) -> String {
    normal(s).replace(' ', "")
}

fn redondear(x: f64) -> f64 {
    (x * 10_000.0).round() / 10_000.0
}

/// Pone un parámetro si existe en la tecnología y está en rango (lo raro de los archivos se ignora).
fn poner(m: &mut Map<String, Value>, clave: &str, v: Option<f64>) {
    let (Some(v), Some(c)) = (v, CAMPOS.iter().find(|c| c.clave == clave)) else {
        return;
    };
    if !v.is_finite() || v < c.min || v > c.max {
        return;
    }
    if c.tipo == "entero" {
        m.insert(clave.into(), json!(v.round() as i64));
    } else {
        m.insert(clave.into(), json!(redondear(v)));
    }
}

fn leer_hasta(mut r: impl Read, max: u64) -> Result<Vec<u8>, String> {
    let mut v = Vec::new();
    r.by_ref()
        .take(max + 1)
        .read_to_end(&mut v)
        .map_err(|e| format!("no se pudo leer: {e}"))?;
    if v.len() as u64 > max {
        return Err("demasiado grande para ser un perfil".into());
    }
    Ok(v)
}

const MAX_JSON: u64 = 16 << 20;

fn json_de(bytes: &[u8]) -> Result<Value, String> {
    serde_json::from_slice(bytes).map_err(|e| format!("JSON dañado: {e}"))
}

fn texto<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("").trim()
}

fn num(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(Value::as_f64).filter(|x| x.is_finite())
}

// --- Photon Workshop ---

#[derive(Debug, Clone)]
struct ResinaPw {
    /// `property.name` (en las nuevas: «Marca@Resina@Película@Ajuste»)
    clave: String,
    nombre: String,
    marca: String,
    tipo: String,
    precio: Option<f64>,
    volumen: Option<f64>,
    densidad: Option<f64>,
    capa: Option<f64>,
    parametros: Map<String, Value>,
}

fn resina_pw(v: &Value) -> Option<ResinaPw> {
    let p = v.get("property")?;
    let s = v.get("slicepara")?;
    let e = v.get("slice_extpara").cloned().unwrap_or(Value::Null);
    let clave = texto(p, "name").to_string();
    let marca = texto(p, "brand_name").to_string();
    let resina = texto(p, "resin_name");
    let nombre = if resina.is_empty() {
        clave.clone()
    } else if !marca.is_empty() && !normal(resina).starts_with(&normal(&marca)) {
        format!("{marca} {resina}")
    } else {
        resina.to_string()
    };
    if nombre.is_empty() {
        return None;
    }
    let mut m = Map::new();
    poner(&mut m, "exposicion_s", num(s, "exposure_time"));
    poner(&mut m, "exposicion_base_s", num(s, "bott_time"));
    poner(&mut m, "capas_base", num(s, "bott_layers"));
    poner(
        &mut m,
        "capas_transicion",
        num(&e, "transition_layercount").filter(|n| *n > 0.0),
    );
    poner(&mut m, "alzado_mm", num(s, "zup_height"));
    poner(&mut m, "velocidad_alzado_mm_s", num(s, "zup_speed"));
    poner(&mut m, "velocidad_bajada_mm_s", num(s, "zdown_speed"));
    poner(&mut m, "espera_s", num(s, "off_time"));
    poner(&mut m, "antialiasing", num(s, "anti_count"));
    Some(ResinaPw {
        clave,
        nombre,
        marca,
        tipo: texto(p, "type").to_string(),
        precio: num(p, "price"),
        volumen: num(p, "volume"),
        densidad: num(p, "density").map(redondear),
        capa: num(s, "zthick").map(redondear),
        parametros: m,
    })
}

#[derive(Debug)]
struct EscenaPw {
    maquina: String,
    res_x: Option<i64>,
    res_y: Option<i64>,
    pixel_um: Option<f64>,
    volumen: [Option<f64>; 3],
    resinas: Vec<ResinaPw>,
    activas: Vec<String>,
    /// densidad de las resinas propias: (marca, nombre) en minúsculas
    densidades: HashMap<(String, String), f64>,
}

fn entrada_zip(z: &mut zip::ZipArchive<File>, nombre: &str) -> Result<Option<Vec<u8>>, String> {
    match z.by_name(nombre) {
        Ok(e) => leer_hasta(e, MAX_JSON).map(Some),
        Err(zip::result::ZipError::FileNotFound) => Ok(None),
        Err(e) => Err(format!("{nombre}: {e}")),
    }
}

fn leer_pwscene(ruta: &Path) -> Result<EscenaPw, String> {
    let f = File::open(ruta).map_err(|e| format!("no se pudo abrir: {e}"))?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| format!("no es un zip válido: {e}"))?;
    let conf = entrada_zip(&mut z, "config/anycubic_photon_resins.pwsp")?
        .ok_or("no trae la configuración de la impresora")?;
    let v = json_de(&conf)?;
    let t = v.get("machine_type").ok_or("no trae la impresora")?;
    let ext = v.get("machine_extern").cloned().unwrap_or(Value::Null);
    let resinas = ext
        .get("user_resins")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(resina_pw).collect())
        .unwrap_or_default();
    let activas = ext
        .get("active_resins")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let mut densidades = HashMap::new();
    for archivo in [
        "config/anycubic_resin_names.json",
        "config/custom_resin_names.json",
    ] {
        if let Some(b) = entrada_zip(&mut z, archivo)?
            && let Ok(Value::Array(lista)) = json_de(&b)
        {
            for r in lista {
                if let Some(d) = num(&r, "density") {
                    densidades.insert(
                        (normal(texto(&r, "brand_name")), normal(texto(&r, "name"))),
                        redondear(d),
                    );
                }
            }
        }
    }
    let n = |k: &str| num(t, k).filter(|x| *x > 0.0);
    Ok(EscenaPw {
        maquina: texto(t, "name").to_string(),
        res_x: n("res_x").map(|x| x as i64),
        res_y: n("res_y").map(|x| x as i64),
        pixel_um: n("xy_pixel").map(redondear),
        volumen: [
            n("print_xsize").map(|x| (x * 10.0).round() / 10.0),
            n("print_ysize").map(|x| (x * 10.0).round() / 10.0),
            n("print_zsize").map(|x| (x * 10.0).round() / 10.0),
        ],
        resinas,
        activas,
        densidades,
    })
}

fn leer_pwsr(ruta: &Path) -> Result<Vec<ResinaPw>, String> {
    let f = File::open(ruta).map_err(|e| format!("no se pudo abrir: {e}"))?;
    let v = json_de(&leer_hasta(f, MAX_JSON)?)?;
    let lista = match v {
        Value::Array(a) => a,
        otro => vec![otro],
    };
    let r: Vec<ResinaPw> = lista.iter().filter_map(resina_pw).collect();
    if r.is_empty() {
        return Err("no trae ningún perfil de resina".into());
    }
    Ok(r)
}

// --- Orca ---

/// En los perfiles de Orca casi todo es texto o una lista de textos: «["190"]», «"7%"».
fn orca<'a>(v: &'a Value, k: &str) -> Option<&'a Value> {
    match v.get(k)? {
        Value::Array(a) => a.first(),
        x => Some(x),
    }
}

fn orca_texto(v: &Value, k: &str) -> String {
    orca(v, k)
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

fn orca_num(v: &Value, k: &str) -> Option<f64> {
    match orca(v, k)? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().trim_end_matches('%').trim().parse().ok(),
        _ => None,
    }
    .filter(|x: &f64| x.is_finite())
}

/// «0.20mm Standard @Creality…» → 0.2
fn capa_del_nombre(s: &str) -> Option<f64> {
    let i = s.find("mm")?;
    let antes = s[..i].trim_end();
    let inicio = antes
        .rfind(|c: char| !(c.is_ascii_digit() || c == '.'))
        .map(|j| j + 1)
        .unwrap_or(0);
    antes[inicio..]
        .parse::<f64>()
        .ok()
        .filter(|x| *x > 0.0 && *x < 2.0)
}

#[derive(Debug, Default)]
struct PaqueteOrca {
    impresoras: Vec<Value>,
    /// con el JSON suelto de donde salió (en las carpetas desempacadas)
    filamentos: Vec<(Option<i64>, Value)>,
    procesos: Vec<Value>,
    /// El `.orca_printer`, que se liga a todos sus perfiles.
    archivos: Vec<i64>,
    ruta: String,
    /// Carpeta de la impresora: donde está el `.orca_printer` o la carpeta del paquete.
    carpeta: String,
}

fn clasificar_orca(p: &mut PaqueteOrca, v: Value, archivo: Option<i64>) {
    if !v.is_object() {
        return;
    }
    if v.get("printer_settings_id").is_some() || v.get("printer_model").is_some() {
        p.impresoras.push(v);
    } else if v.get("filament_settings_id").is_some() || v.get("filament_type").is_some() {
        p.filamentos.push((archivo, v));
    } else if v.get("print_settings_id").is_some() || v.get("layer_height").is_some() {
        p.procesos.push(v);
    }
}

fn leer_orca_zip(ruta: &Path) -> Result<PaqueteOrca, String> {
    let f = File::open(ruta).map_err(|e| format!("no se pudo abrir: {e}"))?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| format!("no es un zip válido: {e}"))?;
    let nombres: Vec<String> = z
        .file_names()
        .filter_map(Result::ok)
        .filter(|n| n.to_lowercase().ends_with(".json") && !n.ends_with("bundle_structure.json"))
        .map(|n| n.to_string())
        .collect();
    let mut p = PaqueteOrca::default();
    for n in nombres {
        if let Some(b) = entrada_zip(&mut z, &n)?
            && let Ok(v) = json_de(&b)
        {
            clasificar_orca(&mut p, v, None);
        }
    }
    Ok(p)
}

// --- armado de las propuestas ---

#[derive(Default)]
struct Armado {
    lista: Vec<Propuesta>,
    indice: HashMap<String, usize>,
    errores: Vec<String>,
}

impl Armado {
    fn tiene(&self, clave: &str) -> bool {
        self.indice.contains_key(clave)
    }

    fn get(&mut self, clave: &str) -> Option<&mut Propuesta> {
        let i = *self.indice.get(clave)?;
        Some(&mut self.lista[i])
    }

    fn agregar(&mut self, p: Propuesta) -> &mut Propuesta {
        let i = self.lista.len();
        self.indice.insert(p.clave.clone(), i);
        self.lista.push(p);
        &mut self.lista[i]
    }

    fn perfil_igual(
        &self,
        imp: &str,
        capa: Option<f64>,
        parametros: &Map<String, Value>,
        solo_vigentes: bool,
    ) -> Option<usize> {
        self.lista.iter().position(|p| {
            p.clase == "perfil"
                && p.impresora.as_deref() == Some(imp)
                && (p.marcada || !solo_vigentes)
                && mismo_perfil_resina(&p.datos, capa, parametros)
        })
    }

    fn impresoras(&self) -> Vec<(String, String, String)> {
        self.lista
            .iter()
            .filter(|p| p.clase == "impresora")
            .map(|p| {
                (
                    p.clave.clone(),
                    p.nombre.clone(),
                    p.datos
                        .get("modelo")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                )
            })
            .collect()
    }

    /// La impresora cuyo nombre o modelo aparece en el texto (el más largo gana: «Photon Mono 4» antes que
    /// «Photon Mono»).
    fn impresora_en(&self, texto: &str) -> Option<String> {
        let t = compacto(texto);
        self.impresoras()
            .into_iter()
            .flat_map(|(clave, nombre, modelo)| {
                [compacto(&nombre), compacto(&modelo)]
                    .into_iter()
                    .filter(|c| c.len() >= 4 && t.contains(c.as_str()))
                    .map(move |c| (c.len(), clave.clone()))
            })
            .max()
            .map(|(_, c)| c)
    }

    fn impresora(
        &mut self,
        nombre: &str,
        tecnologia: &str,
        fuente: &Fuente,
        carpeta: &str,
    ) -> String {
        let clave = format!("impresora:{}", normal(nombre));
        let (marca, modelo) = match nombre.split_once(' ') {
            Some((m, resto)) if !resto.trim().is_empty() => {
                (m.to_string(), resto.trim().to_string())
            }
            _ => (String::new(), nombre.to_string()),
        };
        // la carpeta solo cuenta si es la de esta impresora (no la de un modelo con una escena suya)
        let carpeta_buena = compacto(carpeta).contains(&compacto(&modelo));
        if !self.tiene(&clave) {
            let mut p = Propuesta::nueva("impresora", clave.clone(), nombre.to_string());
            p.datos.insert("nombre".into(), json!(nombre));
            p.datos.insert("tecnologia".into(), json!(tecnologia));
            p.datos.insert("marca".into(), json!(marca));
            p.datos.insert("modelo".into(), json!(modelo));
            p.datos.insert("carpeta".into(), json!(""));
            self.agregar(p);
        }
        let p = self.get(&clave).expect("recién agregada");
        p.fuente(&fuente.ruta);
        if carpeta_buena && p.datos.get("carpeta").and_then(Value::as_str) == Some("") {
            p.datos.insert("carpeta".into(), json!(carpeta));
        }
        clave
    }

    fn material(
        &mut self,
        tipo: &str,
        marca: &str,
        nombre: &str,
        fuente: &str,
        datos: Map<String, Value>,
    ) -> String {
        let clave = format!("material:{tipo}:{}:{}", normal(marca), normal(nombre));
        if !self.tiene(&clave) {
            let mut p = Propuesta::nueva(
                "material",
                clave.clone(),
                format!("{marca} {nombre}").trim().to_string(),
            );
            p.datos = datos;
            p.datos.insert("tipo".into(), json!(tipo));
            p.datos.insert("marca".into(), json!(marca));
            p.datos.insert("nombre".into(), json!(nombre));
            let unidad = if tipo == "resina" { "ml" } else { "g" };
            let precio = p.datos.get("precio").and_then(Value::as_f64);
            let pres = p.datos.get("presentacion").and_then(Value::as_f64);
            p.detalle = match (precio, pres) {
                (Some(pr), Some(pe)) => format!("{tipo} · ${pr} por {pe} {unidad}"),
                _ => format!("{tipo} · sin precio"),
            };
            match (precio, pres) {
                (None, _) => p
                    .avisos
                    .push("No trae precio: captúralo para calcular costos.".into()),
                (Some(pr), _) if pr == 220.0 && tipo == "resina" => p
                    .avisos
                    .push("220 es el precio de fábrica de Photon Workshop; revísalo.".into()),
                (Some(pr), Some(pe)) if pe >= 500.0 && pr < 100.0 => p.avisos.push(format!(
                    "${pr} por {pe} {unidad} parece estar en dólares; revísalo."
                )),
                _ => {}
            }
            self.agregar(p);
        }
        self.get(&clave).expect("recién agregado").fuente(fuente);
        clave
    }

    fn escena_pw(&mut self, fu: &Fuente, e: EscenaPw, mas_nueva: bool) {
        if e.maquina.is_empty() {
            return;
        }
        let imp = self.impresora(&e.maquina, "resina", fu, &fu.carpeta);
        let p = self.get(&imp).expect("existe");
        for (k, v) in [
            ("res_x", e.res_x.map(|x| json!(x))),
            ("res_y", e.res_y.map(|x| json!(x))),
            ("pixel_um", e.pixel_um.map(|x| json!(x))),
            ("volumen_x_mm", e.volumen[0].map(|x| json!(x))),
            ("volumen_y_mm", e.volumen[1].map(|x| json!(x))),
            ("volumen_z_mm", e.volumen[2].map(|x| json!(x))),
        ] {
            if let Some(v) = v {
                p.datos.entry(k).or_insert(v);
            }
        }
        let maquina = e.maquina.clone();
        for r in e.resinas {
            let activa = e.activas.contains(&r.clave);
            let densidad = e
                .densidades
                .get(&(
                    normal(&r.marca),
                    normal(r.clave.split('@').nth(1).unwrap_or(&r.nombre)),
                ))
                .copied()
                .or(r.densidad);
            self.resina(fu, &imp, &maquina, r, densidad, activa, mas_nueva);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn resina(
        &mut self,
        fu: &Fuente,
        imp: &str,
        maquina: &str,
        r: ResinaPw,
        densidad: Option<f64>,
        activa: bool,
        mas_nueva: bool,
    ) {
        let clave = format!("perfil:{}:{}", normal(maquina), normal(&r.nombre));
        // un .pwsr suelto es casi siempre un perfil exportado: va con el vigente que tiene sus mismos valores
        if fu.ext == "pwsr"
            && let Some(i) = self.perfil_igual(imp, r.capa, &r.parametros, true)
        {
            let p = &mut self.lista[i];
            p.fuente(&fu.ruta);
            p.archivo(fu.id);
            return;
        }
        if self.tiene(&clave) {
            let p = self.get(&clave).expect("existe");
            p.fuente(&fu.ruta);
            if fu.ext == "pwsr" {
                p.archivo(fu.id);
            }
            p.predeterminado |= activa && mas_nueva;
            return;
        }
        // el mismo perfil exportado aparte (.pwsr) con otro nombre: se liga al que ya está
        if fu.ext == "pwsr"
            && let Some(i) = self.perfil_igual(imp, r.capa, &r.parametros, false)
        {
            let p = &mut self.lista[i];
            p.fuente(&fu.ruta);
            p.archivo(fu.id);
            return;
        }
        let tipo_resina = if r.tipo.is_empty() {
            "Resina".to_string()
        } else {
            r.tipo.clone()
        };
        let mut dm = Map::new();
        dm.insert("presentacion".into(), json!(r.volumen.unwrap_or(1000.0)));
        if let Some(p) = r.precio.filter(|p| *p > 0.0) {
            dm.insert("precio".into(), json!(redondear(p)));
        }
        if let Some(d) = densidad.filter(|d| (0.3..=5.0).contains(d)) {
            dm.insert("densidad".into(), json!(d));
        }
        let mat = self.material("resina", &r.marca, &tipo_resina, &fu.ruta, dm);
        let mut p = Propuesta::nueva("perfil", clave, r.nombre.clone());
        p.impresora = Some(imp.to_string());
        p.material = Some(mat);
        p.predeterminado = activa && mas_nueva;
        p.marcada = mas_nueva;
        if !mas_nueva {
            p.avisos.push(
                "Solo está en un archivo más viejo de Photon Workshop: quizá ya no lo usas.".into(),
            );
        }
        p.detalle = maquina.to_string();
        p.datos.insert("nombre".into(), json!(r.nombre));
        if let Some(c) = r.capa.filter(|c| (0.005..=1.0).contains(c)) {
            p.datos.insert("capa_mm".into(), json!(c));
        }
        p.datos
            .insert("parametros".into(), Value::Object(r.parametros));
        p.datos.insert(
            "notas".into(),
            json!(format!("Importado de Photon Workshop: {}", fu.ruta)),
        );
        p.fuente(&fu.ruta);
        if fu.ext == "pwsr" {
            p.archivo(fu.id);
        }
        self.agregar(p);
    }

    fn paquete_orca(&mut self, fu: &Fuente, paq: PaqueteOrca) {
        // la impresora del paquete (casi siempre una; las variantes del mismo modelo se juntan)
        let mut imp: Option<String> = None;
        for v in &paq.impresoras {
            let tecnologia = orca_texto(v, "printer_technology");
            if !tecnologia.is_empty() && tecnologia != "FFF" {
                continue;
            }
            let nombre_preset = orca_texto(v, "name");
            let modelo = orca_texto(v, "printer_model");
            let nombre = if modelo.is_empty() {
                nombre_preset.clone()
            } else {
                modelo
            };
            if nombre.is_empty() {
                continue;
            }
            let clave = self.impresora(&nombre, "fdm", fu, &paq.carpeta);
            let p = self.get(&clave).expect("existe");
            // printable_area: ["0x0", "220x0", "220x220", "0x220"]
            if let Some(Value::Array(area)) = v.get("printable_area") {
                let puntos: Vec<(f64, f64)> = area
                    .iter()
                    .filter_map(|x| {
                        let (a, b) = x.as_str()?.split_once('x')?;
                        Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
                    })
                    .collect();
                let max_x = puntos.iter().map(|p| p.0).fold(0.0, f64::max);
                let max_y = puntos.iter().map(|p| p.1).fold(0.0, f64::max);
                if max_x > 0.0 && max_y > 0.0 {
                    p.datos.entry("volumen_x_mm").or_insert(json!(max_x));
                    p.datos.entry("volumen_y_mm").or_insert(json!(max_y));
                }
            }
            if let Some(z) = orca_num(v, "printable_height").filter(|z| *z > 0.0) {
                p.datos.entry("volumen_z_mm").or_insert(json!(z));
            }
            if let Some(b) = orca_num(v, "nozzle_diameter").filter(|b| (0.1..=2.0).contains(b)) {
                p.datos.entry("boquilla_mm").or_insert(json!(b));
                if let Some(dice) =
                    boquilla_del_nombre(&nombre_preset).filter(|d| (d - b).abs() > 0.01)
                {
                    let aviso = format!(
                        "Los perfiles de Orca dicen {dice} en el nombre, pero su boquilla es de {b} mm."
                    );
                    if !p.avisos.contains(&aviso) {
                        p.avisos.push(aviso);
                    }
                }
            }
            imp.get_or_insert(clave);
        }
        let Some(imp) = imp else {
            if !paq.filamentos.is_empty() {
                self.errores.push(format!(
                    "{}: trae filamentos pero ninguna impresora; no sé de cuál son",
                    paq.ruta
                ));
            }
            return;
        };
        let nombre_imp = self.get(&imp).map(|p| p.nombre.clone()).unwrap_or_default();
        // proceso: altura de capa y relleno (si el paquete trae uno solo)
        let mut base = Map::new();
        let mut capa = None;
        let mut notas_proceso = String::new();
        if let [pr] = paq.procesos.as_slice() {
            let nombre = orca_texto(pr, "name");
            capa = orca_num(pr, "layer_height")
                .or_else(|| capa_del_nombre(&nombre))
                .or_else(|| capa_del_nombre(&orca_texto(pr, "inherits")));
            poner(
                &mut base,
                "relleno_pct",
                orca_num(pr, "sparse_infill_density"),
            );
            poner(&mut base, "paredes", orca_num(pr, "wall_loops"));
            let patron = orca_texto(pr, "sparse_infill_pattern");
            if !patron.is_empty() && patron.chars().count() <= 60 {
                base.insert("patron_relleno".into(), json!(patron));
            }
            notas_proceso = format!(" Proceso: «{nombre}».");
        }
        for (propio, v) in &paq.filamentos {
            let preset = orca_texto(v, "name");
            let preset = if preset.is_empty() {
                orca_texto(v, "filament_settings_id")
            } else {
                preset
            };
            if preset.is_empty() {
                continue;
            }
            let corto = preset
                .split(" @")
                .next()
                .unwrap_or(&preset)
                .trim()
                .to_string();
            let hereda = texto(v, "inherits").to_string();
            let mut subtipo = orca_texto(v, "filament_type");
            if subtipo.is_empty() {
                // «Creality Generic PETG @Ender-3V3-all» → PETG
                subtipo = [
                    "PETG", "PLA", "ABS", "ASA", "TPU", "PA", "PC", "PVA", "HIPS",
                ]
                .into_iter()
                .find(|t| {
                    format!(" {} ", hereda.to_uppercase().replace(['@', '-', '_'], " "))
                        .contains(&format!(" {t} "))
                })
                .unwrap_or("")
                .to_string();
            }
            let marca = orca_texto(v, "filament_vendor");
            let mut dm = Map::new();
            dm.insert("presentacion".into(), json!(1000.0));
            dm.insert("subtipo".into(), json!(subtipo));
            if let Some(c) = orca_num(v, "filament_cost").filter(|c| *c > 0.0) {
                dm.insert("precio".into(), json!(redondear(c)));
            }
            if let Some(d) = orca_num(v, "filament_density").filter(|d| (0.3..=5.0).contains(d)) {
                dm.insert("densidad".into(), json!(d));
            }
            if let Some(d) = orca_num(v, "filament_diameter").filter(|d| (0.5..=5.0).contains(d)) {
                dm.insert("diametro_mm".into(), json!(d));
            }
            let color = orca_texto(v, "default_filament_colour");
            if color.len() == 7
                && color.starts_with('#')
                && color[1..].chars().all(|c| c.is_ascii_hexdigit())
            {
                dm.insert("color_hex".into(), json!(color.to_lowercase()));
            }
            // «ML PLA Básico» de la marca ML es el material «PLA Básico»
            let nombre_mat = match corto.split_once(' ') {
                Some((m, resto)) if !marca.is_empty() && normal(m) == normal(&marca) => {
                    resto.trim().to_string()
                }
                _ => corto.clone(),
            };
            let mat = self.material("filamento", &marca, &nombre_mat, &paq.ruta, dm);

            let clave = format!("perfil:{}:{}", normal(&nombre_imp), normal(&corto));
            if !self.tiene(&clave) {
                let mut m = base.clone();
                let temp = orca_num(v, "nozzle_temperature");
                poner(&mut m, "temp_boquilla_c", temp);
                poner(
                    &mut m,
                    "temp_boquilla_inicial_c",
                    orca_num(v, "nozzle_temperature_initial_layer"),
                );
                let cama =
                    orca_num(v, "textured_plate_temp").or_else(|| orca_num(v, "hot_plate_temp"));
                poner(&mut m, "temp_cama_c", cama);
                poner(
                    &mut m,
                    "temp_cama_inicial_c",
                    orca_num(v, "textured_plate_temp_initial_layer")
                        .or_else(|| orca_num(v, "hot_plate_temp_initial_layer")),
                );
                poner(&mut m, "ventilador_pct", orca_num(v, "fan_max_speed"));
                poner(&mut m, "relacion_flujo", orca_num(v, "filament_flow_ratio"));
                if orca_texto(v, "enable_pressure_advance") == "1" {
                    poner(&mut m, "avance_presion", orca_num(v, "pressure_advance"));
                }
                poner(
                    &mut m,
                    "flujo_max_mm3_s",
                    orca_num(v, "filament_max_volumetric_speed"),
                );
                let mut p = Propuesta::nueva("perfil", clave.clone(), corto.clone());
                p.impresora = Some(imp.clone());
                p.material = Some(mat);
                p.detalle = format!(
                    "{nombre_imp} · {}{}",
                    if subtipo.is_empty() {
                        "filamento".to_string()
                    } else {
                        subtipo.clone()
                    },
                    temp.map(|t| format!(" · {t} °C")).unwrap_or_default()
                );
                p.datos.insert("nombre".into(), json!(corto));
                if let Some(c) = capa.filter(|c| (0.005..=1.0).contains(c)) {
                    p.datos.insert("capa_mm".into(), json!(redondear(c)));
                }
                let mut notas =
                    format!("Importado de Orca: perfil de filamento «{preset}».{notas_proceso}");
                if !hereda.is_empty() {
                    notas.push_str(&format!(
                        " Hereda de «{hereda}»: lo que no cambiaste en Orca no viene en el archivo."
                    ));
                    if m.len() <= base.len() {
                        p.avisos.push(format!(
                            "Casi todo sale de «{hereda}»; el archivo no trae temperaturas."
                        ));
                    }
                }
                p.datos.insert("parametros".into(), Value::Object(m));
                p.datos.insert("notas".into(), json!(notas));
                self.agregar(p);
            }
            let p = self.get(&clave).expect("existe");
            p.fuente(&paq.ruta);
            for a in paq.archivos.iter().chain(propio) {
                p.archivo(*a);
            }
        }
    }

    fn lychee(&mut self, fu: &Fuente) {
        let tallo = fu
            .nombre
            .rsplit_once('.')
            .map(|x| x.0)
            .unwrap_or(&fu.nombre);
        let imp = self.impresora_en(&format!("{} {tallo}", fu.carpeta));
        let nombre_imp = imp
            .as_deref()
            .and_then(|c| self.indice.get(c).map(|i| self.lista[*i].nombre.clone()))
            .unwrap_or_default();
        let clave = format!("perfil:{}:lychee {}", normal(&nombre_imp), normal(tallo));
        if self.tiene(&clave) {
            return;
        }
        let mut p = Propuesta::nueva("perfil", clave, format!("{tallo} (Lychee)"));
        p.detalle = if nombre_imp.is_empty() {
            "Lychee".into()
        } else {
            format!("{nombre_imp} · Lychee")
        };
        p.datos.insert("nombre".into(), json!(p.nombre));
        p.datos.insert("parametros".into(), json!({}));
        p.datos.insert(
            "notas".into(),
            json!(
                "Perfil de resina de Lychee. La app no puede leer sus valores porque el archivo viene cifrado: \
                 cópialos a mano desde Lychee."
            ),
        );
        p.avisos.push(
            "Sin valores: el archivo de Lychee viene cifrado y solo se liga al perfil.".into(),
        );
        if imp.is_none() {
            p.importable = false;
            p.marcada = false;
            p.avisos
                .push("No sé de qué impresora es: créalo a mano y ligale el archivo.".into());
        }
        p.impresora = imp;
        p.fuente(&fu.ruta);
        p.archivo(fu.id);
        self.agregar(p);
    }
}

/// Lee los archivos de perfiles y arma las propuestas (sin tocar la base). Es lento solo la primera vez
/// que se leen las escenas del disco.
fn armar(fuentes: Vec<Fuente>) -> Armado {
    let mut a = Armado::default();
    let mut fuentes = fuentes;
    // lo más nuevo primero: sus valores ganan
    fuentes.sort_by_key(|f| std::cmp::Reverse(f.mtime_ms));

    // 1. Photon Workshop: las escenas traen la impresora y las resinas propias
    let mut vistas: HashSet<String> = HashSet::new();
    for fu in fuentes.iter().filter(|f| f.ext == "pwscene") {
        match leer_pwscene(&fu.abs()) {
            Ok(e) => {
                let nueva = vistas.insert(normal(&e.maquina));
                a.escena_pw(fu, e, nueva);
            }
            Err(e) => a.errores.push(format!("{}: {e}", fu.ruta)),
        }
    }
    // 2. Orca: los .orca_printer y las carpetas de paquetes desempacados
    for fu in fuentes.iter().filter(|f| f.ext == "orca_printer") {
        match leer_orca_zip(&fu.abs()) {
            Ok(mut p) => {
                p.archivos = vec![fu.id];
                p.ruta = fu.ruta.clone();
                p.carpeta = fu.carpeta.clone();
                a.paquete_orca(fu, p);
            }
            Err(e) => a.errores.push(format!("{}: {e}", fu.ruta)),
        }
    }
    for fu in fuentes
        .iter()
        .filter(|f| f.nombre == "bundle_structure.json")
    {
        let dir = format!("{}/", fu.carpeta);
        let mut p = PaqueteOrca {
            ruta: fu.carpeta.clone(),
            carpeta: padre(&fu.carpeta).to_string(),
            ..Default::default()
        };
        for j in fuentes.iter().filter(|j| {
            j.ext == "json" && j.ruta.starts_with(&dir) && j.nombre != "bundle_structure.json"
        }) {
            let leido = File::open(j.abs())
                .map_err(|e| e.to_string())
                .and_then(|f| leer_hasta(f, MAX_JSON))
                .and_then(|b| json_de(&b));
            match leido {
                Ok(v) => clasificar_orca(&mut p, v, Some(j.id)),
                Err(e) => a.errores.push(format!("{}: {e}", j.ruta)),
            }
        }
        a.paquete_orca(fu, p);
    }
    // 3. Perfiles sueltos de Photon Workshop: van con la impresora de su carpeta
    for fu in fuentes.iter().filter(|f| f.ext == "pwsr") {
        match leer_pwsr(&fu.abs()) {
            Ok(rs) => {
                let Some(imp) = a.impresora_en(&fu.ruta) else {
                    a.errores.push(format!(
                        "{}: no sé de qué impresora es (ponlo en la carpeta de la impresora)",
                        fu.ruta
                    ));
                    continue;
                };
                let maquina = a.get(&imp).map(|p| p.nombre.clone()).unwrap_or_default();
                for r in rs {
                    let d = r.densidad;
                    a.resina(fu, &imp, &maquina, r, d, false, true);
                }
            }
            Err(e) => a.errores.push(format!("{}: {e}", fu.ruta)),
        }
    }
    // 4. Lychee: no se puede leer, solo se liga
    for fu in fuentes.iter().filter(|f| f.ext == "lyr") {
        a.lychee(fu);
    }
    for p in a.lista.iter_mut().filter(|p| p.clase == "impresora") {
        p.detalle = detalle_impresora(&p.datos);
    }
    a
}

/// «resina · 9024×5120 · 153.4×87×165 mm» o «FDM · boquilla 0.6 mm · 220×220×245 mm».
fn detalle_impresora(d: &Map<String, Value>) -> String {
    let n = |k: &str| d.get(k).and_then(Value::as_f64);
    let mut partes = vec![
        if d.get("tecnologia").and_then(Value::as_str) == Some("fdm") {
            "FDM".to_string()
        } else {
            "resina".to_string()
        },
    ];
    if let (Some(x), Some(y)) = (n("res_x"), n("res_y")) {
        partes.push(format!("pantalla de {x}×{y}"));
    }
    if let Some(b) = n("boquilla_mm") {
        partes.push(format!("boquilla de {b} mm"));
    }
    if let (Some(x), Some(y), Some(z)) = (n("volumen_x_mm"), n("volumen_y_mm"), n("volumen_z_mm")) {
        partes.push(format!("{x}×{y}×{z} mm"));
    }
    partes.join(" · ")
}

#[derive(Debug, Serialize)]
pub struct Importables {
    pub propuestas: Vec<Propuesta>,
    pub errores: Vec<String>,
}

/// Lo que se puede importar de los archivos de perfiles de la biblioteca principal, marcando lo que ya está.
pub async fn propuestas(pool: &SqlitePool) -> anyhow::Result<Importables> {
    let fuentes: Vec<Fuente> = sqlx::query_as(
        "SELECT a.id, b.ruta AS base, a.ruta, a.carpeta, a.nombre, a.ext, a.mtime_ms
         FROM archivos a JOIN bibliotecas b ON b.id = a.biblioteca_id AND b.tipo = 'principal'
         WHERE a.estado = 'presente' AND (
            a.ext IN ('pwscene', 'pwsr', 'lyr', 'orca_printer')
            OR a.nombre = 'bundle_structure.json'
            OR (a.ext = 'json' AND (a.carpeta LIKE '%/printer' OR a.carpeta LIKE '%/filament'
                                    OR a.carpeta LIKE '%/process')))",
    )
    .fetch_all(pool)
    .await?;
    let mut a = tokio::task::spawn_blocking(move || armar(fuentes)).await?;

    // lo que ya está en la app: por la clave con que se importó o por el nombre
    let mut ya: HashMap<String, i64> = HashMap::new();
    for t in ["impresoras", "materiales", "perfiles"] {
        let filas: Vec<(String, i64)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT clave_origen, id FROM {t} WHERE clave_origen IS NOT NULL"
        )))
        .fetch_all(pool)
        .await?;
        ya.extend(filas);
    }
    let impresoras: Vec<(i64, String)> = sqlx::query_as("SELECT id, nombre FROM impresoras")
        .fetch_all(pool)
        .await?;
    let materiales: Vec<(i64, String, String, String)> =
        sqlx::query_as("SELECT id, tipo, marca, nombre FROM materiales")
            .fetch_all(pool)
            .await?;
    let perfiles: Vec<(i64, i64, String)> =
        sqlx::query_as("SELECT id, impresora_id, nombre FROM perfiles")
            .fetch_all(pool)
            .await?;
    // primero impresoras (los perfiles se buscan por la impresora ya resuelta)
    let mut resueltas: HashMap<String, i64> = HashMap::new();
    for p in a.lista.iter_mut() {
        p.ya = ya.get(&p.clave).copied();
        if p.ya.is_none() {
            p.ya = match p.clase {
                "impresora" => impresoras
                    .iter()
                    .find(|(_, n)| compacto(n) == compacto(&p.nombre))
                    .map(|x| x.0),
                "material" => {
                    let s = |k: &str| {
                        p.datos
                            .get(k)
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string()
                    };
                    materiales
                        .iter()
                        .find(|(_, t, m, n)| {
                            *t == s("tipo")
                                && compacto(m) == compacto(&s("marca"))
                                && compacto(n) == compacto(&s("nombre"))
                        })
                        .map(|x| x.0)
                }
                _ => None,
            };
        }
        if let Some(id) = p.ya
            && p.clase == "impresora"
        {
            resueltas.insert(p.clave.clone(), id);
        }
    }
    for p in a
        .lista
        .iter_mut()
        .filter(|p| p.clase == "perfil" && p.ya.is_none())
    {
        if let Some(imp) = p.impresora.as_ref().and_then(|c| resueltas.get(c)) {
            p.ya = perfiles
                .iter()
                .find(|(_, i, n)| i == imp && compacto(n) == compacto(&p.nombre))
                .map(|x| x.0);
        }
    }
    for p in a.lista.iter_mut().filter(|p| p.ya.is_some()) {
        p.marcada = false;
    }
    let orden = |c: &str| match c {
        "impresora" => 0,
        "material" => 1,
        _ => 2,
    };
    a.lista.sort_by(|x, y| {
        (orden(x.clase), &x.impresora, &x.nombre).cmp(&(orden(y.clase), &y.impresora, &y.nombre))
    });
    Ok(Importables {
        propuestas: a.lista,
        errores: a.errores,
    })
}

#[derive(Debug, Default, Serialize)]
pub struct Importado {
    pub impresoras: usize,
    pub materiales: usize,
    pub perfiles: usize,
    pub archivos: usize,
}

fn columnas(datos: &Map<String, Value>, permitidas: &[&str]) -> Vec<(String, Value)> {
    permitidas
        .iter()
        .filter_map(|k| datos.get(*k).map(|v| (k.to_string(), v.clone())))
        .collect()
}

async fn insertar(
    tx: &mut sqlx::SqliteConnection,
    tabla: &str,
    columnas: Vec<(String, Value)>,
) -> Result<i64, sqlx::Error> {
    let mut qb: sqlx::QueryBuilder<sqlx::Sqlite> =
        sqlx::QueryBuilder::new(format!("INSERT INTO {tabla} ("));
    qb.push(
        columnas
            .iter()
            .map(|(k, _)| k.as_str())
            .collect::<Vec<_>>()
            .join(", "),
    );
    qb.push(") VALUES (");
    let mut sep = qb.separated(", ");
    for (_, v) in columnas {
        match v {
            Value::Null => sep.push_bind(None::<String>),
            Value::Bool(b) => sep.push_bind(b),
            Value::Number(n) if n.is_i64() => sep.push_bind(n.as_i64()),
            Value::Number(n) => sep.push_bind(n.as_f64()),
            Value::String(s) => sep.push_bind(s),
            otro => sep.push_bind(otro.to_string()),
        };
    }
    qb.push(") RETURNING id");
    qb.build_query_scalar().fetch_one(&mut *tx).await
}

/// Importa las propuestas elegidas (y las impresoras y materiales que sus perfiles necesitan).
pub async fn importar(pool: &SqlitePool, claves: &[String]) -> anyhow::Result<Importado> {
    let Importables { propuestas, .. } = propuestas(pool).await?;
    let por_clave: HashMap<&str, &Propuesta> =
        propuestas.iter().map(|p| (p.clave.as_str(), p)).collect();
    let mut elegidas: HashSet<&str> = claves
        .iter()
        .filter_map(|c| por_clave.get(c.as_str()))
        .filter(|p| p.ya.is_none() && p.importable)
        .map(|p| p.clave.as_str())
        .collect();
    let dependencias: Vec<&str> = elegidas
        .iter()
        .filter_map(|c| por_clave.get(c))
        .flat_map(|p| [p.impresora.as_deref(), p.material.as_deref()])
        .flatten()
        .collect();
    elegidas.extend(dependencias);

    let t = ahora();
    let mut ids: HashMap<String, i64> = propuestas
        .iter()
        .filter_map(|p| p.ya.map(|id| (p.clave.clone(), id)))
        .collect();
    let mut res = Importado::default();
    let mut tx = pool.begin().await?;
    for clase in ["impresora", "material", "perfil"] {
        for p in propuestas
            .iter()
            .filter(|p| p.clase == clase && p.ya.is_none() && elegidas.contains(p.clave.as_str()))
        {
            let id = match clase {
                "impresora" => {
                    let mut c = columnas(
                        &p.datos,
                        &[
                            "nombre",
                            "tecnologia",
                            "marca",
                            "modelo",
                            "volumen_x_mm",
                            "volumen_y_mm",
                            "volumen_z_mm",
                            "boquilla_mm",
                            "res_x",
                            "res_y",
                            "pixel_um",
                            "carpeta",
                        ],
                    );
                    c.push(("clave_origen".into(), json!(p.clave)));
                    c.push(("creada".into(), json!(t)));
                    c.push(("actualizada".into(), json!(t)));
                    res.impresoras += 1;
                    insertar(&mut tx, "impresoras", c).await?
                }
                "material" => {
                    let mut c = columnas(
                        &p.datos,
                        &[
                            "tipo",
                            "marca",
                            "nombre",
                            "subtipo",
                            "color_hex",
                            "presentacion",
                            "precio",
                            "densidad",
                            "diametro_mm",
                        ],
                    );
                    c.push(("clave_origen".into(), json!(p.clave)));
                    c.push(("creado".into(), json!(t)));
                    c.push(("actualizado".into(), json!(t)));
                    res.materiales += 1;
                    insertar(&mut tx, "materiales", c).await?
                }
                _ => {
                    let Some(imp) = p.impresora.as_ref().and_then(|c| ids.get(c)).copied() else {
                        continue;
                    };
                    let mut c = columnas(&p.datos, &["nombre", "capa_mm", "notas"]);
                    c.push(("impresora_id".into(), json!(imp)));
                    if let Some(m) = p.material.as_ref().and_then(|c| ids.get(c)) {
                        c.push(("material_id".into(), json!(m)));
                    }
                    c.push((
                        "parametros".into(),
                        json!(
                            p.datos
                                .get("parametros")
                                .cloned()
                                .unwrap_or(json!({}))
                                .to_string()
                        ),
                    ));
                    c.push(("origen".into(), json!("importado")));
                    c.push(("clave_origen".into(), json!(p.clave)));
                    c.push(("creado".into(), json!(t)));
                    c.push(("actualizado".into(), json!(t)));
                    let id = insertar(&mut tx, "perfiles", c).await?;
                    for a in &p.archivos {
                        res.archivos += sqlx::query(
                            "INSERT OR IGNORE INTO perfil_archivos (perfil_id, archivo_id) VALUES (?, ?)",
                        )
                        .bind(id)
                        .bind(a)
                        .execute(&mut *tx)
                        .await?
                        .rows_affected() as usize;
                    }
                    if p.predeterminado {
                        sqlx::query(
                            "UPDATE impresoras SET perfil_id = ? WHERE id = ? AND perfil_id IS NULL",
                        )
                        .bind(id)
                        .bind(imp)
                        .execute(&mut *tx)
                        .await?;
                    }
                    res.perfiles += 1;
                    id
                }
            };
            ids.insert(p.clave.clone(), id);
        }
    }
    tx.commit().await?;
    Ok(res)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn parametros_validos_y_no() {
        let p = json!({ "exposicion_s": 2.5, "capas_base": 5, "alzado_mm": null });
        let m = validar_parametros("resina", p.as_object().unwrap()).unwrap();
        assert_eq!(m.len(), 2);
        let mal = json!({ "temp_boquilla_c": 200 });
        assert!(validar_parametros("resina", mal.as_object().unwrap()).is_err());
        let fuera = json!({ "exposicion_s": 9000 });
        assert!(validar_parametros("resina", fuera.as_object().unwrap()).is_err());
        let medio = json!({ "capas_base": 2.5 });
        assert!(validar_parametros("resina", medio.as_object().unwrap()).is_err());
        let texto = json!({ "patron_relleno": " gyroid " });
        assert_eq!(
            validar_parametros("fdm", texto.as_object().unwrap()).unwrap()["patron_relleno"],
            "gyroid"
        );
    }

    #[test]
    fn capa_en_el_nombre() {
        assert_eq!(
            capa_del_nombre("0.20mm Standard @Creality Ender3V3KE"),
            Some(0.2)
        );
        assert_eq!(capa_del_nombre("Creality Ender-3 V3 KE 0.4 mm"), Some(0.4));
        assert_eq!(capa_del_nombre("Sin capa"), None);
    }

    #[test]
    fn reconoce_impresora_y_perfil() {
        let t = Taller {
            impresoras: vec![ImpresoraMin {
                id: 1,
                nombre: "Mono 4".into(),
                tecnologia: "resina".into(),
                res_x: Some(9024),
                res_y: Some(5120),
            }],
            perfiles: vec![PerfilMin {
                id: 7,
                nombre: "Sunlu".into(),
                impresora_id: 1,
                material_id: None,
                capa_mm: Some(0.03),
                exposicion_s: Some(1.8),
                exposicion_base_s: Some(15.0),
            }],
            materiales: HashMap::new(),
        };
        assert_eq!(
            t.impresora_de(Some(5120), Some(9024)).map(|i| i.id),
            Some(1)
        );
        assert!(t.impresora_de(Some(1440), Some(2560)).is_none());
        assert_eq!(
            t.perfil_de(1, Some(0.0299), Some(1.8), Some(15.0))
                .map(|p| p.id),
            Some(7)
        );
        assert!(t.perfil_de(1, Some(0.05), Some(1.8), None).is_none());
    }
}
