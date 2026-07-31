import path from 'node:path'
import tailwindcss from '@tailwindcss/vite'
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// tauri.conf.json points its development webview at this port. Without this
// explicit match, `tauri dev` starts Vite on 5173 and the desktop webview
// never receives the Tauri bridge.
export default defineConfig({
  plugins: [tailwindcss(), react()],
  clearScreen: false,
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src-ui'),
    },
  },
  server: { port: 1420, strictPort: true },
})
