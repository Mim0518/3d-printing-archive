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

Estado: **fase 0** (base: servidor, inicio de sesión, CI y despliegue). Ver la [especificación](docs/especificacion.md) y la [guía de instalación](deploy/README.md).

## Desarrollo

Necesitas Rust (edición 2024) y Node 22.

```bash
# 1. Crea tu usuario en la base de desarrollo (backend/datos/archivo.db)
cd backend && cargo run -- usuario <tu-usuario>

# 2. Arranca la API en http://127.0.0.1:8080
cd backend && cargo run

# 3. En otra terminal, el frontend con recarga en vivo: http://localhost:5173 (manda /api al 8080)
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
