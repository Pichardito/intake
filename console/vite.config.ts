import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// Served by intake-server under /console (see docs/architecture.md §3), so every asset
// URL is rooted there. In dev, /admin/* is proxied to the mock API
// (`pnpm -C widget dev`, port 8787) or to a real server via INTAKE_API.
export default defineConfig({
  base: "/console/",
  plugins: [react()],
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "es2022",
    sourcemap: false,
  },
  server: {
    port: 5174,
    proxy: {
      "/admin": process.env["INTAKE_API"] ?? "http://127.0.0.1:8787",
    },
  },
});
