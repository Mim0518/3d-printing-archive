//! Dónde vive una copia en frío y cómo se le habla: un remoto de rclone (Google Drive) o una carpeta montada
//! (un disco externo). La app nunca lee los tokens de rclone: llama al programa con los remotos que el usuario
//! configuró con `rclone config`.
//!
//! Las rutas dentro del almacén son las mismas de la colección («carpeta tal cual»): el archivo
//! `Wargames/…/Torso.stl` queda en `<remoto>/Wargames/…/Torso.stl`.

use std::{
    fs::File,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

use md5::{Digest, Md5};
use serde::Deserialize;

/// Nombre del archivo que marca la raíz de un disco externo (con su identificador adentro).
pub const MARCA: &str = ".archivo-3d-ubicacion";

#[derive(Debug, Clone)]
pub enum Almacen {
    Rclone { programa: String, remoto: String },
    Carpeta { base: PathBuf },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArchivoFrio {
    pub ruta: String,
    pub bytes: i64,
    pub md5: Option<String>,
}

/// MD5 de un archivo, leyéndolo por pedazos.
pub fn md5_de(ruta: &Path) -> std::io::Result<String> {
    let mut f = File::open(ruta)?;
    let mut h = Md5::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

/// Une una ruta relativa a la base sin dejar salir de ella (`..`, rutas absolutas).
pub fn dentro(base: &Path, rel: &str) -> Result<PathBuf, String> {
    if rel.is_empty()
        || rel.starts_with('/')
        || rel
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
    {
        return Err(format!("ruta no válida: {rel}"));
    }
    Ok(base.join(rel))
}

/// Un remoto de rclone válido: «Nombre:ruta», sin opciones escondidas.
pub fn remoto_valido(r: &str) -> bool {
    match r.split_once(':') {
        Some((nombre, ruta)) => {
            !nombre.is_empty()
                && nombre
                    .chars()
                    .all(|c| c.is_alphanumeric() || "-_ .".contains(c))
                && !nombre.starts_with('-')
                && !ruta.starts_with('-')
                && !r.contains(['\n', '\r', '\0'])
        }
        None => false,
    }
}

#[derive(Debug, Deserialize)]
struct EntradaRclone {
    #[serde(rename = "Path")]
    ruta: String,
    #[serde(rename = "Size")]
    bytes: i64,
    #[serde(rename = "Hashes", default)]
    hashes: std::collections::HashMap<String, String>,
}

fn lista_temporal(rutas: &[String]) -> Result<tempfile_simple::Temporal, String> {
    let t = tempfile_simple::Temporal::nuevo("lista").map_err(|e| e.to_string())?;
    let mut f = File::create(&t.0).map_err(|e| e.to_string())?;
    for r in rutas {
        writeln!(f, "{r}").map_err(|e| e.to_string())?;
    }
    Ok(t)
}

/// Un archivo temporal que se borra solo.
mod tempfile_simple {
    use std::path::PathBuf;

    pub struct Temporal(pub PathBuf);

    impl Temporal {
        pub fn nuevo(prefijo: &str) -> std::io::Result<Self> {
            let n: u64 = rand::random();
            let p = std::env::temp_dir()
                .join(format!("archivo3d-{prefijo}-{}-{n:x}", std::process::id()));
            std::fs::File::create(&p)?;
            Ok(Self(p))
        }
    }

    impl Drop for Temporal {
        fn drop(&mut self) {
            std::fs::remove_file(&self.0).ok();
        }
    }
}

/// Corre rclone; con `progreso`, lee sus estadísticas (JSON en stderr) para contar los bytes transferidos.
fn rclone(
    programa: &str,
    args: &[&str],
    progreso: Option<&AtomicU64>,
) -> Result<(i32, String), String> {
    let mut hijo = Command::new(programa)
        .args(args)
        .args([
            "--use-json-log",
            "--stats",
            "1s",
            "--stats-log-level",
            "NOTICE",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("no se pudo correr rclone ({programa}): {e}"))?;
    let stderr = hijo.stderr.take().expect("stderr");
    let mut errores = Vec::new();
    let lector = std::thread::scope(|s| {
        let h = s.spawn(move || {
            let mut errs = Vec::new();
            for linea in BufReader::new(stderr).lines().map_while(Result::ok) {
                let Ok(v) = serde_json::from_str::<serde_json::Value>(&linea) else {
                    errs.push(linea);
                    continue;
                };
                if let (Some(p), Some(b)) = (progreso, v["stats"]["bytes"].as_u64()) {
                    p.store(b, Ordering::Relaxed);
                }
                if v["level"] == "error" {
                    errs.push(v["msg"].as_str().unwrap_or("").to_string());
                }
            }
            errs
        });
        let mut salida = String::new();
        if let Some(mut o) = hijo.stdout.take() {
            o.read_to_string(&mut salida).ok();
        }
        (salida, h.join().unwrap_or_default())
    });
    let (salida, errs) = lector;
    errores.extend(errs);
    let st = hijo.wait().map_err(|e| e.to_string())?;
    let codigo = st.code().unwrap_or(-1);
    if codigo != 0 && codigo != 3 {
        return Err(format!(
            "rclone terminó con error {codigo}: {}",
            errores
                .iter()
                .rev()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(" · ")
        ));
    }
    Ok((codigo, salida))
}

impl Almacen {
    /// Lo que hay bajo `prefijo` (vacío: todo), con su MD5.
    pub fn listar(&self, prefijo: &str) -> Result<Vec<ArchivoFrio>, String> {
        match self {
            Almacen::Rclone { programa, remoto } => {
                let destino = if prefijo.is_empty() {
                    remoto.clone()
                } else {
                    format!("{}/{prefijo}", remoto.trim_end_matches('/'))
                };
                let (codigo, salida) = rclone(
                    programa,
                    &[
                        "lsjson",
                        "-R",
                        "--files-only",
                        "--hash",
                        "--hash-type",
                        "md5",
                        "--no-mimetype",
                        "--no-modtime",
                        &destino,
                    ],
                    None,
                )?;
                if codigo == 3 {
                    // no existe la carpeta: no hay nada
                    return Ok(Vec::new());
                }
                let lista: Vec<EntradaRclone> = serde_json::from_str(&salida)
                    .map_err(|e| format!("rclone devolvió algo raro: {e}"))?;
                Ok(lista
                    .into_iter()
                    .map(|e| ArchivoFrio {
                        ruta: if prefijo.is_empty() {
                            e.ruta
                        } else {
                            format!("{}/{}", prefijo.trim_end_matches('/'), e.ruta)
                        },
                        bytes: e.bytes,
                        md5: e.hashes.get("md5").filter(|m| !m.is_empty()).cloned(),
                    })
                    .collect())
            }
            Almacen::Carpeta { base } => {
                let dir = if prefijo.is_empty() {
                    base.clone()
                } else {
                    dentro(base, prefijo.trim_end_matches('/'))?
                };
                if !dir.exists() {
                    return Ok(Vec::new());
                }
                let mut out = Vec::new();
                for e in walkdir::WalkDir::new(&dir).follow_links(false) {
                    let e = e.map_err(|e| e.to_string())?;
                    if !e.file_type().is_file() || e.file_name() == MARCA {
                        continue;
                    }
                    let rel = e
                        .path()
                        .strip_prefix(base)
                        .map_err(|e| e.to_string())?
                        .to_string_lossy()
                        .replace('\\', "/");
                    let bytes = e.metadata().map_err(|e| e.to_string())?.len() as i64;
                    out.push(ArchivoFrio {
                        md5: Some(md5_de(e.path()).map_err(|e| format!("{rel}: {e}"))?),
                        ruta: rel,
                        bytes,
                    });
                }
                Ok(out)
            }
        }
    }

    /// Sube esas rutas (relativas a `raiz`) a sus mismas rutas en el almacén. No borra nada en el destino.
    pub fn subir(&self, raiz: &Path, rutas: &[String], progreso: &AtomicU64) -> Result<(), String> {
        if rutas.is_empty() {
            return Ok(());
        }
        match self {
            Almacen::Rclone { programa, remoto } => {
                let lista = lista_temporal(rutas)?;
                let lista = lista.0.to_string_lossy().to_string();
                let raiz = raiz.to_string_lossy().to_string();
                rclone(
                    programa,
                    &[
                        "copy",
                        "--checksum",
                        "--no-traverse",
                        "--files-from-raw",
                        &lista,
                        &raiz,
                        remoto,
                    ],
                    Some(progreso),
                )
                .map(|_| ())
            }
            Almacen::Carpeta { base } => {
                for r in rutas {
                    let destino = dentro(base, r)?;
                    if let Some(p) = destino.parent() {
                        std::fs::create_dir_all(p).map_err(|e| format!("{r}: {e}"))?;
                    }
                    let origen = dentro(raiz, r)?;
                    // a un temporal y luego en su lugar: nunca queda un archivo a medias con el nombre bueno
                    let tmp = destino.with_extension("archivo3d-parcial");
                    let n = std::fs::copy(&origen, &tmp).map_err(|e| format!("{r}: {e}"))?;
                    File::open(&tmp)
                        .and_then(|f| f.sync_all())
                        .map_err(|e| format!("{r}: {e}"))?;
                    std::fs::rename(&tmp, &destino).map_err(|e| format!("{r}: {e}"))?;
                    progreso.fetch_add(n, Ordering::Relaxed);
                }
                Ok(())
            }
        }
    }

    /// Baja esas rutas del almacén a `destino/<ruta>`.
    pub fn bajar(
        &self,
        rutas: &[String],
        destino: &Path,
        progreso: &AtomicU64,
    ) -> Result<(), String> {
        if rutas.is_empty() {
            return Ok(());
        }
        match self {
            Almacen::Rclone { programa, remoto } => {
                let lista = lista_temporal(rutas)?;
                let lista = lista.0.to_string_lossy().to_string();
                let destino = destino.to_string_lossy().to_string();
                rclone(
                    programa,
                    &[
                        "copy",
                        "--no-traverse",
                        "--files-from-raw",
                        &lista,
                        remoto,
                        &destino,
                    ],
                    Some(progreso),
                )
                .map(|_| ())
            }
            Almacen::Carpeta { base } => {
                for r in rutas {
                    let origen = dentro(base, r)?;
                    let d = dentro(destino, r)?;
                    if let Some(p) = d.parent() {
                        std::fs::create_dir_all(p).map_err(|e| format!("{r}: {e}"))?;
                    }
                    let n = std::fs::copy(&origen, &d).map_err(|e| format!("{r}: {e}"))?;
                    progreso.fetch_add(n, Ordering::Relaxed);
                }
                Ok(())
            }
        }
    }

    /// ¿Se puede usar ahora? (el remoto responde, o el disco está montado y es el mismo).
    pub fn probar(&self, marca: &str) -> Result<(), String> {
        match self {
            Almacen::Rclone { programa, remoto } => rclone(
                programa,
                &["lsf", "--max-depth", "1", "--dirs-only", remoto],
                None,
            )
            .map(|_| ()),
            Almacen::Carpeta { base } => {
                let leida = std::fs::read_to_string(base.join(MARCA))
                    .map_err(|_| format!("el disco no está conectado en {}", base.display()))?;
                if leida.trim() != marca {
                    return Err(format!("en {} hay otro disco", base.display()));
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn rutas_y_remotos() {
        let b = Path::new("/base");
        assert!(dentro(b, "a/b.stl").is_ok());
        for mala in ["", "/etc/passwd", "a/../b", "a//b", "./a"] {
            assert!(dentro(b, mala).is_err(), "{mala}");
        }
        assert!(remoto_valido("DriveMemo:3DP-archivo"));
        assert!(remoto_valido("DriveMemo:"));
        assert!(!remoto_valido("-x:algo"));
        assert!(!remoto_valido("DriveMemo:--delete"));
        assert!(!remoto_valido("sin-dos-puntos"));
    }

    #[test]
    fn carpeta_ida_y_vuelta() {
        let t = std::env::temp_dir().join(format!("almacen-{}", std::process::id()));
        std::fs::remove_dir_all(&t).ok();
        let raiz = t.join("raiz");
        let base = t.join("frio");
        std::fs::create_dir_all(raiz.join("M/sub")).unwrap();
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(raiz.join("M/a.stl"), b"hola").unwrap();
        std::fs::write(raiz.join("M/sub/b.stl"), b"mundo").unwrap();
        std::fs::write(base.join(MARCA), "abc").unwrap();
        let a = Almacen::Carpeta { base: base.clone() };
        a.probar("abc").unwrap();
        assert!(a.probar("otro").is_err());
        let p = AtomicU64::new(0);
        let rutas = vec!["M/a.stl".to_string(), "M/sub/b.stl".to_string()];
        a.subir(&raiz, &rutas, &p).unwrap();
        assert_eq!(p.load(Ordering::Relaxed), 9);
        let mut l = a.listar("M").unwrap();
        l.sort_by(|x, y| x.ruta.cmp(&y.ruta));
        assert_eq!(l[0].ruta, "M/a.stl");
        assert_eq!(
            l[0].md5.as_deref(),
            Some("4d186321c1a7f0f354b297e8914ab240")
        );
        assert!(a.listar("No/Existe").unwrap().is_empty());
        let vuelta = t.join("vuelta");
        a.bajar(&rutas, &vuelta, &p).unwrap();
        assert_eq!(std::fs::read(vuelta.join("M/sub/b.stl")).unwrap(), b"mundo");
        std::fs::remove_dir_all(&t).ok();
    }
}
