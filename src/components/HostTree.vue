<script setup lang="ts">
import { computed, h, nextTick, onBeforeUnmount, onMounted, ref } from "vue"
import {
  NTree,
  NIcon,
  NButton,
  NInput,
  NDropdown,
  NModal,
  NCard,
  NSpace,
  NEmpty,
  NSpin,
  NTooltip,
  NTreeSelect,
  NForm,
  NFormItem,
  useMessage,
  useDialog,
  type TreeOption,
  type TreeSelectOption,
  type DropdownOption,
} from "naive-ui"
import {
  FolderOutline,
  FolderOpenOutline,
  TerminalOutline,
  AddOutline,
  RefreshOutline,
  CreateOutline,
  TrashOutline,
  SearchOutline,
  CloseOutline,
  CopyOutline,
  ServerOutline,
  DownloadOutline,
} from "@vicons/ionicons5"
import { FolderAddOutlined } from "@vicons/antd"
import { useI18n } from "vue-i18n"
import { useHostStore } from "@/stores/hosts"
import { useIconStore } from "@/stores/icons"
import SshConfigImportModal from "@/components/SshConfigImportModal.vue"
import type { HostNode, Host } from "@/types"

const emit = defineEmits<{
  close: []
  "open-host": [node: HostNode, forceNew?: boolean]
  "create-host": [parentGid: number]
  "edit-host": [host: Host]
}>()

const { t } = useI18n()
const store = useHostStore()
const iconStore = useIconStore()
const message = useMessage()
const dialog = useDialog()

const filter = ref("")
const selectedKeys = ref<string[]>([])
const expandedKeys = ref<string[]>([])
const importModalShow = ref(false)

onMounted(() => {
  void iconStore.ensureLoaded()
})

const treeData = computed<TreeOption[]>(
  () => store.tree as unknown as TreeOption[],
)

/* ---------- 仅含目录（folder）的列表，供 NTreeSelect 选父级 ---------- */
function buildFolderOptions(list: HostNode[]): TreeSelectOption[] {
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

const folderSelectOptions = computed<TreeSelectOption[]>(() => [
  { key: 0, label: t("common.rootDir"), children: buildFolderOptions(store.tree) },
])

function renderPrefix({ option }: { option: TreeOption }) {
  const node = option as unknown as HostNode
  if (node.type === "folder") {
    return h(
      NIcon,
      { color: "#f1c27d", size: 16 },
      { default: () => h(option.expanded ? FolderOpenOutline : FolderOutline) },
    )
  }
  const iconUrl = node.icon ? iconStore.urlOf(node.icon) : null
  if (iconUrl) {
    return h("img", {
      src: iconUrl,
      width: 16,
      height: 16,
      style: {
        borderRadius: "3px",
        objectFit: "contain",
        verticalAlign: "middle",
      },
    })
  }
  const color = node.color ?? "#7c5cff"
  return h(
    NIcon,
    { color, size: 16 },
    { default: () => h(TerminalOutline) },
  )
}

function findParentAndIndex(
  list: HostNode[],
  key: string,
  parent: HostNode | null = null,
): { parent: HostNode | null; list: HostNode[]; index: number } | null {
  for (let i = 0; i < list.length; i++) {
    const item = list[i]!
    if (item.key === key) return { parent, list, index: i }
    if (item.children) {
      const found = findParentAndIndex(item.children, key, item)
      if (found) return found
    }
  }
  return null
}

function findNode(key: string): HostNode | null {
  const found = findParentAndIndex(store.tree, key)
  return found ? found.list[found.index]! : null
}

function collectAncestorKeys(key: string): string[] {
  const keys: string[] = []
  function walk(list: HostNode[], trail: string[]): boolean {
    for (const item of list) {
      if (item.key === key) {
        keys.push(...trail)
        return true
      }
      if (item.children && walk(item.children, [...trail, item.key])) return true
    }
    return false
  }
  walk(store.tree, [])
  return keys
}

function resolveSelectedFolderGid(): number {
  const k = selectedKeys.value[0]
  if (!k) return 0
  const node = findNode(k)
  if (!node) return 0
  if (node.type === "folder") return node.id
  const found = findParentAndIndex(store.tree, k)
  if (found?.parent && found.parent.type === "folder") return found.parent.id
  return 0
}

const ctxMenuShow = ref(false)
const ctxMenuX = ref(0)
const ctxMenuY = ref(0)
const ctxMenuKey = ref<string | null>(null)

/** 空白处右键：弹出根级操作菜单（新建连接 / 新建目录 / 导入 / 刷新） */
function onBlankContextMenu(e: MouseEvent) {
  e.preventDefault()
  e.stopPropagation()
  ctxMenuKey.value = null
  ctxMenuX.value = e.clientX
  ctxMenuY.value = e.clientY
  ctxMenuShow.value = false
  requestAnimationFrame(() => (ctxMenuShow.value = true))
}

/* ---------- 拖拽（pointer events，wry 稳定） ---------- */
const DRAG_THRESHOLD = 5

interface DragState {
  hostNode: HostNode
  startX: number
  startY: number
  active: boolean
}

let drag: DragState | null = null
const dragGhost = ref<{ x: number; y: number; label: string } | null>(null)
const dropTargetKey = ref<string | null>(null) // "folder-<id>" | "__root__" | null

const treeBodyEl = ref<HTMLElement | null>(null)

function beginHostDrag(e: PointerEvent, node: HostNode) {
  drag = {
    hostNode: node,
    startX: e.clientX,
    startY: e.clientY,
    active: false,
  }
  window.addEventListener("pointermove", onDragMove)
  window.addEventListener("pointerup", onDragEnd)
  window.addEventListener("pointercancel", onDragEnd)
}

function onDragMove(e: PointerEvent) {
  if (!drag) return
  if (!drag.active) {
    const dx = e.clientX - drag.startX
    const dy = e.clientY - drag.startY
    if (Math.hypot(dx, dy) < DRAG_THRESHOLD) return
    drag.active = true
    document.body.style.userSelect = "none"
    document.body.style.cursor = "grabbing"
  }
  dragGhost.value = {
    x: e.clientX,
    y: e.clientY,
    label: drag.hostNode.label,
  }
  dropTargetKey.value = findFolderKeyAtPoint(e.clientX, e.clientY)
}

function onDragEnd(_e: PointerEvent) {
  if (!drag) return
  const wasActive = drag.active
  const node = drag.hostNode
  const target = dropTargetKey.value
  cleanupDrag()
  if (!wasActive) return
  if (!target) return

  const newGid =
    target === "__root__" ? 0 : Number(target.replace(/^folder-/, ""))
  if (!Number.isFinite(newGid)) return
  if (node.type !== "host") return

  const host = store.findHost(node.id)
  if (!host) return
  if (host.gid === newGid) return

  // 抑制 pointerup 后的 click（避免触发 NTree 的选中/展开）
  const suppress = (ev: MouseEvent) => {
    ev.preventDefault()
    ev.stopPropagation()
  }
  window.addEventListener("click", suppress, { capture: true, once: true })
  setTimeout(() => {
    window.removeEventListener("click", suppress, { capture: true } as never)
  }, 50)

  void moveHostToGid(host, newGid)
}

function cleanupDrag() {
  drag = null
  dragGhost.value = null
  dropTargetKey.value = null
  document.body.style.userSelect = ""
  document.body.style.cursor = ""
  window.removeEventListener("pointermove", onDragMove)
  window.removeEventListener("pointerup", onDragEnd)
  window.removeEventListener("pointercancel", onDragEnd)
}

onBeforeUnmount(cleanupDrag)

/** 反查光标下的 folder 节点 key；命中 host 时落到其父；空白处落到根 */
function findFolderKeyAtPoint(x: number, y: number): string | null {
  const root = treeBodyEl.value
  if (!root) return null
  const rect = root.getBoundingClientRect()
  if (x < rect.left || x > rect.right || y < rect.top || y > rect.bottom) {
    return null
  }
  const els = document.elementsFromPoint(x, y)
  for (const el of els) {
    if (!(el instanceof HTMLElement)) continue
    if (!root.contains(el)) continue
    const carrier = el.closest<HTMLElement>("[data-node-key]")
    if (!carrier || !root.contains(carrier)) continue
    const key = carrier.dataset.nodeKey
    if (!key) continue
    const node = findNode(key)
    if (!node) continue
    if (node.type === "folder") return key
    const found = findParentAndIndex(store.tree, key)
    if (found?.parent && found.parent.type === "folder") return found.parent.key
    return "__root__"
  }
  return "__root__"
}

async function moveHostToGid(host: Host, newGid: number) {
  try {
    await store.editHost(host.id, { gid: newGid })
    const folderName =
      newGid === 0 ? t("hosts.message.rootDir") : (store.findGroup(newGid)?.name ?? `#${newGid}`)
    message.success(t("hosts.message.moved", { name: host.name, target: folderName }))
    await nextTick()
    revealKey(`host-${host.id}`)
  } catch (e) {
    message.error(t("hosts.message.moveFailed", { error: String(e) }))
  }
}

function nodeProps({ option }: { option: TreeOption }) {
  const key = option.key as string
  const isDrop = dropTargetKey.value === key
  return {
    "data-node-key": key,
    "data-drop-active": isDrop ? "true" : "false",
    onContextmenu(e: MouseEvent) {
      e.preventDefault()
      e.stopPropagation()
      ctxMenuKey.value = key
      ctxMenuX.value = e.clientX
      ctxMenuY.value = e.clientY
      ctxMenuShow.value = false
      requestAnimationFrame(() => (ctxMenuShow.value = true))
    },
    onDblclick() {
      const node = findNode(key)
      if (!node) return
      if (node.type === "host") emit("open-host", node, true)
      else toggleExpand(node.key)
    },
    onPointerdown(e: PointerEvent) {
      if (e.button !== 0) return
      const node = findNode(key)
      if (!node || node.type !== "host") return
      beginHostDrag(e, node)
    },
  }
}

function toggleExpand(key: string) {
  const set = new Set(expandedKeys.value)
  if (set.has(key)) set.delete(key)
  else set.add(key)
  expandedKeys.value = Array.from(set)
}

function renderMenuIcon(comp: unknown) {
  return () => h(NIcon, null, { default: () => h(comp as never) })
}

const ctxMenuOptions = computed<DropdownOption[]>(() => {
  const node = ctxMenuKey.value ? findNode(ctxMenuKey.value) : null
  const opts: DropdownOption[] = []
  if (!node) {
    // 空白处右键：根级操作
    opts.push(
      {
        label: t("hosts.ctxMenu.newHost"),
        key: "new-host",
        icon: renderMenuIcon(ServerOutline),
      },
      {
        label: t("hosts.tree.newFolder"),
        key: "new-folder",
        icon: renderMenuIcon(FolderAddOutlined),
      },
      { type: "divider", key: "d-blank-1" },
      {
        label: t("hosts.tree.importSshConfig"),
        key: "import-ssh",
        icon: renderMenuIcon(DownloadOutline),
      },
      {
        label: t("hosts.tree.refresh"),
        key: "refresh",
        icon: renderMenuIcon(RefreshOutline),
      },
    )
    return opts
  }
  if (node.type === "folder") {
    opts.push(
      {
        label: t("hosts.ctxMenu.newSubFolder"),
        key: "new-folder",
        icon: renderMenuIcon(FolderAddOutlined),
      },
      {
        label: t("hosts.ctxMenu.newHost"),
        key: "new-host",
        icon: renderMenuIcon(ServerOutline),
      },
      { type: "divider", key: "d1" },
      {
        label: t("hosts.ctxMenu.rename"),
        key: "rename",
        icon: renderMenuIcon(CreateOutline),
      },
      {
        label: t("hosts.ctxMenu.delete"),
        key: "delete",
        icon: renderMenuIcon(TrashOutline),
      },
    )
  } else {
    opts.push(
      {
        label: t("hosts.ctxMenu.openTerminal"),
        key: "open",
        icon: renderMenuIcon(TerminalOutline),
      },
      {
        label: t("hosts.ctxMenu.newSession"),
        key: "open-new",
        icon: renderMenuIcon(AddOutline),
      },
      { type: "divider", key: "d0" },
      { label: t("hosts.ctxMenu.edit"), key: "edit", icon: renderMenuIcon(CreateOutline) },
      { label: t("hosts.ctxMenu.copy"), key: "copy", icon: renderMenuIcon(CopyOutline) },
      {
        label: t("hosts.ctxMenu.delete"),
        key: "delete",
        icon: renderMenuIcon(TrashOutline),
      },
    )
  }
  return opts
})

function onCtxSelect(key: string) {
  ctxMenuShow.value = false
  const targetKey = ctxMenuKey.value
  const node = targetKey ? findNode(targetKey) : null
  switch (key) {
    case "new-folder":
      openCreateFolder(node && node.type === "folder" ? node.id : 0)
      break
    case "new-host":
      emit("create-host", node && node.type === "folder" ? node.id : 0)
      break
    case "refresh":
      void onRefresh()
      break
    case "import-ssh":
      importModalShow.value = true
      break
    case "open":
      if (node?.type === "host") emit("open-host", node)
      break
    case "open-new":
      if (node?.type === "host") emit("open-host", node, true)
      break
    case "edit":
      if (node?.type === "host") openEditHost(node)
      break
    case "copy":
      if (node?.type === "host") copyHost(node)
      break
    case "rename":
      if (node) openRename(node)
      break
    case "delete":
      if (node) confirmDelete(node)
      break
  }
}

const folderModalOpen = ref(false)
const folderName = ref("")
const folderParentGid = ref<number>(0)
const folderSubmitting = ref(false)

function openCreateFolder(parentGid: number) {
  folderParentGid.value = parentGid
  folderName.value = ""
  folderModalOpen.value = true
}

async function submitFolder() {
  const name = folderName.value.trim()
  if (!name) {
    message.warning(t("hosts.message.folderNameRequired"))
    return
  }
  folderSubmitting.value = true
  try {
    const g = await store.addGroup({
      parent_id: folderParentGid.value,
      name,
    })
    folderModalOpen.value = false
    message.success(t("hosts.message.folderCreated", { name }))
    await nextTick()
    revealKey(`folder-${g.id}`)
  } catch (e) {
    message.error(t("hosts.message.createFailed", { error: String(e) }))
  } finally {
    folderSubmitting.value = false
  }
}

const renameModalOpen = ref(false)
const renameValue = ref("")
const renameTarget = ref<HostNode | null>(null)
const renameSubmitting = ref(false)

function openRename(node: HostNode) {
  if (node.type === "host") {
    const host = store.findHost(node.id)
    if (host) emit("edit-host", host as Host)
    return
  }
  renameTarget.value = node
  renameValue.value = node.label
  renameModalOpen.value = true
}

async function submitRename() {
  const target = renameTarget.value
  if (!target) return
  const name = renameValue.value.trim()
  if (!name) {
    message.warning(t("hosts.message.nameRequired"))
    return
  }
  renameSubmitting.value = true
  try {
    await store.editGroup(target.id, { name })
    renameModalOpen.value = false
    message.success(t("hosts.message.renamed"))
  } catch (e) {
    message.error(t("hosts.message.renameFailed", { error: String(e) }))
  } finally {
    renameSubmitting.value = false
  }
}

function openEditHost(node: HostNode) {
  const host = store.findHost(node.id)
  if (!host) {
    message.error(t("hosts.message.hostNotFound"))
    return
  }
  emit("edit-host", host as Host)
}

async function copyHost(node: HostNode) {
  const host = store.findHost(node.id)
  if (!host) return
  try {
    await store.addHost({
      gid: host.gid,
      name: t("hosts.message.copiedName", { name: host.name }),
      addr: host.addr,
      port: host.port,
      username: host.username,
      icon: host.icon ?? null,
      color: host.color ?? null,
      desc: host.desc ?? null,
    })
    message.success(t("hosts.message.copied"))
  } catch (e) {
    message.error(t("hosts.message.copyFailed", { error: String(e) }))
  }
}

function confirmDelete(node: HostNode) {
  const tip =
    node.type === "folder"
      ? t("hosts.message.deleteFolderConfirm", { name: node.label })
      : t("hosts.message.deleteHostConfirm", { name: node.label })
  dialog.warning({
    title: t("hosts.message.deleteTitle"),
    content: tip,
    positiveText: t("common.delete"),
    negativeText: t("common.cancel"),
    onPositiveClick: async () => {
      try {
        if (node.type === "folder") await store.removeGroup(node.id)
        else await store.removeHost(node.id)
        message.success(t("hosts.message.deleted"))
      } catch (e) {
        message.error(t("hosts.message.deleteFailed", { error: String(e) }))
      }
    },
  })
}

function revealKey(key: string) {
  const ancestors = collectAncestorKeys(key)
  const set = new Set(expandedKeys.value)
  for (const a of ancestors) set.add(a)
  expandedKeys.value = Array.from(set)
  selectedKeys.value = [key]
}

function newFolderAtSelection() {
  openCreateFolder(resolveSelectedFolderGid())
}

function newHostAtSelection() {
  emit("create-host", resolveSelectedFolderGid())
}

async function onRefresh() {
  try {
    await store.refresh()
    message.success(t("hosts.message.refreshed"))
  } catch (e) {
    message.error(t("hosts.message.refreshFailed", { error: String(e) }))
  }
}
</script>

<template>
  <div class="host-tree">
    <div class="tree-header">
      <span class="tree-title">{{ t("hosts.tree.title") }}</span>
      <NSpace :size="4">
        <NTooltip>
          <template #trigger>
            <NButton
              size="small"
              quaternary
              circle
              :loading="store.loading"
              @click="onRefresh"
            >
              <template #icon>
                <NIcon><RefreshOutline /></NIcon>
              </template>
            </NButton>
          </template>
          {{ t("hosts.tree.refresh") }}
        </NTooltip>
        <NTooltip>
          <template #trigger>
            <NButton
              size="small"
              quaternary
              circle
              @click="newFolderAtSelection"
            >
              <template #icon>
                <NIcon><FolderAddOutlined /></NIcon>
              </template>
            </NButton>
          </template>
          {{ t("hosts.tree.newFolder") }}
        </NTooltip>
        <NTooltip>
          <template #trigger>
            <NButton
              size="small"
              quaternary
              circle
              @click="importModalShow = true"
            >
              <template #icon>
                <NIcon><DownloadOutline /></NIcon>
              </template>
            </NButton>
          </template>
          {{ t("hosts.tree.importSshConfig") }}
        </NTooltip>
        <NButton size="small" type="primary" @click="newHostAtSelection">
          <template #icon>
            <NIcon><AddOutline /></NIcon>
          </template>
          {{ t("hosts.tree.newHost") }}
        </NButton>
        <NButton size="small" quaternary circle @click="emit('close')">
          <template #icon>
            <NIcon><CloseOutline /></NIcon>
          </template>
        </NButton>
      </NSpace>
    </div>

    <div class="tree-search">
      <NInput
        v-model:value="filter"
        size="small"
        :placeholder="t('hosts.tree.searchPlaceholder')"
        clearable
      >
        <template #prefix>
          <NIcon><SearchOutline /></NIcon>
        </template>
      </NInput>
    </div>

    <div
      ref="treeBodyEl"
      class="tree-body"
      :class="{ 'drop-on-root': dropTargetKey === '__root__' }"
      :data-drop-key="dropTargetKey ?? ''"
      @contextmenu="onBlankContextMenu"
    >
      <NSpin :show="store.loading" class="tree-spin">
        <NEmpty
          v-if="!store.loading && treeData.length === 0"
          :description="t('hosts.tree.empty')"
        />
        <NTree
          v-else
          :data="treeData"
          :pattern="filter"
          block-line
          expand-on-click
          :selected-keys="selectedKeys"
          :expanded-keys="expandedKeys"
          :render-prefix="renderPrefix"
          :node-props="nodeProps"
          :selectable="true"
          key-field="key"
          label-field="label"
          children-field="children"
          @update:selected-keys="(k: string[]) => (selectedKeys = k)"
          @update:expanded-keys="(k: string[]) => (expandedKeys = k)"
        />
      </NSpin>
    </div>

    <NDropdown
      placement="bottom-start"
      trigger="manual"
      :x="ctxMenuX"
      :y="ctxMenuY"
      :options="ctxMenuOptions"
      :show="ctxMenuShow"
      @select="onCtxSelect"
      @clickoutside="ctxMenuShow = false"
    />

    <!-- 拖拽 ghost：跟随鼠标显示主机名 -->
    <Teleport to="body">
      <div
        v-if="dragGhost"
        class="drag-ghost"
        :style="{ left: dragGhost.x + 12 + 'px', top: dragGhost.y + 12 + 'px' }"
      >
        <NIcon :size="14" color="#7c5cff"><TerminalOutline /></NIcon>
        <span>{{ dragGhost.label }}</span>
      </div>
    </Teleport>

    <NModal v-model:show="folderModalOpen">
      <NCard
        style="width: 420px"
        :title="t('hosts.tree.newFolderDialog')"
        size="small"
        :bordered="false"
        role="dialog"
        aria-modal="true"
      >
        <NForm
          label-placement="top"
          require-mark-placement="right-hanging"
          size="small"
        >
          <NFormItem :label="t('hosts.tree.parentDir')" :show-feedback="false">
            <NTreeSelect
              v-model:value="folderParentGid"
              :options="folderSelectOptions"
              key-field="key"
              label-field="label"
              children-field="children"
              default-expand-all
              :consistent-menu-width="false"
              :placeholder="t('hosts.tree.parentDirPlaceholder')"
            />
          </NFormItem>
          <div style="margin-top: 12px">
            <NFormItem :label="t('hosts.tree.name')" :show-feedback="false">
              <NInput
                v-model:value="folderName"
                :placeholder="t('hosts.tree.namePlaceholder')"
                autofocus
                @keydown.enter="submitFolder"
              />
            </NFormItem>
          </div>
        </NForm>
        <template #footer>
          <NSpace justify="end">
            <NButton :disabled="folderSubmitting" @click="folderModalOpen = false">
              {{ t("hosts.tree.cancel") }}
            </NButton>
            <NButton
              type="primary"
              :loading="folderSubmitting"
              @click="submitFolder"
            >
              {{ t("hosts.tree.create") }}
            </NButton>
          </NSpace>
        </template>
      </NCard>
    </NModal>

    <NModal v-model:show="renameModalOpen">
      <NCard
        style="width: 380px"
        :title="t('hosts.tree.renameDialog')"
        size="small"
        :bordered="false"
      >
        <NInput
          v-model:value="renameValue"
          autofocus
          @keydown.enter="submitRename"
        />
        <template #footer>
          <NSpace justify="end">
            <NButton :disabled="renameSubmitting" @click="renameModalOpen = false">
              {{ t("hosts.tree.cancel") }}
            </NButton>
            <NButton
              type="primary"
              :loading="renameSubmitting"
              @click="submitRename"
            >
              {{ t("hosts.tree.save") }}
            </NButton>
          </NSpace>
        </template>
      </NCard>
    </NModal>

    <SshConfigImportModal v-model:show="importModalShow" />
  </div>
</template>

<style scoped>
.host-tree {
  display: flex;
  flex-direction: column;
  height: 100%;
  padding: 12px;
  gap: 10px;
}

.tree-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.tree-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--ashell-text-subtle);
}

.tree-search {
  flex-shrink: 0;
}

.tree-body {
  flex: 1;
  overflow: auto;
  margin: 0 -8px;
  padding: 0 4px;
  border-radius: 6px;
  transition: background 0.12s ease, box-shadow 0.12s ease;
}

.tree-body.drop-on-root {
  background: rgba(124, 92, 255, 0.06);
  box-shadow: inset 0 0 0 1px rgba(124, 92, 255, 0.45);
}

.tree-spin {
  height: 100%;
}

:deep(.tree-spin .n-spin-content) {
  height: 100%;
}

:deep(.n-tree-node-content) {
  font-size: 13px;
  border-radius: 6px;
}

:deep(.n-tree-node--selected .n-tree-node-content) {
  background: rgba(124, 92, 255, 0.15) !important;
}

/* 拖拽中：drop 目标 folder 高亮 */
.tree-body[data-drop-key] :deep([data-node-key]) {
  transition: background 0.12s ease, box-shadow 0.12s ease;
}
.tree-body[data-drop-key=""] :deep([data-node-key]) {
  background: transparent;
}
</style>

<style>
/* drop target 高亮（穿透 scoped，因为 data-node-key 是动态加在内部节点上） */
.host-tree .tree-body [data-node-key].drop-target,
.host-tree .tree-body [data-node-key][data-drop-active="true"] {
  outline: 2px solid rgba(124, 92, 255, 0.55);
  outline-offset: -2px;
  border-radius: 6px;
}

.drag-ghost {
  position: fixed;
  z-index: 9999;
  pointer-events: none;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 4px 10px;
  background: var(--ashell-panel-bg, #1f1f24);
  color: var(--ashell-text, #e5e5ea);
  border: 1px solid rgba(124, 92, 255, 0.5);
  border-radius: 6px;
  font-size: 12px;
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.35);
  user-select: none;
  white-space: nowrap;
}
</style>
