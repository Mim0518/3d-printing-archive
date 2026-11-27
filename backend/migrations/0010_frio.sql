-- Fase 8: almacenamiento frío. Mandar un modelo a frío (Drive con rclone o un disco externo), verificarlo por
-- MD5, quitar la copia local a la papelera de la app y traerlo de regreso.
--
-- Decisiones del usuario: lo nuevo va como carpeta tal cual (cada archivo con su ruta), en DriveMemo:3DP-archivo.

ALTER TABLE ubicaciones_frio ADD COLUMN ruta_base TEXT NOT NULL DEFAULT '';      -- disco externo: dónde se monta
ALTER TABLE ubicaciones_frio ADD COLUMN marca TEXT NOT NULL DEFAULT '';          -- disco externo: su archivo marca
ALTER TABLE ubicaciones_frio ADD COLUMN escribible INTEGER NOT NULL DEFAULT 0;   -- se puede mandar ahí
ALTER TABLE ubicaciones_frio ADD COLUMN predeterminada INTEGER NOT NULL DEFAULT 0;

UPDATE ubicaciones_frio SET escribible = 1, predeterminada = 1 WHERE remoto = 'DriveMemo:3DP-archivo';

ALTER TABLE copias_frio ADD COLUMN archivo_id INTEGER REFERENCES archivos (id) ON DELETE SET NULL;
ALTER TABLE copias_frio ADD COLUMN verificado INTEGER;                -- cuándo se comprobó su MD5 en el destino
ALTER TABLE copias_frio ADD COLUMN estado TEXT NOT NULL DEFAULT 'presente' CHECK (estado IN ('presente', 'falta'));
ALTER TABLE copias_frio ADD COLUMN visto_en INTEGER;                  -- último listado que la vio
ALTER TABLE copias_frio ADD COLUMN origen TEXT NOT NULL DEFAULT 'importada' CHECK (origen IN ('importada', 'app', 'listado'));

CREATE INDEX copias_frio_archivo ON copias_frio (archivo_id);

-- MD5 de los archivos del disco, válido mientras no cambie su clave (inode-tamaño-fecha).
CREATE TABLE hashes (
    archivo_id  INTEGER PRIMARY KEY REFERENCES archivos (id) ON DELETE CASCADE,
    clave       TEXT    NOT NULL,
    md5         TEXT    NOT NULL,
    calculado   INTEGER NOT NULL
);

-- Cada vez que se manda o se trae algo.
CREATE TABLE transferencias (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    modelo_id     INTEGER REFERENCES modelos (id) ON DELETE SET NULL,
    ubicacion_id  INTEGER NOT NULL REFERENCES ubicaciones_frio (id),
    direccion     TEXT    NOT NULL CHECK (direccion IN ('subir', 'traer')),
    estado        TEXT    NOT NULL DEFAULT 'en_curso'
                  CHECK (estado IN ('en_curso', 'terminada', 'con_errores', 'fallo')),
    quitar_local  INTEGER NOT NULL DEFAULT 0,
    archivos      INTEGER NOT NULL DEFAULT 0,
    bytes         INTEGER NOT NULL DEFAULT 0,
    verificados   INTEGER NOT NULL DEFAULT 0,
    lote_id       INTEGER,                         -- el lote de la papelera, si se quitó la copia local
    trabajo_id    INTEGER REFERENCES trabajos (id) ON DELETE SET NULL,
    resultado     TEXT    NOT NULL DEFAULT '',     -- JSON: errores, conflictos, lo que se omitió
    usuario       TEXT    NOT NULL DEFAULT '',
    creada        INTEGER NOT NULL,
    terminada     INTEGER
);

CREATE INDEX transferencias_modelo ON transferencias (modelo_id);

-- La papelera de la app: lo que se quita del disco se mueve (con rename, en el mismo disco) a
-- `<biblioteca>/.archivo-papelera/<lote>/<ruta>`. Se puede restaurar; vaciarla borra para siempre y pide
-- confirmación escrita.
CREATE TABLE papelera_lotes (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    biblioteca_id INTEGER NOT NULL REFERENCES bibliotecas (id),
    motivo        TEXT    NOT NULL,                -- «mandado a frío: …»
    modelo_id     INTEGER REFERENCES modelos (id) ON DELETE SET NULL,
    ubicacion_id  INTEGER REFERENCES ubicaciones_frio (id) ON DELETE SET NULL,  -- dónde quedó la copia
    estado        TEXT    NOT NULL DEFAULT 'en_papelera' CHECK (estado IN ('en_papelera', 'restaurado', 'vaciado')),
    archivos      INTEGER NOT NULL DEFAULT 0,
    bytes         INTEGER NOT NULL DEFAULT 0,
    usuario       TEXT    NOT NULL DEFAULT '',
    creado        INTEGER NOT NULL,
    cerrado       INTEGER                          -- cuándo se restauró o se vació
);

CREATE TABLE papelera_archivos (
    lote_id      INTEGER NOT NULL REFERENCES papelera_lotes (id) ON DELETE CASCADE,
    archivo_id   INTEGER REFERENCES archivos (id) ON DELETE SET NULL,
    ruta         TEXT    NOT NULL,                 -- de dónde salió (relativa a la biblioteca)
    bytes        INTEGER NOT NULL,
    md5          TEXT    NOT NULL DEFAULT '',
    nlink        INTEGER NOT NULL DEFAULT 1,       -- >1: hay otro nombre del mismo archivo en el disco
    estado       TEXT    NOT NULL DEFAULT 'en_papelera' CHECK (estado IN ('en_papelera', 'restaurado', 'borrado', 'conflicto')),
    PRIMARY KEY (lote_id, ruta)
);
