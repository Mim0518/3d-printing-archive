-- Fase 3: miniaturas (una por archivo renderizable; el archivo de imagen vive en la caché, en disco) y portada
-- elegida a mano para cada modelo.

CREATE TABLE miniaturas (
    archivo_id  INTEGER PRIMARY KEY REFERENCES archivos (id) ON DELETE CASCADE,
    clave       TEXT    NOT NULL,     -- inode-tamaño-fecha del archivo cuando se generó (hex)
    estado      TEXT    NOT NULL CHECK (estado IN ('ok', 'error')),
    error       TEXT    NOT NULL DEFAULT '',
    generada    INTEGER NOT NULL
);

CREATE INDEX miniaturas_clave ON miniaturas (clave);

ALTER TABLE modelos ADD COLUMN portada_archivo_id INTEGER REFERENCES archivos (id) ON DELETE SET NULL;
