<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, shallowRef, watch } from "vue"
import {
  NButton,
  NIcon,
  NModal,
  NSpin,
  useDialog,
  useMessage,
} from "naive-ui"
import { CloseOutline, ContractOutline, ExpandOutline, SaveOutline } from "@vicons/ionicons5"
import { EditorView, keymap } from "@codemirror/view"
import { Compartment, EditorState } from "@codemirror/state"
import type { Extension } from "@codemirror/state"
import { highlightSelectionMatches, search } from "@codemirror/search"
import { basicSetup } from "codemirror"
import { javascript } from "@codemirror/lang-javascript"
import { python } from "@codemirror/lang-python"
import { json } from "@codemirror/lang-json"
import { html } from "@codemirror/lang-html"
import { css } from "@codemirror/lang-css"
import { markdown } from "@codemirror/lang-markdown"
import { xml } from "@codemirror/lang-xml"
import { sql } from "@codemirror/lang-sql"
import { rust } from "@codemirror/lang-rust"
import { cpp } from "@codemirror/lang-cpp"
import { java } from "@codemirror/lang-java"
import { php } from "@codemirror/lang-php"
import { oneDark } from "@codemirror/theme-one-dark"
import { StreamLanguage } from "@codemirror/language"
import { shell as shellMode } from "@codemirror/legacy-modes/mode/shell"
import { yaml as yamlMode } from "@codemirror/legacy-modes/mode/yaml"
import { dockerFile as dockerMode } from "@codemirror/legacy-modes/mode/dockerfile"
import { go as goMode } from "@codemirror/legacy-modes/mode/go"
import { ruby as rubyMode } from "@codemirror/legacy-modes/mode/ruby"
import { toml as tomlMode } from "@codemirror/legacy-modes/mode/toml"
import { properties as propertiesMode } from "@codemirror/legacy-modes/mode/properties"
import { lua as luaMode } from "@codemirror/legacy-modes/mode/lua"
import { diff as diffMode } from "@codemirror/legacy-modes/mode/diff"
import { nginx as nginxMode } from "@codemirror/legacy-modes/mode/nginx"
import { powerShell as psMode } from "@codemirror/legacy-modes/mode/powershell"
import { useI18n } from "vue-i18n"
import { readText, writeText } from "@/api/sftp"
import type { SftpFile } from "@/types"
import { humanSize } from "@/utils/humanSize"

interface Props {
  open: boolean
  sid: string | null
  file: SftpFile | null
}

const props = defineProps<Props>()
const emit = defineEmits<{
  "update:open": [value: boolean]
  saved: [path: string]
}>()

const { t, locale } = useI18n()
const message = useMessage()
const dialog = useDialog()

const MAX_FILE_SIZE = 2 * 1024 * 1024

const BINARY_EXTS = new Set([
  "png", "jpg", "jpeg", "gif", "bmp", "ico", "webp", "tiff", "tif",
  "pdf", "zip", "gz", "tar", "bz2", "7z", "rar", "xz", "tgz",
  "exe", "dll", "so", "dylib", "bin", "dat", "db", "sqlite", "sqlite3",
  "class", "jar", "war", "pyc", "o", "a", "lib", "wasm",
  "mp3", "mp4", "avi", "mkv", "wmv", "flv", "wav", "ogg", "flac", "aac", "mov",
  "eot", "ttf", "otf", "woff", "woff2",
])

const loading = ref(false)
const saving = ref(false)
const dirty = ref(false)
const readOnly = ref(false)
const maximized = ref(false)
const errorMsg = ref("")
const editorHost = ref<HTMLElement | null>(null)
const view = shallowRef<EditorView | null>(null)

const themeCompartment = new Compartment()
const readOnlyCompartment = new Compartment()
const fontSizeCompartment = new Compartment()
const wrapCompartment = new Compartment()

const FONT_SIZE_KEY = "ashell:editor-font-size"
const WORD_WRAP_KEY = "ashell:editor-word-wrap"
const FONT_SIZE_MIN = 10
const FONT_SIZE_MAX = 24
const FONT_SIZE_DEFAULT = 13

function loadFontSize(): number {
  const raw = Number(localStorage.getItem(FONT_SIZE_KEY))
  if (Number.isFinite(raw) && raw >= FONT_SIZE_MIN && raw <= FONT_SIZE_MAX)
    return raw
  return FONT_SIZE_DEFAULT
}

const fontSize = ref(loadFontSize())
const wordWrap = ref(localStorage.getItem(WORD_WRAP_KEY) !== "false")
const cursorLine = ref(1)
const cursorCol = ref(1)
const totalLines = ref(1)

function isDarkTheme(): boolean {
  return document.documentElement.dataset.ashellTheme === "dark"
}

function getLanguage(filename: string): Extension[] {
  const ext = filename.split(".").pop()?.toLowerCase() ?? ""
  const base = filename.split("/").pop()?.toLowerCase() ?? ""
  if (base === "dockerfile" || base.startsWith("dockerfile."))
    return [StreamLanguage.define(dockerMode)]
  switch (ext) {
    case "js": case "mjs": case "cjs":
      return [javascript()]
    case "jsx":
      return [javascript({ jsx: true })]
    case "ts":
      return [javascript({ typescript: true })]
    case "tsx":
      return [javascript({ typescript: true, jsx: true })]
    case "py": case "pyw":
      return [python()]
    case "json":
      return [json()]
    case "html": case "htm": case "xhtml":
      return [html()]
    case "css": case "scss": case "sass": case "less":
      return [css()]
    case "md": case "markdown":
      return [markdown()]
    case "xml": case "svg": case "rss": case "atom":
      return [xml()]
    case "sql":
      return [sql()]
    case "rs":
      return [rust()]
    case "c": case "h": case "cpp": case "cc": case "cxx": case "hpp": case "hxx":
      return [cpp()]
    case "java":
      return [java()]
    case "php": case "phtml":
      return [php()]
    case "sh": case "bash": case "zsh": case "ksh":
      return [StreamLanguage.define(shellMode)]
    case "yml": case "yaml":
      return [StreamLanguage.define(yamlMode)]
    case "toml":
      return [StreamLanguage.define(tomlMode)]
    case "go":
      return [StreamLanguage.define(goMode)]
    case "rb":
      return [StreamLanguage.define(rubyMode)]
    case "ini": case "cfg": case "conf": case "properties":
      return [StreamLanguage.define(propertiesMode)]
    case "lua":
      return [StreamLanguage.define(luaMode)]
    case "diff": case "patch":
      return [StreamLanguage.define(diffMode)]
    case "ps1": case "psm1":
      return [StreamLanguage.define(psMode)]
    default:
      if (base === "nginx.conf" || base.endsWith(".nginx"))
        return [StreamLanguage.define(nginxMode)]
      return []
  }
}

function isBinaryFile(filename: string): boolean {
  const ext = filename.split(".").pop()?.toLowerCase() ?? ""
  return BINARY_EXTS.has(ext)
}

function checkReadOnly(file: SftpFile): boolean {
  const perm = file.permissions || ""
  if (perm.length < 9) return false
  return perm.charAt(1) !== "w" && perm.charAt(4) !== "w" && perm.charAt(7) !== "w"
}

function getThemeExt(): Extension {
  return isDarkTheme() ? oneDark : []
}

function fontSizeExt(size: number): Extension {
  return EditorView.theme({ "&": { fontSize: `${size}px` } })
}

const zhSearchPhrases = {
  Find: "查找",
  Replace: "替换",
  next: "下一个",
  previous: "上一个",
  all: "全部",
  "match case": "区分大小写",
  regexp: "正则",
  "by word": "全词匹配",
  replace: "替换",
  "replace all": "全部替换",
  close: "关闭",
  "current match": "当前匹配",
  "on line": "所在行",
  "replaced $ matches": "已替换 $ 处匹配",
  "replaced match on line $": "已替换第 $ 行的匹配",
  "Go to line": "跳转到行",
  go: "跳转",
}

function setFontSize(size: number) {
  const clamped = Math.min(
    FONT_SIZE_MAX,
    Math.max(FONT_SIZE_MIN, Math.round(size)),
  )
  if (clamped === fontSize.value) return
  fontSize.value = clamped
  localStorage.setItem(FONT_SIZE_KEY, String(clamped))
  view.value?.dispatch({
    effects: fontSizeCompartment.reconfigure(fontSizeExt(clamped)),
  })
}

function toggleWordWrap() {
  wordWrap.value = !wordWrap.value
  localStorage.setItem(WORD_WRAP_KEY, String(wordWrap.value))
  view.value?.dispatch({
    effects: wrapCompartment.reconfigure(
      wordWrap.value ? EditorView.lineWrapping : [],
    ),
  })
}

function syncCursor(state: EditorState) {
  const head = state.selection.main.head
  const line = state.doc.lineAt(head)
  cursorLine.value = line.number
  cursorCol.value = head - line.from + 1
  totalLines.value = state.doc.lines
}

function createEditor(content: string) {
  if (!editorHost.value) return
  view.value?.destroy()

  const filename = props.file?.file_name ?? ""
  const extensions: Extension[] = [
    basicSetup,
    search(),
    highlightSelectionMatches(),
    ...getLanguage(filename),
    themeCompartment.of(getThemeExt()),
    readOnlyCompartment.of(EditorState.readOnly.of(readOnly.value)),
    fontSizeCompartment.of(fontSizeExt(fontSize.value)),
    wrapCompartment.of(wordWrap.value ? EditorView.lineWrapping : []),
    ...(locale.value === "zh-CN"
      ? [EditorState.phrases.of(zhSearchPhrases)]
      : []),
    keymap.of([
      {
        key: "Mod-s",
        preventDefault: true,
        run: () => {
          if (!readOnly.value) void save()
          return true
        },
      },
      {
        key: "Mod-=",
        preventDefault: true,
        run: () => {
          setFontSize(fontSize.value + 1)
          return true
        },
      },
      {
        key: "Mod-+",
        preventDefault: true,
        run: () => {
          setFontSize(fontSize.value + 1)
          return true
        },
      },
      {
        key: "Mod--",
        preventDefault: true,
        run: () => {
          setFontSize(fontSize.value - 1)
          return true
        },
      },
      {
        key: "Mod-0",
        preventDefault: true,
        run: () => {
          setFontSize(FONT_SIZE_DEFAULT)
          return true
        },
      },
    ]),
    EditorView.updateListener.of((u) => {
      if (u.docChanged) dirty.value = true
      if (u.selectionSet || u.docChanged) syncCursor(u.state)
    }),
  ]

  view.value = new EditorView({
    state: EditorState.create({ doc: content, extensions }),
    parent: editorHost.value,
  })
  syncCursor(view.value.state)
  dirty.value = false
}

function destroyEditor() {
  view.value?.destroy()
  view.value = null
  dirty.value = false
  readOnly.value = false
  errorMsg.value = ""
}

async function loadContent() {
  if (!props.sid || !props.file) return
  const file = props.file

  if (isBinaryFile(file.file_name)) {
    errorMsg.value = t("sftp.editor.binaryFile")
    return
  }
  const size = file.size_bytes
  if (typeof size === "number" && size > MAX_FILE_SIZE) {
    errorMsg.value = t("sftp.editor.fileTooLarge", {
      size: humanSize(size),
      max: humanSize(MAX_FILE_SIZE),
    })
    return
  }

  readOnly.value = checkReadOnly(file)
  loading.value = true
  errorMsg.value = ""
  try {
    const content = await readText(props.sid, file.full_path)
    if (content.includes("\0")) {
      errorMsg.value = t("sftp.editor.binaryFile")
      return
    }
    await nextTick()
    if (!props.open) return
    createEditor(content)
  } catch (e) {
    errorMsg.value = (e as Error).message
  } finally {
    loading.value = false
  }
}

async function save() {
  if (!props.sid || !props.file || !view.value || !dirty.value || saving.value)
    return
  saving.value = true
  try {
    const content = view.value.state.doc.toString()
    await writeText(props.sid, props.file.full_path, content)
    dirty.value = false
    message.success(t("sftp.editor.saved"))
    emit("saved", props.file.full_path)
  } catch (e) {
    const msg = (e as Error).message.toLowerCase()
    if (msg.includes("permission") || msg.includes("denied")) {
      readOnly.value = true
      dirty.value = false
      if (view.value) {
        view.value.dispatch({
          effects: readOnlyCompartment.reconfigure(EditorState.readOnly.of(true)),
        })
      }
      message.error(t("sftp.editor.permissionDenied"))
    } else {
      message.error(
        t("sftp.editor.saveFailed", { error: (e as Error).message }),
      )
    }
  } finally {
    saving.value = false
  }
}

function onModalShowUpdate(v: boolean) {
  if (!v) requestClose()
  else emit("update:open", v)
}

function requestClose() {
  if (dirty.value) {
    dialog.warning({
      title: t("sftp.editor.unsavedTitle"),
      content: t("sftp.editor.unsavedConfirm"),
      positiveText: t("sftp.editor.discard"),
      negativeText: t("common.cancel"),
      onPositiveClick: () => emit("update:open", false),
    })
  } else {
    emit("update:open", false)
  }
}

let themeObserver: MutationObserver | null = null

watch(
  () => props.open,
  (open) => {
    if (open) {
      void loadContent()
      themeObserver = new MutationObserver(() => {
        if (view.value) {
          view.value.dispatch({
            effects: themeCompartment.reconfigure(getThemeExt()),
          })
        }
      })
      themeObserver.observe(document.documentElement, {
        attributes: true,
        attributeFilter: ["data-ashell-theme"],
      })
    } else {
      destroyEditor()
      themeObserver?.disconnect()
      themeObserver = null
    }
  },
)

watch(
  () => props.file?.full_path,
  (path, prev) => {
    if (props.open && path && path !== prev) void loadContent()
  },
)

onBeforeUnmount(() => {
  destroyEditor()
  themeObserver?.disconnect()
})

const editorTitle = computed(() => {
  if (!props.file) return t("sftp.editor.title")
  const name = props.file.file_name
  if (readOnly.value) return `${name} (${t("sftp.editor.readOnly")})`
  return dirty.value ? `${name} •` : name
})

const modalStyle = computed(() =>
  maximized.value
    ? {
        width: "100vw",
        maxWidth: "none",
        height: "100vh",
        borderRadius: "0",
        display: "flex",
        flexDirection: "column" as const,
      }
    : {
        width: "80vw",
        maxWidth: "1200px",
        height: "80vh",
        display: "flex",
        flexDirection: "column" as const,
      },
)

const canSave = computed(
  () =>
    dirty.value &&
    !loading.value &&
    !errorMsg.value &&
    !saving.value &&
    !readOnly.value,
)
</script>

<template>
  <NModal
    :show="props.open"
    preset="card"
    :title="editorTitle"
    :bordered="false"
    :closable="false"
    size="small"
    :header-style="{ padding: '8px 12px' }"
    :style="modalStyle"
    :content-style="{
      padding: '0',
      flex: '1 1 0',
      minHeight: '0',
      overflow: 'hidden',
      position: 'relative',
    }"
    :mask-closable="false"
    @update:show="onModalShowUpdate"
  >
    <template #header-extra>
      <div class="editor-header-actions">
        <NButton
          size="small"
          quaternary
          circle
          :type="canSave ? 'primary' : 'default'"
          :loading="saving"
          :disabled="!canSave"
          :title="`${t('common.save')} (Ctrl+S)`"
          @click="save"
        >
          <template #icon>
            <NIcon><SaveOutline /></NIcon>
          </template>
        </NButton>
        <NButton
          size="small"
          quaternary
          circle
          :title="maximized ? t('sftp.editor.restore') : t('sftp.editor.maximize')"
          @click="maximized = !maximized"
        >
          <template #icon>
            <NIcon>
              <ContractOutline v-if="maximized" />
              <ExpandOutline v-else />
            </NIcon>
          </template>
        </NButton>
        <NButton
          size="small"
          quaternary
          circle
          :title="t('common.close')"
          @click="requestClose"
        >
          <template #icon>
            <NIcon><CloseOutline /></NIcon>
          </template>
        </NButton>
      </div>
    </template>

    <div class="editor-body">
      <div ref="editorHost" class="editor-host" />
      <div v-if="loading" class="editor-overlay">
        <NSpin size="large" />
      </div>
      <div v-else-if="errorMsg" class="editor-overlay editor-error-wrap">
        <span class="editor-error-text">{{ errorMsg }}</span>
      </div>
      <div v-if="!loading && !errorMsg" class="editor-statusbar">
        <span class="status-text">
          {{ t("sftp.editor.lineCol", { line: cursorLine, col: cursorCol }) }}
        </span>
        <span class="status-text">
          {{ t("sftp.editor.totalLines", { count: totalLines }) }}
        </span>
        <span class="status-spacer" />
        <button
          type="button"
          class="status-btn"
          :class="{ active: wordWrap }"
          @click="toggleWordWrap"
        >
          {{ t("sftp.editor.wordWrap") }}
        </button>
        <button
          type="button"
          class="status-btn status-btn-mono"
          :title="`${t('sftp.editor.fontSizeDecrease')} (Ctrl+-)`"
          @click="setFontSize(fontSize - 1)"
        >
          A-
        </button>
        <span class="status-text">{{ fontSize }}px</span>
        <button
          type="button"
          class="status-btn status-btn-mono"
          :title="`${t('sftp.editor.fontSizeIncrease')} (Ctrl+=)`"
          @click="setFontSize(fontSize + 1)"
        >
          A+
        </button>
      </div>
    </div>
  </NModal>
</template>

<style scoped>
.editor-header-actions {
  display: flex;
  align-items: center;
  gap: 4px;
}

.editor-body {
  position: relative;
  height: 100%;
  min-height: 0;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.editor-host {
  flex: 1 1 0;
  min-height: 0;
}

.editor-host :deep(.cm-editor) {
  height: 100%;
}

.editor-host :deep(.cm-editor.cm-focused) {
  outline: none;
}

.editor-host :deep(.cm-scroller) {
  overflow: auto;
}

.editor-host :deep(.cm-content),
.editor-host :deep(.cm-gutters) {
  font-family: "Fira Code", Consolas, "Courier New", monospace;
}

/* 查找替换面板 */
.editor-host :deep(.cm-panels) {
  background: var(--ashell-panel-bg-soft);
  color: var(--ashell-text);
}

.editor-host :deep(.cm-panels.cm-panels-top) {
  border-bottom: 1px solid var(--ashell-border);
}

.editor-host :deep(.cm-panel.cm-search) {
  padding: 6px 30px 6px 10px;
  font-size: 12px;
  line-height: 1.9;
}

.editor-host :deep(.cm-panel.cm-search input),
.editor-host :deep(.cm-panel.cm-search button),
.editor-host :deep(.cm-panel.cm-search label) {
  margin: 2px 6px 2px 0;
}

.editor-host :deep(.cm-panel.cm-search input.cm-textfield) {
  background: var(--ashell-bg);
  color: var(--ashell-text);
  border: 1px solid var(--ashell-border);
  border-radius: 4px;
  padding: 3px 8px;
  font-size: 12px;
  font-family: inherit;
  outline: none;
  min-width: 180px;
}

.editor-host :deep(.cm-panel.cm-search input.cm-textfield:focus) {
  border-color: var(--ashell-primary);
  box-shadow: 0 0 0 2px rgba(124, 92, 255, 0.18);
}

.editor-host :deep(.cm-panel.cm-search button.cm-button) {
  background: var(--ashell-hover);
  background-image: none;
  color: var(--ashell-text);
  border: 1px solid var(--ashell-border);
  border-radius: 4px;
  padding: 3px 8px;
  font-size: 12px;
  font-family: inherit;
  cursor: pointer;
}

.editor-host :deep(.cm-panel.cm-search button.cm-button:hover) {
  background: var(--ashell-active);
}

.editor-host :deep(.cm-panel.cm-search label) {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 12px;
  color: var(--ashell-text-muted);
  white-space: nowrap;
  cursor: pointer;
}

.editor-host :deep(.cm-panel.cm-search input[type="checkbox"]) {
  accent-color: var(--ashell-primary);
  margin: 0;
}

.editor-host :deep(.cm-panel.cm-search button[name="close"]) {
  top: 4px;
  right: 8px;
  color: var(--ashell-text-muted);
  font-size: 15px;
  line-height: 1;
  padding: 2px 4px;
  border-radius: 4px;
  cursor: pointer;
}

.editor-host :deep(.cm-panel.cm-search button[name="close"]:hover) {
  color: var(--ashell-text-strong);
  background: var(--ashell-hover);
}

/* 搜索/选中匹配高亮 */
.editor-host :deep(.cm-editor .cm-searchMatch) {
  background: rgba(124, 92, 255, 0.28);
  outline: 1px solid rgba(124, 92, 255, 0.35);
  border-radius: 2px;
}

.editor-host :deep(.cm-editor .cm-searchMatch-selected) {
  background: rgba(255, 158, 64, 0.5);
  outline: 1px solid rgba(255, 158, 64, 0.65);
}

.editor-host :deep(.cm-editor .cm-selectionMatch) {
  background: rgba(124, 92, 255, 0.18);
  border-radius: 2px;
}

/* 状态栏 */
.editor-statusbar {
  display: flex;
  align-items: center;
  gap: 10px;
  height: 26px;
  padding: 0 10px;
  flex-shrink: 0;
  border-top: 1px solid var(--ashell-border-soft);
  background: var(--ashell-panel-bg-soft);
  font-size: 11px;
  color: var(--ashell-text-muted);
  user-select: none;
}

.status-text {
  white-space: nowrap;
}

.status-spacer {
  flex: 1;
}

.status-btn {
  border: none;
  background: transparent;
  color: var(--ashell-text-muted);
  font: inherit;
  padding: 2px 6px;
  border-radius: 4px;
  cursor: pointer;
  white-space: nowrap;
}

.status-btn:hover {
  background: var(--ashell-hover);
  color: var(--ashell-text);
}

.status-btn.active {
  background: var(--ashell-active);
  color: var(--ashell-primary);
}

.status-btn-mono {
  font-weight: 600;
}

.editor-overlay {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--ashell-panel-bg, rgba(30, 30, 30, 0.95));
  z-index: 1;
}

.editor-error-wrap {
  flex-direction: column;
  gap: 12px;
  padding: 24px;
}

.editor-error-text {
  color: var(--ashell-text-strong, #e0e0e0);
  font-size: 14px;
  text-align: center;
  word-break: break-word;
}
</style>
