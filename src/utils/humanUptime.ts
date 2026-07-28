type TFunc = (key: string, params?: Record<string, unknown>) => string

/** Linux 风格运行时长，例如 12 天 3 小时 5 分钟。
 *  传入 t 函数时使用 i18n 本地化单位，否则回退中文。 */
export function humanUptime(
  secs: number | undefined | null,
  t?: TFunc,
): string {
  if (secs === undefined || secs === null || !Number.isFinite(secs) || secs < 0)
    return "-"
  const s = Math.floor(secs)
  const d = Math.floor(s / 86400)
  const h = Math.floor((s % 86400) / 3600)
  const m = Math.floor((s % 3600) / 60)
  const parts: string[] = []
  if (d > 0) parts.push(`${d} ${t ? t("hostInfo.uptime.day") : "天"}`)
  if (h > 0) parts.push(`${h} ${t ? t("hostInfo.uptime.hour") : "小时"}`)
  if (m > 0 || parts.length === 0) parts.push(`${m} ${t ? t("hostInfo.uptime.minute") : "分钟"}`)
  return parts.join(" ")
}
