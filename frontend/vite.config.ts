import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// En desarrollo, /api va al backend de Rust (cargo run en backend/, puerto 8080).
// En producción el mismo binario sirve dist/ y la API, así que no hace falta proxy.
export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      '/api': 'http://127.0.0.1:8080',
    },
  },
})
