//! Lectores de escenas y laminados: datos de impresión y vista previa. Leen solo los pedazos que hacen falta
//! (encabezados e índices), nunca el archivo completo, y nunca escriben.
//!
//! - Chitubox clásico (`.photon`, `.cbddlp`, `.ctb` sin cifrar): encabezado de 112 bytes, parámetros de impresión
//!   y vista previa en RGB555 con repeticiones.
//! - Photon Workshop (`.pws`, `.pwmo`, `.pm4n`…): secciones `HEADER`, `PREVIEW`, `LAYERDEF` y `MACHINE`; la
//!   vista previa es BGR565 sin comprimir.
//! - Lychee (`.lys` y sus autoguardados `.lyt`): un paquete con índice JSON que trae `preview.png`.
//! - Escena de Photon Workshop (`.pwscene`): un zip; algunas traen `preview/image_0.png`.
//! - 3MF: un zip; es un proyecto de laminador (una placa) si trae su configuración o sus placas en `Metadata/`.
//! - `.chitubox` y la escena de Anycubic (`.scene`): no traen datos legibles ni vista previa.

use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

use image::{DynamicImage, RgbImage};
use serde::Serialize;

/// Lo que se sabe de una escena o un laminado. Los números que no cuadran (basura en el archivo) quedan vacíos.
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct Datos {
    pub formato: String,
    /// Falso para un 3MF que solo es una malla.
    pub es_placa: bool,
    pub vista_previa: bool,
    pub maquina: String,
    pub res_x: Option<i64>,
    pub res_y: Option<i64>,
    pub capa_mm: Option<f64>,
    pub exposicion_s: Option<f64>,
    pub exposicion_base_s: Option<f64>,
    pub capas_base: Option<i64>,
    pub capas: Option<i64>,
    pub alto_mm: Option<f64>,
    pub tiempo_s: Option<i64>,
    pub volumen_ml: Option<f64>,
    pub peso_g: Option<f64>,
    pub n_placas: Option<i64>,
}

const EXT_CHITUBOX: [&str; 3] = ["photon", "cbddlp", "ctb"];
const EXT_PHOTON_WORKSHOP: [&str; 12] = [
    "pws", "pw0", "pwmo", "pwmx", "pwma", "pwms", "pwmb", "pmsq", "pm3", "pm3m", "pm3r", "pm4n",
];
const EXT_LYCHEE: [&str; 2] = ["lys", "lyt"];

const MAGIA_PHOTON: u32 = 0x12FD_0019;
const MAGIA_CTB: u32 = 0x12FD_0086;
/// Ni las vistas previas ni los índices de estos formatos se acercan a esto; más es un archivo dañado.
const MAX_PEDAZO: u64 = 32 << 20;
const MAX_LADO: u32 = 4096;

// ---------- utilidades ----------

fn u16_en(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}

fn u32_en(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

fn f32_en(b: &[u8], i: usize) -> f32 {
    f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

/// Lee `n` bytes desde `desde`, revisando que quepan en el archivo.
fn pedazo(f: &mut File, largo: u64, desde: u64, n: u64) -> Result<Vec<u8>, String> {
    if n > MAX_PEDAZO || desde.checked_add(n).is_none_or(|fin| fin > largo) {
        return Err(format!(
            "el archivo está cortado o dañado (pide {n} bytes en {desde}, mide {largo})"
        ));
    }
    f.seek(SeekFrom::Start(desde))
        .map_err(|e| format!("no se pudo leer: {e}"))?;
    let mut v = vec![0u8; n as usize];
    f.read_exact(&mut v)
        .map_err(|e| format!("no se pudo leer: {e}"))?;
    Ok(v)
}

fn abrir(ruta: &Path) -> Result<(File, u64), String> {
    let f = File::open(ruta).map_err(|e| format!("no se pudo abrir: {e}"))?;
    let largo = f
        .metadata()
        .map_err(|e| format!("no se pudo leer: {e}"))?
        .len();
    Ok((f, largo))
}

/// Un número de punto flotante que tenga sentido (mayor que cero y menor que `max`).
fn sensato(x: f32, max: f64) -> Option<f64> {
    let x = x as f64;
    (x.is_finite() && x > 0.0 && x < max).then_some((x * 10_000.0).round() / 10_000.0)
}

fn entero(x: u32, max: u32) -> Option<i64> {
    (x > 0 && x < max).then_some(x as i64)
}

/// Texto terminado en cero, solo si es legible.
fn texto(b: &[u8]) -> String {
    let fin = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    match std::str::from_utf8(&b[..fin]) {
        Ok(s) if s.chars().all(|c| !c.is_control()) => s.trim().to_string(),
        _ => String::new(),
    }
}

// ---------- qué se puede leer ----------

/// Lee lo que se pueda del archivo según su extensión.
pub fn leer(ruta: &Path, ext: &str) -> Result<Datos, String> {
    if EXT_CHITUBOX.contains(&ext) {
        chitubox(ruta)
    } else if EXT_PHOTON_WORKSHOP.contains(&ext) {
        photon_workshop(ruta)
    } else if EXT_LYCHEE.contains(&ext) {
        lychee(ruta).map(|(d, _)| d)
    } else if ext == "pwscene" {
        zip_con_imagen(ruta, "photon_workshop_escena", &["preview/image_0.png"])
    } else if ext == "3mf" {
        tres_mf(ruta)
    } else {
        Ok(Datos {
            formato: match ext {
                "chitubox" => "chitubox_escena".into(),
                "scene" => "anycubic_escena".into(),
                otro => otro.into(),
            },
            es_placa: true,
            ..Default::default()
        })
    }
}

/// La imagen que trae adentro el archivo, si trae.
pub fn vista_previa(ruta: &Path, ext: &str) -> Result<Option<DynamicImage>, String> {
    if EXT_CHITUBOX.contains(&ext) {
        vista_chitubox(ruta)
    } else if EXT_PHOTON_WORKSHOP.contains(&ext) {
        vista_photon_workshop(ruta)
    } else if EXT_LYCHEE.contains(&ext) {
        match lychee(ruta)?.1 {
            Some(png) => decodificar(&png).map(Some),
            None => Ok(None),
        }
    } else if ext == "pwscene" {
        imagen_de_zip(ruta, &["preview/image_0.png"])
    } else if ext == "3mf" {
        imagen_de_zip(ruta, &["Metadata/plate_1.png", "Metadata/thumbnail.png"])
    } else {
        Ok(None)
    }
}

fn decodificar(datos: &[u8]) -> Result<DynamicImage, String> {
    image::load_from_memory(datos).map_err(|e| format!("la vista previa no se pudo leer: {e}"))
}

// ---------- Chitubox clásico (.photon, .cbddlp, .ctb) ----------

fn cabecera_chitubox(f: &mut File, largo: u64) -> Result<Vec<u8>, String> {
    let c = pedazo(f, largo, 0, 112)?;
    match u32_en(&c, 0) {
        MAGIA_PHOTON | MAGIA_CTB => Ok(c),
        m => Err(format!(
            "formato no reconocido (firma {m:#010x}; ¿un CTB cifrado?)"
        )),
    }
}

fn chitubox(ruta: &Path) -> Result<Datos, String> {
    let (mut f, largo) = abrir(ruta)?;
    let c = cabecera_chitubox(&mut f, largo)?;
    let version = u32_en(&c, 4);
    let capa = sensato(f32_en(&c, 32), 1.0);
    let capas = entero(u32_en(&c, 68), 1_000_000);
    let mut d = Datos {
        formato: if u32_en(&c, 0) == MAGIA_CTB {
            "ctb"
        } else {
            "photon"
        }
        .into(),
        es_placa: true,
        res_x: entero(u32_en(&c, 52), 100_000),
        res_y: entero(u32_en(&c, 56), 100_000),
        capa_mm: capa,
        exposicion_s: sensato(f32_en(&c, 36), 600.0),
        exposicion_base_s: sensato(f32_en(&c, 40), 3600.0),
        capas_base: entero(u32_en(&c, 48), 10_000),
        capas,
        alto_mm: sensato(f32_en(&c, 28), 2000.0).or_else(|| Some(capa? * capas? as f64)),
        tiempo_s: entero(u32_en(&c, 76), 60 * 86_400),
        ..Default::default()
    };
    // parámetros de impresión (versión 2 en adelante): volumen y peso
    let (pp, pp_largo) = (u32_en(&c, 84) as u64, u32_en(&c, 88) as u64);
    if version >= 2
        && pp > 0
        && pp_largo >= 28
        && let Ok(p) = pedazo(&mut f, largo, pp, 28)
    {
        d.volumen_ml = sensato(f32_en(&p, 20), 10_000.0);
        d.peso_g = sensato(f32_en(&p, 24), 20_000.0);
    }
    // nombre de la impresora, en la información del laminador
    let (si, si_largo) = (u32_en(&c, 104) as u64, u32_en(&c, 108) as u64);
    if si > 0
        && si_largo >= 36
        && let Ok(s) = pedazo(&mut f, largo, si, 36)
    {
        let (dir, n) = (u32_en(&s, 28) as u64, u32_en(&s, 32) as u64);
        if (1..=256).contains(&n)
            && let Ok(nombre) = pedazo(&mut f, largo, dir, n)
        {
            d.maquina = texto(&nombre);
        }
    }
    let prev = u32_en(&c, 60) as u64;
    d.vista_previa = prev > 0
        && pedazo(&mut f, largo, prev, 16).is_ok_and(|p| {
            (1..=MAX_LADO).contains(&u32_en(&p, 0)) && (1..=MAX_LADO).contains(&u32_en(&p, 4))
        });
    Ok(d)
}

/// Vista previa RGB555 con repeticiones: cada punto ocupa 2 bytes; si trae el bit 0x20, los 2 siguientes dicen
/// cuántas veces más se repite.
pub fn decodificar_rle_chitubox(datos: &[u8], ancho: u32, alto: u32) -> RgbImage {
    let total = (ancho * alto) as usize;
    let mut px: Vec<u8> = Vec::with_capacity(total * 3);
    let mut i = 0;
    while i + 1 < datos.len() && px.len() < total * 3 {
        let punto = u16_en(datos, i);
        i += 2;
        let rgb = [
            (((punto >> 11) & 0x1f) << 3) as u8,
            (((punto >> 6) & 0x1f) << 3) as u8,
            ((punto & 0x1f) << 3) as u8,
        ];
        let mut veces = 1usize;
        if punto & 0x20 != 0 && i + 1 < datos.len() {
            veces += (datos[i] as usize) | (((datos[i + 1] & 0x0f) as usize) << 8);
            i += 2;
        }
        for _ in 0..veces.min(total - px.len() / 3) {
            px.extend_from_slice(&rgb);
        }
    }
    px.resize(total * 3, 0);
    RgbImage::from_raw(ancho, alto, px).unwrap_or_else(|| RgbImage::new(ancho, alto))
}

fn vista_chitubox(ruta: &Path) -> Result<Option<DynamicImage>, String> {
    let (mut f, largo) = abrir(ruta)?;
    let c = cabecera_chitubox(&mut f, largo)?;
    let prev = u32_en(&c, 60) as u64;
    if prev == 0 {
        return Ok(None);
    }
    let p = pedazo(&mut f, largo, prev, 16)?;
    let (ancho, alto) = (u32_en(&p, 0), u32_en(&p, 4));
    if !(1..=MAX_LADO).contains(&ancho) || !(1..=MAX_LADO).contains(&alto) {
        return Err(format!("vista previa de {ancho}×{alto}: dañada"));
    }
    let datos = pedazo(&mut f, largo, u32_en(&p, 8) as u64, u32_en(&p, 12) as u64)?;
    Ok(Some(DynamicImage::ImageRgb8(decodificar_rle_chitubox(
        &datos, ancho, alto,
    ))))
}

// ---------- Photon Workshop (.pws, .pwmo, .pm4n…) ----------

/// Revisa que en `dir` empiece la sección `marca`.
fn seccion(f: &mut File, largo: u64, dir: u64, marca: &str, n: u64) -> Option<Vec<u8>> {
    if dir == 0 {
        return None;
    }
    let s = pedazo(f, largo, dir, 16 + n).ok()?;
    s[..12].starts_with(marca.as_bytes()).then_some(s)
}

fn marca_pws(f: &mut File, largo: u64) -> Result<Vec<u8>, String> {
    let m = pedazo(f, largo, 0, 48)?;
    if !m.starts_with(b"ANYCUBIC") {
        return Err("formato no reconocido (no empieza con ANYCUBIC)".into());
    }
    Ok(m)
}

fn photon_workshop(ruta: &Path) -> Result<Datos, String> {
    let (mut f, largo) = abrir(ruta)?;
    let m = marca_pws(&mut f, largo)?;
    // direcciones: encabezado en 20, vista previa en 28, capas en 36; la máquina (versión 2 en adelante) en 44
    let h = seccion(&mut f, largo, u32_en(&m, 20) as u64, "HEADER", 72)
        .ok_or("no tiene la sección HEADER")?;
    let capa = sensato(f32_en(&h, 20), 1.0);
    let capas = seccion(&mut f, largo, u32_en(&m, 36) as u64, "LAYERDEF", 4)
        .and_then(|s| entero(u32_en(&s, 16), 1_000_000));
    let maquina = seccion(&mut f, largo, u32_en(&m, 44) as u64, "MACHINE", 96)
        .map(|s| texto(&s[16..112]))
        .unwrap_or_default();
    let vista_previa = seccion(&mut f, largo, u32_en(&m, 28) as u64, "PREVIEW", 12).is_some();
    Ok(Datos {
        formato: "photon_workshop".into(),
        es_placa: true,
        vista_previa,
        maquina,
        res_x: entero(u32_en(&h, 60), 100_000),
        res_y: entero(u32_en(&h, 64), 100_000),
        capa_mm: capa,
        exposicion_s: sensato(f32_en(&h, 24), 600.0),
        exposicion_base_s: sensato(f32_en(&h, 32), 3600.0),
        capas_base: sensato(f32_en(&h, 36), 10_000.0).map(|x| x.round() as i64),
        capas,
        alto_mm: capa
            .zip(capas)
            .map(|(c, n)| (c * n as f64 * 100.0).round() / 100.0),
        tiempo_s: entero(u32_en(&h, 84), 60 * 86_400),
        volumen_ml: sensato(f32_en(&h, 52), 10_000.0),
        // el peso viene vacío o con basura en varias versiones; solo se toma si cuadra con el volumen
        peso_g: sensato(f32_en(&h, 68), 20_000.0).filter(|p| {
            sensato(f32_en(&h, 52), 10_000.0).is_some_and(|v| *p > v * 0.8 && *p < v * 2.5)
        }),
        ..Default::default()
    })
}

fn vista_photon_workshop(ruta: &Path) -> Result<Option<DynamicImage>, String> {
    let (mut f, largo) = abrir(ruta)?;
    let m = marca_pws(&mut f, largo)?;
    let Some(s) = seccion(&mut f, largo, u32_en(&m, 28) as u64, "PREVIEW", 12) else {
        return Ok(None);
    };
    let (ancho, alto) = (u32_en(&s, 16), u32_en(&s, 24));
    if !(1..=MAX_LADO).contains(&ancho) || !(1..=MAX_LADO).contains(&alto) {
        return Err(format!("vista previa de {ancho}×{alto}: dañada"));
    }
    let dir = u32_en(&m, 28) as u64 + 28;
    let datos = pedazo(&mut f, largo, dir, ancho as u64 * alto as u64 * 2)?;
    Ok(Some(DynamicImage::ImageRgb8(decodificar_bgr565(
        &datos, ancho, alto,
    ))))
}

/// Vista previa de Photon Workshop: 2 bytes por punto, azul en los bits altos.
pub fn decodificar_bgr565(datos: &[u8], ancho: u32, alto: u32) -> RgbImage {
    let mut px = Vec::with_capacity(datos.len() / 2 * 3);
    for par in datos.as_chunks::<2>().0 {
        let p = u16::from_le_bytes(*par);
        px.extend_from_slice(&[
            ((p & 0x1f) << 3) as u8,
            (((p >> 5) & 0x3f) << 2) as u8,
            (((p >> 11) & 0x1f) << 3) as u8,
        ]);
    }
    px.resize((ancho * alto * 3) as usize, 0);
    RgbImage::from_raw(ancho, alto, px).unwrap_or_else(|| RgbImage::new(ancho, alto))
}

// ---------- Lychee (.lys, .lyt) ----------

/// Datos y vista previa (PNG) de una escena de Lychee. El archivo empieza con 4 números de 32 bits
/// (4, tamaño del índice, …, largo del JSON) y el JSON dice dónde está cada parte, contando desde 8 + índice.
fn lychee(ruta: &Path) -> Result<(Datos, Option<Vec<u8>>), String> {
    let (mut f, largo) = abrir(ruta)?;
    let c = pedazo(&mut f, largo, 0, 16)?;
    let (indice, json) = (u32_en(&c, 4) as u64, u32_en(&c, 12) as u64);
    if u32_en(&c, 0) != 4 || json + 8 > indice {
        return Err("formato no reconocido (no es una escena de Lychee)".into());
    }
    let j = pedazo(&mut f, largo, 16, json)?;
    let v: serde_json::Value =
        serde_json::from_slice(&j).map_err(|e| format!("índice de Lychee dañado: {e}"))?;
    let prev = ["mangoFiles", "files"]
        .iter()
        .find_map(|k| v.get(k)?.get("preview.png"));
    let png = match prev {
        Some(p) => {
            let desde = match &p["offset"] {
                serde_json::Value::String(s) => s.parse::<u64>().ok(),
                otro => otro.as_u64(),
            };
            let n = p["size"].as_u64();
            match desde.zip(n) {
                Some((desde, n)) => Some(pedazo(&mut f, largo, 8 + indice + desde, n)?),
                None => None,
            }
        }
        None => None,
    };
    let d = Datos {
        formato: "lychee".into(),
        es_placa: true,
        vista_previa: png.as_ref().is_some_and(|p| p.starts_with(b"\x89PNG")),
        ..Default::default()
    };
    Ok((d, png))
}

// ---------- zip: .pwscene y 3MF ----------

fn zip_de(ruta: &Path) -> Result<zip::ZipArchive<File>, String> {
    let f = File::open(ruta).map_err(|e| format!("no se pudo abrir: {e}"))?;
    zip::ZipArchive::new(f).map_err(|e| format!("no es un zip válido: {e}"))
}

fn zip_con_imagen(ruta: &Path, formato: &str, imagenes: &[&str]) -> Result<Datos, String> {
    let z = zip_de(ruta)?;
    let tiene = z
        .file_names()
        .filter_map(Result::ok)
        .any(|n| imagenes.contains(&n.as_ref()));
    Ok(Datos {
        formato: formato.into(),
        es_placa: true,
        vista_previa: tiene,
        ..Default::default()
    })
}

fn imagen_de_zip(ruta: &Path, imagenes: &[&str]) -> Result<Option<DynamicImage>, String> {
    let mut z = zip_de(ruta)?;
    for nombre in imagenes {
        let Ok(mut e) = z.by_name(nombre) else {
            continue;
        };
        if e.size() > MAX_PEDAZO {
            return Err(format!("{nombre} es demasiado grande"));
        }
        let mut v = Vec::with_capacity(e.size() as usize);
        e.read_to_end(&mut v)
            .map_err(|e| format!("no se pudo leer {nombre}: {e}"))?;
        return decodificar(&v).map(Some);
    }
    Ok(None)
}

/// Un 3MF es un proyecto de laminador (Orca, Bambu, Prusa, Cura) si trae su configuración o sus placas.
fn tres_mf(ruta: &Path) -> Result<Datos, String> {
    let z = zip_de(ruta)?;
    let mut proyecto = false;
    let mut placas = std::collections::BTreeSet::new();
    let mut vista = false;
    for n in z.file_names().filter_map(Result::ok) {
        let l = n.to_ascii_lowercase();
        if l == "metadata/project_settings.config"
            || l == "metadata/slic3r_pe.config"
            || l == "metadata/model_settings.config"
            || l.starts_with("cura/")
        {
            proyecto = true;
        }
        if let Some(resto) = l.strip_prefix("metadata/plate_")
            && let Some(num) = resto
                .strip_suffix(".png")
                .or_else(|| resto.strip_suffix(".json"))
            && let Ok(k) = num.parse::<u32>()
        {
            proyecto = true;
            placas.insert(k);
        }
        if l == "metadata/plate_1.png" || l == "metadata/thumbnail.png" {
            vista = true;
        }
    }
    Ok(Datos {
        formato: if proyecto { "3mf_proyecto" } else { "3mf" }.into(),
        es_placa: proyecto,
        vista_previa: vista,
        n_placas: (!placas.is_empty()).then_some(placas.len() as i64),
        ..Default::default()
    })
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use std::io::Write;

    fn poner_u32(v: &mut [u8], i: usize, x: u32) {
        v[i..i + 4].copy_from_slice(&x.to_le_bytes());
    }

    fn poner_f32(v: &mut [u8], i: usize, x: f32) {
        v[i..i + 4].copy_from_slice(&x.to_le_bytes());
    }

    fn temporal(nombre: &str, datos: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("lectores-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(nombre);
        std::fs::write(&p, datos).unwrap();
        p
    }

    /// Un .photon mínimo: encabezado, vista previa de 4×2 (rojo ×5, azul ×3) y parámetros de impresión.
    fn photon_de_prueba() -> Vec<u8> {
        let mut v = vec![0u8; 112];
        poner_u32(&mut v, 0, MAGIA_PHOTON);
        poner_u32(&mut v, 4, 2);
        poner_f32(&mut v, 28, 31.45);
        poner_f32(&mut v, 32, 0.05);
        poner_f32(&mut v, 36, 12.0);
        poner_f32(&mut v, 40, 90.0);
        poner_u32(&mut v, 48, 8);
        poner_u32(&mut v, 52, 1440);
        poner_u32(&mut v, 56, 2560);
        poner_u32(&mut v, 68, 629);
        poner_u32(&mut v, 76, 18_916);
        // vista previa en 112: encabezado de 16 bytes y datos en 128
        poner_u32(&mut v, 60, 112);
        let rojo: u16 = 0x1f << 11 | 0x20; // con repetición
        let azul: u16 = 0x1f | 0x20;
        let mut img = Vec::new();
        img.extend_from_slice(&rojo.to_le_bytes());
        img.extend_from_slice(&[4, 0]); // 1 + 4 = 5
        img.extend_from_slice(&azul.to_le_bytes());
        img.extend_from_slice(&[2, 0]); // 1 + 2 = 3
        let mut cab = vec![0u8; 16];
        poner_u32(&mut cab, 0, 4);
        poner_u32(&mut cab, 4, 2);
        poner_u32(&mut cab, 8, 128);
        poner_u32(&mut cab, 12, img.len() as u32);
        v.extend_from_slice(&cab);
        v.extend_from_slice(&img);
        // parámetros de impresión
        let pp = v.len();
        poner_u32(&mut v, 84, pp as u32);
        poner_u32(&mut v, 88, 60);
        let mut p = vec![0u8; 60];
        poner_f32(&mut p, 20, 42.3);
        poner_f32(&mut p, 24, 46.5);
        v.extend_from_slice(&p);
        v
    }

    #[test]
    fn lee_un_photon() {
        let p = temporal("a.photon", &photon_de_prueba());
        let d = leer(&p, "photon").unwrap();
        assert_eq!(d.formato, "photon");
        assert_eq!((d.res_x, d.res_y), (Some(1440), Some(2560)));
        assert_eq!(d.capa_mm, Some(0.05));
        assert_eq!(d.exposicion_s, Some(12.0));
        assert_eq!(d.capas, Some(629));
        assert_eq!(d.tiempo_s, Some(18_916));
        assert_eq!(d.volumen_ml, Some(42.3));
        assert_eq!(d.peso_g, Some(46.5));
        assert!(d.vista_previa);
        let img = vista_previa(&p, "photon").unwrap().unwrap().to_rgb8();
        assert_eq!(img.dimensions(), (4, 2));
        assert_eq!(img.get_pixel(0, 0).0, [248, 0, 0]);
        assert_eq!(img.get_pixel(0, 1).0, [248, 0, 0]); // el quinto punto
        assert_eq!(img.get_pixel(1, 1).0, [0, 0, 248]);
    }

    #[test]
    fn rechaza_lo_que_no_reconoce() {
        let p = temporal("b.ctb", &[7u8; 200]);
        assert!(leer(&p, "ctb").unwrap_err().contains("no reconocido"));
        let corto = temporal("c.photon", &photon_de_prueba()[..50]);
        assert!(leer(&corto, "photon").is_err());
    }

    /// Un .pm4n mínimo con sus cuatro secciones y una vista previa de 2×1.
    fn pws_de_prueba() -> Vec<u8> {
        let mut v = vec![0u8; 56];
        v[..8].copy_from_slice(b"ANYCUBIC");
        poner_u32(&mut v, 12, 0x205);
        let seccion = |v: &mut Vec<u8>, marca: &str, cuerpo: &[u8]| -> u32 {
            let dir = v.len() as u32;
            let mut m = [0u8; 12];
            m[..marca.len()].copy_from_slice(marca.as_bytes());
            v.extend_from_slice(&m);
            v.extend_from_slice(&(cuerpo.len() as u32).to_le_bytes());
            v.extend_from_slice(cuerpo);
            dir
        };
        let mut h = vec![0u8; 80];
        poner_f32(&mut h, 4, 0.05); // capa (16 + 4 = 20)
        poner_f32(&mut h, 8, 2.8);
        poner_f32(&mut h, 16, 30.0);
        poner_f32(&mut h, 20, 5.0);
        poner_f32(&mut h, 36, 42.26);
        poner_u32(&mut h, 44, 9024);
        poner_u32(&mut h, 48, 5120);
        poner_f32(&mut h, 52, f32::NAN); // peso basura
        poner_u32(&mut h, 68, 10_610);
        let dh = seccion(&mut v, "HEADER", &h);
        let mut p = vec![0u8; 12];
        poner_u32(&mut p, 0, 2);
        p[4] = b'x';
        poner_u32(&mut p, 8, 1);
        p.extend_from_slice(&0x001fu16.to_le_bytes()); // azul en BGR565 → rojo en RGB
        p.extend_from_slice(&0xf800u16.to_le_bytes());
        let dp = seccion(&mut v, "PREVIEW", &p);
        let mut l = vec![0u8; 4];
        poner_u32(&mut l, 0, 1224);
        let dl = seccion(&mut v, "LAYERDEF", &l);
        let mut m = vec![0u8; 96];
        m[..22].copy_from_slice(b"Anycubic Photon Mono 4");
        let dm = seccion(&mut v, "MACHINE", &m);
        poner_u32(&mut v, 20, dh);
        poner_u32(&mut v, 28, dp);
        poner_u32(&mut v, 36, dl);
        poner_u32(&mut v, 44, dm);
        v
    }

    #[test]
    fn lee_un_pm4n() {
        let p = temporal("a.pm4n", &pws_de_prueba());
        let d = leer(&p, "pm4n").unwrap();
        assert_eq!(d.formato, "photon_workshop");
        assert_eq!(d.maquina, "Anycubic Photon Mono 4");
        assert_eq!((d.res_x, d.res_y), (Some(9024), Some(5120)));
        assert_eq!(d.capa_mm, Some(0.05));
        assert_eq!(d.exposicion_s, Some(2.8));
        assert_eq!(d.capas_base, Some(5));
        assert_eq!(d.capas, Some(1224));
        assert_eq!(d.alto_mm, Some(61.2));
        assert_eq!(d.tiempo_s, Some(10_610));
        assert_eq!(d.volumen_ml, Some(42.26));
        assert_eq!(d.peso_g, None);
        let img = vista_previa(&p, "pm4n").unwrap().unwrap().to_rgb8();
        assert_eq!(img.get_pixel(0, 0).0, [248, 0, 0]);
        assert_eq!(img.get_pixel(1, 0).0, [0, 0, 248]);
    }

    #[test]
    fn lee_una_escena_de_lychee() {
        let mut png = Vec::new();
        DynamicImage::ImageRgb8(RgbImage::from_pixel(3, 2, image::Rgb([1, 2, 3])))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let json = format!(
            r#"{{"files":{{}},"mangoFiles":{{"scene.bin":{{"size":5,"offset":"0"}},"preview.png":{{"size":{},"offset":"5"}}}}}}"#,
            png.len()
        );
        let indice = (8 + json.len()).div_ceil(4) * 4;
        let mut v = Vec::new();
        for x in [4, indice as u32, indice as u32 - 4, json.len() as u32] {
            v.extend_from_slice(&x.to_le_bytes());
        }
        v.extend_from_slice(json.as_bytes());
        v.resize(8 + indice, 0);
        v.extend_from_slice(b"12345");
        v.extend_from_slice(&png);
        let p = temporal("a.lys", &v);
        let d = leer(&p, "lys").unwrap();
        assert!(d.vista_previa && d.es_placa);
        let img = vista_previa(&p, "lys").unwrap().unwrap();
        assert_eq!((img.width(), img.height()), (3, 2));
    }

    #[test]
    fn distingue_proyectos_3mf() {
        let crear = |nombre: &str, entradas: &[&str]| {
            let p = temporal(nombre, b"");
            let mut z = zip::ZipWriter::new(File::create(&p).unwrap());
            let op = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            for e in entradas {
                z.start_file(*e, op).unwrap();
                z.write_all(b"x").unwrap();
            }
            z.finish().unwrap();
            p
        };
        let malla = crear("m.3mf", &["3D/3dmodel.model", "Metadata/thumbnail.png"]);
        let d = leer(&malla, "3mf").unwrap();
        assert!(!d.es_placa);
        let proyecto = crear(
            "p.3mf",
            &[
                "3D/3dmodel.model",
                "Metadata/plate_1.png",
                "Metadata/plate_2.png",
                "Metadata/plate_1.json",
                "Metadata/project_settings.config",
            ],
        );
        let d = leer(&proyecto, "3mf").unwrap();
        assert!(d.es_placa && d.vista_previa);
        assert_eq!(d.n_placas, Some(2));
        assert_eq!(d.formato, "3mf_proyecto");
    }
}
