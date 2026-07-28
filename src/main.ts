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
  console.error('[AShell] global error:', e.error ?? e.message)
})
window.addEventListener('unhandledrejection', (e) => {
  console.error('[AShell] unhandled rejection:', e.reason)
})

const app = createApp(App)
app.use(createPinia())
app.use(i18n)
app.mount('#app')

document.getElementById('boot-splash')?.remove()
