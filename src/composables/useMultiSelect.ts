import { computed, ref, type Ref } from "vue"
import type { SftpFile } from "@/types"

/**
 * SFTP 双栏文件列表的多选（资源管理器语义）：
 * 单击独占选中、Ctrl/Cmd+单击切换、Shift+单击区间（替换）、Ctrl+Shift 区间叠加。
 * 右键未选中的行时独占选中该行（已选中则保持集合，让右键作用于多选）。
 *
 * 选择集只负责高亮与集合维护；传输（中间条按钮 / 拖拽）时再按 file_type
 * 过滤，目录行可选可高亮但不参与传输。
 */
export function useMultiSelect(files: Ref<SftpFile[]>) {
  /** 选中行的 full_path 集合（整体替换以驱动响应） */
  const selectedKeys = ref<ReadonlySet<string>>(new Set())
  /** 最近一次点选的行，Shift 区间选择的锚点 */
  const anchorKey = ref<string | null>(null)

  const selectedFiles = computed(() =>
    files.value.filter((f) => selectedKeys.value.has(f.full_path)),
  )

  function isSelected(row: SftpFile): boolean {
    return selectedKeys.value.has(row.full_path)
  }

  /** 独占选中一行（单击 / 右键未选中行时） */
  function selectExclusive(row: SftpFile) {
    selectedKeys.value = new Set([row.full_path])
    anchorKey.value = row.full_path
  }

  function onRowClick(row: SftpFile, e: MouseEvent) {
    const key = row.full_path
    const ctrl = e.ctrlKey || e.metaKey

    if (e.shiftKey && anchorKey.value && anchorKey.value !== key) {
      const list = files.value
      const from = list.findIndex((f) => f.full_path === anchorKey.value)
      const to = list.findIndex((f) => f.full_path === key)
      if (from >= 0 && to >= 0) {
        const [lo, hi] = from < to ? [from, to] : [to, from]
        const range = list.slice(lo, hi + 1).map((f) => f.full_path)
        selectedKeys.value = ctrl
          ? new Set([...selectedKeys.value, ...range])
          : new Set(range)
        return
      }
      // 锚点不在当前列表（已刷新）：退化为普通单击
    }

    if (ctrl) {
      const next = new Set(selectedKeys.value)
      if (next.has(key)) next.delete(key)
      else next.add(key)
      selectedKeys.value = next
      anchorKey.value = key
      return
    }

    selectExclusive(row)
  }

  /** 拖拽取数：行在选择集内则取整个选择集（含目录行），否则取当前行。
   *  文件/目录的传输分流（直传 vs 递归整树）由调用方处理（WinSCP 语义） */
  function collectForTransfer(row: SftpFile): SftpFile[] {
    if (selectedKeys.value.has(row.full_path)) {
      return [...selectedFiles.value]
    }
    return [row]
  }

  function clearSelection() {
    selectedKeys.value = new Set()
    anchorKey.value = null
  }

  /** 全选当前列表（Ctrl/Cmd+A） */
  function selectAll() {
    selectedKeys.value = new Set(files.value.map((f) => f.full_path))
    anchorKey.value = null
  }

  return {
    selectedKeys,
    selectedFiles,
    isSelected,
    selectExclusive,
    onRowClick,
    collectForTransfer,
    clearSelection,
    selectAll,
  }
}
