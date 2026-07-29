<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import {
  NModal,
  NCard,
  NSpace,
  NButton,
  NSpin,
  NEmpty,
  NCheckbox,
  NCheckboxGroup,
  NTreeSelect,
  NTag,
  NIcon,
  useMessage,
  type TreeSelectOption,
} from "naive-ui"
import { DownloadOutline } from "@vicons/ionicons5"
import { useI18n } from "vue-i18n"
import { listSshConfigHosts } from "@/api/hosts"
import { useHostStore } from "@/stores/hosts"
import type { SshConfigHost } from "@/types"

const props = defineProps<{ show: boolean }>()
const emit = defineEmits<{ "update:show": [v: boolean] }>()

const { t } = useI18n()
const store = useHostStore()
const message = useMessage()

const loading = ref(false)
const submitting = ref(false)
const configHosts = ref<SshConfigHost[]>([])
const checkedKeys = ref<number[]>([])
const targetGid = ref<number>(0)

/** 现有主机去重 key 集合：addr|port|username */
const existingKeys = computed(() => {
  const set = new Set<string>()
  for (const h of store.hosts) {
    set.add(`${h.addr}|${h.port}|${h.username}`)
  }
  return set
})

function isDuplicate(h: SshConfigHost): boolean {
  return existingKeys.value.has(`${h.addr}|${h.port}|${h.username ?? ""}`)
}

/** 文件夹选项（复用 HostTree 的 buildFolderOptions 逻辑） */
function buildFolderOptions(list: import("@/types").HostNode[]): TreeSelectOption[] {
  const out: TreeSelectOption[] = []
  for (const n of list) {
    if (n.type !== "folder") continue
    const children = n.children ? buildFolderOptions(n.children) : []
    const opt: TreeSelectOption = { key: n.id, label: n.label }
    if (children.length > 0) opt.children = children
    out.push(opt)
  }
  return out
}

const folderOptions = computed<TreeSelectOption[]>(() => [
  { key: 0, label: t("common.rootDir"), children: buildFolderOptions(store.tree) },
])

const allChecked = computed({
  get: () =>
    configHosts.value.length > 0 &&
    configHosts.value
      .filter((_, i) => !isDuplicate(configHosts.value[i]!))
      .every((_, i) => checkedKeys.value.includes(i)),
  set: (val: boolean) => {
    if (val) {
      checkedKeys.value = configHosts.value
        .map((h, i) => (isDuplicate(h) ? -1 : i))
        .filter((i) => i >= 0)
    } else {
      checkedKeys.value = []
    }
  },
})

async function loadConfig() {
  loading.value = true
  try {
    configHosts.value = await listSshConfigHosts()
    // 默认勾选非重复项
    checkedKeys.value = configHosts.value
      .map((h, i) => (isDuplicate(h) ? -1 : i))
      .filter((i) => i >= 0)
  } catch (e) {
    message.error(t("hosts.import.loadFailed", { error: String(e) }))
  } finally {
    loading.value = false
  }
}

watch(
  () => props.show,
  (v) => {
    if (v) {
      targetGid.value = 0
      void loadConfig()
    }
  },
)

async function doImport() {
  if (checkedKeys.value.length === 0) {
    message.warning(t("hosts.import.noneSelected"))
    return
  }
  submitting.value = true
  let ok = 0
  let fail = 0
  for (const i of checkedKeys.value) {
    const h = configHosts.value[i]
    if (!h) continue
    try {
      await store.addHost({
        gid: targetGid.value,
        name: h.name,
        addr: h.addr,
        port: h.port,
        username: h.username ?? "",
        private_key_path: h.identity_file ?? null,
        protocol: "ssh",
      })
      ok++
    } catch {
      fail++
    }
  }
  submitting.value = false
  if (ok > 0) message.success(t("hosts.import.imported", { count: ok }))
  if (fail > 0) message.error(t("hosts.import.importFailed", { count: fail }))
  if (fail === 0) emit("update:show", false)
}
</script>

<template>
  <NModal :show="props.show" @update:show="emit('update:show', $event)">
    <NCard
      style="width: min(680px, 90vw)"
      :title="t('hosts.import.title')"
      size="small"
      :bordered="false"
      role="dialog"
      aria-modal="true"
      closable
      @close="emit('update:show', false)"
    >
      <NSpin :show="loading">
        <NEmpty
          v-if="!loading && configHosts.length === 0"
          :description="t('hosts.import.empty')"
        />
        <div v-else class="import-body">
          <div class="import-toolbar">
            <NSpace align="center" :size="8">
              <NCheckbox v-model:checked="allChecked">
                {{ t("hosts.import.selectAll") }}
              </NCheckbox>
            </NSpace>
            <NSpace align="center" :size="8">
              <span class="toolbar-label">{{ t("hosts.import.targetFolder") }}</span>
              <NTreeSelect
                v-model:value="targetGid"
                :options="folderOptions"
                key-field="key"
                label-field="label"
                children-field="children"
                default-expand-all
                :consistent-menu-width="false"
                style="width: 200px"
                size="small"
              />
            </NSpace>
          </div>

          <NCheckboxGroup v-model:value="checkedKeys" class="host-list">
            <div
              v-for="(h, i) in configHosts"
              :key="i"
              class="host-row"
              :class="{ 'is-dup': isDuplicate(h) }"
            >
              <NCheckbox :value="i" :disabled="isDuplicate(h)">
                <span class="host-name">{{ h.name }}</span>
              </NCheckbox>
              <span class="host-detail">{{ h.addr }}:{{ h.port }}</span>
              <span class="host-user">{{ h.username || "-" }}</span>
              <NTag
                v-if="isDuplicate(h)"
                size="small"
                type="warning"
                :bordered="false"
              >
                {{ t("hosts.import.exists") }}
              </NTag>
            </div>
          </NCheckboxGroup>
        </div>
      </NSpin>

      <template #footer>
        <NSpace justify="end">
          <NButton :disabled="submitting" @click="emit('update:show', false)">
            {{ t("hosts.import.cancel") }}
          </NButton>
          <NButton
            type="primary"
            :loading="submitting"
            :disabled="checkedKeys.length === 0"
            @click="doImport"
          >
            <template #icon>
              <NIcon><DownloadOutline /></NIcon>
            </template>
            {{ t("hosts.import.import", { count: checkedKeys.length }) }}
          </NButton>
        </NSpace>
      </template>
    </NCard>
  </NModal>
</template>

<style scoped>
.import-body {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.import-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: 8px;
}

.toolbar-label {
  font-size: 13px;
  color: var(--ashell-text-subtle, #999);
}

.host-list {
  max-height: min(400px, 50vh);
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.host-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 6px 8px;
  border-radius: 6px;
  transition: background 0.1s ease;
}

.host-row:hover {
  background: rgba(255, 255, 255, 0.04);
}

.host-row.is-dup {
  opacity: 0.5;
}

.host-name {
  font-size: 13px;
  font-weight: 500;
  min-width: 100px;
}

.host-detail {
  font-size: 12px;
  color: var(--ashell-text-subtle, #888);
  font-family: var(--ashell-mono, monospace);
}

.host-user {
  font-size: 12px;
  color: var(--ashell-text-subtle, #888);
  margin-left: auto;
}
</style>
