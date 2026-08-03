<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue"
import { NButton, NIcon, NModal, NSpin } from "naive-ui"
import {
  CloseOutline,
  ContractOutline,
  DownloadOutline,
  ExpandOutline,
  MusicalNotesOutline,
} from "@vicons/ionicons5"
import { downloadStream, isAbortError } from "@/api/sftp"
import type { SftpFile } from "@/types"
import { useI18n } from "vue-i18n"
import { humanSize } from "@/utils/humanSize"
import { getPreviewType, getMimeType } from "@/utils/fileType"
import type { PreviewType } from "@/utils/fileType"

interface Props {
  open: boolean
  sid: string | null
  file: SftpFile | null
}

const props = defineProps<Props>()
const emit = defineEmits<{
  "update:open": [value: boolean]
  download: [file: SftpFile]
}>()

const { t } = useI18n()

const MAX_PREVIEW_SIZE: Record<PreviewType, number> = {
  image: 50 * 1024 * 1024,
  video: 200 * 1024 * 1024,
  audio: 100 * 1024 * 1024,
  pdf: 100 * 1024 * 1024,
}

const loading = ref(false)
const errorMsg = ref("")
const objectUrl = ref<string | null>(null)
const previewType = ref<PreviewType | null>(null)
const maximized = ref(false)

const mediaWidth = ref(0)
const mediaHeight = ref(0)
const mediaDuration = ref(0)

const zoom = ref(1)
const panX = ref(0)
const panY = ref(0)
const dragging = ref(false)
const dragStartX = ref(0)
const dragStartY = ref(0)

let abortCtrl: AbortController | null = null

function cleanup() {
  if (objectUrl.value) {
    URL.revokeObjectURL(objectUrl.value)
    objectUrl.value = null
  }
  previewType.value = null
  errorMsg.value = ""
  mediaWidth.value = 0
  mediaHeight.value = 0
  mediaDuration.value = 0
  zoom.value = 1
  panX.value = 0
  panY.value = 0
  dragging.value = false
  window.removeEventListener("mousemove", onWindowMouseMove)
  window.removeEventListener("mouseup", onWindowMouseUp)
}

async function loadPreview() {
  if (!props.sid || !props.file) return
  const file = props.file
  const type = getPreviewType(file.file_name)
  if (!type) return

  const sizeLimit = MAX_PREVIEW_SIZE[type]
  if (typeof file.size_bytes === "number" && file.size_bytes > sizeLimit) {
    errorMsg.value = t("sftp.preview.fileTooLarge", {
      size: humanSize(file.size_bytes),
      max: humanSize(sizeLimit),
    })
    return
  }

  cleanup()
  loading.value = true
  abortCtrl = new AbortController()
  try {
    const { blob } = await downloadStream(props.sid, file.full_path, {
      signal: abortCtrl.signal,
    })
    const mime = getMimeType(file.file_name)
    const typedBlob = new Blob([blob], { type: mime })
    objectUrl.value = URL.createObjectURL(typedBlob)
    previewType.value = type
  } catch (e) {
    if (!isAbortError(e)) {
      errorMsg.value = (e as Error).message
    }
  } finally {
    loading.value = false
  }
}

function onMediaError() {
  errorMsg.value = t("sftp.preview.unsupportedFormat")
}

function formatDuration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return ""
  const h = Math.floor(seconds / 3600)
  const m = Math.floor((seconds % 3600) / 60)
  const s = Math.floor(seconds % 60)
  const pad = (n: number) => String(n).padStart(2, "0")
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${pad(m)}:${pad(s)}`
}

function onImageLoad(e: Event) {
  const img = e.target as HTMLImageElement
  mediaWidth.value = img.naturalWidth
  mediaHeight.value = img.naturalHeight
}

function onMediaLoadedMetadata(e: Event) {
  const el = e.target as HTMLMediaElement
  mediaDuration.value = el.duration
  if (el instanceof HTMLVideoElement) {
    mediaWidth.value = el.videoWidth
    mediaHeight.value = el.videoHeight
  }
}

const metaItems = computed<string[]>(() => {
  const items: string[] = []
  const file = props.file
  if (!file) return items
  if (mediaWidth.value && mediaHeight.value) {
    items.push(`${mediaWidth.value}×${mediaHeight.value}`)
  }
  if (mediaDuration.value > 0) {
    items.push(formatDuration(mediaDuration.value))
  }
  if (typeof file.size_bytes === "number") {
    items.push(humanSize(file.size_bytes))
  }
  const ext = file.file_name.split(".").pop()?.toUpperCase()
  if (ext) items.push(ext)
  if (previewType.value === "image" && zoom.value !== 1) {
    items.push(`${Math.round(zoom.value * 100)}%`)
  }
  return items
})

const imageStyle = computed(() => ({
  transform: `translate(${panX.value}px, ${panY.value}px) scale(${zoom.value})`,
  cursor: zoom.value > 1 ? (dragging.value ? "grabbing" : "grab") : "default",
}))

function onImageWheel(e: WheelEvent) {
  const oldZoom = zoom.value
  const factor = e.deltaY < 0 ? 1.15 : 1 / 1.15
  const newZoom = Math.min(10, Math.max(0.1, oldZoom * factor))
  if (newZoom === oldZoom) return

  const img = e.currentTarget as HTMLImageElement
  const container = img.parentElement
  if (!container) {
    zoom.value = newZoom
    return
  }
  const rect = container.getBoundingClientRect()
  const ccx = rect.left + rect.width / 2
  const ccy = rect.top + rect.height / 2

  const ratio = newZoom / oldZoom
  panX.value += (e.clientX - ccx - panX.value) * (1 - ratio)
  panY.value += (e.clientY - ccy - panY.value) * (1 - ratio)
  zoom.value = newZoom
}

function onImageMouseDown(e: MouseEvent) {
  if (zoom.value <= 1) return
  e.preventDefault()
  dragging.value = true
  dragStartX.value = e.clientX - panX.value
  dragStartY.value = e.clientY - panY.value
  window.addEventListener("mousemove", onWindowMouseMove)
  window.addEventListener("mouseup", onWindowMouseUp)
}

function onWindowMouseMove(e: MouseEvent) {
  if (!dragging.value) return
  panX.value = e.clientX - dragStartX.value
  panY.value = e.clientY - dragStartY.value
}

function onWindowMouseUp() {
  dragging.value = false
  window.removeEventListener("mousemove", onWindowMouseMove)
  window.removeEventListener("mouseup", onWindowMouseUp)
}

function resetZoom() {
  zoom.value = 1
  panX.value = 0
  panY.value = 0
}

function onModalShowUpdate(v: boolean) {
  if (!v) requestClose()
  else emit("update:open", v)
}

function requestClose() {
  emit("update:open", false)
}

function onDownload() {
  if (props.file) emit("download", props.file)
}

watch(
  () => props.open,
  (open) => {
    if (open) {
      void loadPreview()
    } else {
      abortCtrl?.abort()
      cleanup()
      maximized.value = false
    }
  },
)

watch(
  () => props.file?.full_path,
  (path, prev) => {
    if (props.open && path && path !== prev) void loadPreview()
  },
)

onBeforeUnmount(() => {
  abortCtrl?.abort()
  cleanup()
})

const title = computed(() => props.file?.file_name ?? t("sftp.preview.title"))

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
</script>

<template>
  <NModal
    :show="props.open"
    preset="card"
    :title="title"
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
    :mask-closable="true"
    @update:show="onModalShowUpdate"
  >
    <template #header-extra>
      <div class="preview-header-actions">
        <NButton
          size="small"
          quaternary
          circle
          :title="t('common.download')"
          @click="onDownload"
        >
          <template #icon>
            <NIcon><DownloadOutline /></NIcon>
          </template>
        </NButton>
        <NButton
          size="small"
          quaternary
          circle
          :title="maximized ? t('sftp.preview.restore') : t('sftp.preview.maximize')"
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

    <div class="preview-body">
      <div v-if="loading" class="preview-overlay">
        <NSpin size="large" />
      </div>
      <div v-else-if="errorMsg" class="preview-overlay preview-error-wrap">
        <span class="preview-error-text">{{ errorMsg }}</span>
        <NButton
          v-if="props.file"
          size="small"
          secondary
          @click="onDownload"
        >
          <template #icon>
            <NIcon><DownloadOutline /></NIcon>
          </template>
          {{ t('common.download') }}
        </NButton>
      </div>
      <div v-else-if="objectUrl && previewType === 'image'" class="preview-content">
        <img
          :src="objectUrl"
          :alt="props.file?.file_name"
          class="preview-media preview-img"
          :style="imageStyle"
          draggable="false"
          @load="onImageLoad"
          @wheel.prevent="onImageWheel"
          @mousedown="onImageMouseDown"
          @dblclick="resetZoom"
        />
      </div>
      <div v-else-if="objectUrl && previewType === 'video'" class="preview-content">
        <video
          :src="objectUrl"
          controls
          autoplay
          class="preview-media preview-video"
          @error="onMediaError"
          @loadedmetadata="onMediaLoadedMetadata"
        />
      </div>
      <div v-else-if="objectUrl && previewType === 'audio'" class="preview-content audio-wrap">
        <NIcon :size="64" class="audio-icon">
          <MusicalNotesOutline />
        </NIcon>
        <audio :src="objectUrl" controls autoplay class="preview-audio" @error="onMediaError" @loadedmetadata="onMediaLoadedMetadata" />
      </div>
      <div v-else-if="objectUrl && previewType === 'pdf'" class="preview-content">
        <iframe :src="objectUrl" class="preview-pdf" sandbox="allow-same-origin" />
      </div>

      <div v-if="metaItems.length" class="preview-statusbar">
        <template v-for="(item, i) in metaItems" :key="i">
          <span v-if="i > 0" class="status-sep">|</span>
          <span class="status-text">{{ item }}</span>
        </template>
      </div>
    </div>
  </NModal>
</template>

<style scoped>
.preview-header-actions {
  display: flex;
  align-items: center;
  gap: 4px;
}

.preview-body {
  position: relative;
  height: 100%;
  min-height: 0;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.preview-content {
  flex: 1 1 0;
  min-height: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: auto;
  padding: 12px;
}

.preview-media {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
  border-radius: 4px;
}

.preview-video {
  background: #000;
}

.preview-img {
  user-select: none;
  -webkit-user-drag: none;
  transition: none;
}

.preview-pdf {
  width: 100%;
  height: 100%;
  border: none;
}

.audio-wrap {
  flex-direction: column;
  gap: 20px;
}

.audio-icon {
  color: var(--ashell-text-muted);
  opacity: 0.6;
}

.preview-audio {
  width: 100%;
  max-width: 500px;
}

.preview-overlay {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--ashell-panel-bg, rgba(30, 30, 30, 0.95));
  z-index: 1;
}

.preview-error-wrap {
  flex-direction: column;
  gap: 16px;
  padding: 24px;
}

.preview-error-text {
  color: var(--ashell-text-strong, #e0e0e0);
  font-size: 14px;
  text-align: center;
  word-break: break-word;
}

.preview-statusbar {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  height: 26px;
  padding: 0 12px;
  flex-shrink: 0;
  border-top: 1px solid var(--ashell-border-soft);
  background: var(--ashell-panel-bg-soft);
  font-size: 11px;
  color: var(--ashell-text-muted);
  user-select: none;
}

.preview-statusbar .status-sep {
  opacity: 0.4;
}

.preview-statusbar .status-text {
  white-space: nowrap;
}
</style>
