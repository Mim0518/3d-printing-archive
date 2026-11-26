-- Fase 0: usuario y sesiones. Las tablas del catálogo llegan en la fase 1.

CREATE TABLE usuarios (
    id      INTEGER PRIMARY KEY,
    nombre  TEXT    NOT NULL UNIQUE,
    hash    TEXT    NOT NULL,          -- PHC de argon2id
    creado  INTEGER NOT NULL           -- segundos Unix
);

CREATE TABLE sesiones (
    token_hash  TEXT    PRIMARY KEY,   -- SHA-256 del token de la cookie (el token no se guarda)
    usuario_id  INTEGER NOT NULL REFERENCES usuarios (id) ON DELETE CASCADE,
    creada      INTEGER NOT NULL,
    expira      INTEGER NOT NULL,
    ultimo_uso  INTEGER NOT NULL
);

CREATE INDEX sesiones_expira ON sesiones (expira);
