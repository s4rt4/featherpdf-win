import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

const host = process.env.TAURI_DEV_HOST;

// Port 1440/1441 dipilih khusus agar TIDAK bentrok dengan project Tauri lain:
//   meusic & mark-hulk = 1420/1421, m-calc = 1430/1431 (semua strictPort).
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1440,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1441 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
});
