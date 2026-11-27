//! Fase 9: bandeja «Por catalogar».
//!
//! - Detecta los modelos que se dejan en `Por catalogar/` con las reglas de `catalogo_modelos.py`: se baja por las
//!   carpetas que se llaman como una facción, un estudio o una categoría, y una carpeta es de modelo si tiene mallas,
//!   escenas o comprimidos, o si sus subcarpetas son de piezas o de formato (Supported, Heads, v2…).
//! - Detecta también los modelos que se pusieron directo en su lugar (carpetas de Wargames o DND que ninguna fila
//!   cubre) y los propone leyendo la ruta.
//! - Propone Juego, Facción, Modelo y Estudio con las reglas de `catalogo_propuesta.py`, con su confianza y el
//!   porqué.
//! - Extrae los .zip de la bandeja comprobando el CRC de cada entrada.
//!
//! Aprobar un grupo crea su fila del catálogo y lo mueve con las mudanzas (ver `rutas/bandeja.rs`).

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    path::Path,
    sync::{Arc, atomic::AtomicU64},
};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::SqlitePool;

use crate::{
    db::ahora,
    estructura::{SUELTOS, padre, ruta_nueva},
    transferencias::{ENTRANTE, principal},
};

pub const ENTRADA: &str = "Por catalogar";
const GAMES: [&str; 3] = ["Warhammer 40K", "One Page Rules", "D&D"];

// Generado a partir de wargames/scripts/catalogo_modelos_v1.py y catalogo_propuesta_v1.py.

const MODEL_EXT: &[&str] = &[
    "3mf", "7z", "blend", "chitubox", "ctb", "elesat", "fpp", "lys", "lyt", "mix", "obj", "obv",
    "part", "photon", "pws", "pwscene", "rar", "scene", "step", "stl", "zip", "zpr",
];

const PART_TOKENS: &[&str] = &[
    "3d",
    "accesories",
    "accessories",
    "accessory",
    "acessories",
    "alt",
    "alternate",
    "alternative",
    "arm",
    "armor",
    "armour",
    "arms",
    "assembled",
    "autosave",
    "axe",
    "axes",
    "backpack",
    "backpacks",
    "banner",
    "banners",
    "base",
    "bases",
    "beltless",
    "bit",
    "bits",
    "bitz",
    "blade",
    "blades",
    "bodies",
    "body",
    "bolter",
    "bolters",
    "cannon",
    "cannons",
    "cape",
    "capes",
    "chest",
    "chests",
    "chitu",
    "chitubox",
    "claw",
    "claws",
    "cloak",
    "cloaks",
    "cockpit",
    "combined",
    "connector",
    "connectors",
    "crew",
    "ctb",
    "d",
    "decals",
    "decor",
    "door",
    "doors",
    "engine",
    "engines",
    "exploded",
    "extra",
    "extras",
    "face",
    "faces",
    "feet",
    "file",
    "files",
    "fist",
    "fists",
    "fixed",
    "flight",
    "foot",
    "gear",
    "guide",
    "gun",
    "guns",
    "hair",
    "hammer",
    "hammers",
    "hand",
    "hands",
    "head",
    "heads",
    "heavyrifle",
    "helmet",
    "helmets",
    "helms",
    "hollow",
    "hollowed",
    "holsters",
    "horns",
    "hull",
    "image",
    "images",
    "img",
    "imgs",
    "instruction",
    "instructions",
    "jump",
    "jumppack",
    "key",
    "keys",
    "l",
    "left",
    "leg",
    "legacy",
    "legs",
    "licence",
    "licences",
    "license",
    "lights",
    "lychee",
    "lys",
    "magazines",
    "manual",
    "melee",
    "melta",
    "misc",
    "miscellaneous",
    "mm",
    "multimelta",
    "new",
    "nosupport",
    "nosupports",
    "nsfw",
    "obj",
    "objs",
    "old",
    "option",
    "optional",
    "options",
    "ornament",
    "ornaments",
    "pack",
    "packs",
    "pad",
    "pads",
    "part",
    "parts",
    "pauldron",
    "pauldrons",
    "pendants",
    "photo",
    "photon",
    "photos",
    "pics",
    "picture",
    "pictures",
    "pilot",
    "pilots",
    "pistol",
    "pistols",
    "plate",
    "plates",
    "pose",
    "poses",
    "pouches",
    "power",
    "powerpack",
    "pres",
    "presup",
    "presupport",
    "presupported",
    "presupports",
    "preview",
    "previews",
    "print",
    "printing",
    "purityseals",
    "r",
    "raw",
    "rawstl",
    "readme",
    "ready",
    "references",
    "render",
    "rendered",
    "renders",
    "rifle",
    "rifles",
    "right",
    "sculpt",
    "sculpts",
    "seals",
    "sergeant",
    "sfw",
    "shield",
    "shields",
    "shin",
    "shinpad",
    "shins",
    "shoulder",
    "shoulderpads",
    "shoulders",
    "skirt",
    "skirts",
    "solid",
    "split",
    "sprue",
    "sprues",
    "stand",
    "stands",
    "stl",
    "stls",
    "storm",
    "stormbolter",
    "sup",
    "support",
    "supported",
    "supports",
    "sword",
    "swords",
    "tabard",
    "tabards",
    "tail",
    "tails",
    "textures",
    "thrusters",
    "torso",
    "torsos",
    "track",
    "tracks",
    "transfers",
    "turret",
    "turrets",
    "unsup",
    "unsupported",
    "update",
    "updated",
    "upgrade",
    "upgrades",
    "variant",
    "variants",
    "version",
    "versions",
    "waist",
    "weapon",
    "weapons",
    "whole",
    "wing",
    "wings",
];

const CAT: &[&str] = &[
    "bases",
    "battleline",
    "bitsaccesories",
    "bitsaccessories",
    "characters",
    "dedicatedtransports",
    "dnd",
    "dreadnoughts",
    "elites",
    "enemies",
    "fastattack",
    "flyers",
    "fortifications",
    "heavysupport",
    "hq",
    "infantry",
    "lordsofwar",
    "monsters",
    "studios",
    "subfactions",
    "transports",
    "troops",
    "units",
    "unsorted",
    "vehicles",
];

const FILLER: &[&str] = &[
    "all", "and", "for", "kit", "no", "of", "pack", "range", "set", "sets", "the", "with", "x",
];

/// Nombres de descargas que se corrigen al proponer (el de la izquierda es como suele venir).
const NAME_FIX: &[(&str, &str)] = &[
    ("Abbadon the Despoiler", "Abaddon the Despoiler"),
    ("Demon Prince", "Daemon Prince"),
    ("Chaos Dreadnought", "Helbrute"),
    ("Nurlge Daemon Prince", "Daemon Prince of Nurgle"),
    ("Chosen of Mortarion", "Deathshroud Terminators"),
    ("Plague Champion", "Plague Marine Champion"),
    ("Exhalted Eightbound", "Exalted Eightbound"),
    ("Battle Sisters Squad", "Battle Sister Squad"),
    ("Lohkust Lord", "Lokhust Lord"),
    ("Warriors (8E)", "Necron Warriors (8E)"),
    ("Warriors (9E)", "Necron Warriors (9E)"),
    ("Lord", "Necron Lord"),
    ("Erradicator Squad", "Eradicator Squad"),
    ("Heavy Intercessors", "Heavy Intercessor Squad"),
    ("Intercessors", "Intercessor Squad"),
    ("Assault Intercessors", "Assault Intercessor Squad"),
    ("Captain Gravis", "Captain in Gravis Armour"),
    ("Captain Jump Pack", "Captain with Jump Pack"),
    ("Captain Phobos", "Captain in Phobos Armour"),
    ("Captain Primaris", "Captain"),
    ("Captain Terminator", "Captain in Terminator Armour"),
    ("Chaplain Primaris", "Chaplain"),
    ("Chaplain Terminator", "Chaplain in Terminator Armour"),
    ("Librarian MKVI", "Librarian"),
    ("Librarian Phobos", "Librarian in Phobos Armour"),
    ("Librarian Primaris", "Librarian"),
    ("Librarian Terminator", "Librarian in Terminator Armour"),
    ("Lieutenant Phobos", "Lieutenant in Phobos Armour"),
    ("Techmarine Primaris", "Techmarine"),
    ("Dreadnought Ballistus", "Ballistus Dreadnought"),
    ("Dreadnought Brutalis", "Brutalis Dreadnought"),
    ("Dreadnought Leviathan", "Leviathan Dreadnought"),
    ("Dreadnought Redemptor", "Redemptor Dreadnought"),
    ("Repulsor", "Repulsor"),
    ("Thunderhawk", "Thunderhawk Gunship"),
    ("Grimaldus", "Chaplain Grimaldus"),
    ("Calgar", "Marneus Calgar"),
    ("Guilliman", "Roboute Guilliman"),
    ("Kor'Sarro Kahn", "Kor'sarro Khan"),
    ("Dominion Zephon", "Brother Zephon"),
    ("Vespids", "Vespid Stingwings"),
    ("XV 104 Riptide", "Riptide Battlesuit"),
    ("Troops Builder", "Strike Team"),
    ("Gargoyle", "Gargoyles"),
    ("Genestealer", "Genestealers"),
    ("Hormagaunt", "Hormagaunts"),
    ("Termagant", "Termagants"),
    ("Ravener", "Raveners"),
    ("Ripper Swarm", "Ripper Swarms"),
    ("Biovore", "Biovores"),
    ("Pyrovore", "Pyrovores"),
    ("Zoanthrope and Neurothrope", "Zoanthropes and Neurothrope"),
    ("Spore Mines Cluster", "Spore Mines"),
    ("Mucolid Spore", "Mucolid Spores"),
    ("Lictor and Deathleaper", "Lictor and Deathleaper"),
    (
        "Carnifexes and Thornbacks and Screamer-killers and Old one eye",
        "Carnifexes",
    ),
    ("Mawloc and Trygon and Trygon Prime", "Trygon and Mawloc"),
    ("Neurogants", "Neurogaunts"),
    (
        "Hive Tyrant and The Swarmlord",
        "Hive Tyrant and The Swarmlord",
    ),
    ("Paragon Warsuits", "Paragon Warsuits"),
    ("Accursed Cultists", "Accursed Cultists"),
    ("Chaos Terminator Squad", "Chaos Terminators"),
    ("Legionaries", "Legionaries"),
    ("Sanguinius", "Sanguinius"),
    ("Messinius", "Lieutenant Messinius"),
    ("Castellan", "Marshal"),
    ("Seraptek Heavy Construct", "Seraptek Heavy Construct"),
];

// ---------- reglas de nombres ----------

/// «LowerBodyV2_x5» → ["lower", "body", "v", "2", "x", "5"] (como `tokens` de la versión de Python).
fn tokens(name: &str) -> Vec<String> {
    let mut s = String::with_capacity(name.len() + 8);
    let mut prev: Option<char> = None;
    for c in name.chars() {
        if let Some(p) = prev {
            let camel = p.is_ascii_lowercase() && c.is_ascii_uppercase();
            let num_letra = (p.is_ascii_digit() && c.is_ascii_alphabetic())
                || (p.is_ascii_alphabetic() && c.is_ascii_digit());
            if camel || num_letra {
                s.push(' ');
            }
        }
        s.push(c);
        prev = Some(c);
    }
    s.to_lowercase()
        .split(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit()))
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

fn es_version(t: &str) -> bool {
    t.len() > 1 && t.starts_with('v') && t[1..].chars().all(|c| c.is_ascii_digit())
}

/// Carpeta de piezas o de formato: todas sus palabras son de piezas, números o relleno («1-8-Heads», «v2»), y al
/// menos una es de piezas o una versión.
pub fn part_like(name: &str) -> bool {
    // «v2» se parte en «v» y «2»: se vuelven a unir como versión
    let mut t: Vec<String> = Vec::new();
    for x in tokens(name) {
        match t.last_mut() {
            Some(v) if v == "v" && x.chars().all(|c| c.is_ascii_digit()) => v.push_str(&x),
            _ => t.push(x),
        }
    }
    if t.is_empty() {
        return false;
    }
    if t.iter().all(|x| x.chars().all(|c| c.is_ascii_digit())) {
        return true;
    }
    let mm_x = |x: &str| {
        (x.len() > 2 && x.ends_with("mm") && x[..x.len() - 2].chars().all(|c| c.is_ascii_digit()))
            || (x.len() > 1 && x.starts_with('x') && x[1..].chars().all(|c| c.is_ascii_digit()))
    };
    let ok = t.iter().all(|x| {
        PART_TOKENS.contains(&x.as_str())
            || FILLER.contains(&x.as_str())
            || x.chars().all(|c| c.is_ascii_digit())
            || es_version(x)
            || mm_x(x)
    });
    ok && t
        .iter()
        .any(|x| PART_TOKENS.contains(&x.as_str()) || es_version(x))
}

/// Carpeta de categoría genérica («Characters», «HQ», «Lords of War»).
pub fn is_cat(name: &str) -> bool {
    let letras: String = name
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_lowercase())
        .collect();
    CAT.contains(&letras.as_str())
}

/// Minúsculas, solo letras y números separados por un espacio.
fn norm(s: &str) -> String {
    s.to_lowercase()
        .split(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit()))
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Como `norm`, separando antes las palabras en camelCase.
fn norm_nombre(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    let mut prev: Option<char> = None;
    for c in s.chars() {
        if prev.is_some_and(|p| p.is_ascii_lowercase()) && c.is_ascii_uppercase() {
            out.push(' ');
        }
        out.push(c);
        prev = Some(c);
    }
    norm(&out)
}

fn junk(nombre: &str, ext: &str) -> bool {
    nombre.starts_with('.') || ext == "ini"
}

/// «Nombre (Estudio)» → ("Nombre", "Estudio").
fn con_estudio(name: &str) -> Option<(&str, &str)> {
    let sin = name.strip_suffix(')')?;
    let i = sin.rfind(" (")?;
    let (base, estudio) = (&sin[..i], &sin[i + 2..]);
    if base.trim().is_empty()
        || base.ends_with(' ')
        || estudio.is_empty()
        || estudio.contains(['(', ')'])
    {
        return None;
    }
    Some((base, estudio))
}

/// Quita la extensión, guiones bajos y espacios de más, y corrige los nombres que suelen venir mal.
fn clean(name: &str) -> String {
    let mut n = name.to_string();
    if let Some(i) = n.rfind('.') {
        let ext = n[i + 1..].to_lowercase();
        if ["zip", "rar", "7z", "stl", "obj", "lys", "chitubox", "3mf"].contains(&ext.as_str()) {
            n.truncate(i);
        }
    }
    let n: String = n
        .chars()
        .map(|c| if c == '_' || c == '+' { ' ' } else { c })
        .collect();
    let n = n.split_whitespace().collect::<Vec<_>>().join(" ");
    let n = n.trim_matches(|c| c == ' ' || c == '-').to_string();
    NAME_FIX
        .iter()
        .find(|(a, _)| *a == n)
        .map(|(_, b)| b.to_string())
        .unwrap_or(n)
}

/// Profundidad mínima de una carpeta de modelo nueva según la sección (como en `catalogo_rutas.py`).
pub fn min_depth(parts: &[&str]) -> usize {
    if parts.first() == Some(&"DND") {
        return 3;
    }
    if parts.len() > 1 && (parts[1] == "Warhammer 40K" || parts[1] == "One Page Rules") {
        if parts.len() > 3 && parts[3] == "Bits" {
            return 5;
        }
        return if parts[1] == "Warhammer 40K" { 4 } else { 3 };
    }
    3
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Propuesta {
    pub juego: String,
    pub faccion: String,
    pub nombre: String,
    pub estudio: String,
    pub subcarpeta: String,
    /// alta | media | baja
    pub confianza: String,
    pub porque: Vec<String>,
}

/// Una carpeta de la estructura nueva → sus campos (como `parse_path`). Un paréntesis al final del modelo es el
/// estudio, salvo que el nombre completo ya sea un modelo del catálogo.
pub fn parse_path(d: &str, conocidos: &HashSet<String>) -> Propuesta {
    let p: Vec<&str> = d.split('/').collect();
    let (juego, rest): (String, Vec<&str>) = if p[0] == "DND" {
        ("D&D".into(), p[1..].to_vec())
    } else {
        (
            p.get(1).map(|s| s.to_string()).unwrap_or_default(),
            p.get(2..).map(|x| x.to_vec()).unwrap_or_default(),
        )
    };
    let (mut fac, mut model, mut sub) = (String::new(), String::new(), Vec::<&str>::new());
    if rest.len() == 1 {
        if ["Game Aids", "Rules", "Otros"].contains(&juego.as_str()) {
            model = rest[0].to_string();
        } else {
            fac = rest[0].to_string();
        }
    } else if juego == "Otros" && rest.len() == 2 {
        fac = rest[0].to_string();
        model = rest[1].to_string();
    } else if rest.len() >= 3 && rest[1] == "Bits" {
        fac = rest[0].to_string();
        model = format!("Bits/{}", rest[2]);
        sub = rest[3..].to_vec();
    } else if rest.len() >= 2 {
        fac = rest[0].to_string();
        model = rest[1].to_string();
        sub = rest[2..].to_vec();
    }
    let mut estudio = String::new();
    let nombre = model.rsplit('/').next().unwrap_or("").to_string();
    if let Some((base, e)) = con_estudio(&nombre)
        && !conocidos.contains(&nombre)
    {
        estudio = e.to_string();
        model = format!("{}{}", &model[..model.len() - nombre.len()], base);
    }
    Propuesta {
        juego,
        faccion: fac,
        nombre: model,
        estudio,
        subcarpeta: sub.join("/"),
        ..Default::default()
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ModeloMin {
    pub id: i64,
    pub carpeta: String,
    pub juego: String,
    pub faccion: String,
    pub nombre: String,
    pub estudio: String,
}

/// Lo que ya hay en el catálogo, para reconocer descargas nuevas.
pub struct Pistas {
    fac: HashMap<String, (String, String)>,
    studio: HashMap<String, String>,
    /// nombre normalizado → (juego, facción, modelo, estudio)
    model: HashMap<String, BTreeSet<(String, String, String, String)>>,
    models_by_len: Vec<String>,
}

impl Pistas {
    pub fn new(modelos: &[ModeloMin]) -> Self {
        let mut fac = HashMap::new();
        let mut studio = HashMap::new();
        let mut model: HashMap<String, BTreeSet<(String, String, String, String)>> = HashMap::new();
        for m in modelos {
            if GAMES.contains(&m.juego.as_str()) && !m.faccion.is_empty() {
                fac.entry(norm_nombre(&m.faccion))
                    .or_insert((m.juego.clone(), m.faccion.clone()));
            }
            if !m.estudio.is_empty() {
                studio
                    .entry(norm_nombre(&m.estudio))
                    .or_insert(m.estudio.clone());
            }
            if !m.nombre.is_empty() && !m.nombre.starts_with('(') {
                let ultimo = m.nombre.rsplit('/').next().unwrap_or(&m.nombre);
                model.entry(norm_nombre(ultimo)).or_default().insert((
                    m.juego.clone(),
                    m.faccion.clone(),
                    m.nombre.clone(),
                    m.estudio.clone(),
                ));
            }
        }
        let mut models_by_len: Vec<String> = model
            .keys()
            .filter(|k| k.split(' ').count() >= 2)
            .cloned()
            .collect();
        models_by_len.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
        Self {
            fac,
            studio,
            model,
            models_by_len,
        }
    }

    /// Propuesta para un grupo de «Por catalogar» (como `Pistas.propose`).
    pub fn propose(&self, clave: &str) -> Propuesta {
        let carpeta = clave.strip_suffix(SUELTOS).unwrap_or(clave);
        let rel = carpeta
            .strip_prefix(&format!("{ENTRADA}/"))
            .unwrap_or(carpeta);
        let comps: Vec<&str> = rel.split('/').filter(|c| !c.is_empty()).collect();
        let mut name = comps.last().map(|c| clean(c)).unwrap_or_default();
        let (mut juego, mut fac, mut studio) = (String::new(), String::new(), String::new());
        let mut porque = Vec::new();
        if let Some((base, e)) = con_estudio(&name)
            && let Some(s) = self.studio.get(&norm_nombre(e))
        {
            studio = s.clone();
            name = base.to_string();
        }
        for c in &comps {
            let n = norm_nombre(&clean(c));
            if studio.is_empty()
                && let Some(s) = self.studio.get(&n)
            {
                studio = s.clone();
            }
            if fac.is_empty()
                && let Some((j, f)) = self.fac.get(&n)
            {
                juego = j.clone();
                fac = f.clone();
                porque.push(format!("la carpeta «{c}» es una facción"));
            }
        }
        if !studio.is_empty() {
            porque.push(format!("estudio {studio}"));
        }
        let mut model = name.clone();
        let n = norm_nombre(&name);
        let hit = self.model.get(&n).or_else(|| {
            let rodeado = format!(" {n} ");
            self.models_by_len
                .iter()
                .find(|k| rodeado.contains(&format!(" {k} ")))
                .and_then(|k| self.model.get(k))
        });
        if let Some(hit) = hit {
            let facs: BTreeSet<(String, String)> = if fac.is_empty() {
                hit.iter()
                    .map(|(j, f, _, _)| (j.clone(), f.clone()))
                    .collect()
            } else {
                [(juego.clone(), fac.clone())].into_iter().collect()
            };
            let de_fac: Vec<&(String, String, String, String)> = hit
                .iter()
                .filter(|(_, f, _, _)| fac.is_empty() || *f == fac)
                .collect();
            let names: BTreeSet<&String> = de_fac.iter().map(|(_, _, m, _)| m).collect();
            if names.len() == 1 {
                model = names.into_iter().next().expect("uno").clone();
                porque.push(format!("el modelo «{model}» ya está en el catálogo"));
                // sin estudio en el nombre: el del modelo del catálogo, si es uno solo
                let estudios: BTreeSet<&String> = de_fac
                    .iter()
                    .map(|(_, _, _, e)| e)
                    .filter(|e| !e.is_empty())
                    .collect();
                if studio.is_empty() && estudios.len() == 1 {
                    studio = estudios.into_iter().next().expect("uno").clone();
                    porque.push(format!("estudio {studio}, como el del catálogo"));
                }
            }
            if fac.is_empty() && facs.len() == 1 {
                let (j, f) = facs.into_iter().next().expect("una");
                juego = j;
                fac = f;
            }
        }
        let confianza = if !fac.is_empty() && hit.is_some() {
            "alta"
        } else if !fac.is_empty() || hit.is_some() {
            "media"
        } else {
            "baja"
        };
        if clave.ends_with(SUELTOS) && hit.is_none() {
            model = "(vistas previas)".into();
        }
        if porque.is_empty() {
            porque.push("no se reconoció: llena Juego, Facción y Modelo".into());
        }
        Propuesta {
            juego,
            faccion: fac,
            nombre: model,
            estudio: studio,
            subcarpeta: String::new(),
            confianza: confianza.into(),
            porque,
        }
    }
}

// ---------- detección ----------

#[derive(Debug, Clone, sqlx::FromRow)]
struct Suelto {
    id: i64,
    ruta: String,
    carpeta: String,
    nombre: String,
    ext: String,
    tipo: String,
    bytes: i64,
    inode: i64,
    miniatura: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Comprimido {
    pub id: i64,
    pub ruta: String,
    pub nombre: String,
    pub bytes: i64,
    /// dónde se extrajo, si ya se extrajo
    pub extraido_en: Option<String>,
    /// solo los .zip se pueden extraer desde la app
    pub se_puede_extraer: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Grupo {
    pub clave: String,
    /// por_catalogar | nueva (ya en su lugar en Wargames o DND)
    pub seccion: &'static str,
    pub archivos: usize,
    pub mallas: usize,
    pub bytes: i64,
    pub ejemplo: String,
    pub portada: Option<Value>,
    pub comprimidos: Vec<Comprimido>,
    pub propuesta: Propuesta,
    pub ruta_nueva: String,
    pub descartado: bool,
    #[serde(skip)]
    pub archivo_ids: Vec<i64>,
}

#[derive(Debug, Serialize)]
pub struct Bandeja {
    /// ¿existe la carpeta «Por catalogar» en la colección?
    pub existe_entrada: bool,
    pub grupos: Vec<Grupo>,
}

/// Los grupos de archivos que no son de ningún modelo: los de «Por catalogar» y los nuevos en su lugar.
pub async fn detectar(pool: &SqlitePool) -> anyhow::Result<Bandeja> {
    let (_, raiz) = principal(pool).await?;
    let modelos: Vec<ModeloMin> =
        sqlx::query_as("SELECT id, carpeta, juego, faccion, nombre, estudio FROM modelos")
            .fetch_all(pool)
            .await?;
    let claves_archivo: HashSet<String> = sqlx::query_scalar(
        "SELECT m.carpeta FROM modelos m WHERE EXISTS (SELECT 1 FROM archivos a WHERE a.ruta = m.carpeta)",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect();
    // contenedores: las carpetas de arriba de cada fila; directas: de la carpeta del modelo a la de la fila
    let mut containers: HashSet<String> = HashSet::new();
    let mut directas: HashSet<String> = HashSet::new();
    for m in &modelos {
        let k = m.carpeta.strip_suffix(SUELTOS).unwrap_or(&m.carpeta);
        let k = if claves_archivo.contains(k) {
            padre(k)
        } else {
            k
        };
        let mf = ruta_nueva(&m.juego, &m.faccion, &m.nombre, &m.estudio, "");
        if k == mf || k.starts_with(&format!("{mf}/")) {
            let partes: Vec<&str> = k.split('/').collect();
            for n in mf.split('/').count()..=partes.len() {
                directas.insert(partes[..n].join("/"));
            }
        }
        let partes: Vec<&str> = k.split('/').collect();
        for n in 1..partes.len() {
            containers.insert(partes[..n].join("/"));
        }
    }
    for d in directas.clone() {
        let partes: Vec<&str> = d.split('/').collect();
        for n in 1..partes.len() {
            containers.insert(partes[..n].join("/"));
        }
    }
    let agrupa: HashSet<String> = modelos
        .iter()
        .flat_map(|m| [norm(&m.faccion), norm(&m.estudio)])
        .filter(|s| !s.is_empty())
        .collect();
    let sueltos: Vec<Suelto> = sqlx::query_as(
        "SELECT a.id, a.ruta, a.carpeta, a.nombre, a.ext, a.tipo, a.bytes, a.inode, mi.clave AS miniatura
         FROM archivos a JOIN bibliotecas b ON b.id = a.biblioteca_id AND b.tipo = 'principal'
         LEFT JOIN miniaturas mi ON mi.archivo_id = a.id AND mi.estado = 'ok'
         WHERE a.estado = 'presente' AND a.modelo_id IS NULL
           AND (a.ruta LIKE 'Wargames/%' OR a.ruta LIKE 'DND/%' OR a.ruta LIKE 'Por catalogar/%')
         ORDER BY a.ruta",
    )
    .fetch_all(pool)
    .await?;

    let mut grupos: HashMap<String, (&'static str, Vec<usize>)> = HashMap::new();
    let mut files: HashMap<String, Vec<usize>> = HashMap::new();
    let mut children: HashMap<String, BTreeSet<String>> = HashMap::new();
    for (i, s) in sueltos.iter().enumerate() {
        let d = s.carpeta.as_str();
        if d == ENTRADA || d.starts_with(&format!("{ENTRADA}/")) {
            files.entry(d.to_string()).or_default().push(i);
            let parts: Vec<&str> = d.split('/').collect();
            for n in 1..parts.len() {
                children
                    .entry(parts[..n].join("/"))
                    .or_default()
                    .insert(parts[..=n].join("/"));
            }
            continue;
        }
        if junk(&s.nombre, &s.ext) || d.is_empty() {
            continue;
        }
        let parts: Vec<&str> = d.split('/').collect();
        let mut clave = None;
        for n in min_depth(&parts)..=parts.len() {
            let c = parts[..n].join("/");
            if !containers.contains(&c) && !directas.contains(&c) {
                clave = Some(c);
                break;
            }
        }
        let clave = clave.unwrap_or_else(|| format!("{d}{SUELTOS}"));
        grupos
            .entry(clave)
            .or_insert(("nueva", Vec::new()))
            .1
            .push(i);
    }

    // «Por catalogar»: separar en modelos
    fn subtree(
        d: &str,
        files: &HashMap<String, Vec<usize>>,
        children: &HashMap<String, BTreeSet<String>>,
    ) -> Vec<usize> {
        let mut out = files.get(d).cloned().unwrap_or_default();
        if let Some(k) = children.get(d) {
            for c in k {
                out.extend(subtree(c, files, children));
            }
        }
        out
    }
    let base_de = |d: &str| d.rsplit('/').next().unwrap_or(d).to_string();
    let model_like = |d: &str| -> bool {
        let kids: Vec<&String> = children
            .get(d)
            .map(|k| k.iter().collect())
            .unwrap_or_default();
        if kids.is_empty()
            || files.get(d).is_some_and(|fs| {
                fs.iter()
                    .any(|i| MODEL_EXT.contains(&sueltos[*i].ext.as_str()))
            })
        {
            return true;
        }
        let base = base_de(d).to_lowercase();
        let names: Vec<String> = kids.iter().map(|k| base_de(k)).collect();
        let part: Vec<bool> = names
            .iter()
            .map(|n| part_like(n) || n.to_lowercase().starts_with(&base))
            .collect();
        // la cabeza más común (8 letras) entre las subcarpetas
        let mut cuenta: Vec<(String, usize)> = Vec::new();
        for n in &names {
            if n.chars().count() >= 8 {
                let h: String = n.to_lowercase().chars().take(8).collect();
                match cuenta.iter_mut().find(|x| x.0 == h) {
                    Some(x) => x.1 += 1,
                    None => cuenta.push((h, 1)),
                }
            }
        }
        let mejor = cuenta
            .iter()
            .fold(None::<&(String, usize)>, |m, x| match m {
                Some(y) if y.1 >= x.1 => Some(y),
                _ => Some(x),
            });
        let head = mejor
            .filter(|(h, c)| *c >= 2 && !h.starts_with("scan") && !h.starts_with("proxy"))
            .map(|(h, _)| h.clone());
        let same: Vec<bool> = names
            .iter()
            .map(|n| {
                head.as_ref()
                    .is_some_and(|h| n.to_lowercase().starts_with(h))
            })
            .collect();
        let n = names.len();
        let n_part = part.iter().filter(|x| **x).count();
        let n_same = same.iter().filter(|x| **x).count();
        let n_any = part.iter().zip(&same).filter(|(a, b)| **a || **b).count();
        n_part * 2 >= n || n_same * 4 >= n * 3 || n_any * 10 >= n * 9
    };
    let mut pila: Vec<String> = children
        .get(ENTRADA)
        .map(|k| k.iter().rev().cloned().collect())
        .unwrap_or_default();
    while let Some(d) = pila.pop() {
        let b = base_de(&d);
        let kids = children.get(&d).cloned().unwrap_or_default();
        if !is_cat(&b) && !agrupa.contains(&norm(&b)) && (model_like(&d) || kids.len() == 1) {
            grupos.insert(d.clone(), ("por_catalogar", subtree(&d, &files, &children)));
            continue;
        }
        for k in kids.iter().rev() {
            pila.push(k.clone());
        }
        let sueltos_aqui: Vec<usize> = files
            .get(&d)
            .map(|fs| {
                fs.iter()
                    .copied()
                    .filter(|i| !junk(&sueltos[*i].nombre, &sueltos[*i].ext))
                    .collect()
            })
            .unwrap_or_default();
        if !sueltos_aqui.is_empty() {
            grupos.insert(format!("{d}{SUELTOS}"), ("por_catalogar", sueltos_aqui));
        }
    }
    for i in files.get(ENTRADA).cloned().unwrap_or_default() {
        let s = &sueltos[i];
        if !junk(&s.nombre, &s.ext) {
            grupos.insert(s.ruta.clone(), ("por_catalogar", vec![i]));
        }
    }

    let pistas = Pistas::new(&modelos);
    let conocidos: HashSet<String> = modelos
        .iter()
        .map(|m| m.nombre.rsplit('/').next().unwrap_or(&m.nombre).to_string())
        .collect();
    let descartes: HashSet<String> = sqlx::query_scalar("SELECT clave FROM bandeja_descartes")
        .fetch_all(pool)
        .await?
        .into_iter()
        .collect();
    let extraidos: HashMap<i64, String> =
        sqlx::query_as::<_, (i64, String)>("SELECT archivo_id, carpeta FROM bandeja_comprimidos")
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect();
    let mut out: Vec<Grupo> = grupos
        .into_iter()
        .map(|(clave, (seccion, idx))| {
            let fs: Vec<&Suelto> = idx.iter().map(|i| &sueltos[*i]).collect();
            let mut vistos = HashSet::new();
            let bytes = fs
                .iter()
                .filter(|s| vistos.insert(s.inode))
                .map(|s| s.bytes)
                .sum();
            let mallas: Vec<&&Suelto> = fs
                .iter()
                .filter(|s| s.ext == "stl" || s.ext == "obj")
                .collect();
            let portada = fs
                .iter()
                .filter(|s| s.miniatura.is_some())
                .min_by_key(|s| {
                    (
                        s.tipo != "imagen",
                        s.tipo != "malla",
                        std::cmp::Reverse(s.bytes),
                    )
                })
                .map(|s| json!({ "id": s.id, "clave": s.miniatura }));
            let comprimidos = fs
                .iter()
                .filter(|s| ["zip", "rar", "7z"].contains(&s.ext.as_str()))
                .map(|s| Comprimido {
                    id: s.id,
                    ruta: s.ruta.clone(),
                    nombre: s.nombre.clone(),
                    bytes: s.bytes,
                    extraido_en: extraidos.get(&s.id).cloned(),
                    se_puede_extraer: s.ext == "zip" && seccion == "por_catalogar",
                })
                .collect();
            let mut propuesta = if seccion == "nueva" {
                let mut p = parse_path(clave.strip_suffix(SUELTOS).unwrap_or(&clave), &conocidos);
                if clave.ends_with(SUELTOS) {
                    p.confianza = "baja".into();
                    p.porque = vec![
                        "archivos sueltos de la carpeta: sin modelo se quedan donde están".into(),
                    ];
                } else {
                    p.confianza = "alta".into();
                    p.porque = vec!["ya está en su lugar: los campos salen de la ruta".into()];
                }
                p
            } else {
                pistas.propose(&clave)
            };
            propuesta.subcarpeta = propuesta.subcarpeta.trim().to_string();
            let ruta = if propuesta.juego.is_empty() {
                String::new()
            } else {
                ruta_nueva(
                    &propuesta.juego,
                    &propuesta.faccion,
                    &propuesta.nombre,
                    &propuesta.estudio,
                    &propuesta.subcarpeta,
                )
            };
            Grupo {
                descartado: descartes.contains(&clave),
                archivos: fs.len(),
                mallas: mallas.len(),
                bytes,
                ejemplo: mallas
                    .first()
                    .map(|s| s.nombre.clone())
                    .or_else(|| fs.first().map(|s| s.nombre.clone()))
                    .unwrap_or_default(),
                portada,
                comprimidos,
                archivo_ids: fs.iter().map(|s| s.id).collect(),
                propuesta,
                ruta_nueva: ruta,
                seccion,
                clave,
            }
        })
        .filter(|g| g.archivos > 0)
        .collect();
    out.sort_by(|a, b| (a.seccion, &a.clave).cmp(&(b.seccion, &b.clave)));
    Ok(Bandeja {
        existe_entrada: raiz.join(ENTRADA).is_dir(),
        grupos: out,
    })
}

// ---------- extraer un zip ----------

#[derive(Debug, Default, Serialize)]
pub struct ResultadoExtraer {
    pub carpeta: String,
    pub entradas: usize,
    pub bytes: u64,
}

/// Revisa que se pueda extraer ese comprimido; devuelve su ruta.
pub async fn validar_extraer(pool: &SqlitePool, archivo: i64) -> anyhow::Result<String> {
    let (biblioteca, _) = principal(pool).await?;
    let (ruta, ext, estado): (String, String, String) =
        sqlx::query_as("SELECT ruta, ext, estado FROM archivos WHERE id = ? AND biblioteca_id = ?")
            .bind(archivo)
            .bind(biblioteca)
            .fetch_optional(pool)
            .await?
            .context("no existe ese archivo")?;
    if !ruta.starts_with(&format!("{ENTRADA}/")) {
        bail!("solo se extraen comprimidos de «{ENTRADA}»");
    }
    if ext != "zip" || estado != "presente" {
        bail!("solo se pueden extraer .zip que estén en el disco");
    }
    let ya: i64 =
        sqlx::query_scalar("SELECT count(*) FROM bandeja_comprimidos WHERE archivo_id = ?")
            .bind(archivo)
            .fetch_one(pool)
            .await?;
    if ya > 0 {
        bail!("ese comprimido ya se extrajo");
    }
    Ok(ruta)
}

/// Extrae un .zip de la bandeja en una carpeta con su nombre (junto a él), comprobando el CRC de cada entrada, y
/// guarda el comprimido dentro de esa carpeta para que viaje con el modelo. Luego vuelve a escanear.
pub async fn extraer(
    pool: &SqlitePool,
    archivo: i64,
    usuario: &str,
) -> anyhow::Result<ResultadoExtraer> {
    let (biblioteca, raiz) = principal(pool).await?;
    let ruta = validar_extraer(pool, archivo).await?;
    let nombre = ruta.rsplit('/').next().unwrap_or(&ruta).to_string();
    let tallo = Path::new(&nombre)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "comprimido".into());
    let padre_rel = padre(&ruta).to_string();
    let mut carpeta_rel = format!("{padre_rel}/{tallo}");
    let mut n = 2;
    while raiz.join(&carpeta_rel).exists() {
        if n > 50 {
            bail!("ya hay demasiadas carpetas llamadas «{tallo}»");
        }
        carpeta_rel = format!("{padre_rel}/{tallo} ({n})");
        n += 1;
    }
    let zip_abs = raiz.join(&ruta);
    let staging = raiz.join(ENTRANTE).join(format!("bandeja-{archivo}"));
    std::fs::remove_dir_all(&staging).ok();
    let destino = raiz.join(&carpeta_rel);
    let (z, s, d) = (zip_abs.clone(), staging.clone(), destino.clone());
    let nombre_b = nombre.clone();
    let (entradas, bytes) = tokio::task::spawn_blocking(move || -> anyhow::Result<(usize, u64)> {
        let r = crate::transferencias::extraer_zip_en(&z, &s, true);
        let (n, b) = match r {
            Ok(x) => x,
            Err(e) => {
                std::fs::remove_dir_all(&s).ok();
                bail!("{e}");
            }
        };
        std::fs::create_dir_all(d.parent().expect("tiene carpeta"))?;
        std::fs::rename(&s, &d)?;
        // el comprimido va dentro de su carpeta
        std::fs::rename(&z, d.join(&nombre_b))?;
        Ok((n, b))
    })
    .await??;
    std::fs::remove_dir(raiz.join(ENTRANTE)).ok();
    let nueva_ruta = format!("{carpeta_rel}/{nombre}");
    sqlx::query("UPDATE archivos SET ruta = ?, carpeta = ? WHERE id = ?")
        .bind(&nueva_ruta)
        .bind(&carpeta_rel)
        .bind(archivo)
        .execute(pool)
        .await?;
    sqlx::query(
        "INSERT INTO bandeja_comprimidos (archivo_id, carpeta, entradas, bytes, usuario, extraido) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(archivo)
    .bind(&carpeta_rel)
    .bind(entradas as i64)
    .bind(bytes as i64)
    .bind(usuario)
    .bind(ahora())
    .execute(pool)
    .await?;
    crate::inventario::escanear(pool, biblioteca, Arc::new(AtomicU64::new(0))).await?;
    Ok(ResultadoExtraer {
        carpeta: carpeta_rel,
        entradas,
        bytes,
    })
}

/// Lo que trae un zip (para verlo antes de extraer).
pub fn contenido_zip(ruta: &Path) -> Result<(usize, u64, Vec<String>), String> {
    let f = std::fs::File::open(ruta).map_err(|e| e.to_string())?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| format!("no es un zip válido: {e}"))?;
    let mut nombres = Vec::new();
    let mut bytes = 0;
    let mut n = 0;
    for i in 0..z.len() {
        let e = z.by_index_raw(i).map_err(|e| e.to_string())?;
        if e.is_dir() {
            continue;
        }
        n += 1;
        bytes += e.size();
        if nombres.len() < 200 {
            nombres.push(e.name().map(|x| x.to_string()).unwrap_or_default());
        }
    }
    Ok((n, bytes, nombres))
}

// ---------- revisar y aprobar ----------

/// Los campos que el usuario le pone a un grupo.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CamposGrupo {
    pub clave: String,
    #[serde(default)]
    pub juego: String,
    #[serde(default)]
    pub faccion: String,
    #[serde(default)]
    pub nombre: String,
    #[serde(default)]
    pub estudio: String,
    #[serde(default)]
    pub subcarpeta: String,
}

impl CamposGrupo {
    fn limpio(&self) -> Self {
        Self {
            clave: self.clave.clone(),
            juego: self.juego.trim().to_string(),
            faccion: self.faccion.trim().to_string(),
            nombre: self.nombre.trim().to_string(),
            estudio: self.estudio.trim().to_string(),
            subcarpeta: self.subcarpeta.trim().to_string(),
        }
    }

    fn ruta(&self) -> String {
        ruta_nueva(
            &self.juego,
            &self.faccion,
            &self.nombre,
            &self.estudio,
            &self.subcarpeta,
        )
    }
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ModeloCorto {
    pub id: i64,
    pub nombre: String,
    pub carpeta: String,
}

/// Lo que pasaría con esos campos: la ruta, si se mueve, errores, choques y con qué modelo se podría juntar.
#[derive(Debug, Default, Serialize)]
pub struct Revision {
    pub ruta_nueva: String,
    pub se_mueve: bool,
    pub errores: Vec<String>,
    pub choques: Vec<String>,
    /// modelos en su lugar cuya carpeta contiene la ruta: el grupo se puede juntar con ellos
    pub juntar: Vec<ModeloCorto>,
}

pub async fn revisar(pool: &SqlitePool, c: &CamposGrupo) -> anyhow::Result<Revision> {
    let c = c.limpio();
    let mut r = Revision::default();
    for (campo, v) in [
        ("juego", &c.juego),
        ("faccion", &c.faccion),
        ("nombre", &c.nombre),
        ("estudio", &c.estudio),
        ("subcarpeta", &c.subcarpeta),
    ] {
        if let Err(e) = crate::edicion::validar(campo, v) {
            r.errores.push(format!("{campo}: {e}"));
        }
    }
    if c.juego.is_empty() {
        r.errores.push("Falta el juego".into());
        return Ok(r);
    }
    if c.faccion.is_empty() && c.nombre.is_empty() {
        r.errores.push("Falta la facción o el modelo".into());
    }
    let ruta = c.ruta();
    let k = c.clave.strip_suffix(SUELTOS).unwrap_or(&c.clave);
    r.se_mueve = !(k == ruta || k.starts_with(&format!("{ruta}/")));
    r.ruta_nueva = ruta;
    if !r.se_mueve {
        return Ok(r);
    }
    let modelos: Vec<crate::edicion::Campos> = sqlx::query_as(
        "SELECT id, carpeta, juego, faccion, nombre, estudio, subcarpeta, confianza, notas, revisado FROM modelos",
    )
    .fetch_all(pool)
    .await?;
    let claves_archivo: HashSet<String> = sqlx::query_scalar(
        "SELECT m.carpeta FROM modelos m WHERE EXISTS (SELECT 1 FROM archivos a WHERE a.ruta = m.carpeta)",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect();
    let j = r.ruta_nueva.clone();
    for m in &modelos {
        let sueltos = m.carpeta.ends_with(SUELTOS);
        let o = m.carpeta.strip_suffix(SUELTOS).unwrap_or(&m.carpeta);
        let dentro = j == o || j.starts_with(&format!("{o}/"));
        if crate::mudanzas::choca(o, sueltos, &j) {
            let nombre = if m.nombre.is_empty() {
                m.faccion.clone()
            } else {
                m.nombre.clone()
            };
            r.choques
                .push(format!("«{j}» choca con la carpeta de «{nombre}» ({o})"));
            if dentro
                && !m.carpeta.ends_with(SUELTOS)
                && !claves_archivo.contains(&m.carpeta)
                && !crate::edicion::esta_pendiente(m)
            {
                r.juntar.push(ModeloCorto {
                    id: m.id,
                    nombre,
                    carpeta: m.carpeta.clone(),
                });
            }
        }
    }
    Ok(r)
}

fn si() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct Aprobacion {
    #[serde(flatten)]
    pub campos: CamposGrupo,
    #[serde(default)]
    pub etiquetas: Vec<String>,
    #[serde(default)]
    pub notas: String,
    /// moverlo a su ruta nueva (si no, la fila queda pendiente de mover)
    #[serde(default = "si")]
    pub mover: bool,
    /// juntar los archivos con un modelo que ya está en su lugar, en vez de crear una fila nueva
    #[serde(default)]
    pub juntar_con: Option<i64>,
    /// mandar a frío los comprimidos ya extraídos (y quitarlos del disco), después de moverlos
    #[serde(default = "si")]
    pub comprimidos_a_frio: bool,
}

#[derive(Debug, Default, Serialize)]
pub struct Aprobado {
    /// listo | mover (falta correr la mudanza) | conflictos
    pub estado: &'static str,
    pub modelo_id: Option<i64>,
    pub mudanza_id: Option<i64>,
    pub transferencia_id: Option<i64>,
    pub ubicacion: Option<String>,
    pub archivos: u64,
    pub conflictos: Vec<String>,
    pub avisos: Vec<String>,
}

/// Quita la fila recién creada si algo falló después (los archivos vuelven a la bandeja).
async fn deshacer_alta(pool: &SqlitePool, id: i64) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM modelos_fts WHERE rowid = ?")
        .bind(id)
        .execute(pool)
        .await?;
    sqlx::query("UPDATE archivos SET modelo_id = NULL WHERE modelo_id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM modelos WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

async fn descartar_plan(pool: &SqlitePool, id: i64) -> anyhow::Result<()> {
    sqlx::query("UPDATE mudanzas SET estado = 'descartada' WHERE id = ? AND estado = 'plan'")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Aprueba un grupo: crea su fila (o lo junta con un modelo) y arma la mudanza; si no se puede mover, no deja
/// nada a medias. La mudanza (y el comprimido a frío) los corre la ruta en segundo plano.
pub async fn aprobar(
    pool: &SqlitePool,
    a: &Aprobacion,
    usuario: &str,
) -> Result<Aprobado, crate::error::ApiError> {
    use crate::error::ApiError;
    let c = a.campos.limpio();
    let bandeja = detectar(pool).await?;
    let g = bandeja
        .grupos
        .into_iter()
        .find(|g| g.clave == c.clave)
        .ok_or_else(|| {
            ApiError::Conflicto("Ese grupo ya no está en la bandeja; vuelve a cargarla".into())
        })?;
    let rev = revisar(pool, &c).await?;
    if !rev.errores.is_empty() {
        return Err(ApiError::Invalido(rev.errores.join("; ")));
    }
    let extraidos: Vec<i64> = g
        .comprimidos
        .iter()
        .filter(|z| z.extraido_en.is_some())
        .map(|z| z.id)
        .collect();
    let se_mueve = a.juntar_con.is_some() || (a.mover && rev.se_mueve);
    if se_mueve {
        crate::trabajos::alguno_en_curso(pool).await?;
    }
    let frio: Option<(i64, String)> = if se_mueve && a.comprimidos_a_frio && !extraidos.is_empty() {
        Some(
            sqlx::query_as(
                "SELECT id, nombre FROM ubicaciones_frio WHERE predeterminada = 1 AND escribible = 1 LIMIT 1",
            )
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| {
                ApiError::Invalido(
                    "No hay una ubicación de frío predeterminada para el comprimido: márcala en «Frío» o \
                     desmarca «mandar el comprimido a frío»"
                        .into(),
                )
            })?,
        )
    } else {
        None
    };
    let mut res = Aprobado::default();

    if let Some(t) = a.juntar_con {
        if !rev.juntar.iter().any(|m| m.id == t) {
            return Err(ApiError::Invalido(
                "No se puede juntar con ese modelo: la ruta no cae dentro de su carpeta".into(),
            ));
        }
        let plan = crate::mudanzas::armar_juntar(
            pool,
            t,
            &g.archivo_ids,
            &g.clave,
            &rev.ruta_nueva,
            usuario,
        )
        .await?;
        if !plan.conflictos.is_empty() {
            descartar_plan(pool, plan.id).await?;
            res.estado = "conflictos";
            res.conflictos = plan.conflictos;
            res.avisos = plan.avisos;
            return Ok(res);
        }
        sqlx::query(
            "INSERT INTO historial (modelo_id, campo, antes, despues, usuario, fecha) VALUES (?, 'bandeja', ?, ?, ?, ?)",
        )
        .bind(t)
        .bind(&g.clave)
        .bind(format!("se juntó en {}", rev.ruta_nueva))
        .bind(usuario)
        .bind(ahora())
        .execute(pool)
        .await?;
        res.modelo_id = Some(t);
        res.mudanza_id = Some(plan.id);
        res.archivos = plan.archivos;
        res.avisos = plan.avisos;
    } else {
        let ya: i64 = sqlx::query_scalar("SELECT count(*) FROM modelos WHERE carpeta = ?")
            .bind(&g.clave)
            .fetch_one(pool)
            .await?;
        if ya > 0 {
            return Err(ApiError::Conflicto(
                "Ya hay una fila del catálogo con esa carpeta".into(),
            ));
        }
        let p = &g.propuesta;
        let igual = (&c.juego, &c.faccion, &c.nombre, &c.estudio, &c.subcarpeta)
            == (&p.juego, &p.faccion, &p.nombre, &p.estudio, &p.subcarpeta);
        let t = ahora();
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO modelos (carpeta, juego, faccion, nombre, estudio, subcarpeta, confianza, notas, revisado,
                                  creado, actualizado)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(&g.clave)
        .bind(&c.juego)
        .bind(&c.faccion)
        .bind(&c.nombre)
        .bind(&c.estudio)
        .bind(&c.subcarpeta)
        .bind(&p.confianza)
        .bind(a.notas.trim())
        .bind(if igual { "OK" } else { "Corregido" })
        .bind(t)
        .bind(t)
        .fetch_one(pool)
        .await?;
        let r: Result<(), ApiError> = async {
            sqlx::query(
                "INSERT INTO historial (modelo_id, campo, antes, despues, usuario, fecha) VALUES (?, 'bandeja', '', ?, ?, ?)",
            )
            .bind(id)
            .bind(if g.seccion == "nueva" {
                "agregado desde su lugar".to_string()
            } else {
                format!("aprobado en «{ENTRADA}»")
            })
            .bind(usuario)
            .bind(t)
            .execute(pool)
            .await?;
            if !a.etiquetas.is_empty() {
                let cambios = crate::edicion::Cambios {
                    etiquetas: Some(a.etiquetas.clone()),
                    ..Default::default()
                };
                crate::edicion::aplicar(pool, id, &cambios, usuario).await?;
            }
            crate::catalogo::ligar_archivos(pool).await?;
            crate::catalogo::actualizar_estadisticas(pool).await?;
            crate::edicion::recalcular(pool, Some(&[id])).await?;
            crate::edicion::reindexar(pool, id).await?;
            let n: i64 = sqlx::query_scalar("SELECT count(*) FROM archivos WHERE modelo_id = ?")
                .bind(id)
                .fetch_one(pool)
                .await?;
            if n == 0 {
                return Err(ApiError::Conflicto(
                    "No se pudieron ligar los archivos a la fila nueva".into(),
                ));
            }
            Ok(())
        }
        .await;
        if let Err(e) = r {
            deshacer_alta(pool, id).await?;
            crate::catalogo::actualizar_estadisticas(pool).await?;
            return Err(e);
        }
        res.modelo_id = Some(id);
        if se_mueve {
            let plan = match crate::mudanzas::armar(pool, Some(&[id]), usuario).await {
                Ok(p) => p,
                Err(e) => {
                    deshacer_alta(pool, id).await?;
                    return Err(e);
                }
            };
            if !plan.conflictos.is_empty() {
                descartar_plan(pool, plan.id).await?;
                deshacer_alta(pool, id).await?;
                crate::catalogo::actualizar_estadisticas(pool).await?;
                res.modelo_id = None;
                res.estado = "conflictos";
                res.conflictos = plan.conflictos;
                res.avisos = plan.avisos;
                return Ok(res);
            }
            res.mudanza_id = Some(plan.id);
            res.archivos = plan.archivos;
            res.avisos = plan.avisos;
        }
    }
    sqlx::query("DELETE FROM bandeja_descartes WHERE clave = ?")
        .bind(&g.clave)
        .execute(pool)
        .await?;
    res.estado = if res.mudanza_id.is_some() {
        "mover"
    } else {
        "listo"
    };
    if let (Some((u, nombre)), Some(m)) = (frio, res.modelo_id) {
        let t = crate::transferencias::nueva_transferencia_de(
            pool, m, u, "subir", true, usuario, &extraidos,
        )
        .await?;
        res.transferencia_id = Some(t);
        res.ubicacion = Some(nombre);
    }
    Ok(res)
}

// ---------- descartar ----------

/// Manda un grupo a «No mover»: no se vuelve a proponer.
pub async fn descartar(
    pool: &SqlitePool,
    clave: &str,
    motivo: &str,
    usuario: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO bandeja_descartes (clave, motivo, usuario, fecha) VALUES (?, ?, ?, ?)
         ON CONFLICT (clave) DO UPDATE SET motivo = excluded.motivo, usuario = excluded.usuario, fecha = excluded.fecha",
    )
    .bind(clave)
    .bind(motivo.trim())
    .bind(usuario)
    .bind(ahora())
    .execute(pool)
    .await?;
    Ok(())
}

/// Lo vuelve a proponer.
pub async fn volver(pool: &SqlitePool, clave: &str) -> anyhow::Result<bool> {
    Ok(sqlx::query("DELETE FROM bandeja_descartes WHERE clave = ?")
        .bind(clave)
        .execute(pool)
        .await?
        .rows_affected()
        > 0)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn piezas_y_categorias() {
        for si in [
            "Supported",
            "Heads",
            "1-8-Heads",
            "Heads-RawStl",
            "v2",
            "Heads Set 1",
            "32mm",
            "Lychee",
        ] {
            assert!(part_like(si), "{si}");
        }
        for no in ["Canoptek Wraiths", "Warriors", "Set", ""] {
            assert!(!part_like(no), "{no}");
        }
        assert!(is_cat("Lords of War"));
        assert!(is_cat("HQ"));
        assert!(!is_cat("Necrons"));
        assert_eq!(
            tokens("LowerBodyV2_x5"),
            ["lower", "body", "v", "2", "x", "5"]
        );
    }

    #[test]
    fn rutas_a_campos() {
        let c = HashSet::new();
        let p = parse_path(
            "Wargames/Warhammer 40K/Necrons/Tomb Blades (SF)/Supported",
            &c,
        );
        assert_eq!(
            (
                p.juego.as_str(),
                p.faccion.as_str(),
                p.nombre.as_str(),
                p.estudio.as_str(),
                p.subcarpeta.as_str()
            ),
            ("Warhammer 40K", "Necrons", "Tomb Blades", "SF", "Supported")
        );
        let p = parse_path("Wargames/Warhammer 40K/Necrons/Bits/Heads (X)", &c);
        assert_eq!((p.nombre.as_str(), p.estudio.as_str()), ("Bits/Heads", "X"));
        let p = parse_path("DND/Beast/Boar", &c);
        assert_eq!(
            (p.juego.as_str(), p.faccion.as_str(), p.nombre.as_str()),
            ("D&D", "Beast", "Boar")
        );
        let conocidos: HashSet<String> = ["FishSuit (XV8 Crisis Battlesuit)".to_string()].into();
        let p = parse_path(
            "Wargames/Warhammer 40K/T'au Empire/FishSuit (XV8 Crisis Battlesuit)",
            &conocidos,
        );
        assert_eq!(
            (p.nombre.as_str(), p.estudio.as_str()),
            ("FishSuit (XV8 Crisis Battlesuit)", "")
        );
        assert_eq!(
            min_depth(&["Wargames", "Warhammer 40K", "Necrons", "Bits", "X"]),
            5
        );
        assert_eq!(min_depth(&["DND", "Beast"]), 3);
    }

    #[test]
    fn propuestas_por_nombre() {
        let m = |j: &str, f: &str, n: &str, e: &str| ModeloMin {
            id: 1,
            carpeta: String::new(),
            juego: j.into(),
            faccion: f.into(),
            nombre: n.into(),
            estudio: e.into(),
        };
        let p = Pistas::new(&[
            m("Warhammer 40K", "Orks", "Boyz Mob", "GW"),
            m("Warhammer 40K", "Necrons", "Wraiths", "Station Forge"),
        ]);
        let x = p.propose("Por catalogar/Boyz_Mob Extra");
        assert_eq!(
            (x.juego.as_str(), x.faccion.as_str(), x.nombre.as_str()),
            ("Warhammer 40K", "Orks", "Boyz Mob")
        );
        assert_eq!(x.confianza, "alta");
        assert_eq!(
            x.estudio, "GW",
            "el estudio del único modelo que se llama así"
        );
        let x = p.propose("Por catalogar/Necrons/Canoptek Wraiths (Station Forge)");
        assert_eq!(
            (
                x.faccion.as_str(),
                x.nombre.as_str(),
                x.estudio.as_str(),
                x.confianza.as_str()
            ),
            ("Necrons", "Canoptek Wraiths", "Station Forge", "media")
        );
        let x = p.propose("Por catalogar/cosa rara.stl");
        assert_eq!(
            (x.juego.as_str(), x.nombre.as_str(), x.confianza.as_str()),
            ("", "cosa rara", "baja")
        );
        assert_eq!(clean("Demon Prince.zip"), "Daemon Prince");
    }
}
