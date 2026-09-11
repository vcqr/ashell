import { h } from "vue"
import type { Component } from "vue"
import {
  FileAlt,
  FileArchive,
  FileAudio,
  FileCode,
  FileExcel,
  FileImage,
  FilePdf,
  FilePowerpoint,
  FileRegular,
  FileVideo,
  FileWord,
  Folder,
  Link,
} from "@vicons/fa"
import { NIcon } from "naive-ui"
import type { SftpFile } from "@/types"

/** 文件类型配色：用于文件图标 */
export const FILE_TYPE_COLORS = {
  dir: "#f1c27d",
  symlink: "#7c5cff",
  file: "#9aa0a6",
}

function extOf(name: string): string {
  const i = name.lastIndexOf(".")
  return i >= 0 ? name.slice(i + 1).toLowerCase() : ""
}

/** 扩展名 -> [图标组件, 颜色]。按类别归组（压缩包/代码/媒体/办公文档），
 *  未命中的回落到通用文件图标。 */
type ExtIconEntry = [Component, string]
const EXT_ICON_MAP: Record<string, ExtIconEntry> = (() => {
  const map: Record<string, ExtIconEntry> = {}
  const put = (icon: Component, color: string, exts: string[]) => {
    for (const e of exts) map[e] = [icon, color]
  }
  put(FileImage, "#6bc1ff", [
    "png", "jpg", "jpeg", "gif", "bmp", "webp", "svg", "ico", "avif", "tiff",
  ])
  put(FileVideo, "#d97fc0", [
    "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "mpg", "mpeg",
  ])
  put(FileAudio, "#b48cff", ["mp3", "wav", "flac", "aac", "ogg", "m4a", "wma", "opus"])
  put(FileArchive, "#e8a15a", [
    "zip", "tar", "gz", "tgz", "bz2", "xz", "7z", "rar", "iso", "zst",
  ])
  put(FileCode, "#5fd0a5", [
    "js", "mjs", "cjs", "ts", "tsx", "jsx", "vue", "py", "rs", "go", "java",
    "c", "h", "cpp", "hpp", "cs", "rb", "php", "swift", "kt", "sh", "bat",
    "ps1", "json", "yaml", "yml", "toml", "xml", "html", "htm", "css", "scss",
    "less", "sql", "lua", "pl",
  ])
  put(FileAlt, "#8a93a6", [
    "ini", "conf", "cfg", "env", "lock", "service", "log", "diff", "patch",
  ])
  put(FilePdf, "#ef6b6b", ["pdf"])
  put(FileWord, "#5e9bff", ["doc", "docx", "rtf", "odt"])
  put(FileExcel, "#58b368", ["xls", "xlsx", "ods", "csv", "tsv"])
  put(FilePowerpoint, "#f0824c", ["ppt", "pptx", "odp"])
  return map
})()

/** SFTP 文件列表通用图标：目录/软链接特殊处理，其余按扩展名映射，
 *  本地与远程列表共用，保证两栏图标与配色一致。 */
export function sftpFileIcon(file: SftpFile) {
  if (file.file_type === "dir") {
    return h(NIcon, { size: 16, color: FILE_TYPE_COLORS.dir }, { default: () => h(Folder) })
  }
  if (file.file_type === "symlink") {
    return h(NIcon, { size: 16, color: FILE_TYPE_COLORS.symlink }, { default: () => h(Link) })
  }
  const hit = EXT_ICON_MAP[extOf(file.file_name)]
  if (hit) {
    return h(NIcon, { size: 16, color: hit[1] }, { default: () => h(hit[0]) })
  }
  return h(NIcon, { size: 16, color: FILE_TYPE_COLORS.file }, { default: () => h(FileRegular) })
}
