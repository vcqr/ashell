export type PreviewType = "image" | "video" | "audio" | "pdf"

const IMAGE_EXTS = new Set([
  "png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "ico", "avif",
])
const VIDEO_EXTS = new Set(["mp4", "webm", "mov"])
const AUDIO_EXTS = new Set(["mp3", "wav", "ogg", "flac", "aac", "m4a"])
const PDF_EXTS = new Set(["pdf"])

const MIME_MAP: Record<string, string> = {
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  gif: "image/gif",
  webp: "image/webp",
  bmp: "image/bmp",
  svg: "image/svg+xml",
  ico: "image/x-icon",
  avif: "image/avif",
  mp4: "video/mp4",
  webm: "video/webm",
  mov: "video/quicktime",
  mp3: "audio/mpeg",
  wav: "audio/wav",
  ogg: "audio/ogg",
  flac: "audio/flac",
  aac: "audio/aac",
  m4a: "audio/mp4",
  pdf: "application/pdf",
}

export function getExt(filename: string): string {
  return filename.split(".").pop()?.toLowerCase() ?? ""
}

export function getPreviewType(filename: string): PreviewType | null {
  const ext = getExt(filename)
  if (IMAGE_EXTS.has(ext)) return "image"
  if (VIDEO_EXTS.has(ext)) return "video"
  if (AUDIO_EXTS.has(ext)) return "audio"
  if (PDF_EXTS.has(ext)) return "pdf"
  return null
}

export function isPreviewable(filename: string): boolean {
  return getPreviewType(filename) !== null
}

export function getMimeType(filename: string): string {
  return MIME_MAP[getExt(filename)] ?? "application/octet-stream"
}
