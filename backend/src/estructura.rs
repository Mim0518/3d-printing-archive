//! Estructura de carpetas de la colección (la misma regla que la columna J de la antigua hoja):
//!
//! ```text
//! Wargames/<Juego>/<Facción>/<Modelo> (<Estudio>)/[<Subcarpeta>]
//! DND/<Tipo>/<Modelo> (<Estudio>)/[<Subcarpeta>]
//! ```

/// Clave de una fila del catálogo que no es una carpeta sino los archivos sueltos de una carpeta.
pub const SUELTOS: &str = "/(sueltos)";

/// Ruta donde debe estar un modelo según sus campos.
pub fn ruta_nueva(
    juego: &str,
    faccion: &str,
    nombre: &str,
    estudio: &str,
    subcarpeta: &str,
) -> String {
    let mut p = if juego == "D&D" {
        "DND".to_string()
    } else {
        format!("Wargames/{juego}")
    };
    if !faccion.is_empty() {
        p.push('/');
        p.push_str(faccion);
    }
    if !nombre.is_empty() {
        p.push('/');
        p.push_str(nombre);
        if !estudio.is_empty() {
            p.push_str(" (");
            p.push_str(estudio);
            p.push(')');
        }
    }
    if !subcarpeta.is_empty() {
        p.push('/');
        p.push_str(subcarpeta);
    }
    p
}

/// Carpeta padre de una ruta relativa ('' si está en la raíz).
pub fn padre(ruta: &str) -> &str {
    ruta.rfind('/').map(|i| &ruta[..i]).unwrap_or("")
}

/// Extensión en minúsculas, sin punto ('' si no tiene).
pub fn extension(nombre: &str) -> String {
    match nombre.rfind('.') {
        Some(i) if i > 0 && i + 1 < nombre.len() => nombre[i + 1..].to_lowercase(),
        _ => String::new(),
    }
}

/// Tipo de archivo según la extensión.
pub fn tipo_de(ext: &str) -> &'static str {
    match ext {
        "stl" | "obj" | "3mf" | "step" | "stp" | "blend" | "ply" | "ztl" | "fbx" => "malla",
        "lys" | "chitubox" | "lyt" | "scene" | "pwscene" => "escena",
        "pwsr" | "lyr" | "orca_printer" | "orca_filament" => "perfil",
        "ctb" | "photon" | "pwmx" | "pws" | "pwma" | "pm4n" | "pm3" | "pm3m" | "pm3r" | "pwmb"
        | "pwms" | "pwmo" | "pmsq" | "goo" | "prz" | "cbddlp" | "jxs" | "gcode" | "bgcode" => {
            "laminado"
        }
        "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp" | "tif" | "tiff" | "heic" | "avif" => {
            "imagen"
        }
        "zip" | "rar" | "7z" | "gz" | "tar" | "tgz" | "xz" => "comprimido",
        e if !e.is_empty() && e.chars().all(|c| c.is_ascii_digit()) => "comprimido", // volúmenes .001, .002…
        "pdf" | "txt" | "md" | "doc" | "docx" | "xls" | "xlsx" | "csv" | "html" | "htm" | "url"
        | "json" | "rtf" | "odt" => "documento",
        "mp4" | "mov" | "mkv" | "webm" | "avi" => "video",
        _ => "otro",
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn ruta_como_la_hoja() {
        assert_eq!(
            ruta_nueva(
                "Warhammer 40K",
                "Necrons",
                "Canoptek Wraiths",
                "Station Forge",
                ""
            ),
            "Wargames/Warhammer 40K/Necrons/Canoptek Wraiths (Station Forge)"
        );
        assert_eq!(
            ruta_nueva("D&D", "Beast", "Boar", "", "Supported"),
            "DND/Beast/Boar/Supported"
        );
        assert_eq!(
            ruta_nueva("Game Aids", "", "Bases", "", ""),
            "Wargames/Game Aids/Bases"
        );
    }

    #[test]
    fn tipos_y_extensiones() {
        assert_eq!(extension("Pieza.STL"), "stl");
        assert_eq!(extension(".DS_Store"), "");
        assert_eq!(extension("sin_ext"), "");
        assert_eq!(tipo_de("stl"), "malla");
        assert_eq!(tipo_de("lys"), "escena");
        assert_eq!(tipo_de("pm4n"), "laminado");
        assert_eq!(tipo_de("pwsr"), "perfil");
        assert_eq!(tipo_de("002"), "comprimido");
        assert_eq!(tipo_de("xyz"), "otro");
        assert_eq!(padre("a/b/c.stl"), "a/b");
        assert_eq!(padre("c.stl"), "");
    }
}
