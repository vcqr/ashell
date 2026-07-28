/**
 * 文本型进度条解析（纯函数，不依赖 Vue / xterm）。
 *
 * ProgressAddon 只认 OSC 9;4 转义序列；但 cargo / apt / pip / tqdm / wget2 / curl
 * / brew / git / rsync / scp 等很多 CLI 用纯文本"重写当前行"的方式画进度条，例如：
 *   `[===>      ] 5/10`           —— 方括号 + n/n
 *   `file.zip  12%[==>      ] 5.6M` —— wget2 风格，百分比 + 方括号
 *   `[##---] 45%`                  —— 方括号 + 百分比
 *   `################ 35.5%`       —— curl --progress-bar / brew 下载，纯 # 填充 + 百分比
 *   `Receiving objects:  45% (12/27)` —— git clone，百分比 + 圆括号 n/n
 *   `file.zip   12%   1.2MB/s   87s`  —— rsync / scp，百分比 + 速度单位
 *   `[===>      ] 12.3MB/45.2MB`     —— docker pull / podman pull layer 进度，方括号 + 带单位 n/n
 * 还有 indeterminate 等待（无具体百分比）：braille spinner（⠋⠙⠹）、`Working...`、
 * `Loading…` 等，切到 state=3 渐变条。具体百分比/n/n 优先于 spinner。
 * 这里在 term.write 之前拦截字节流，按行扫描这种文本进度，喂给同一套
 * progressState / progressValue。OSC 与文本进度同时存在时，OSC 优先
 * （progressAddon.progress.state !== 0 时文本侧不写 UI）。
 */

export interface TextProgressMatch {
  percent: number
  done: boolean
  indeterminate: boolean
}

// 形如 `[===>      ] 5/10` / `[#########---------] 9/18` —— 方括号进度条 + n/n
const TEXT_PROGRESS_FRACTION_RE = /\[[^\]]+\]\s*(\d+)\s*\/\s*(\d+)/g
// 形如 `[===>      ] 12.3MB/45.2MB` —— docker pull / podman pull 的 layer 进度。
// 方括号 + 带单位（KB/MB/GB/TB/KiB/MiB 等）的 n/n。和纯数字 n/n 分开一条规则，
// 因为带单位时无法用 current/total 直接判断 done（单位可能不同，如 512KB/1.2MB），
// 改用百分比 ≥ 100 判定 done。
const TEXT_PROGRESS_BYTES_RE =
  /\[[^\]]+\]\s*(\d+(?:\.\d+)?)\s*([KMGT]i?B)\s*\/\s*(\d+(?:\.\d+)?)\s*([KMGT]i?B)/g

const BYTE_UNITS: Record<string, number> = {
  B: 1,
  KB: 1000, MB: 1000 ** 2, GB: 1000 ** 3, TB: 1000 ** 4,
  KIB: 1024, MIB: 1024 ** 2, GIB: 1024 ** 3, TIB: 1024 ** 4,
}

function parseByteValue(num: string, unit: string): number {
  const n = parseFloat(num)
  const factor = BYTE_UNITS[unit.toUpperCase()] ?? 1
  return n * factor
}
// 形如 wget2 的 `file.zip   12%[==>                ] 5.6M` 或 `[##---] 45%` ——
// 方括号进度条 + 百分比。要求同行必须有方括号，避免误报纯百分比日志（如 "cpu 80%"）。
const TEXT_PROGRESS_PERCENT_RE =
  /(?:\[[#=*\-+>~.\s]+\]\s*(\d{1,3})\s*%|(\d{1,3})\s*%\s*\[[#=*\-+>~.\s]+\])/g
// curl --progress-bar / brew 下载 bottle 的格式：纯 `#` 填充 + 百分比，无方括号。
//   `##################      35.5%`
//   `######################## 100.0%`
// 要求至少 5 个连续 `#` 或 `=` 降低误报（普通日志里的单个 `#` 注释不会中）。
// 百分比支持小数（`35.5%` → 取整 35）。
const TEXT_PROGRESS_HASH_RE = /([#=]{5,})\s*(\d{1,3}(?:\.\d+)?)\s*%/g
// git clone: `Receiving objects:  45% (12/27)` —— 百分比 + 圆括号 n/n 同时出现。
// 要求两者都有，避免纯百分比日志误报。done 判定同时看百分比和 n/n。
const TEXT_PROGRESS_GIT_RE = /(\d{1,3})\s*%\s*\((\d+)\s*\/\s*(\d+)\)/g
// rsync / scp: `file.zip   12%   1.2MB/s   87s` —— 百分比 + 速度单位。
// 要求同时出现速度单位（KB/s、MB/s、GB/s、MiB/s 等），否则纯百分比日志会误报。
const TEXT_PROGRESS_SPEED_RE =
  /(\d{1,3})\s*%\s+\d+(?:\.\d+)?\s*[KMGT]i?B\/s/g
// pnpm: `Progress: resolved 120, reused 100, downloaded 10` —— 没有条形，用 done/total 推百分比。
const TEXT_PROGRESS_PNPM_RE =
  /resolved\s+(\d+),\s*reused\s+(\d+),\s*downloaded\s+(\d+)/g
// dnf / yum / apk 下载或安装序号：`(3/25): foo.rpm ...`、`(1/5) Installing foo ...`
const TEXT_PROGRESS_PAREN_FRACTION_RE = /^\s*\((\d+)\s*\/\s*(\d+)\)\s*:?/
// dnf / yum 事务阶段：`Installing  : foo-1.0.x86_64   3/25`、`Verifying : bar 25/25`
const TEXT_PROGRESS_DNF_TRANSACTION_RE =
  /(?:^|\s)(?:installing|updating|reinstalling|removing|verifying|cleanup|安装|更新|重装|删除|卸载|验证|清理)\s*:\s*.+?\s(\d+)\s*\/\s*(\d+)\s*$/i
// apt：下载完成 `Fetched 10.5 MB in 2s (5.2 MB/s)` 视为一次完成脉冲；
// `Get:/Hit:/Unpacking/Setting up/Processing triggers` 等阶段没有总量，给 indeterminate。
const TEXT_PROGRESS_APT_FETCHED_RE =
  /^\s*fetched\s+.+\s+in\s+\d+(?:\.\d+)?s\s*\(.+\)\s*$/i
const TEXT_PROGRESS_APT_PHASE_RE =
  /^(?:\s*(?:get|hit|ign|err):\d+|\s*(?:preparing to unpack|unpacking|setting up|processing triggers|selecting previously unselected|removing|purging|configuring|reading package lists|building dependency tree|reading state information)\b)/i
// 兜底可视进度识别：tqdm `45%|████▌ | 5/10`、pip/rich `━━━╸ 1.2/2.3 MB`、
// yarn/docker `[2/4]` 等。只有同行确实像进度条（条形字符或 [n/n] 步骤）时，
// 才接受通用的 n/n、字节 n/n、百分比，避免把普通日志里的比例/百分比误判成进度。
// 注意不带 /g：此规则只用于 .test()，带 /g 会让 lastIndex 在连续调用间推进，
// 造成相邻行隔一次漏检。
const TEXT_PROGRESS_PIPE_BAR_RE =
  /\|[#=█▉▊▋▌▍▎▏░▒▓━─╸╺┄┅┈┉\s.*+\-<>~]{2,}\|/
const TEXT_PROGRESS_SQUARE_RE = /\[([^\]\r\n]{2,})\]/g
const TEXT_PROGRESS_BAR_CHAR_RE =
  /[#=█▉▊▋▌▍▎▏░▒▓━─╸╺┄┅┈┉*<>~.+\-]/
const TEXT_PROGRESS_STEP_RE = /^\s*\d+\s*\/\s*\d+\s*$/
const TEXT_PROGRESS_ANY_BYTES_RE =
  /(\d+(?:\.\d+)?)(?:\s*([KMGT]i?B))?\s*\/\s*(\d+(?:\.\d+)?)\s*([KMGT]i?B)/g
const TEXT_PROGRESS_ANY_FRACTION_RE = /(\d+)\s*\/\s*(\d+)/g
const TEXT_PROGRESS_ANY_PERCENT_RE = /(\d{1,3}(?:\.\d+)?)\s*%/g
// 廉价预过滤：没有进度线索的普通日志直接跳过，避免大流量输出反复跑一整组规则。
const PROGRESS_HINT_RE =
  /[%/\[\]#|⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏█▉▊▋▌▍▎▏░▒▓━─╸╺┄┅┈┉]|fetched|resolved|installing|verifying|unpacking|setting up|processing triggers|reading package lists|building dependency|downloading|fetching|working|loading|processing|等待|处理|加载|获取|下载|安装|构建|编译|解析|准备|启动|连接|初始化|运行/i
// Spinner / indeterminate 等待：braille 转圈（⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏）、ASCII 转圈（|/-\）、
// 省略号（... / …）+ 等待关键词。仅当同行没有具体百分比/n/n 时才认（具体进度优先）。
// 命中后切到 state=3（indeterminate 渐变条），value 被忽略。
const SPINNER_BRAILLE_RE = /[⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏]/
const SPINNER_ASCII_RE = /[|\/\\\-][\s_]?\S*[\s_]?[|\/\\\-]/
const SPINNER_DOTS_RE = /(?:\.{3,}|…)/
const SPINNER_KEYWORD_RE =
  /\b(waiting|working|processing|loading|fetching|downloading|installing|building|compiling|resolving|preparing|starting|connecting|initializing|running)\b/i
const SPINNER_KEYWORD_ZH_RE =
  /(?:正在)?(?:等待|处理|加载|获取|下载|安装|构建|编译|解析|准备|启动|连接|初始化|运行)中?/

function matchSpinner(line: string): boolean {
  if (SPINNER_BRAILLE_RE.test(line)) return true
  const hasKeyword =
    SPINNER_KEYWORD_RE.test(line) || SPINNER_KEYWORD_ZH_RE.test(line)
  if (SPINNER_DOTS_RE.test(line) && hasKeyword) return true
  if (SPINNER_ASCII_RE.test(line) && hasKeyword) return true
  return false
}

/** 去掉 ANSI 控制序列，避免颜色/清屏序列打断 tqdm / rich / pip 的文本进度识别。 */
export function stripAnsiForProgress(input: string): string {
  let out = ""
  let i = 0
  while (i < input.length) {
    const ch = input[i]!
    if (ch === "\x1b") {
      const next = input[i + 1]
      if (next === "[") {
        i += 2
        while (i < input.length) {
          const c = input[i]!
          i++
          if (c >= "@" && c <= "~") break
        }
        continue
      }
      if (next === "]") {
        i += 2
        while (i < input.length) {
          if (input[i] === "\x07") {
            i++
            break
          }
          if (input[i] === "\x1b" && input[i + 1] === "\\") {
            i += 2
            break
          }
          i++
        }
        continue
      }
      i += 2
      continue
    }
    out += ch
    i++
  }
  return out
}

/**
 * 若字符串末尾是一个不完整的 ESC 序列（被 chunk 边界截断），返回该 ESC 的
 * 起始下标；否则返回 -1。
 *
 * 二进制帧按任意边界切分，颜色码 / OSC 标题序列可能被拆进两个 chunk：
 * 不完整的 `\x1b[31` 直接喂给 stripAnsiForProgress 会被吞到串尾，下一个
 * chunk 开头的 `m` 则作为字面文本泄漏进文本行缓冲，干扰进度匹配。
 * OSC（`\x1b]`）以 BEL 或 ST（`\x1b\`）结束；ST 的内层 ESC 会被
 * lastIndexOf 命中，按双字符序列判定为完整。
 */
export function incompleteEscapeStart(input: string): number {
  const esc = input.lastIndexOf("\x1b")
  if (esc === -1) return -1
  const next = input[esc + 1]
  if (next === undefined) return esc
  if (next === "[") {
    for (let i = esc + 2; i < input.length; i++) {
      const c = input.charCodeAt(i)
      if (c >= 0x40 && c <= 0x7e) return -1
    }
    return esc
  }
  if (next === "]") {
    for (let i = esc + 2; i < input.length; i++) {
      if (input[i] === "\x07") return -1
      if (input[i] === "\x1b" && input[i + 1] === "\\") return -1
    }
    return esc
  }
  return -1
}

function hasVisualProgressBar(line: string): boolean {
  if (TEXT_PROGRESS_PIPE_BAR_RE.test(line)) return true
  // matchAll 内部会克隆正则，不污染共享实例的 lastIndex
  for (const m of line.matchAll(TEXT_PROGRESS_SQUARE_RE)) {
    const content = m[1] ?? ""
    if (TEXT_PROGRESS_STEP_RE.test(content)) return true
    if (TEXT_PROGRESS_BAR_CHAR_RE.test(content)) return true
  }
  return false
}

export function matchTextProgress(line: string): TextProgressMatch | null {
  if (!PROGRESS_HINT_RE.test(line)) return null
  let last: TextProgressMatch | null = null
  // 所有 /g 规则统一走 matchAll：内部克隆正则，不污染共享实例的 lastIndex
  for (const m of line.matchAll(TEXT_PROGRESS_FRACTION_RE)) {
    const currentStr = m[1]
    const totalStr = m[2]
    if (currentStr === undefined || totalStr === undefined) continue
    const current = parseInt(currentStr, 10)
    const total = parseInt(totalStr, 10)
    if (total > 0 && current >= 0 && current <= total) {
      last = {
        percent: Math.min(100, Math.round((current / total) * 100)),
        done: current >= total,
        indeterminate: false,
      }
    }
  }
  for (const m of line.matchAll(TEXT_PROGRESS_BYTES_RE)) {
    const curNum = m[1]
    const curUnit = m[2]
    const totalNum = m[3]
    const totalUnit = m[4]
    if (curNum === undefined || curUnit === undefined || totalNum === undefined || totalUnit === undefined) continue
    const current = parseByteValue(curNum, curUnit)
    const total = parseByteValue(totalNum, totalUnit)
    if (total > 0 && current >= 0) {
      const pct = Math.min(100, Math.round((current / total) * 100))
      last = { percent: pct, done: pct >= 100, indeterminate: false }
    }
  }
  for (const m of line.matchAll(TEXT_PROGRESS_PERCENT_RE)) {
    const pctStr = m[1] ?? m[2]
    if (pctStr === undefined) continue
    const pct = parseInt(pctStr, 10)
    if (pct >= 0 && pct <= 100) {
      last = { percent: pct, done: pct >= 100, indeterminate: false }
    }
  }
  for (const m of line.matchAll(TEXT_PROGRESS_HASH_RE)) {
    const pctStr = m[2]
    if (pctStr === undefined) continue
    const pct = Math.round(parseFloat(pctStr))
    if (pct >= 0 && pct <= 100) {
      last = { percent: pct, done: pct >= 100, indeterminate: false }
    }
  }
  for (const m of line.matchAll(TEXT_PROGRESS_GIT_RE)) {
    const pctStr = m[1]
    const currentStr = m[2]
    const totalStr = m[3]
    if (pctStr === undefined || currentStr === undefined || totalStr === undefined) continue
    const pct = parseInt(pctStr, 10)
    const current = parseInt(currentStr, 10)
    const total = parseInt(totalStr, 10)
    if (pct >= 0 && pct <= 100) {
      last = { percent: pct, done: pct >= 100 || (total > 0 && current >= total), indeterminate: false }
    }
  }
  for (const m of line.matchAll(TEXT_PROGRESS_SPEED_RE)) {
    const pctStr = m[1]
    if (pctStr === undefined) continue
    const pct = parseInt(pctStr, 10)
    if (pct >= 0 && pct <= 100) {
      last = { percent: pct, done: pct >= 100, indeterminate: false }
    }
  }
  for (const m of line.matchAll(TEXT_PROGRESS_PNPM_RE)) {
    const resolvedStr = m[1]
    const reusedStr = m[2]
    const downloadedStr = m[3]
    if (resolvedStr === undefined || reusedStr === undefined || downloadedStr === undefined) continue
    const total = parseInt(resolvedStr, 10)
    const current = parseInt(reusedStr, 10) + parseInt(downloadedStr, 10)
    if (total > 0 && current >= 0) {
      const pct = Math.min(100, Math.round((current / total) * 100))
      last = { percent: pct, done: current >= total || pct >= 100, indeterminate: false }
    }
  }
  const parenFraction = TEXT_PROGRESS_PAREN_FRACTION_RE.exec(line)
  if (!last && parenFraction) {
    const currentStr = parenFraction[1]
    const totalStr = parenFraction[2]
    if (currentStr !== undefined && totalStr !== undefined) {
      const current = parseInt(currentStr, 10)
      const total = parseInt(totalStr, 10)
      if (total > 0 && current >= 0 && current <= total) {
        last = {
          percent: Math.min(100, Math.round((current / total) * 100)),
          done: current >= total,
          indeterminate: false,
        }
      }
    }
  }
  const dnfTransaction = TEXT_PROGRESS_DNF_TRANSACTION_RE.exec(line)
  if (!last && dnfTransaction) {
    const currentStr = dnfTransaction[1]
    const totalStr = dnfTransaction[2]
    if (currentStr !== undefined && totalStr !== undefined) {
      const current = parseInt(currentStr, 10)
      const total = parseInt(totalStr, 10)
      if (total > 0 && current >= 0 && current <= total) {
        last = {
          percent: Math.min(100, Math.round((current / total) * 100)),
          done: current >= total,
          indeterminate: false,
        }
      }
    }
  }
  if (!last && TEXT_PROGRESS_APT_FETCHED_RE.test(line)) {
    last = { percent: 100, done: true, indeterminate: false }
  }
  if (!last && TEXT_PROGRESS_APT_PHASE_RE.test(line)) {
    last = { percent: 0, done: false, indeterminate: true }
  }
  // 通用兜底：只有同行存在可视进度条（条形字符 / [n/n] 步骤）时才接受
  // 通用 字节 n/n → 数字 n/n → 百分比。覆盖 tqdm、pip/rich、yarn/docker 步骤等。
  if (!last && hasVisualProgressBar(line)) {
    for (const m of line.matchAll(TEXT_PROGRESS_ANY_BYTES_RE)) {
      const curNum = m[1]
      const totalNum = m[3]
      const totalUnit = m[4]
      // pip/rich 常写 `1.2/2.3 MB`：当前值单位缺省时按总量单位算
      const curUnit = m[2] ?? totalUnit
      if (curNum === undefined || curUnit === undefined || totalNum === undefined || totalUnit === undefined) continue
      const current = parseByteValue(curNum, curUnit)
      const total = parseByteValue(totalNum, totalUnit)
      if (total > 0 && current >= 0) {
        const pct = Math.min(100, Math.round((current / total) * 100))
        last = { percent: pct, done: pct >= 100, indeterminate: false }
      }
    }
    if (!last) {
      for (const m of line.matchAll(TEXT_PROGRESS_ANY_FRACTION_RE)) {
        const currentStr = m[1]
        const totalStr = m[2]
        if (currentStr === undefined || totalStr === undefined) continue
        const current = parseInt(currentStr, 10)
        const total = parseInt(totalStr, 10)
        if (total > 0 && current >= 0 && current <= total) {
          last = {
            percent: Math.min(100, Math.round((current / total) * 100)),
            done: current >= total,
            indeterminate: false,
          }
        }
      }
    }
    if (!last) {
      for (const m of line.matchAll(TEXT_PROGRESS_ANY_PERCENT_RE)) {
        const pctStr = m[1]
        if (pctStr === undefined) continue
        const pct = Math.round(parseFloat(pctStr))
        if (pct >= 0 && pct <= 100) {
          last = { percent: pct, done: pct >= 100, indeterminate: false }
        }
      }
    }
  }
  // 具体进度没命中才看 spinner——具体进度优先，spinner 让位
  if (!last && matchSpinner(line)) {
    last = { percent: 0, done: false, indeterminate: true }
  }
  return last
}
