<script setup lang="ts">
import { computed } from "vue"
import { NButton, NModal, NTooltip } from "naive-ui"
import { useI18n } from "vue-i18n"
import { copyText } from "@/utils/clipboard"

/**
 * SSH 主机密钥指纹确认弹窗（TOFU）。
 * 首次连接展示指纹请求信任；指纹变更时红色强警告并对比新旧指纹。
 */
export interface HostKeyInfo {
  /** addr:port */
  label: string
  keyType: string
  fingerprint: string
  /** 已保存指纹；非空即指纹变更场景 */
  previous: string | null
}

const props = defineProps<{
  show: boolean
  info: HostKeyInfo
}>()

const emit = defineEmits<{
  "update:show": [value: boolean]
  "trust": []
  "decline": []
}>()

const { t } = useI18n()

const changed = computed(() => !!props.info.previous)

/** SHA256 base64 指纹按 4 字符分组，人工比对远比整串可靠。
 *  串首可能带 "SHA256:" 前缀（后端格式），先剥掉——标签已注明算法，
 *  混进分组既误导比对（"SHA2"/"56:v"）又会多出一组造成孤行。 */
function stripAlgoPrefix(fp: string): string {
  return fp.replace(/^SHA256:/i, "")
}

function groupFp(fp: string): string[] {
  const groups: string[] = []
  const s = stripAlgoPrefix(fp)
  for (let i = 0; i < s.length; i += 4) groups.push(s.slice(i, i + 4))
  return groups
}

const newGroups = computed(() => groupFp(props.info.fingerprint))
const oldGroups = computed(() => (props.info.previous ? groupFp(props.info.previous) : []))

/** 指纹分组按行切分：每行固定 6 组（44 字符 → 6+5 两行）。不用 flex-wrap
 *  是因为它在不同弹窗宽度/DPI 下会产生"最后一行只剩 1 组"的孤行。 */
const FP_PER_ROW = 6

function chunkRows(groups: string[]): string[][] {
  const rows: string[][] = []
  for (let i = 0; i < groups.length; i += FP_PER_ROW) {
    rows.push(groups.slice(i, i + FP_PER_ROW))
  }
  return rows
}

const newRows = computed(() => chunkRows(newGroups.value))
const oldRows = computed(() => chunkRows(oldGroups.value))

/** 新旧指纹存在差异的组下标：逐组 diff，让视线直接落在变化段。 */
const differingIdx = computed(() => {
  const s = new Set<number>()
  if (!props.info.previous) return s
  const len = Math.max(newGroups.value.length, oldGroups.value.length)
  for (let i = 0; i < len; i++) {
    if (newGroups.value[i] !== oldGroups.value[i]) s.add(i)
  }
  return s
})

async function copyFp() {
  try {
    await copyText(props.info.fingerprint)
    return
  } catch {
    // ignore
  }
  try {
    await navigator.clipboard?.writeText(props.info.fingerprint)
  } catch {
    // ignore
  }
}

// 只发事件不自行改 show：由父组件（TerminalView）统一关闭并回发响应，
// 避免右上角 X 关闭时 update:show 先清掉状态导致响应丢失
function decline() {
  emit("decline")
}

function trust() {
  emit("trust")
}
</script>

<template>
  <NModal
    :show="props.show"
    preset="card"
    :title="changed ? t('terminal.hostKeyChangedTitle') : t('terminal.hostKeyTitle')"
    class="hostkey-modal"
    style="max-width: 560px; width: calc(100vw - 32px)"
    :mask-closable="false"
    :bordered="false"
    size="small"
    @update:show="(v: boolean) => { if (!v) decline() }"
  >
    <div v-if="changed" class="hostkey-warning">
      {{ t("terminal.hostKeyChangedDesc", { label: props.info.label }) }}
    </div>
    <div v-else class="hostkey-desc">
      {{ t("terminal.hostKeyDesc", { label: props.info.label }) }}
    </div>

    <div class="hostkey-field">
      <span class="hostkey-field-label">{{ t("terminal.hostKeyKeyType") }}</span>
      <span class="hostkey-field-value">{{ props.info.keyType }}</span>
    </div>

    <div class="hostkey-fp-block">
      <div v-if="changed" class="hostkey-fp-row">
        <span class="hostkey-fp-label">{{ t("terminal.hostKeyPrevious") }}</span>
        <span class="hostkey-fp-groups old">
          <span
            v-for="(row, r) in oldRows"
            :key="r"
            class="fp-row-line"
          >
            <span
              v-for="(g, c) in row"
              :key="c"
              class="fp-group"
              :class="{ diff: differingIdx.has(r * FP_PER_ROW + c) }"
            >{{ g }}</span>
          </span>
        </span>
      </div>
      <div class="hostkey-fp-row">
        <span class="hostkey-fp-label">
          {{ changed ? t("terminal.hostKeyNewFingerprint") : t("terminal.hostKeyFingerprint") }}
        </span>
        <div class="hostkey-fp-main">
          <span class="hostkey-fp-groups" :class="{ danger: changed }">
            <span
              v-for="(row, r) in newRows"
              :key="r"
              class="fp-row-line"
            >
              <span
                v-for="(g, c) in row"
                :key="c"
                class="fp-group"
                :class="{ diff: changed && differingIdx.has(r * FP_PER_ROW + c) }"
              >{{ g }}</span>
            </span>
          </span>
          <NTooltip trigger="hover">
            <template #trigger>
              <!-- 自绘 SVG 而非 NButton+NIcon：后者在此处曾渲染不出图标 -->
              <button
                type="button"
                class="hostkey-copy"
                :aria-label="t('common.copy')"
                @click="copyFp"
              >
                <svg
                  viewBox="0 0 24 24"
                  width="14"
                  height="14"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                >
                  <rect x="9" y="9" width="12" height="12" rx="2" />
                  <path d="M5 15V5a2 2 0 0 1 2-2h10" />
                </svg>
              </button>
            </template>
            {{ t("common.copy") }}
          </NTooltip>
        </div>
      </div>
    </div>

    <template #footer>
      <div class="hostkey-actions">
        <NButton size="small" @click="decline">
          {{ t("common.cancel") }}
        </NButton>
        <NButton size="small" :type="changed ? 'error' : 'primary'" @click="trust">
          {{ changed ? t("terminal.hostKeyTrustChanged") : t("terminal.hostKeyTrust") }}
        </NButton>
      </div>
    </template>
  </NModal>
</template>

<style scoped>
.hostkey-desc {
  font-size: 13px;
  line-height: 1.6;
  color: var(--ashell-text);
  margin-bottom: 12px;
}

.hostkey-warning {
  font-size: 13px;
  line-height: 1.6;
  color: #d03050;
  background: rgba(208, 48, 80, 0.08);
  border: 1px solid rgba(208, 48, 80, 0.3);
  border-radius: 6px;
  padding: 8px 12px;
  margin-bottom: 12px;
}

.hostkey-field {
  display: flex;
  gap: 10px;
  align-items: baseline;
  font-size: 12px;
  margin-bottom: 8px;
}

.hostkey-field-label {
  flex: 0 0 auto;
  color: var(--ashell-text-muted, #98a2b3);
}

.hostkey-field-value {
  word-break: break-all;
  color: var(--ashell-text);
}

/* 指纹块：新旧两行同构排布，4 字符一组等宽对齐，便于逐组比对 */
.hostkey-fp-block {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-bottom: 10px;
}

.hostkey-fp-row {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  font-size: 12px;
}

.hostkey-fp-label {
  flex: 0 0 auto;
  min-width: 56px;
  color: var(--ashell-text-muted, #98a2b3);
  line-height: 22px;
}

.hostkey-fp-groups {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
}

.fp-row-line {
  display: flex;
  gap: 3px;
}

.fp-group {
  font-family: var(--ashell-mono, monospace);
  font-size: 12px;
  line-height: 18px;
  padding: 1px 3px;
  border-radius: 3px;
  color: var(--ashell-text);
  /* 每组固定 4ch + padding，保证两行逐组纵向对齐 */
  width: 4ch;
  text-align: center;
  box-sizing: content-box;
}

.hostkey-fp-main {
  display: flex;
  align-items: center;
  gap: 6px;
  flex: 1;
  min-width: 0;
}

.hostkey-fp-groups.old .fp-group {
  color: var(--ashell-text-muted, #98a2b3);
  background: var(--ashell-hover, rgba(128, 128, 128, 0.12));
}

.hostkey-fp-groups.danger .fp-group {
  color: #d03050;
  background: rgba(208, 48, 80, 0.08);
}

/* 新旧不一致的组：强高亮，视线直接落在变化段 */
.fp-group.diff {
  outline: 1px solid rgba(208, 48, 80, 0.55);
  font-weight: 700;
}

.hostkey-fp-groups.old .fp-group.diff {
  background: rgba(208, 48, 80, 0.14);
  color: #d03050;
}

.hostkey-copy {
  flex: 0 0 auto;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border-radius: 6px;
  border: 1px solid var(--ashell-border, rgba(128, 128, 128, 0.35));
  background: transparent;
  color: var(--ashell-text-muted, #98a2b3);
  cursor: pointer;
  transition:
    background 0.12s ease,
    color 0.12s ease,
    border-color 0.12s ease;
}

.hostkey-copy:hover {
  color: var(--ashell-text-strong, #fff);
  border-color: var(--ashell-border);
  background: var(--ashell-hover, rgba(255, 255, 255, 0.06));
}

.hostkey-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
