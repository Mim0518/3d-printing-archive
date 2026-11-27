-- Fase 7: clientes, pedidos, pagos y colecciones de venta.
--
-- Un pedido tiene piezas (modelo, acabado, cantidad y precio). Una pieza se surte apartando miniaturas de las
-- existencias: cada apartado es un movimiento de salida ligado a la pieza, así que quitar la pieza o cancelar
-- el pedido las regresa. Lo que falta se imprime; una impresión hecha para el pedido aparta sola lo que sale.

CREATE TABLE clientes (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    nombre       TEXT    NOT NULL,
    whatsapp     TEXT    NOT NULL DEFAULT '',
    telefono     TEXT    NOT NULL DEFAULT '',
    correo       TEXT    NOT NULL DEFAULT '',
    redes        TEXT    NOT NULL DEFAULT '',     -- Facebook, Instagram, usuario de Mercado Libre…
    notas        TEXT    NOT NULL DEFAULT '',
    creado       INTEGER NOT NULL,
    actualizado  INTEGER NOT NULL
);

CREATE TABLE colecciones (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    nombre       TEXT    NOT NULL,
    descripcion  TEXT    NOT NULL DEFAULT '',
    estado       TEXT    NOT NULL DEFAULT 'borrador' CHECK (estado IN ('borrador', 'activa', 'archivada')),
    precio_fijo  REAL,                            -- si no, la suma de sus modelos
    creada       INTEGER NOT NULL,
    actualizada  INTEGER NOT NULL
);

CREATE TABLE coleccion_modelos (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    coleccion_id   INTEGER NOT NULL REFERENCES colecciones (id) ON DELETE CASCADE,
    modelo_id      INTEGER NOT NULL REFERENCES modelos (id),
    acabado        TEXT    NOT NULL DEFAULT 'sin_pintar' CHECK (acabado IN ('sin_pintar', 'imprimada', 'pintada')),
    cantidad       INTEGER NOT NULL DEFAULT 1 CHECK (cantidad > 0),
    precio         REAL,                          -- por miniatura, puesto a mano; si no, el de venta o el sugerido
    foto_id        INTEGER REFERENCES archivos (id) ON DELETE SET NULL,  -- si no, la portada del modelo
    nota           TEXT    NOT NULL DEFAULT '',
    orden          INTEGER NOT NULL DEFAULT 0,
    UNIQUE (coleccion_id, modelo_id, acabado)
);

CREATE TABLE pedidos (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    cliente_id     INTEGER REFERENCES clientes (id) ON DELETE SET NULL,
    estado         TEXT    NOT NULL DEFAULT 'cotizado'
                   CHECK (estado IN ('cotizado', 'confirmado', 'imprimiendo', 'listo', 'entregado', 'cancelado')),
    coleccion_id   INTEGER REFERENCES colecciones (id) ON DELETE SET NULL,
    descuento      REAL    NOT NULL DEFAULT 0,
    envio          REAL    NOT NULL DEFAULT 0,
    fecha_entrega  INTEGER,                        -- la prometida
    entregado      INTEGER,                        -- cuándo se entregó
    notas          TEXT    NOT NULL DEFAULT '',
    creado         INTEGER NOT NULL,
    actualizado    INTEGER NOT NULL
);

CREATE INDEX pedidos_cliente ON pedidos (cliente_id);

CREATE TABLE pedido_piezas (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    pedido_id       INTEGER NOT NULL REFERENCES pedidos (id) ON DELETE CASCADE,
    modelo_id       INTEGER NOT NULL REFERENCES modelos (id),
    acabado         TEXT    NOT NULL DEFAULT 'sin_pintar' CHECK (acabado IN ('sin_pintar', 'imprimada', 'pintada')),
    cantidad        INTEGER NOT NULL CHECK (cantidad > 0),
    precio          REAL    NOT NULL DEFAULT 0,  -- por miniatura
    notas           TEXT    NOT NULL DEFAULT ''
);

CREATE INDEX pedido_piezas_pedido ON pedido_piezas (pedido_id);

CREATE TABLE pagos (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    pedido_id  INTEGER NOT NULL REFERENCES pedidos (id) ON DELETE CASCADE,
    monto      REAL    NOT NULL CHECK (monto <> 0),
    fecha      INTEGER NOT NULL,
    metodo     TEXT    NOT NULL DEFAULT 'efectivo' CHECK (metodo IN ('efectivo', 'transferencia', 'tarjeta', 'otro')),
    notas      TEXT    NOT NULL DEFAULT '',
    usuario    TEXT    NOT NULL DEFAULT '',
    creado     INTEGER NOT NULL
);

CREATE INDEX pagos_pedido ON pagos (pedido_id);

-- Un apartado es un movimiento de salida (motivo «venta») ligado a la pieza del pedido; su costo unitario es el
-- promedio de la existencia en ese momento (para la ganancia).
ALTER TABLE movimientos ADD COLUMN pedido_pieza_id INTEGER REFERENCES pedido_piezas (id) ON DELETE CASCADE;
CREATE INDEX movimientos_pedido_pieza ON movimientos (pedido_pieza_id);

-- Impresiones hechas para un pedido.
ALTER TABLE impresiones ADD COLUMN pedido_id INTEGER REFERENCES pedidos (id) ON DELETE SET NULL;
