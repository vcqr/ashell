/** 平台检测：userAgentData 优先（Chromium），退化到 userAgent + platform */
export function detectMac(): boolean {
  if (typeof navigator === "undefined") return false
  const ua = navigator.userAgent || ""
  const platform =
    (navigator as Navigator & { userAgentData?: { platform?: string } })
      .userAgentData?.platform ||
    navigator.platform ||
    ""
  return /Mac|iPhone|iPad|iPod/i.test(`${ua} ${platform}`)
}
