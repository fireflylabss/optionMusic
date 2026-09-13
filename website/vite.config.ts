import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath, URL } from "node:url";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [tailwindcss(), react()],
  clearScreen: false,
  server: { port: 1421, strictPort: true },
  preview: { port: 1421, strictPort: true },
  resolve: {
    alias: {
      "@shared": fileURLToPath(new URL("./shared", import.meta.url)),
    },
  },
});
