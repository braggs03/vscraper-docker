import react from '@vitejs/plugin-react'
import { defineConfig, loadEnv } from 'vite'

import tailwindcss from "@tailwindcss/vite"
import path from "path"

// https://vite.dev/config/
export default defineConfig(({ mode }) => {
    const env = loadEnv(mode, process.cwd(), '');
    return {
        plugins: [react(), tailwindcss()],
        resolve: {
            alias: {
                "@": path.resolve(__dirname, "./src"),
            },
        },
        server: {
            proxy: {
                '/api': {
                    target: env.API_URL,
                    changeOrigin: true,
                    ws: true,
                },
            },
        },
    }
})
