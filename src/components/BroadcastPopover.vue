<script setup lang="ts">
import { computed } from "vue"
import {
  NCheckbox,
  NDivider,
  NEmpty,
  NIcon,
  NScrollbar,
  NSelect,
  NSwitch,
  NTag,
  NTooltip,
  type SelectOption,
} from "naive-ui"
import {
  MegaphoneOutline,
  RadioOutline,
  TerminalOutline,
  ServerOutline,
} from "@vicons/ionicons5"
import { useBroadcastStore } from "@/stores/broadcast"
import { useI18n } from "vue-i18n"
import type { TerminalTab } from "@/types"

interface Props {
  tabs: TerminalTab[]
  activeKey: string
}

const props = defineProps<Props>()
const store = useBroadcastStore()
const { t } = useI18n()

/** 聚合所有窗口的 tab（本窗口 + 远程窗口），用全局 key 标识。 */
interface AggTab {
  gkey: string
  title: string
  kind: string
  isLocal: boolean
  windowId: string
}

const allTabs = computed<AggTab[]>(() =>
  store.getAllTabs(
    props.tabs.map((t) => ({
      key: t.key,
      title: t.title,
      kind: t.kind ?? "ssh",
    })),
  ),
)

const sourceOptions = computed<SelectOption[]>(() => [
  {
    label: t("broadcast.sourceFollowActive", { name: currentSourceTitle.value || "—" }),
    value: "__active__",
  },
  ...allTabs.value.map((tab) => ({
    label: tabLabel(tab),
    value: tab.gkey,
  })),
])

function tabLabel(tab: AggTab): string {
  const kindTag = tab.kind === "local" ? t("broadcast.tabLabel.local") : t("broadcast.tabLabel.ssh")
  const winTag = tab.isLocal ? "" : t("broadcast.tabLabel.remoteWindow")
  return `${kindTag}${winTag} ${tab.title || t("broadcast.tabLabel.unnamed")}`
}

/** 当前生效的源 tab 全局 key。 */
const effectiveSourceGkey = computed(() =>
  store.effectiveSource(props.activeKey ?? null, store.windowId || null),
)

/** 拿当前生效源 tab 的标题，给 UI 显示用。 */
const currentSourceTitle = computed(() => {
  const sk = effectiveSourceGkey.value
  if (!sk) return ""
  const found = allTabs.value.find((t) => t.gkey === sk)
  return found?.title ?? sk
})

/** "跟随激活" → 内部映射成 sourceKey = null */
const sourceSelect = computed<string>({
  get() {
    return store.sourceKey ?? "__active__"
  },
  set(v) {
    store.setSourceKey(v === "__active__" ? null : v)
  },
})

/** 列表里展示的 tab 集合：所有 tab 都列出，但源 tab 不能勾（自己接收自己） */
const tabRows = computed(() =>
  allTabs.value.map((t) => ({
    tab: t,
    isSource: t.gkey === effectiveSourceGkey.value,
    isTarget: store.targetKeys.has(t.gkey),
  })),
)

const selectableTargetGkeys = computed(() =>
  allTabs.value
    .filter((t) => t.gkey !== effectiveSourceGkey.value)
    .map((t) => t.gkey),
)

const allSelected = computed(
  () =>
    selectableTargetGkeys.value.length > 0 &&
    selectableTargetGkeys.value.every((k) => store.targetKeys.has(k)),
)

const someSelected = computed(() =>
  selectableTargetGkeys.value.some((k) => store.targetKeys.has(k)),
)

function selectAll() {
  store.setTargets(selectableTargetGkeys.value)
}

function clearAll() {
  store.clearTargets()
}

function onTopCheck(checked: boolean) {
  if (checked) selectAll()
  else clearAll()
}

/** enabled 走 setter 以触发跨窗口同步 */
const enabledProxy = computed<boolean>({
  get: () => store.enabled,
  set: (v) => store.setEnabled(v),
})

/** appendCR 走 setter 以触发跨窗口同步 */
const appendCRProxy = computed<boolean>({
  get: () => store.appendCR,
  set: (v) => store.setAppendCR(v),
})

function tabIcon(kind: string) {
  return kind === "local" ? TerminalOutline : ServerOutline
}
</script>

<template>
  <div class="broadcast-popover">
    <header class="bp-header">
      <NIcon :size="16" :class="{ active: store.isActive }">
        <MegaphoneOutline />
      </NIcon>
      <span class="bp-title">{{ t("broadcast.title") }}</span>
      <NSwitch v-model:value="enabledProxy" size="small" />
    </header>

    <div v-if="store.enabled" class="bp-body">
      <div class="bp-row">
        <span class="bp-row-label">{{ t("broadcast.source") }}</span>
        <NSelect
          v-model:value="sourceSelect"
          :options="sourceOptions"
          size="small"
          class="bp-source-select"
        />
      </div>

      <NDivider style="margin: 10px 0" />

      <div class="bp-row bp-targets-header">
        <NCheckbox
          :checked="allSelected"
          :indeterminate="!allSelected && someSelected"
          :disabled="selectableTargetGkeys.length === 0"
          @update:checked="onTopCheck"
        >
          {{ t("broadcast.targetTabs") }}
        </NCheckbox>
        <span class="bp-counter">{{ t("broadcast.selectedCount", { count: store.targetKeys.size }) }}</span>
      </div>

      <NScrollbar v-if="tabRows.length > 0" class="bp-targets" :style="{ maxHeight: '200px' }">
        <ul class="bp-tab-list">
          <li
            v-for="row in tabRows"
            :key="row.tab.gkey"
            class="bp-tab-row"
            :class="{ source: row.isSource }"
          >
            <NCheckbox
              :checked="row.isTarget"
              :disabled="row.isSource"
              @update:checked="store.toggleTarget(row.tab.gkey)"
            >
              <span class="bp-tab-line">
                <NIcon :size="14" class="bp-tab-icon">
                  <component :is="tabIcon(row.tab.kind)" />
                </NIcon>
                <span class="bp-tab-title">{{ row.tab.title || t("broadcast.tabLabel.unnamed") }}</span>
                <NTag
                  v-if="!row.tab.isLocal"
                  size="tiny"
                  :bordered="false"
                  class="bp-win-tag"
                >
                  {{ t("broadcast.remoteWindow") }}
                </NTag>
                <NTooltip v-if="row.isSource" trigger="hover">
                  <template #trigger>
                    <span class="bp-source-tag">
                      <NIcon :size="12"><RadioOutline /></NIcon>
                      {{ t("broadcast.sourceTag") }}
                    </span>
                  </template>
                  {{ t("broadcast.sourceTooltip") }}
                </NTooltip>
              </span>
            </NCheckbox>
          </li>
        </ul>
      </NScrollbar>
      <NEmpty v-else size="small" :description="t('broadcast.noTabs')" />

      <NDivider style="margin: 10px 0" />

      <div class="bp-row bp-cr">
        <NCheckbox v-model:checked="appendCRProxy">
          {{ t("broadcast.autoEnter") }}
        </NCheckbox>
      </div>

      <p class="bp-hint">
        {{ t("broadcast.realTimeHint") }}
      </p>
    </div>

    <p v-else class="bp-disabled-hint">
      {{ t("broadcast.enabledHint") }}
    </p>
  </div>
</template>

<style scoped>
.broadcast-popover {
  width: 320px;
  font-size: 13px;
}

.bp-header {
  display: flex;
  align-items: center;
  gap: 8px;
}

.bp-header .n-icon {
  color: var(--ashell-text-secondary, #98a2b3);
  transition: color 160ms ease;
}
.bp-header .n-icon.active {
  color: var(--ashell-accent, #80b5ff);
}

.bp-title {
  flex: 1;
  font-weight: 600;
}

.bp-body {
  margin-top: 10px;
}

.bp-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.bp-row-label {
  width: 60px;
  color: var(--ashell-text-secondary, #98a2b3);
  flex-shrink: 0;
}

.bp-source-select {
  flex: 1;
  min-width: 0;
}

.bp-targets-header {
  justify-content: space-between;
}

.bp-counter {
  color: var(--ashell-text-secondary, #98a2b3);
  font-size: 12px;
}

.bp-targets {
  margin-top: 4px;
}

.bp-tab-list {
  list-style: none;
  margin: 0;
  padding: 0;
}

.bp-tab-row {
  padding: 4px 2px;
}
.bp-tab-row.source {
  opacity: 0.7;
}

.bp-tab-line {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  max-width: 240px;
}

.bp-win-tag {
  flex-shrink: 0;
  font-size: 10px;
  opacity: 0.7;
}

.bp-tab-icon {
  color: var(--ashell-text-secondary, #98a2b3);
  flex-shrink: 0;
}

.bp-tab-title {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.bp-source-tag {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  padding: 0 6px;
  border-radius: 999px;
  font-size: 11px;
  background: var(--ashell-accent-soft, rgba(128, 181, 255, 0.18));
  color: var(--ashell-accent, #80b5ff);
}

.bp-cr {
  margin-top: 4px;
}

.bp-hint,
.bp-disabled-hint {
  margin-top: 10px;
  font-size: 12px;
  color: var(--ashell-text-secondary, #98a2b3);
  line-height: 1.5;
}
</style>
