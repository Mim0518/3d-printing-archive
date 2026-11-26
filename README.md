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

Estado: **fase 3** (miniaturas de todas las mallas e imágenes con f3d, portada de cada modelo, galería y visor 3D en el navegador; antes: catálogo editable con etiquetas, historial y mudanzas con deshacer). Ver la [especificación](docs/especificacion.md) y la [guía de instalación](deploy/README.md).

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
