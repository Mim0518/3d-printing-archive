-- Fase 2: edición del catálogo (historial, ruta nueva, avisos) y mudanzas de carpetas con registro para deshacer.

ALTER TABLE modelos ADD COLUMN ruta_nueva TEXT NOT NULL DEFAULT '';
ALTER TABLE modelos ADD COLUMN pendiente INTEGER NOT NULL DEFAULT 0;   -- 1 si no está en su ruta nueva
ALTER TABLE modelos ADD COLUMN n_avisos INTEGER NOT NULL DEFAULT 0;

CREATE TABLE historial (
    id         INTEGER PRIMARY KEY,
    modelo_id  INTEGER NOT NULL REFERENCES modelos (id) ON DELETE CASCADE,
    campo      TEXT    NOT NULL,
    antes      TEXT    NOT NULL,
    despues    TEXT    NOT NULL,
    usuario    TEXT    NOT NULL,
    fecha      INTEGER NOT NULL
);

CREATE INDEX historial_modelo ON historial (modelo_id, fecha);

-- Una mudanza mueve las carpetas de uno o varios modelos a su ruta nueva. Primero se arma el plan (archivo por
-- archivo) y después se ejecuta; se puede deshacer la última que se ejecutó.
CREATE TABLE mudanzas (
    id         INTEGER PRIMARY KEY,
    estado     TEXT    NOT NULL CHECK (estado IN ('plan', 'en_curso', 'ejecutada', 'con_errores', 'deshecha', 'descartada')),
    usuario    TEXT    NOT NULL,
    creada     INTEGER NOT NULL,
    ejecutada  INTEGER,
    deshecha   INTEGER,
    resumen    TEXT    NOT NULL DEFAULT ''   -- JSON del plan
);

CREATE TABLE mudanza_modelos (
    mudanza_id       INTEGER NOT NULL REFERENCES mudanzas (id) ON DELETE CASCADE,
    modelo_id        INTEGER NOT NULL REFERENCES modelos (id) ON DELETE CASCADE,
    carpeta_antes    TEXT    NOT NULL,
    carpeta_despues  TEXT    NOT NULL,
    PRIMARY KEY (mudanza_id, modelo_id)
);

CREATE TABLE mudanza_archivos (
    id          INTEGER PRIMARY KEY,
    mudanza_id  INTEGER NOT NULL REFERENCES mudanzas (id) ON DELETE CASCADE,
    archivo_id  INTEGER REFERENCES archivos (id) ON DELETE SET NULL,
    modelo_id   INTEGER NOT NULL,
    origen      TEXT    NOT NULL,
    destino     TEXT    NOT NULL,
    bytes       INTEGER NOT NULL,
    inode       INTEGER NOT NULL,
    estado      TEXT    NOT NULL DEFAULT 'pendiente' CHECK (estado IN ('pendiente', 'movido', 'error', 'deshecho')),
    mensaje     TEXT    NOT NULL DEFAULT ''
);

CREATE INDEX mudanza_archivos_mudanza ON mudanza_archivos (mudanza_id);
