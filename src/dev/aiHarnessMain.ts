/**
 * AI 面板 UI 测试挂载页（仅 dev：vite build 默认入口只有 index.html，本文件不进产物）。
 *
 * 用法：`npm run dev` 后访问 http://localhost:5173/ai-harness.html
 * - 挂载真实 AiAssistant 组件（含 i18n / naive-ui Provider / 全量主题 CSS）
 * - store 的 spawn/attach/writeTo/kill 打桩：不依赖后端与真实 sidecar
 * - 经 window.__feed(line) 直接注入 sidecar stdout 协议行，100% 复用
 *   AiAssistant 内部真实的 handleSidecarOutput 解析路径
 * - window.__stdin() 读取面板写入 stdin 的内容（发送/审批/中断断言用）
 */
import 'vfonts/Lato.css'
import 'vfonts/FiraCode.css'
import '../assets/main.css'

import { createApp } from 'vue'
import { createPinia } from 'pinia'
import AiHarness from './AiHarness.vue'
import i18n from '../locales'

const app = createApp(AiHarness)
app.use(createPinia())
app.use(i18n)
app.mount('#app')
