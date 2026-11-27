-- Fase 9: bandeja «Por catalogar». Lo que se detecta y se propone se calcula al vuelo a partir de los archivos
-- que no son de ningún modelo; aquí solo se guarda lo que decide el usuario y lo que hizo la app.

-- Grupos que el usuario mandó a «No mover»: no se vuelven a proponer.
CREATE TABLE bandeja_descartes (
    clave    TEXT    PRIMARY KEY,
    motivo   TEXT    NOT NULL DEFAULT '',
    usuario  TEXT    NOT NULL DEFAULT '',
    fecha    INTEGER NOT NULL
);

-- Comprimidos extraídos en la bandeja: el comprimido se guarda dentro de su carpeta y, al aprobar el modelo, se
-- manda a frío.
CREATE TABLE bandeja_comprimidos (
    archivo_id  INTEGER PRIMARY KEY REFERENCES archivos (id) ON DELETE CASCADE,
    carpeta     TEXT    NOT NULL,       -- dónde se extrajo (relativa a la biblioteca)
    entradas    INTEGER NOT NULL,
    bytes       INTEGER NOT NULL,
    usuario     TEXT    NOT NULL DEFAULT '',
    extraido    INTEGER NOT NULL
);

-- Mandar a frío solo algunos archivos del modelo (por ejemplo, el comprimido original).
ALTER TABLE transferencias ADD COLUMN solo_archivos TEXT NOT NULL DEFAULT '';
