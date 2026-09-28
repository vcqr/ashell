/**
 * 前端偏好快照：随备份导出/恢复的 localStorage 白名单键集合。
 *
 * 数据库里已备份的内容（主机、分组、AI 供应商、应用设置等）之外，
 * 还有大量偏好只存在于浏览器 localStorage（键位、终端配置、主题、
 * 抽屉宽度、SFTP 书签等），换机/重装会全部丢失。备份时把它们收成
 * 一个 key->raw 字符串的快照对象交给后端原样入包，恢复时写回。
 *
 * 白名单原则：
 * - 只收偏好与用户数据，不含会话态与安全敏感项
 * - token（登录凭证）、window-id（会话标识）、tabs（会话恢复）、
 *   broadcast-*（广播会话状态）、sftp-local-dir（机器本地路径）明确排除
 */

/** 快照内键名（与 localStorage 键一致，便于排查） */
export const BACKUP_PREF_KEYS = [
  // 外观
  "ashell:theme-mode",
  "ashell:locale",
  "ashell:acrylic-tint",
  "ashell:window-blur",
  "ashell:window-opacity",
  "ashell:wallpaper-opacity",
  "ashell:activity-bar-visible",
  // 终端与启动
  "ashell:terminal-config",
  "ashell:terminal-custom-themes",
  "ashell:startup-config",
  "ashell:keybindings",
  // 主机列表
  "ashell:host-pinned-ids",
  "ashell:hosts-pinned",
  "ashell:host-sort-mode",
  "ashell:hosts-flat",
  // 面板布局（宽度与固定态）
  "ashell:sftp-width",
  "ashell:ai-width",
  "ashell:ai-pinned",
  "ashell:forward-width",
  "ashell:hostinfo-width",
  "ashell:hosts-width",
  "ashell:local-files-width",
  "ashell:local-files-pinned",
  // SFTP
  "ashell:sftp-bookmarks",
  "ashell:sftp-col-widths",
  "ashell:sftp-dualpane",
  "ashell:sftp-dual-width",
  "ashell:sftp-dual-split",
  "ashell:sftp-cwd-follow",
  // 编辑器
  "ashell:editor-font-size",
  "ashell:editor-word-wrap",
] as const;

export type FrontendPrefs = Record<string, string>;

/** 收集白名单键的当前值（仅存在的键；值为原始字符串，后端透传） */
export function collectFrontendPrefs(): FrontendPrefs {
  const prefs: FrontendPrefs = {};
  if (typeof localStorage === "undefined") return prefs;
  for (const key of BACKUP_PREF_KEYS) {
    try {
      const raw = localStorage.getItem(key);
      if (raw !== null) prefs[key] = raw;
    } catch {
      // ignore
    }
  }
  return prefs;
}

/** 把快照写回 localStorage（仅写快照里有的键），刷新后生效 */
export function applyFrontendPrefs(prefs: unknown): number {
  if (!prefs || typeof prefs !== "object" || Array.isArray(prefs)) return 0;
  if (typeof localStorage === "undefined") return 0;
  let applied = 0;
  for (const [key, value] of Object.entries(prefs as Record<string, unknown>)) {
    if (!(BACKUP_PREF_KEYS as readonly string[]).includes(key)) continue;
    if (typeof value !== "string") continue;
    try {
      localStorage.setItem(key, value);
      applied++;
    } catch {
      // ignore
    }
  }
  return applied;
}
