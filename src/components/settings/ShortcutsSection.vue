<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from "vue";
import { NButton, NTag, NSpace } from "naive-ui";
import { useI18n } from "vue-i18n";
import {
  useKeybindingStore,
  SHORTCUT_ACTIONS,
  formatBinding,
  type ShortcutActionId,
  type ShortcutCategory,
  type ShortcutActionDef,
  type KeyBinding,
} from "@/stores/keybindings";

const { t } = useI18n();
const store = useKeybindingStore();

const recordingId = ref<ShortcutActionId | null>(null);

const categories: {
  id: ShortcutCategory;
  actions: ShortcutActionDef[];
}[] = [
  {
    id: "tabs",
    actions: SHORTCUT_ACTIONS.filter((a) => a.category === "tabs"),
  },
  {
    id: "panels",
    actions: SHORTCUT_ACTIONS.filter((a) => a.category === "panels"),
  },
  {
    id: "search",
    actions: SHORTCUT_ACTIONS.filter((a) => a.category === "search"),
  },
];

function handleRecord(id: ShortcutActionId) {
  if (recordingId.value === id) {
    recordingId.value = null;
    store.recording = false;
    return;
  }
  recordingId.value = id;
  store.recording = true;
}

function onRecordKeydown(e: KeyboardEvent) {
  if (!recordingId.value) return;
  e.preventDefault();
  e.stopPropagation();

  if (e.key === "Escape") {
    recordingId.value = null;
    store.recording = false;
    return;
  }

  if (e.key === "Backspace" || e.key === "Delete") {
    store.setBinding(recordingId.value, null);
    recordingId.value = null;
    store.recording = false;
    return;
  }

  if (["Control", "Shift", "Alt", "Meta"].includes(e.key)) return;

  const binding: KeyBinding = {
    key: e.key.toLowerCase(),
    ctrl: e.ctrlKey,
    meta: e.metaKey,
    shift: e.shiftKey,
    alt: e.altKey,
  };

  store.setBinding(recordingId.value, binding);
  recordingId.value = null;
  store.recording = false;
}

onMounted(() => {
  window.addEventListener("keydown", onRecordKeydown, true);
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onRecordKeydown, true);
  recordingId.value = null;
  store.recording = false;
});
</script>

<template>
  <section class="settings-section">
    <div class="settings-section-title">{{ t("settings.shortcuts.title") }}</div>
    <p class="settings-hint">{{ t("settings.shortcuts.hint") }}</p>

    <div v-for="cat in categories" :key="cat.id">
      <div class="settings-subgroup" style="margin-top: 16px">
        {{ t(`settings.shortcuts.category.${cat.id}`) }}
      </div>
      <div class="shortcut-list">
        <div
          v-for="action in cat.actions"
          :key="action.id"
          class="shortcut-row"
          :class="{ recording: recordingId === action.id }"
        >
          <span class="shortcut-label">
            {{ t(`settings.shortcuts.action.${action.id}`) }}
          </span>
          <div class="shortcut-binding">
            <template v-if="recordingId === action.id">
              <span class="recording-hint">
                {{ t("settings.shortcuts.recording") }}
              </span>
            </template>
            <template v-else-if="store.getBinding(action.id)">
              <NTag
                v-for="(part, i) in formatBinding(store.getBinding(action.id))"
                :key="i"
                size="small"
                :bordered="true"
                type="info"
              >
                {{ part }}
              </NTag>
            </template>
            <template v-else>
              <span class="shortcut-unset">
                {{ t("settings.shortcuts.notSet") }}
              </span>
            </template>
          </div>
          <div class="shortcut-actions">
            <NButton
              size="tiny"
              :type="recordingId === action.id ? 'warning' : 'default'"
              secondary
              @click="handleRecord(action.id)"
            >
              {{ recordingId === action.id
                ? t("settings.shortcuts.cancel")
                : t("settings.shortcuts.record") }}
            </NButton>
            <NButton
              size="tiny"
              quaternary
              :disabled="!store.getBinding(action.id)"
              @click="store.resetBinding(action.id)"
            >
              {{ t("settings.shortcuts.reset") }}
            </NButton>
          </div>
        </div>
      </div>
    </div>

    <NSpace style="margin-top: 16px">
      <NButton size="small" @click="store.resetAll()">
        {{ t("settings.shortcuts.resetAll") }}
      </NButton>
    </NSpace>
  </section>
</template>

<style scoped>
.settings-section {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.settings-section-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--ashell-text-strong);
}

.settings-subgroup {
  font-size: 12px;
  font-weight: 500;
  color: var(--ashell-text-subtle, rgba(255, 255, 255, 0.4));
}

.settings-hint {
  margin: 8px 0 0;
  font-size: 12px;
  color: var(--ashell-text-muted, #98a2b3);
  line-height: 1.6;
}

.shortcut-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
  margin-top: 8px;
}

.shortcut-row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 8px 10px;
  border-radius: 8px;
  transition: background 0.15s ease;
}

.shortcut-row:hover {
  background: var(--ashell-hover);
}

.shortcut-row.recording {
  background: var(--ashell-active);
  outline: 1px solid var(--ashell-primary, #6366f1);
  outline-offset: -1px;
}

.shortcut-label {
  flex: 1;
  font-size: 13px;
  color: var(--ashell-text);
  min-width: 0;
}

.shortcut-binding {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 4px;
  min-height: 24px;
}

.shortcut-actions {
  flex: 0 0 auto;
  display: flex;
  gap: 4px;
}

.recording-hint {
  font-size: 12px;
  color: var(--ashell-text-muted, #98a2b3);
  font-style: italic;
  animation: pulse 1.5s ease infinite;
}

.shortcut-unset {
  font-size: 12px;
  color: var(--ashell-text-muted, #98a2b3);
  opacity: 0.5;
}

@keyframes pulse {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.4;
  }
}
</style>
