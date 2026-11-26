#!/usr/bin/env bash
# Compila el backend (release) y el frontend, y arma paquete/printing-archive-<versión>-linux-x86_64.tar.gz con:
#   printing-archive (binario), dist/ (frontend), printing-archive.service y printing-archive.env.example
# Para el mini PC conviene compilar en Ubuntu 24.04 (lo hace el CI) para que coincida la versión de glibc.
set -euo pipefail

raiz="$(cd "$(dirname "$0")/.." && pwd)"
cd "$raiz"

version="$(sed -n 's/^version = "\(.*\)"/\1/p' backend/Cargo.toml | head -1)"
nombre="printing-archive-${version}-linux-x86_64"

(cd backend && cargo build --release --locked)
(cd frontend && npm ci && npm run build)

rm -rf "paquete/$nombre"
mkdir -p "paquete/$nombre"
cp backend/target/release/printing-archive "paquete/$nombre/"
cp -r frontend/dist "paquete/$nombre/dist"
cp deploy/printing-archive.service deploy/printing-archive.env.example "paquete/$nombre/"
tar -C paquete -czf "paquete/$nombre.tar.gz" "$nombre"
echo "Listo: paquete/$nombre.tar.gz"
