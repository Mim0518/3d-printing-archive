# 3D Printing Archive

Aplicación web para archivar y administrar una colección de impresión 3D:

- inventario de modelos y piezas, etiquetado y miniaturas;
- placas y escenas;
- almacenamiento frío (Google Drive y disco externo);
- impresoras y materiales;
- existencias impresas, pedidos y colecciones para venta.

Tecnología:

- Backend: Rust (Axum + SQLite). Frontend: React + Vite + TypeScript (Mantine).
- Corre en un mini PC en casa; desde fuera se entra por Tailscale.

Estado: **fase 8**. Almacenamiento frío:
- mandar un modelo a Google Drive (rclone) o a un disco externo, como carpeta tal cual, con verificación por MD5 de cada archivo;
- quitar la copia local a la papelera de la app (se restaura o se vacía, con confirmación escrita y volviendo a comprobar la copia);
- traer un modelo (o algunas piezas) de regreso, comprobando MD5 y CRC de los zip, sin sobrescribir nada.

Fase 7, ventas:
- clientes, pedidos (cotizado → confirmado → imprimiendo → listo → entregado, o cancelado) con pagos (anticipo y saldo), descuento y envío;
- las piezas de un pedido se apartan de las existencias o se imprimen para él (lo que sale se aparta solo); cancelar o quitar una pieza las regresa;
- colecciones de venta con fotos, precio (la suma o uno fijo) y si alcanzan las existencias; se mandan como PDF (hoja para imprimir) o imagen para WhatsApp, y se convierten en pedido;
- resumen de lo vendido, la ganancia, lo que falta cobrar y lo más vendido.

Fases anteriores: existencias y costos de impresión con precio sugerido; impresoras, materiales y perfiles importados de Photon Workshop, Orca y Lychee; placas y escenas ligadas a sus modelos; miniaturas con f3d, portadas y visor 3D; catálogo editable con etiquetas, historial y mudanzas con deshacer. Ver la [especificación](docs/especificacion.md) y la [guía de instalación](deploy/README.md).

## Desarrollo

Necesitas Rust (edición 2024), Node 22 y, para las miniaturas, [f3d](https://f3d.app) 3.x.

```bash
# 1. Crea tu usuario en la base de desarrollo (backend/datos/archivo.db)
cd backend && cargo run -- usuario <tu-usuario>

# 2. La colección y los datos (ARCHIVO_BIBLIOTECA es la carpeta de la colección; solo se lee)
export ARCHIVO_BIBLIOTECA=/mnt/documentos/Hobby/3DP
cd backend && cargo run -- importar catalogo <hoja.csv>                # pestaña Catálogo exportada a CSV
cd backend && cargo run -- escanear                                    # inventario del disco (incremental)
cd backend && cargo run -- importar frio <frio.csv> [contenido.csv]   # lo que está en Drive

# 3. Arranca la API en http://127.0.0.1:8080 (con ARCHIVO_BIBLIOTECA puesto, el escaneo también se lanza desde la app)
cd backend && cargo run

# 4. En otra terminal, el frontend con recarga en vivo: http://localhost:5173 (manda /api al 8080)
cd frontend && npm install && npm run dev
```

Para probar como en producción, compila el frontend (`cd frontend && npm run build`) y abre `http://127.0.0.1:8080`: el backend sirve `frontend/dist` por su cuenta.

Revisiones (las mismas que corre el CI):

```bash
cd backend && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
cd frontend && npm run lint && npm run build
```

## Estructura

| Carpeta | Contenido |
|---|---|
| `backend/` | Servidor Rust: API `/api/v1`, migraciones de SQLite (`migrations/`) y pruebas (`tests/`) |
| `frontend/` | Aplicación React (Vite) |
| `deploy/` | Servicio de systemd, ejemplo de configuración, script de empaquetado y guía de instalación |
| `docs/` | Especificación |
