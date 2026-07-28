/** 把字节速率转成可读字符串，例如 1234 -> "1.21 KB/s" */
export function humanRate(bytesPerSec: number | undefined | null): string {
  if (
    bytesPerSec === undefined ||
    bytesPerSec === null ||
    !Number.isFinite(bytesPerSec)
  )
    return "-"
  if (bytesPerSec <= 0) return "0 B/s"
  const units = ["B/s", "KB/s", "MB/s", "GB/s", "TB/s"]
  let i = 0
  let n = bytesPerSec
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024
    i += 1
  }
  return `${n.toFixed(n >= 100 || i === 0 ? 0 : 2)} ${units[i]}`
}
