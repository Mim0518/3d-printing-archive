//! Fase 6: costos de una impresión y precio sugerido.
//!
//! Costo de una impresión = material (consumo × precio por ml o por g) + luz (potencia × tiempo × precio del
//! kWh) + limpieza (fijo por impresión) + fallas (un porcentaje de lo anterior). Se reparte por partes iguales
//! entre las miniaturas que salieron bien.
//!
//! Precio sugerido de una miniatura = (costo + lo de su acabado) × margen + empaque, redondeado hacia arriba.
//! Lo de su acabado: imprimación; si está pintada, además la pintura y las horas de mano de obra.

use std::collections::HashMap;

use serde::Serialize;
use serde_json::Value;
use sqlx::SqlitePool;

/// Un ajuste de costos que se captura (los que faltan no se cuentan, y se avisa).
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Ajuste {
    pub clave: &'static str,
    pub etiqueta: &'static str,
    pub ayuda: &'static str,
    pub unidad: &'static str,
    pub min: f64,
    pub max: f64,
}

const fn ajuste(
    clave: &'static str,
    etiqueta: &'static str,
    ayuda: &'static str,
    unidad: &'static str,
    min: f64,
    max: f64,
) -> Ajuste {
    Ajuste {
        clave,
        etiqueta,
        ayuda,
        unidad,
        min,
        max,
    }
}

pub const AJUSTES: &[Ajuste] = &[
    ajuste(
        "precio_kwh",
        "Precio de la luz",
        "Lo que pagas por kWh (mira tu recibo de CFE).",
        "$/kWh",
        0.0,
        100.0,
    ),
    ajuste(
        "limpieza_resina",
        "Limpieza por impresión de resina",
        "Alcohol, guantes y toallas que se van en cada placa.",
        "$",
        0.0,
        10_000.0,
    ),
    ajuste(
        "limpieza_fdm",
        "Limpieza por impresión FDM",
        "Pegamento, cinta… por placa.",
        "$",
        0.0,
        10_000.0,
    ),
    ajuste(
        "fallas_pct",
        "Fallas",
        "Se suma este porcentaje del costo, por las impresiones que salen mal.",
        "%",
        0.0,
        100.0,
    ),
    ajuste(
        "mano_obra_hora",
        "Mano de obra",
        "Cuánto cobras por hora de pintura.",
        "$/h",
        0.0,
        100_000.0,
    ),
    ajuste(
        "imprimacion_pieza",
        "Imprimación por miniatura",
        "Se suma a las imprimadas y a las pintadas.",
        "$",
        0.0,
        10_000.0,
    ),
    ajuste(
        "pintura_material_pieza",
        "Pintura por miniatura",
        "Material de pintura de una miniatura.",
        "$",
        0.0,
        10_000.0,
    ),
    ajuste(
        "pintura_horas_pieza",
        "Horas de pintura por miniatura",
        "Tiempo que te lleva pintar una miniatura normal.",
        "h",
        0.0,
        200.0,
    ),
    ajuste(
        "empaque_pieza",
        "Empaque por miniatura",
        "Caja, bolsa, espuma…",
        "$",
        0.0,
        10_000.0,
    ),
    ajuste(
        "margen",
        "Margen",
        "El costo se multiplica por esto: 3 quiere decir que cobras el triple de lo que te cuesta.",
        "×",
        1.0,
        50.0,
    ),
];

pub const ACABADOS: [&str; 3] = ["sin_pintar", "imprimada", "pintada"];

#[derive(Debug, Clone, Default, Serialize)]
pub struct Ajustes {
    pub valores: HashMap<String, f64>,
    pub moneda: String,
}

impl Ajustes {
    pub async fn cargar(pool: &SqlitePool) -> Result<Self, sqlx::Error> {
        let filas: Vec<(String, String)> = sqlx::query_as("SELECT clave, valor FROM ajustes")
            .fetch_all(pool)
            .await?;
        let mut a = Ajustes {
            moneda: "MXN".into(),
            ..Default::default()
        };
        for (k, v) in filas {
            match serde_json::from_str::<Value>(&v) {
                Ok(Value::String(s)) if k == "moneda" => a.moneda = s,
                Ok(Value::Number(n)) => {
                    if let Some(x) = n.as_f64() {
                        a.valores.insert(k, x);
                    }
                }
                _ => {}
            }
        }
        Ok(a)
    }

    pub fn get(&self, k: &str) -> Option<f64> {
        self.valores.get(k).copied()
    }

    /// Los ajustes que no se han capturado (por su etiqueta).
    pub fn faltan(&self) -> Vec<&'static str> {
        AJUSTES
            .iter()
            .filter(|a| !self.valores.contains_key(a.clave))
            .map(|a| a.etiqueta)
            .collect()
    }
}

fn centavos(x: f64) -> f64 {
    // + 0.0: sin «-0» (la suma de nada en f64 da -0.0)
    (x * 100.0).round() / 100.0 + 0.0
}

/// Lo que se necesita del material para costear.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MaterialCosto {
    pub id: i64,
    pub nombre: String,
    pub tipo: String,
    pub precio: Option<f64>,
    pub presentacion: Option<f64>,
    pub densidad: Option<f64>,
    pub queda: Option<f64>,
}

impl MaterialCosto {
    pub async fn cargar(pool: &SqlitePool, id: i64) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as(
            "SELECT id, trim(marca || ' ' || nombre) AS nombre, tipo, precio, presentacion, densidad, queda
             FROM materiales WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    /// ml para la resina, g para el filamento.
    pub fn unidad(&self) -> &'static str {
        if self.tipo == "resina" { "ml" } else { "g" }
    }

    /// El consumo pasado a la unidad del material (con la densidad si hace falta).
    pub fn en_su_unidad(&self, consumo: f64, unidad: &str) -> Option<f64> {
        if unidad == self.unidad() {
            return Some(consumo);
        }
        let d = self.densidad.filter(|d| *d > 0.0)?;
        // g → ml: ÷ densidad; ml → g: × densidad (1 ml = 1 cm³)
        Some(if unidad == "g" {
            consumo / d
        } else {
            consumo * d
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct Entrada<'a> {
    /// resina | fdm (para la limpieza)
    pub tecnologia: Option<&'a str>,
    pub potencia_w: Option<f64>,
    pub material: Option<&'a MaterialCosto>,
    pub consumo: Option<f64>,
    /// ml | g
    pub unidad: &'a str,
    pub tiempo_s: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Desglose {
    pub material: Option<f64>,
    pub luz: Option<f64>,
    pub limpieza: Option<f64>,
    pub fallas: Option<f64>,
    pub total: f64,
    /// El consumo en la unidad del material (lo que se descuenta de lo que queda).
    pub consumo_material: Option<f64>,
    /// Lo que no se pudo contar porque falta un dato.
    pub faltan: Vec<String>,
}

pub fn calcular(a: &Ajustes, e: &Entrada) -> Desglose {
    let mut faltan = Vec::new();
    let consumo_material = match (e.material, e.consumo) {
        (None, _) => {
            faltan.push("el material".to_string());
            None
        }
        (Some(_), None) => {
            faltan.push("el consumo de material".to_string());
            None
        }
        (Some(m), Some(c)) => {
            let x = m.en_su_unidad(c, e.unidad);
            if x.is_none() {
                faltan.push(format!("la densidad de {}", m.nombre));
            }
            x
        }
    };
    let material = match (e.material, consumo_material) {
        (Some(m), Some(c)) => match (m.precio, m.presentacion) {
            (Some(p), Some(pr)) if pr > 0.0 => Some(centavos(c * p / pr)),
            _ => {
                faltan.push(format!("el precio de {}", m.nombre));
                None
            }
        },
        _ => None,
    };
    let luz = match (e.potencia_w, e.tiempo_s, a.get("precio_kwh")) {
        (Some(w), Some(t), Some(k)) => Some(centavos(w * t as f64 / 3_600_000.0 * k)),
        (w, t, k) => {
            if w.is_none() {
                faltan.push("la potencia de la impresora".into());
            }
            if t.is_none() {
                faltan.push("el tiempo de impresión".into());
            }
            if k.is_none() {
                faltan.push("el precio de la luz".into());
            }
            None
        }
    };
    let clave_limpieza = if e.tecnologia == Some("fdm") {
        "limpieza_fdm"
    } else {
        "limpieza_resina"
    };
    let limpieza = a.get(clave_limpieza);
    if limpieza.is_none() {
        faltan.push("el costo de limpieza".into());
    }
    let base = material.unwrap_or(0.0) + luz.unwrap_or(0.0) + limpieza.unwrap_or(0.0);
    let fallas = a.get("fallas_pct").map(|f| centavos(base * f / 100.0));
    Desglose {
        material,
        luz,
        limpieza,
        fallas,
        total: centavos(base + fallas.unwrap_or(0.0)),
        consumo_material: consumo_material.map(|c| (c * 1000.0).round() / 1000.0),
        faltan,
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Precio {
    pub costo: Option<f64>,
    /// Imprimación, pintura y mano de obra según el acabado.
    pub acabado: f64,
    pub empaque: f64,
    pub margen: Option<f64>,
    pub sugerido: Option<f64>,
    pub faltan: Vec<String>,
}

/// Precio sugerido de una miniatura con ese costo y acabado.
pub fn precio_sugerido(a: &Ajustes, costo: Option<f64>, acabado: &str) -> Precio {
    let mut faltan = Vec::new();
    let mut extra = 0.0;
    let mut sumar = |clave: &str, nombre: &str, faltan: &mut Vec<String>| match a.get(clave) {
        Some(x) => extra += x,
        None => faltan.push(nombre.to_string()),
    };
    if acabado != "sin_pintar" {
        sumar("imprimacion_pieza", "la imprimación", &mut faltan);
    }
    if acabado == "pintada" {
        sumar("pintura_material_pieza", "la pintura", &mut faltan);
        match (a.get("pintura_horas_pieza"), a.get("mano_obra_hora")) {
            (Some(h), Some(m)) => extra += h * m,
            (h, m) => {
                if h.is_none() {
                    faltan.push("las horas de pintura".into());
                }
                if m.is_none() {
                    faltan.push("la mano de obra".into());
                }
            }
        }
    }
    let empaque = a.get("empaque_pieza").unwrap_or_else(|| {
        faltan.push("el empaque".into());
        0.0
    });
    let margen = a.get("margen");
    if margen.is_none() {
        faltan.push("el margen".into());
    }
    if costo.is_none() {
        faltan.push("el costo de impresión".into());
    }
    let sugerido = costo
        .zip(margen)
        .map(|(c, m)| ((c + extra) * m + empaque).ceil());
    Precio {
        costo: costo.map(centavos),
        acabado: centavos(extra),
        empaque,
        margen,
        sugerido,
        faltan,
    }
}

/// Costo promedio de las miniaturas de un modelo (sus entradas por impresión o capturadas a mano).
pub async fn costo_de_modelo(pool: &SqlitePool, modelo: i64) -> Result<Option<f64>, sqlx::Error> {
    let c: Option<f64> = sqlx::query_scalar(
        "SELECT sum(mv.delta * mv.costo_unitario) / sum(mv.delta) FROM movimientos mv
         JOIN existencias e ON e.id = mv.existencia_id
         WHERE e.modelo_id = ? AND mv.delta > 0 AND mv.motivo IN ('impresion', 'ajuste')
           AND mv.costo_unitario IS NOT NULL",
    )
    .bind(modelo)
    .fetch_one(pool)
    .await?;
    Ok(c.map(centavos))
}

/// Precio de venta de una miniatura de ese modelo y acabado: el puesto a mano en una de sus existencias o, si
/// no hay, el sugerido con su costo promedio. Devuelve también el costo.
pub async fn precio_de(
    pool: &SqlitePool,
    a: &Ajustes,
    modelo: i64,
    acabado: &str,
) -> Result<(Option<f64>, Option<f64>), sqlx::Error> {
    let manual: Option<f64> = sqlx::query_scalar(
        "SELECT precio FROM existencias WHERE modelo_id = ? AND acabado = ? AND precio IS NOT NULL
         ORDER BY actualizada DESC LIMIT 1",
    )
    .bind(modelo)
    .bind(acabado)
    .fetch_optional(pool)
    .await?
    .flatten();
    let costo = costo_de_modelo(pool, modelo).await?;
    Ok((
        manual.or(precio_sugerido(a, costo, acabado).sugerido),
        costo,
    ))
}

/// Cantidad de una existencia `e`: la suma de sus movimientos.
pub const SQL_CANTIDAD: &str =
    "(SELECT coalesce(sum(mv.delta), 0) FROM movimientos mv WHERE mv.existencia_id = e.id)";

/// Costo promedio de una existencia `e`: el de sus entradas, pesado por cantidad.
pub const SQL_COSTO: &str =
    "(SELECT sum(mv.delta * mv.costo_unitario) / sum(mv.delta) FROM movimientos mv
     WHERE mv.existencia_id = e.id AND mv.delta > 0 AND mv.costo_unitario IS NOT NULL)";

#[cfg(test)]
mod pruebas {
    use super::*;

    fn ajustes(pares: &[(&str, f64)]) -> Ajustes {
        Ajustes {
            valores: pares.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            moneda: "MXN".into(),
        }
    }

    fn resina() -> MaterialCosto {
        MaterialCosto {
            id: 1,
            nombre: "Sunlu".into(),
            tipo: "resina".into(),
            precio: Some(437.0),
            presentacion: Some(1000.0),
            densidad: Some(1.1),
            queda: None,
        }
    }

    #[test]
    fn costo_completo() {
        let a = ajustes(&[
            ("precio_kwh", 2.0),
            ("limpieza_resina", 10.0),
            ("fallas_pct", 10.0),
        ]);
        let m = resina();
        let d = calcular(
            &a,
            &Entrada {
                tecnologia: Some("resina"),
                potencia_w: Some(60.0),
                material: Some(&m),
                consumo: Some(40.0),
                unidad: "ml",
                tiempo_s: Some(3 * 3600),
            },
        );
        // 40 ml × 0.437 = 17.48; luz 60 W × 3 h = 0.18 kWh × 2 = 0.36; limpieza 10; fallas 10% de 27.84
        assert_eq!(d.material, Some(17.48));
        assert_eq!(d.luz, Some(0.36));
        assert_eq!(d.fallas, Some(2.78));
        assert_eq!(d.total, 30.62);
        assert!(d.faltan.is_empty(), "{:?}", d.faltan);
    }

    #[test]
    fn convierte_gramos_y_avisa_lo_que_falta() {
        let a = Ajustes::default();
        let m = resina();
        let d = calcular(
            &a,
            &Entrada {
                material: Some(&m),
                consumo: Some(11.0),
                unidad: "g",
                ..Default::default()
            },
        );
        assert_eq!(d.consumo_material, Some(10.0));
        assert_eq!(d.material, Some(4.37));
        assert_eq!(d.total, 4.37);
        assert!(d.faltan.iter().any(|f| f == "el precio de la luz"));
        assert!(d.faltan.iter().any(|f| f == "el costo de limpieza"));
        let sin = MaterialCosto {
            densidad: None,
            ..resina()
        };
        let d = calcular(
            &a,
            &Entrada {
                material: Some(&sin),
                consumo: Some(11.0),
                unidad: "g",
                ..Default::default()
            },
        );
        assert_eq!(d.material, None);
        assert!(d.faltan.iter().any(|f| f.starts_with("la densidad")));
    }

    #[test]
    fn precio_por_acabado() {
        let a = ajustes(&[
            ("margen", 3.0),
            ("empaque_pieza", 5.0),
            ("imprimacion_pieza", 2.0),
            ("pintura_material_pieza", 8.0),
            ("pintura_horas_pieza", 1.5),
            ("mano_obra_hora", 100.0),
        ]);
        assert_eq!(
            precio_sugerido(&a, Some(10.2), "sin_pintar").sugerido,
            Some(36.0)
        );
        assert_eq!(
            precio_sugerido(&a, Some(10.0), "imprimada").sugerido,
            Some(41.0)
        );
        // 10 + 2 + 8 + 150 = 170 × 3 + 5
        let p = precio_sugerido(&a, Some(10.0), "pintada");
        assert_eq!(p.sugerido, Some(515.0));
        assert_eq!(p.acabado, 160.0);
        assert!(p.faltan.is_empty());
        assert_eq!(precio_sugerido(&a, None, "pintada").sugerido, None);
        assert_eq!(
            precio_sugerido(&Ajustes::default(), Some(1.0), "sin_pintar").sugerido,
            None
        );
    }
}
