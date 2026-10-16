import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// The built assets are embedded into the Rust binary (rust-embed reads
// frontend/dist). `base: './'` keeps asset URLs relative so they work when
// served from the actix-web root.
export default defineConfig({
  plugins: [react()],
  base: './',
  build: {
    outDir: 'dist',
    assetsDir: 'assets',
    emptyOutDir: true,
  },
  server: {
    port: 5173,
    proxy: {
      '/api': 'http://127.0.0.1:17733',
      '/ws': { target: 'ws://127.0.0.1:17733', ws: true },
    },
  },
})
