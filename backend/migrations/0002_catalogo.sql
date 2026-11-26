-- Fase 1: bibliotecas, archivos (inventario), modelos (catálogo), etiquetas, búsqueda,
-- almacenamiento frío (solo lectura) y trabajos en segundo plano.

CREATE TABLE bibliotecas (
    id              INTEGER PRIMARY KEY,
    nombre          TEXT    NOT NULL UNIQUE,
    ruta            TEXT    NOT NULL,                 -- carpeta raíz en el disco
    tipo            TEXT    NOT NULL CHECK (tipo IN ('principal', 'entrada', 'externo')),
    activa          INTEGER NOT NULL DEFAULT 1,
    ultimo_escaneo  INTEGER
);

CREATE TABLE modelos (
    id           INTEGER PRIMARY KEY,
    carpeta      TEXT    NOT NULL UNIQUE,  -- «Ruta actual»: relativa a la biblioteca principal
    juego        TEXT    NOT NULL DEFAULT '',
    faccion      TEXT    NOT NULL DEFAULT '',
    nombre       TEXT    NOT NULL DEFAULT '',
    estudio      TEXT    NOT NULL DEFAULT '',
    subcarpeta   TEXT    NOT NULL DEFAULT '',
    confianza    TEXT    NOT NULL DEFAULT '',
    notas        TEXT    NOT NULL DEFAULT '',
    revisado     TEXT    NOT NULL DEFAULT '',
    -- estadísticas que se recalculan después de escanear o importar
    n_archivos   INTEGER NOT NULL DEFAULT 0,
    n_mallas     INTEGER NOT NULL DEFAULT 0,
    bytes        INTEGER NOT NULL DEFAULT 0,  -- sin contar dos veces los enlaces duros
    n_frio       INTEGER NOT NULL DEFAULT 0,
    bytes_frio   INTEGER NOT NULL DEFAULT 0,
    creado       INTEGER NOT NULL,
    actualizado  INTEGER NOT NULL
);

CREATE INDEX modelos_juego ON modelos (juego, faccion);

CREATE TABLE archivos (
    id             INTEGER PRIMARY KEY,
    biblioteca_id  INTEGER NOT NULL REFERENCES bibliotecas (id) ON DELETE CASCADE,
    ruta           TEXT    NOT NULL,          -- relativa a la biblioteca
    carpeta        TEXT    NOT NULL,          -- carpeta de la ruta ('' en la raíz)
    nombre         TEXT    NOT NULL,
    ext            TEXT    NOT NULL,          -- minúsculas, sin punto
    tipo           TEXT    NOT NULL,          -- malla | escena | laminado | imagen | comprimido | documento | video | otro
    bytes          INTEGER NOT NULL,
    mtime_ms       INTEGER NOT NULL,
    inode          INTEGER NOT NULL,
    nlink          INTEGER NOT NULL,
    stl_formato    TEXT,                      -- binario | texto | desconocido (solo .stl)
    triangulos     INTEGER,
    modelo_id      INTEGER REFERENCES modelos (id) ON DELETE SET NULL,
    estado         TEXT    NOT NULL DEFAULT 'presente' CHECK (estado IN ('presente', 'falta')),
    visto_en       INTEGER NOT NULL,
    UNIQUE (biblioteca_id, ruta)
);

CREATE INDEX archivos_carpeta ON archivos (biblioteca_id, carpeta);
CREATE INDEX archivos_modelo ON archivos (modelo_id);
CREATE INDEX archivos_inode ON archivos (biblioteca_id, inode);

CREATE TABLE etiquetas (
    id      INTEGER PRIMARY KEY,
    nombre  TEXT NOT NULL UNIQUE,
    tipo    TEXT NOT NULL DEFAULT '',
    color   TEXT NOT NULL DEFAULT ''
);

CREATE TABLE modelo_etiquetas (
    modelo_id    INTEGER NOT NULL REFERENCES modelos (id) ON DELETE CASCADE,
    etiqueta_id  INTEGER NOT NULL REFERENCES etiquetas (id) ON DELETE CASCADE,
    PRIMARY KEY (modelo_id, etiqueta_id)
);

CREATE INDEX modelo_etiquetas_etiqueta ON modelo_etiquetas (etiqueta_id);

-- Búsqueda de texto; rowid = modelos.id. Se reconstruye después de importar o editar.
CREATE VIRTUAL TABLE modelos_fts USING fts5 (
    nombre, juego, faccion, estudio, subcarpeta, etiquetas, notas, carpeta,
    tokenize = 'unicode61 remove_diacritics 2'
);

-- Almacenamiento frío: cada objeto guardado fuera del disco (un comprimido en Drive, un laminado en la
-- cuarentena…) y, si es un comprimido, lo que trae adentro.
CREATE TABLE ubicaciones_frio (
    id              INTEGER PRIMARY KEY,
    nombre          TEXT NOT NULL UNIQUE,
    tipo            TEXT NOT NULL CHECK (tipo IN ('drive', 'disco_externo')),
    remoto          TEXT NOT NULL DEFAULT '',   -- p. ej. «DriveMemo:3DP-archivo»
    ultimo_listado  INTEGER
);

CREATE TABLE copias_frio (
    id             INTEGER PRIMARY KEY,
    ubicacion_id   INTEGER NOT NULL REFERENCES ubicaciones_frio (id) ON DELETE CASCADE,
    ruta           TEXT    NOT NULL,            -- dentro de la ubicación
    bytes          INTEGER NOT NULL,
    md5            TEXT    NOT NULL DEFAULT '',
    modificado     TEXT    NOT NULL DEFAULT '',
    ruta_original  TEXT    NOT NULL DEFAULT '', -- dónde estaba en el disco antes de mandarse a frío
    modelo_id      INTEGER REFERENCES modelos (id) ON DELETE SET NULL,
    UNIQUE (ubicacion_id, ruta)
);

CREATE INDEX copias_frio_modelo ON copias_frio (modelo_id);

CREATE TABLE copias_contenido (
    copia_id  INTEGER NOT NULL REFERENCES copias_frio (id) ON DELETE CASCADE,
    entrada   TEXT    NOT NULL,
    bytes     INTEGER NOT NULL,
    crc       TEXT    NOT NULL DEFAULT '',
    PRIMARY KEY (copia_id, entrada)
);

CREATE TABLE trabajos (
    id         INTEGER PRIMARY KEY,
    tipo       TEXT    NOT NULL,                -- escaneo | …
    estado     TEXT    NOT NULL CHECK (estado IN ('en_curso', 'terminado', 'fallo')),
    detalle    TEXT    NOT NULL DEFAULT '',
    resultado  TEXT    NOT NULL DEFAULT '',     -- JSON
    creado     INTEGER NOT NULL,
    terminado  INTEGER
);
