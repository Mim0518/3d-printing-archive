-- Fase 4: placas y escenas.
--
-- Una placa reúne la escena (Lychee, Chitubox, Photon Workshop, proyecto 3MF…), sus laminados y sus
-- autoguardados. Se detectan solas a partir de los archivos; el usuario confirma de qué modelos son.

-- Lo que se pudo leer de cada escena o laminado. Se vuelve a leer solo si cambia la clave del archivo.
CREATE TABLE datos_placa (
    archivo_id         INTEGER PRIMARY KEY REFERENCES archivos (id) ON DELETE CASCADE,
    clave              TEXT    NOT NULL,              -- inode-tamaño-fecha cuando se leyó (hex)
    formato            TEXT    NOT NULL DEFAULT '',   -- lychee | chitubox | photon | photon_workshop | ...
    es_placa           INTEGER NOT NULL DEFAULT 1,    -- 0: un 3MF que solo es una malla
    vista_previa       INTEGER NOT NULL DEFAULT 0,    -- 1 si trae una imagen adentro
    maquina            TEXT    NOT NULL DEFAULT '',
    res_x              INTEGER,
    res_y              INTEGER,
    capa_mm            REAL,
    exposicion_s       REAL,
    exposicion_base_s  REAL,
    capas_base         INTEGER,
    capas              INTEGER,
    alto_mm            REAL,
    tiempo_s           INTEGER,
    volumen_ml         REAL,
    peso_g             REAL,
    n_placas           INTEGER,                       -- proyectos 3MF con varias placas
    error              TEXT    NOT NULL DEFAULT ''
);

-- AUTOINCREMENT: el id de una placa borrada no se reusa (sus enlaces no deben llevar a otra placa)
CREATE TABLE placas (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    nombre              TEXT    NOT NULL,
    biblioteca_id       INTEGER REFERENCES bibliotecas (id) ON DELETE SET NULL,
    carpeta             TEXT    NOT NULL DEFAULT '',
    clave               TEXT,                         -- carpeta + nombre en minúsculas (solo las detectadas)
    origen              TEXT    NOT NULL CHECK (origen IN ('detectada', 'manual')),
    estado              TEXT    NOT NULL DEFAULT 'sin_probar' CHECK (estado IN ('sin_probar', 'probada', 'fallida')),
    favorita            INTEGER NOT NULL DEFAULT 0,
    sin_modelo          INTEGER NOT NULL DEFAULT 0,   -- 1: no es de ningún modelo (calibración, pruebas)
    notas               TEXT    NOT NULL DEFAULT '',
    portada_archivo_id  INTEGER REFERENCES archivos (id) ON DELETE SET NULL,  -- elegida a mano
    gemela_id           INTEGER REFERENCES archivos (id) ON DELETE SET NULL,  -- malla con el mismo nombre
    creada              INTEGER NOT NULL,
    actualizada         INTEGER NOT NULL
);

CREATE INDEX placas_clave ON placas (clave);

-- Cada archivo es de una sola placa. `auto` = 0 si el usuario lo puso ahí a mano (la detección no lo mueve).
CREATE TABLE placa_archivos (
    archivo_id  INTEGER PRIMARY KEY REFERENCES archivos (id) ON DELETE CASCADE,
    placa_id    INTEGER NOT NULL REFERENCES placas (id) ON DELETE CASCADE,
    rol         TEXT    NOT NULL CHECK (rol IN ('escena', 'laminado', 'autoguardado')),
    auto        INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX placa_archivos_placa ON placa_archivos (placa_id);

-- De qué modelos es cada placa. La app sugiere (por la carpeta o por el nombre) y el usuario confirma o
-- rechaza; lo rechazado no se vuelve a sugerir. `cantidad`: miniaturas completas del modelo en la placa
-- (NULL si la placa solo trae partes).
CREATE TABLE placa_modelos (
    placa_id     INTEGER NOT NULL REFERENCES placas (id) ON DELETE CASCADE,
    modelo_id    INTEGER NOT NULL REFERENCES modelos (id) ON DELETE CASCADE,
    estado       TEXT    NOT NULL CHECK (estado IN ('sugerido', 'confirmado', 'rechazado')),
    origen       TEXT    NOT NULL CHECK (origen IN ('carpeta', 'nombre', 'manual')),
    cantidad     INTEGER,
    actualizado  INTEGER NOT NULL,
    PRIMARY KEY (placa_id, modelo_id)
);

CREATE INDEX placa_modelos_modelo ON placa_modelos (modelo_id);
