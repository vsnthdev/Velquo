import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig(async () => ({
    clearScreen: false,
    plugins: [react(), tailwindcss()],
    server: {
        port: 1420,
        strictPort: true,
        host: host || false,
        watch: {
            ignored: ["**/src-tauri/**"],
        },
        hmr: host
            ? {
                host,
                port: 1421,
                protocol: "ws",
            }
            : undefined,
    },
}));
