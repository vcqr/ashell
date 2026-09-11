import { fileURLToPath, URL } from 'node:url'

import { defineConfig } from "vite";
import vue from '@vitejs/plugin-vue'
import Icons from 'unplugin-icons/vite'
// import vueDevTools from 'vite-plugin-vue-devtools'


const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [
    vue(),
    // vscode-icons 文件图标集：构建期把 import 到的 SVG 编译成 Vue 组件，
    // 桌面端离线可用，未引用的图标不进产物
    Icons({ compiler: 'vue3' }),
    //vueDevTools(),
  ],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url))
    },
  },
}));
