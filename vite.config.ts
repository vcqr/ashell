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

  build: {
    rollupOptions: {
      output: {
        // vite 8 (rolldown) 分包：把大体积依赖从主 chunk 拆出并行加载；
        // codemirror 随 FileEditor 异步组件按需加载，其余随主入口并行拉取
        advancedChunks: {
          // 默认 true 会把各组命中模块的依赖整树递归拉进该组 chunk，
          // 绕过 tree-shaking 并与 vendor 兜底组重复，实测总体积膨胀 2.6MB
          includeDependenciesRecursively: false,
          groups: [
            { name: 'codemirror', test: /node_modules[\\/](?:@codemirror|@lezer|codemirror)/ },
            { name: 'hljs', test: /node_modules[\\/]highlight\.js/ },
            { name: 'xterm', test: /node_modules[\\/]@xterm/ },
            { name: 'naive-ui', test: /node_modules[\\/]naive-ui/ },
            // 兜底组放最后：剩余 node_modules 归单一 vendor chunk，
            // 避免未被分组的共享依赖被复制进多个 chunk 导致总体积膨胀
            { name: 'vendor', test: /node_modules/ },
          ],
        },
      },
    },
  },
}));
