<script setup lang="ts">
/**
 * AI 面板测试容器（dev-only harness）。
 *
 * 会话预建 + 打桩在 setup 阶段完成（先于子组件挂载后的 ensureSidecarFor）：
 * - 预建 h1 会话并置 fake pid：ensureSidecarFor 命中早退，不触达后端
 * - 协议输出经 AiAssistant.defineExpose 的 feedSidecarLine 注入，
 *   100% 复用组件内部真实的 handleSidecarOutput 解析路径
 */
import { onMounted, ref } from 'vue'
import {
  NConfigProvider,
  NDialogProvider,
  NMessageProvider,
  NNotificationProvider,
  darkTheme,
} from 'naive-ui'
import AiAssistant from '@/components/AiAssistant.vue'
import { useAiStore } from '@/stores/ai'
import { useAiConfigStore } from '@/stores/aiConfig'
import { usePhraseStore } from '@/stores/phrases'
import type { AiEnginesState, AiEngine, AiProvider } from '@/types'

const ready = ref(false)
const TEST_SSID = 'h1'
const aiRef = ref<InstanceType<typeof AiAssistant> | null>(null)

/** naive 组件主题跟随 data-ashell-theme 属性，保证视觉测试与真实应用一致 */
const naiveTheme = ref<typeof darkTheme | null>(darkTheme)
const themeObserver =
  typeof MutationObserver !== 'undefined'
    ? new MutationObserver(() => {
        naiveTheme.value =
          document.documentElement.getAttribute('data-ashell-theme') === 'light'
            ? null
            : darkTheme
      })
    : null

onMounted(() => {
  themeObserver?.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ['data-ashell-theme'],
  })
  const ai = useAiStore()
  const aiConfig = useAiConfigStore()
  const phrases = usePhraseStore()

  // ── 种子引擎配置：绕开 setup 引导卡片，模型选择器有真实数据 ──
  const provider: AiProvider = {
    id: 'p1',
    name: 'Test Provider',
    api_type: 'openai-completions',
    base_url: 'https://example.test/v1',
    api_key: 'sk-test',
    model_ids: 'test-model-large,test-model-mini',
    disabled_model_ids: '',
    sort_order: 0,
    is_del: false,
  }
  const engine: AiEngine = {
    engine: 'pi',
    provider_id: 'p1',
    active_model_id: 'test-model-large',
    thinking_level: 'off',
    context_window: '',
    max_tokens: '',
    max_rounds: '',
    streaming: '1',
  }
  const enginesState: AiEnginesState = { active_engine: 'pi', engines: [engine] }
  aiConfig.$patch({ providers: [provider], enginesState })
  // load/patchEngine 桩掉：不发真实请求
  const cfgStub = aiConfig as unknown as Record<string, unknown>
  cfgStub.load = async () => {}
  cfgStub.patchEngine = async () => engine
  cfgStub.switchEngine = async () => {}

  // ── 会话预建 + sidecar 生命周期打桩 ──
  ai.ensure(TEST_SSID)
  ai.patch(TEST_SSID, { sidecarPid: 4242, isSessionActive: true })
  const stdinLog: string[] = []
  ;(window as unknown as Record<string, unknown>).__stdin = () => [...stdinLog]
  ;(window as unknown as Record<string, unknown>).__clearStdin = () => {
    stdinLog.length = 0
  }
  ;(window as unknown as Record<string, unknown>).__feed = (line: string) => {
    aiRef.value?.feedSidecarLine(line)
  }
  const aiStub = ai as unknown as Record<string, unknown>
  aiStub.writeTo = async (_ssid: string, data: string) => {
    stdinLog.push(data)
  }
  aiStub.killFor = async () => {}
  const phStub = phrases as unknown as Record<string, unknown>
  phStub.load = async () => {}

  ready.value = true
})
</script>

<template>
  <NConfigProvider :theme="naiveTheme">
    <NMessageProvider>
      <NDialogProvider>
        <NNotificationProvider>
          <div class="harness-stage">
            <AiAssistant ref="aiRef" :open="ready" :sid="TEST_SSID" host-name="test-host" />
          </div>
        </NNotificationProvider>
      </NDialogProvider>
    </NMessageProvider>
  </NConfigProvider>
</template>

<style>
body {
  margin: 0;
  background: #101014;
  color: rgba(255, 255, 255, 0.92);
  font-family: Lato, sans-serif;
}

.harness-stage {
  min-height: 100vh;
}
</style>
