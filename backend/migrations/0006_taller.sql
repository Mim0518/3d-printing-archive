-- Fase 5: impresoras, materiales y perfiles de impresión.
--
-- Un perfil son los valores con que una impresora imprime un material (exposición, temperaturas, altura de
-- capa…), con sus archivos del laminador y su historial de versiones. Se capturan a mano o se importan de
-- los archivos de perfiles de la colección (Photon Workshop, Orca, Lychee).

-- Los perfiles de los laminadores dejan de contarse como escenas u otros archivos.
UPDATE archivos SET tipo = 'perfil' WHERE ext IN ('pwsr', 'lyr', 'orca_printer', 'orca_filament');

CREATE TABLE impresoras (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    nombre        TEXT    NOT NULL,
    tecnologia    TEXT    NOT NULL CHECK (tecnologia IN ('resina', 'fdm')),
    marca         TEXT    NOT NULL DEFAULT '',
    modelo        TEXT    NOT NULL DEFAULT '',
    volumen_x_mm  REAL,
    volumen_y_mm  REAL,
    volumen_z_mm  REAL,
    boquilla_mm   REAL,                           -- FDM
    res_x         INTEGER,                        -- resina: pixeles de la pantalla; con esto se reconocen
    res_y         INTEGER,                        --   sus laminados
    pixel_um      REAL,
    potencia_w    REAL,                           -- para el costo de la luz (fase 6)
    carpeta       TEXT    NOT NULL DEFAULT '',    -- carpeta de la biblioteca con sus manuales y mejoras
    perfil_id     INTEGER REFERENCES perfiles (id) ON DELETE SET NULL,  -- el que usa normalmente
    en_uso        INTEGER NOT NULL DEFAULT 1,
    notas         TEXT    NOT NULL DEFAULT '',
    clave_origen  TEXT    UNIQUE,                 -- de qué se importó, para no importarla dos veces
    creada        INTEGER NOT NULL,
    actualizada   INTEGER NOT NULL
);

CREATE TABLE materiales (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    tipo          TEXT    NOT NULL CHECK (tipo IN ('resina', 'filamento')),
    marca         TEXT    NOT NULL DEFAULT '',
    nombre        TEXT    NOT NULL,
    subtipo       TEXT    NOT NULL DEFAULT '',    -- estándar, tipo ABS, lavable con agua, PLA, PETG…
    color         TEXT    NOT NULL DEFAULT '',
    color_hex     TEXT    NOT NULL DEFAULT '',
    presentacion  REAL,                           -- ml (resina) o g (filamento)
    precio        REAL,                           -- por presentación, en MXN
    densidad      REAL,                           -- g/cm³
    diametro_mm   REAL,                           -- filamento
    queda         REAL,                           -- opcional, en ml o g
    en_uso        INTEGER NOT NULL DEFAULT 1,
    notas         TEXT    NOT NULL DEFAULT '',
    clave_origen  TEXT    UNIQUE,
    creado        INTEGER NOT NULL,
    actualizado   INTEGER NOT NULL
);

CREATE TABLE perfiles (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    nombre        TEXT    NOT NULL,
    impresora_id  INTEGER NOT NULL REFERENCES impresoras (id),
    material_id   INTEGER REFERENCES materiales (id) ON DELETE SET NULL,
    capa_mm       REAL,
    parametros    TEXT    NOT NULL DEFAULT '{}',  -- JSON con los campos de su tecnología
    version       INTEGER NOT NULL DEFAULT 1,
    en_uso        INTEGER NOT NULL DEFAULT 1,
    notas         TEXT    NOT NULL DEFAULT '',
    origen        TEXT    NOT NULL DEFAULT 'manual' CHECK (origen IN ('manual', 'importado')),
    clave_origen  TEXT    UNIQUE,
    creado        INTEGER NOT NULL,
    actualizado   INTEGER NOT NULL
);

CREATE INDEX perfiles_impresora ON perfiles (impresora_id);

-- Cómo era cada perfil antes de cambiar sus valores. La versión vigente está en `perfiles`.
CREATE TABLE perfil_versiones (
    perfil_id    INTEGER NOT NULL REFERENCES perfiles (id) ON DELETE CASCADE,
    version      INTEGER NOT NULL,
    material_id  INTEGER,
    capa_mm      REAL,
    parametros   TEXT    NOT NULL,
    desde        INTEGER NOT NULL,
    hasta        INTEGER NOT NULL,
    usuario      TEXT    NOT NULL DEFAULT '',
    PRIMARY KEY (perfil_id, version)
);

-- Archivos del laminador de cada perfil (.pwsr, .lyr, .orca_printer…), que viven en la biblioteca.
CREATE TABLE perfil_archivos (
    perfil_id   INTEGER NOT NULL REFERENCES perfiles (id) ON DELETE CASCADE,
    archivo_id  INTEGER NOT NULL REFERENCES archivos (id) ON DELETE CASCADE,
    PRIMARY KEY (perfil_id, archivo_id)
);

CREATE INDEX perfil_archivos_archivo ON perfil_archivos (archivo_id);

-- Con qué se imprime cada placa (elegido a mano; la impresora también se reconoce por sus laminados).
ALTER TABLE placas ADD COLUMN impresora_id INTEGER REFERENCES impresoras (id) ON DELETE SET NULL;
ALTER TABLE placas ADD COLUMN material_id INTEGER REFERENCES materiales (id) ON DELETE SET NULL;
ALTER TABLE placas ADD COLUMN perfil_id INTEGER REFERENCES perfiles (id) ON DELETE SET NULL;
