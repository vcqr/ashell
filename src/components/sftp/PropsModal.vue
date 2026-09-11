<script setup lang="ts">
import { computed, ref, watch } from "vue"
import { NButton, NCheckbox, NInput, NModal, NSpace, useMessage } from "naive-ui"
import { useI18n } from "vue-i18n"
import { setAttrs } from "@/api/sftp"
import type { SftpFile } from "@/types"
import { humanSize } from "@/utils/humanSize"
import { formatUnix } from "@/utils/time"

interface Props {
  open: boolean
  sid: string | null
  file: SftpFile | null
}

const props = defineProps<Props>()
const emit = defineEmits<{
  "update:open": [value: boolean]
  saved: []
}>()

const { t } = useI18n()
const message = useMessage()

/** file_type 枚举 → 本地化文案；未知类型（socket/fifo 等）回退原文 */
const typeText = computed(() => {
  const ft = props.file?.file_type ?? ""
  const map: Record<string, string> = {
    dir: t("sftp.dialog.typeFolder"),
    file: t("sftp.dialog.typeFile"),
    symlink: t("sftp.dialog.typeSymlink"),
  }
  return map[ft] ?? ft
})

/** 八进制权限串（3 位），九宫格与输入框的共同数据源 */
const octal = ref("644")
const userInput = ref("")
const groupInput = ref("")
const saving = ref(false)

function normalizeOctal(v: string): string {
  const cleaned = v.replace(/[^0-7]/g, "")
  if (cleaned.length >= 3) return cleaned.slice(-3)
  return cleaned.padStart(3, "0")
}

/** 由 octal 派生 3x3 布尔矩阵：row 0=属主 1=组 2=其他；col 0=读 1=写 2=执行 */
const grid = computed<boolean[][]>(() =>
  normalizeOctal(octal.value)
    .split("")
    .map((d) => {
      const n = parseInt(d, 10)
      return [!!(n & 4), !!(n & 2), !!(n & 1)]
    }),
)

function toggleBit(row: number, col: number) {
  const digits = normalizeOctal(octal.value).split("")
  const n = parseInt(digits[row] ?? "0", 10)
  const bit = 4 >> col
  digits[row] = String((n ^ bit) & 7)
  octal.value = digits.join("")
}

watch(
  () => [props.open, props.file] as const,
  ([open, file]) => {
    if (!open || !file) return
    const sym = file.permissions || ""
    if (sym.length >= 9) {
      const triads = [sym.slice(0, 3), sym.slice(3, 6), sym.slice(6, 9)]
      octal.value = triads
        .map((tr) => {
          let n = 0
          if (tr[0] === "r") n |= 4
          if (tr[1] === "w") n |= 2
          if (tr[2] === "x") n |= 1
          return String(n)
        })
        .join("")
    } else {
      octal.value = "644"
    }
    userInput.value = file.user || ""
    groupInput.value = file.group || ""
  },
  { immediate: true },
)

async function apply() {
  if (!props.sid || !props.file || saving.value) return
  saving.value = true
  try {
    // 属主/组只在用户实际改动时提交，避免无意的 chown（可能需要 root 而失败）
    const userChanged = userInput.value.trim() !== (props.file.user || "")
    const groupChanged = groupInput.value.trim() !== (props.file.group || "")
    await setAttrs(props.sid, props.file.full_path, {
      mode: normalizeOctal(octal.value),
      ...(userChanged ? { user: userInput.value.trim() } : {}),
      ...(groupChanged ? { group: groupInput.value.trim() } : {}),
    })
    message.success(t("sftp.props.saved"))
    emit("saved")
    emit("update:open", false)
  } catch (e) {
    message.error(t("sftp.props.saveFailed", { error: (e as Error).message }))
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <NModal
    :show="props.open"
    preset="card"
    :title="t('sftp.properties.title')"
    style="width: 460px"
    :mask-closable="true"
    @update:show="emit('update:open', $event)"
  >
    <div v-if="props.file" class="props-body">
      <div class="info-rows">
        <div class="info-row">
          <span class="k">{{ t("sftp.properties.name") }}</span>
          <span class="v">{{ props.file.file_name }}</span>
        </div>
        <div class="info-row">
          <span class="k">{{ t("sftp.properties.path") }}</span>
          <span class="v mono" :title="props.file.full_path">{{ props.file.full_path }}</span>
        </div>
        <div class="info-row">
          <span class="k">{{ t("sftp.properties.type") }}</span>
          <span class="v">{{ typeText }}</span>
        </div>
        <div class="info-row">
          <span class="k">{{ t("sftp.properties.size") }}</span>
          <span class="v">{{
            typeof props.file.size_bytes === "number"
              ? humanSize(props.file.size_bytes)
              : props.file.size || "-"
          }}</span>
        </div>
        <div class="info-row">
          <span class="k">{{ t("sftp.properties.modifyTime") }}</span>
          <span class="v">{{ formatUnix(props.file.mtime ?? null) }}</span>
        </div>
      </div>

      <div class="perm-section">
        <div class="section-title">{{ t("sftp.props.permTitle") }}</div>
        <div class="perm-grid">
          <span></span>
          <span class="grid-head">{{ t("sftp.props.read") }}</span>
          <span class="grid-head">{{ t("sftp.props.write") }}</span>
          <span class="grid-head">{{ t("sftp.props.exec") }}</span>
          <template v-for="(row, r) in grid" :key="r">
            <span class="row-head">{{
              r === 0
                ? t("sftp.props.owner")
                : r === 1
                  ? t("sftp.props.groupRole")
                  : t("sftp.props.others")
            }}</span>
            <NCheckbox
              v-for="(checked, c) in row"
              :key="c"
              class="grid-check"
              :checked="checked"
              @update:checked="toggleBit(r, c)"
            />
          </template>
        </div>
        <NSpace align="center" :size="8" class="octal-row">
          <span class="octal-label">{{ t("sftp.props.octalLabel") }}</span>
          <NInput v-model:value="octal" size="small" class="octal-input" />
        </NSpace>
      </div>

      <NSpace :size="10" class="owner-row">
        <div class="owner-field">
          <span class="k">{{ t("sftp.props.ownerName") }}</span>
          <NInput v-model:value="userInput" size="small" />
        </div>
        <div class="owner-field">
          <span class="k">{{ t("sftp.props.groupName") }}</span>
          <NInput v-model:value="groupInput" size="small" />
        </div>
      </NSpace>
      <div class="hint">{{ t("sftp.props.nameOrIdHint") }}</div>
    </div>

    <template #footer>
      <NSpace justify="end" :size="8">
        <NButton size="small" @click="emit('update:open', false)">
          {{ t("common.cancel") }}
        </NButton>
        <NButton size="small" type="primary" :loading="saving" @click="apply">
          {{ t("sftp.props.apply") }}
        </NButton>
      </NSpace>
    </template>
  </NModal>
</template>

<style scoped>
.props-body {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.info-rows {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.info-row {
  display: flex;
  gap: 8px;
  font-size: 13px;
  min-width: 0;
}

.info-row .k {
  flex: 0 0 84px;
  color: var(--ashell-text-muted);
}

.info-row .v {
  flex: 1 1 auto;
  min-width: 0;
  color: var(--ashell-text-strong);
  word-break: break-all;
}

.info-row .v.mono {
  font-family: var(--n-font-family-mono);
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.section-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--ashell-text-muted);
  margin-bottom: 8px;
}

.perm-grid {
  display: grid;
  grid-template-columns: 56px repeat(3, 1fr);
  row-gap: 6px;
  column-gap: 4px;
  align-items: center;
  justify-items: center;
}

.perm-grid .grid-head {
  font-size: 12px;
  color: var(--ashell-text-subtle);
}

.perm-grid .row-head {
  justify-self: start;
  font-size: 12px;
  color: var(--ashell-text-strong);
}

.octal-row {
  margin-top: 10px;
}

.octal-label {
  font-size: 12px;
  color: var(--ashell-text-muted);
}

.octal-input {
  width: 80px;
  font-family: var(--n-font-family-mono);
}

.owner-row {
  align-items: flex-end;
}

.owner-field {
  display: flex;
  flex-direction: column;
  gap: 4px;
  flex: 1 1 0;
  min-width: 0;
}

.owner-field .k {
  font-size: 12px;
  color: var(--ashell-text-muted);
}

.hint {
  font-size: 11px;
  color: var(--ashell-text-subtle);
}
</style>
