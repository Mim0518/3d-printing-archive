-- Fase 6: impresiones, existencias y costos.
--
-- Una impresión es una vez que se imprimió una placa: con qué impresora, material y perfil, cómo salió,
-- cuánto material y tiempo gastó y cuánto costó. Las miniaturas que salen bien entran a las existencias.
-- La cantidad de una existencia siempre es la suma de sus movimientos (hay historial).

-- Ajustes de costos: luz, limpieza, fallas, mano de obra, pintura, empaque, margen y moneda.
CREATE TABLE ajustes (
    clave        TEXT    PRIMARY KEY,
    valor        TEXT    NOT NULL,              -- JSON
    actualizado  INTEGER NOT NULL
);

INSERT INTO ajustes (clave, valor, actualizado) VALUES ('moneda', '"MXN"', 0);

CREATE TABLE impresiones (
    id                    INTEGER PRIMARY KEY AUTOINCREMENT,
    placa_id              INTEGER REFERENCES placas (id) ON DELETE SET NULL,
    placa_nombre          TEXT    NOT NULL DEFAULT '',   -- por si la placa se borra
    impresora_id          INTEGER REFERENCES impresoras (id) ON DELETE SET NULL,
    material_id           INTEGER REFERENCES materiales (id) ON DELETE SET NULL,
    perfil_id             INTEGER REFERENCES perfiles (id) ON DELETE SET NULL,
    perfil_version        INTEGER,
    fecha                 INTEGER NOT NULL,
    resultado             TEXT    NOT NULL CHECK (resultado IN ('bien', 'parcial', 'fallida')),
    consumo               REAL,                          -- en `unidad`
    unidad                TEXT    NOT NULL DEFAULT 'ml' CHECK (unidad IN ('ml', 'g')),
    consumo_origen        TEXT    NOT NULL DEFAULT 'capturado' CHECK (consumo_origen IN ('laminado', 'capturado')),
    tiempo_s              INTEGER,
    -- lo que costó, calculado al registrarla con los precios y ajustes de ese momento
    costo_material        REAL,
    costo_luz             REAL,
    costo_limpieza        REAL,
    costo_fallas          REAL,
    costo_total           REAL    NOT NULL DEFAULT 0,
    piezas                INTEGER NOT NULL DEFAULT 0,    -- miniaturas completas que salieron bien
    material_descontado   REAL,                          -- lo que se restó de lo que queda del material
    notas                 TEXT    NOT NULL DEFAULT '',
    usuario               TEXT    NOT NULL DEFAULT '',
    creada                INTEGER NOT NULL
);

CREATE INDEX impresiones_placa ON impresiones (placa_id);
CREATE INDEX impresiones_fecha ON impresiones (fecha);

CREATE TABLE existencias (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    modelo_id    INTEGER NOT NULL REFERENCES modelos (id),
    acabado      TEXT    NOT NULL CHECK (acabado IN ('sin_pintar', 'imprimada', 'pintada')),
    ubicacion    TEXT    NOT NULL DEFAULT '',          -- por ejemplo «Caja 3»
    precio       REAL,                                 -- puesto a mano; si no, se usa el sugerido
    minimo       INTEGER,                              -- avisa cuando hay menos
    notas        TEXT    NOT NULL DEFAULT '',
    creada       INTEGER NOT NULL,
    actualizada  INTEGER NOT NULL,
    UNIQUE (modelo_id, acabado, ubicacion)
);

CREATE TABLE movimientos (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    existencia_id   INTEGER NOT NULL REFERENCES existencias (id) ON DELETE CASCADE,
    delta           INTEGER NOT NULL CHECK (delta <> 0),
    motivo          TEXT    NOT NULL CHECK (motivo IN ('impresion', 'venta', 'merma', 'ajuste', 'acabado', 'traslado')),
    costo_unitario  REAL,                              -- en las entradas: con esto se saca el costo promedio
    impresion_id    INTEGER REFERENCES impresiones (id) ON DELETE CASCADE,
    par_id          INTEGER,                           -- el otro lado de un cambio de acabado o de lugar
    notas           TEXT    NOT NULL DEFAULT '',
    usuario         TEXT    NOT NULL DEFAULT '',
    fecha           INTEGER NOT NULL
);

CREATE INDEX movimientos_existencia ON movimientos (existencia_id);
CREATE INDEX movimientos_impresion ON movimientos (impresion_id);
