//! Configuración por variables de entorno (ver `deploy/printing-archive.env.example`).

use std::{env, net::SocketAddr, path::PathBuf};

use anyhow::Context;

#[derive(Clone, Debug)]
pub struct Config {
    /// Dirección y puerto donde escucha el servidor.
    pub direccion: SocketAddr,
    /// Archivo de la base SQLite.
    pub base: PathBuf,
    /// Carpeta con el frontend compilado (`frontend/dist`). Si no existe, solo se sirve la API.
    pub estaticos: Option<PathBuf>,
    /// Marca la cookie de sesión como `Secure` (solo viaja por HTTPS, por ejemplo con `tailscale serve`).
    pub cookie_segura: bool,
    /// Días que dura una sesión.
    pub dias_sesion: i64,
    /// Carpeta de la colección (biblioteca principal). En el mini PC, `/srv/3dp`.
    pub biblioteca: Option<PathBuf>,
    /// Caché de miniaturas.
    pub miniaturas: PathBuf,
    /// Programa para renderizar mallas (f3d).
    pub f3d: String,
    /// Cuántas miniaturas se generan a la vez.
    pub paralelo: usize,
}

impl Config {
    pub fn desde_entorno() -> anyhow::Result<Self> {
        let direccion = env::var("ARCHIVO_DIRECCION")
            .unwrap_or_else(|_| "127.0.0.1:8080".into())
            .parse()
            .context("ARCHIVO_DIRECCION no es una dirección válida (ej. 127.0.0.1:8080)")?;
        let base = env::var("ARCHIVO_BASE")
            .unwrap_or_else(|_| "datos/archivo.db".into())
            .into();
        let estaticos = match env::var("ARCHIVO_ESTATICOS") {
            Ok(v) if v.is_empty() => None,
            Ok(v) => Some(PathBuf::from(v)),
            Err(_) => Some(PathBuf::from("../frontend/dist")),
        }
        .filter(|p| p.join("index.html").is_file());
        let cookie_segura = matches!(
            env::var("ARCHIVO_COOKIE_SEGURA").as_deref(),
            Ok("1" | "true" | "si" | "sí")
        );
        let dias_sesion = env::var("ARCHIVO_DIAS_SESION")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30);
        let biblioteca = env::var("ARCHIVO_BIBLIOTECA")
            .ok()
            .filter(|v| !v.is_empty())
            .map(PathBuf::from);
        let miniaturas = env::var("ARCHIVO_MINIATURAS")
            .unwrap_or_else(|_| "datos/miniaturas".into())
            .into();
        let f3d = env::var("ARCHIVO_F3D").unwrap_or_else(|_| "f3d".into());
        let nucleos = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(2);
        let paralelo = env::var("ARCHIVO_PARALELO")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or((nucleos / 2).max(1));
        Ok(Self {
            direccion,
            base,
            estaticos,
            cookie_segura,
            dias_sesion,
            biblioteca,
            miniaturas,
            f3d,
            paralelo,
        })
    }

    /// Configuración mínima para pruebas (sin frontend, cookie sin Secure, caché en una carpeta temporal).
    pub fn de_prueba(biblioteca: Option<PathBuf>, estaticos: Option<PathBuf>) -> Self {
        let n = std::process::id();
        Self {
            direccion: "127.0.0.1:0".parse().expect("dirección fija"),
            base: ":memory:".into(),
            estaticos,
            cookie_segura: false,
            dias_sesion: 30,
            biblioteca,
            miniaturas: std::env::temp_dir().join(format!("pa-miniaturas-{n}")),
            f3d: "f3d".into(),
            paralelo: 2,
        }
    }

    pub fn opciones_miniaturas(&self) -> crate::miniaturas::Opciones {
        crate::miniaturas::Opciones {
            dir: self.miniaturas.clone(),
            f3d: self.f3d.clone(),
            paralelo: self.paralelo,
        }
    }
}
