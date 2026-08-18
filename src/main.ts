import 'vfonts/Lato.css'
import 'vfonts/FiraCode.css'
import '@xterm/xterm/css/xterm.css'
import './assets/main.css'

import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import i18n from './locales'

// 全局未捕获异常兜底，避免静默白屏
window.addEventListener('error', (e) => {
  // 浏览器 ResizeObserver 固有告警：一帧内连续布局变化所致，
  // naive-ui 组件常态触发，无功能影响，过滤以免刷屏
  if (e.message?.includes('ResizeObserver loop')) {
    e.preventDefault() // 抑制 webview 对该 error 事件的原生控制台输出
    return
  }
  console.error('[AShell] global error:', e.error ?? e.message)
})
window.addEventListener('unhandledrejection', (e) => {
  console.error('[AShell] unhandled rejection:', e.reason)
})

// 屏蔽 webview 默认右键菜单（Reload / Back / Inspect 等）。
// 误点 Reload 会重载整个 webview，所有终端会话全部断开。
// 例外：
// - contentEditable 元素（CodeMirror 等富文本编辑器需要右键菜单）
// - 有选中文本时（让用户能右键 Copy）
// 终端区域通过 onContextMenu 里的 stopPropagation 自行决定是否放行。
document.addEventListener('contextmenu', (e) => {
  const target = e.target as HTMLElement | null
  if (target && target.isContentEditable) return
  const selection = window.getSelection()
  if (selection && selection.toString().length > 0) return
  e.preventDefault()
})

const app = createApp(App)
app.use(createPinia())
app.use(i18n)
app.mount('#app')

document.getElementById('boot-splash')?.remove()
