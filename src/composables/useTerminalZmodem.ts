import { ref } from "vue"
// 包的 index.js 只聚合协议核心(Sentry/Session),Browser 工具(send_files/
// save_to_disk)在浏览器入口里,故从 src 子路径导入完整聚合
import Zmodem from "zmodem.js/src/zmodem_browser.js"
import type {
  ZmodemDetection,
  ZmodemSession,
  ZmodemSentry,
  ZmodemTransfer,
} from "zmodem.js/src/zmodem_browser.js"
import { isTauri } from "@/utils/platform"
import { pickTransferDir, pickTransferFileObjects, pickTransferFilePaths } from "@/utils/fileInterop"
import { readLocalFileStream, saveLocalFile } from "@/api/local"

export type ZmodemItemStatus = "running" | "done" | "error" | "cancelled"

export interface ZmodemTransferItem {
  id: number
  name: string
  direction: "upload" | "download"
  size: number
  loaded: number
  status: ZmodemItemStatus
  error?: string
}

interface TerminalZmodemOptions {
  /** 上屏出口(含时间戳/进度等包装) */
  writeToTerm: (data: Uint8Array | string) => void
  /** ws 二进制帧发送(ZMODEM 协议字节走此通道直通 SSH channel) */
  sendBinary: (data: Uint8Array<ArrayBuffer>) => boolean
  /** ws JSON 控制帧发送 */
  sendControl: (msg: Record<string, unknown>) => void
  /** 是否 SSH 会话(仅 SSH 后端解析 zmodem_state 控制帧) */
  isSsh: () => boolean
  /** toast 通知(kind 对应 naive-ui message API) */
  notify: (kind: "success" | "error" | "warning" | "info", content: string) => void
  t: (key: string, params?: Record<string, unknown>) => string
}

/** ZMODEM 起始帧签名 `**\x18B0`(ZRQINIT 十六进制帧头,与 zmodem.js 的
 *  COMMON_ZM_HEX_START 一致)。sz/rz 均以此开头。 */
const ZS_START = [42, 42, 24, 66, 48]
/** 尾部持回长度上限:签名的最长真前缀为 4 字节 */
const HOLD_MAX = ZS_START.length - 1
/** 持回字节冲刷延时:流静默时把误扣的普通输出尾字节放行上屏 */
const HOLD_FLUSH_MS = 150
/** 进入 zmodem 模式却始终未确认会话的兜底退出时长 */
const NO_CONFIRM_TIMEOUT_MS = 8000
/** 用户取消后忽略重新检测的冷却期:远端 rz/sz 会重试 ZRQINIT,不冷却
 *  会让对话框反复弹出;期间起始帧原样上屏(与取消的现实一致) */
const CANCEL_COOLDOWN_MS = 30000

let itemSeq = 0

/**
 * 终端 ZMODEM(sz/rz)传输子系统。
 *
 * 工作方式:正常态在 writeToTerm 之前用 5 字节滑动窗口嗅探 ZMODEM 起始
 * 帧签名(未命中时正常路径零改动,仅按前缀扣留 ≤4 字节尾部并定时放行);
 * 命中后进入 zmodem 模式,把字节流交给 zmodem.js 的 Sentry 解析——非协议
 * 字节原样上屏,协议字节驱动会话:
 * - role=receive(远端 sz):逐个 offer 收文件,整会话选一次保存目录,
 *   完成后 multipart 落盘(Web 端走浏览器下载)
 * - role=send(远端 rz):弹多选文件框,Zmodem.Browser.send_files 批量发送
 * 会话期间用户键盘输入被丢弃(与 XShell/SecureCRT 一致),结束后恢复。
 */
export function useTerminalZmodem(opts: TerminalZmodemOptions) {
  const items = ref<ZmodemTransferItem[]>([])
  /** zmodem 模式(签名命中后到会话结束,输入丢弃、输出走 Sentry) */
  const busy = ref(false)
  /** 协议会话已确认(检测→confirm 之间的短暂窗口) */
  const confirmed = ref(false)

  // ── 非响应式内部状态 ──
  let inZmodem = false
  let sentry: ZmodemSentry | null = null
  let session: ZmodemSession | null = null
  /** 签名前缀持回(未上屏字节) */
  let holdback: number[] = []
  let holdFlushTimer: number | null = null
  let noConfirmTimer: number | null = null
  /** on_detect 与 confirm 之间吞掉 ZRQINIT 回显帧(20 字节十六进制残迹) */
  let suppressEcho = false
  /** 用户取消冷却截止时刻(ms);0 = 无冷却 */
  let cooldownUntil = 0
  /** 整会话取消标志:offer/send_files 回调看到它不再启动新文件 */
  let aborting = false

  function toast(kind: "success" | "error" | "warning" | "info", key: string, params?: Record<string, unknown>) {
    opts.notify(kind, opts.t(`terminal.zmodem.${key}`, params))
  }

  function pushItem(item: Omit<ZmodemTransferItem, "id">): ZmodemTransferItem {
    const full: ZmodemTransferItem = { id: ++itemSeq, ...item }
    items.value.push(full)
    return full
  }

  function markItem(item: ZmodemTransferItem | undefined, status: ZmodemItemStatus, error?: string) {
    if (!item) return
    item.status = status
    if (error !== undefined) item.error = error
  }

  // ── 会话收尾 ──

  function exitZmodem() {
    if (noConfirmTimer !== null) {
      window.clearTimeout(noConfirmTimer)
      noConfirmTimer = null
    }
    inZmodem = false
    busy.value = false
    confirmed.value = false
    sentry = null
    session = null
    aborting = false
    holdback = []
    if (opts.isSsh()) opts.sendControl({ kind: "zmodem_state", active: false })
  }

  /** 会话结束统一出口:统计结果并 toast(以 session 是否仍在为准,幂等) */
  function onSessionEnd() {
    if (!session) return
    const wasConfirmed = confirmed.value
    exitZmodem()
    if (!wasConfirmed) return
    const done = items.value.filter((it) => it.status === "done").length
    const failed = items.value.filter((it) => it.status === "error").length
    if (done === 0 && failed === 0) {
      // 没有成功也没有失败(用户取消 / 远端超时退出)只给一条取消提示
      toast("info", "cancelled")
    } else if (failed > 0) {
      toast("warning", "finishedWithErrors", { ok: done, fail: failed })
    } else {
      toast("success", "finished", { ok: done })
    }
  }

  /** 用户发起的取消:发 abort 序列,远端 rz/sz 会尽快退出。
   *  abort() 同步触发 session_end → onSessionEnd 统一 toast,这里不重复弹 */
  function cancelSession() {
    aborting = true
    for (const it of items.value) {
      if (it.status === "running") it.status = "cancelled"
    }
    cooldownUntil = Date.now() + CANCEL_COOLDOWN_MS
    try {
      session?.abort()
    } catch {
      // 已结束的会话 abort 会抛错,忽略
    }
    exitZmodem()
  }

  // ── 上传(远端 rz)──

  async function handleSendSession(detection: ZmodemDetection) {
    const zsession = detection.confirm()
    session = zsession
    zsession.on("session_end", onSessionEnd)

    // 选择要上传的文件(桌面:原生多选对话框 → 路径 → 后端流式读成
    // File;Web:<input type=file multiple> 直接给 File)
    let files: File[] | null = null
    if (isTauri) {
      const paths = await pickTransferFilePaths()
      if (paths && paths.length > 0) {
        const out: File[] = []
        for (const p of paths) {
          const blob = await readLocalFileStream(p)
          const name = p.split(/[\\/]/).pop() || p
          out.push(new File([blob], name))
        }
        files = out
      }
    } else {
      files = await pickTransferFileObjects()
    }
    if (!files || files.length === 0) {
      cancelSession()
      return
    }

    const itemByFile = new Map<File, ZmodemTransferItem>()
    for (const f of files) {
      itemByFile.set(f, pushItem({ name: f.name, direction: "upload", size: f.size, loaded: 0, status: "running" }))
    }
    try {
      await Zmodem.Browser.send_files(zsession, files, {
        on_offer_response: (file, xfer) => {
          // xfer 为 undefined = 对端跳过(ZSKIP)该文件
          markItem(itemByFile.get(file), xfer ? "running" : "cancelled")
        },
        on_progress: (file, xfer) => {
          const it = itemByFile.get(file)
          if (it) it.loaded = xfer.get_offset()
        },
        on_file_complete: (file) => {
          const it = itemByFile.get(file)
          if (it) it.loaded = it.size
          markItem(itemByFile.get(file), "done")
        },
      })
    } catch (e) {
      if (!aborting) {
        for (const it of itemByFile.values()) {
          if (it.status === "running") markItem(it, "error", String(e))
        }
        try {
          zsession.abort()
        } catch {
          // ignore
        }
        return
      }
    }
    // 批量发完,正常收尾(ZFIN 握手;收到的头不满足关闭条件时会同步抛
    // 错——此时会话已无数据可传,忽略即可,session_end 仍会由线路驱动)
    try {
      zsession.close()
    } catch {
      // ignore
    }
  }

// ── 下载(远端 sz)──

/** zmodem.js 会话对象补丁所需的内部成员(非公开,随 0.1.10 固定) */
interface ZmodemSubpacketLike {
  get_payload(): Uint8Array<ArrayBuffer>
  frame_end(): boolean
  ack_expected(): boolean
}

interface ReceiveSessionInternals {
  _accepted_offer: boolean
  _file_offset: number
  _next_header_handler: Record<
    string,
    ((hdr: { get_offset(): number }) => void) | null
  > | null
  _next_subpacket_handler: ((sp: ZmodemSubpacketLike) => void) | null
  _consume_ZDATA(hdr: { get_offset(): number }): void
  _consume_ZEOF(hdr: { get_offset(): number }): void
  _consume_ZDATA_data(sp: ZmodemSubpacketLike): void
  _make_promise_for_between_files(): void
  _send_ZRPOS(): void
  _send_ZRINIT(): void
  _send_header(name: string, args?: unknown): void
  _accept(offset?: number): Promise<void>
}

/**
 * lrzsz 互操作补丁(运行时,不改包体):
 * 上游 Receive 会话按"每文件一个 ZDATA 头"假设实现,而 lrzsz sz 实际为
 * 1) 首个数据子包用 ZCRCW(帧尾+需应答)且每帧都以独立 ZDATA 头开始;
 * 2) 上游对 ZCRCW 不回 ZACK、只认一个 ZDATA——实测 sz 发送首个 1KB 后
 *    永久停滞。按 ZMODEM 规范等价改写:
 *    - ZCRCW 子包回 ZACK(携带已收偏移)
 *    - _accept 的帧头处理链改为每帧重新武装(接受任意次 ZDATA,直至 ZEOF)
 */
function patchReceiveInterop(zsession: ZmodemSession) {
  const zs = zsession as unknown as ReceiveSessionInternals
  const origData = zs._consume_ZDATA_data.bind(zs)
  zs._consume_ZDATA_data = function (sp: ZmodemSubpacketLike) {
    origData(sp)
    if (sp.ack_expected() && sp.frame_end()) {
      zs._send_header("ZACK", Zmodem.ENCODELIB.pack_u32_le(zs._file_offset))
    }
  }
  zs._accept = function (offset?: number) {
    zs._accepted_offer = true
    zs._file_offset = offset ?? 0
    const arm = () => {
      zs._next_header_handler = {
        ZDATA: function (hdr) {
          zs._consume_ZDATA(hdr)
          zs._next_subpacket_handler = zs._consume_ZDATA_data
          arm()
        },
        ZEOF: function (hdr) {
          zs._consume_ZEOF(hdr)
          zs._next_subpacket_handler = null
          zs._make_promise_for_between_files()
          resolveAccept()
          zs._send_ZRINIT()
        },
      }
    }
    let resolveAccept!: () => void
    const ret = new Promise<void>((res) => {
      resolveAccept = res
    })
    arm()
    zs._send_ZRPOS()
    return ret
  }
}

  // 把收完的文件落盘:桌面 multipart 写入选定目录,Web 浏览器下载
  async function persistFile(name: string, chunks: BlobPart[], saveDir: string | null, item: ZmodemTransferItem) {
    if (isTauri && saveDir) {
      // 复用拖放落盘端点(multipart 流式写盘,同名覆盖)
      await saveLocalFile(saveDir, name, new File([new Blob(chunks)], name))
    } else {
      Zmodem.Browser.save_to_disk(chunks, name)
    }
    markItem(item, "done")
  }

  async function handleReceiveSession(detection: ZmodemDetection) {
    const zsession = detection.confirm()
    session = zsession
    patchReceiveInterop(zsession)
    zsession.on("session_end", onSessionEnd)

    let saveDir: string | null = null
    let dirPicked = !isTauri // Web 端逐文件走浏览器下载,无需选目录

    zsession.on("offer", (xfer: ZmodemTransfer) => {
      // sz 逐文件等应答,offer 天然串行;异步处理不阻塞线路解析
      void (async () => {
        if (aborting) return
        const details = xfer.get_details()
        const name = details.name || `zmodem-${Date.now()}`
        const item = pushItem({
          name,
          direction: "download",
          size: details.size ?? 0,
          loaded: 0,
          status: "running",
        })
        try {
          // 整会话只在第一个文件前选一次保存目录;取消 = 放弃整个传输
          if (!dirPicked) {
            saveDir = await pickTransferDir()
            dirPicked = true
            if (isTauri && !saveDir) {
              cancelSession()
              return
            }
          }
          const chunks: BlobPart[] = []
          xfer.on("input", (payload: Uint8Array<ArrayBuffer>) => {
            chunks.push(payload)
            item.loaded += payload.length
          })
          await xfer.accept()
          await persistFile(name, chunks, saveDir, item)
        } catch (e) {
          if (!aborting) {
            markItem(item, "error", String(e))
          }
        }
      })()
    })

    // 启动握手:发出 ZRINIT,远端 sz 才会开始发第一个 ZFILE(不调用则
    // 远端等超时重发 ZRQINIT,会话直接报 Unhandled header)
    zsession.start()
  }

  // ── 检测与接管 ──

  function handleDetect(detection: ZmodemDetection) {
    if (!detection.is_valid()) return
    // confirm() 会立刻经 sender 发出 ZRINIT(二进制帧);zmodem_state 控制帧
    // 在进入 zmodem 模式时已先发,后端按 WS 帧序先旁路过滤器再收到协议字节。
    // 吞回显:本次 consume 触发的 on_detect 之后,Sentry 还会把含 ZRQINIT
    // 十六进制帧的整块字节送到 to_terminal——保持抑制直到该次 consume 结束
    // (enterZmodem / feedOutput 里清除)
    suppressEcho = true
    const role = detection.get_session_role()
    if (role === "receive") {
      void handleReceiveSession(detection)
    } else {
      void handleSendSession(detection)
    }
    confirmed.value = true
    if (noConfirmTimer !== null) {
      window.clearTimeout(noConfirmTimer)
      noConfirmTimer = null
    }
  }

  function enterZmodem(rest: Uint8Array) {
    inZmodem = true
    busy.value = true
    confirmed.value = false
    aborting = false
    holdback = []
    if (holdFlushTimer !== null) {
      window.clearTimeout(holdFlushTimer)
      holdFlushTimer = null
    }
    sentry = new Zmodem.Sentry({
      to_terminal: (octets: number[]) => {
        if (!suppressEcho && octets.length > 0) opts.writeToTerm(new Uint8Array(octets))
      },
      sender: (octets) => {
        if (!opts.sendBinary(new Uint8Array(octets))) {
          // 线路已断:本地终止会话
          exitZmodem()
        }
      },
      on_detect: handleDetect,
      on_retract: () => {
        // 未确认的误报由 noConfirmTimer 兜底退出;已确认会话不受影响
      },
    })
    // 仅 SSH 后端解析 zmodem_state(其余中继无过滤器/保活,无需旁路)
    if (opts.isSsh()) opts.sendControl({ kind: "zmodem_state", active: true })
    // 误报兜底:进入模式后 8s 仍未确认会话则退回正常路径
    noConfirmTimer = window.setTimeout(() => {
      noConfirmTimer = null
      if (!confirmed.value) exitZmodem()
    }, NO_CONFIRM_TIMEOUT_MS)
    sentry.consume(rest)
    // 首次 consume 结束:ZRQINIT 回显吞没完毕,恢复透传
    suppressEcho = false
  }

  // ── 输出喂入(所有终端类型共用的检测入口)──

  /** combined 尾部可作签名前缀的最大长度(0..4),用于跨 chunk 嗅探 */
  function tailPrefixLen(bytes: Uint8Array): number {
    const n = Math.min(HOLD_MAX, bytes.length)
    for (let k = n; k > 0; k--) {
      let ok = true
      for (let j = 0; j < k; j++) {
        if (bytes[bytes.length - k + j] !== ZS_START[j]) {
          ok = false
          break
        }
      }
      if (ok) return k
    }
    return 0
  }

  function flushHoldback() {
    holdFlushTimer = null
    if (holdback.length === 0 || inZmodem) return
    const data = new Uint8Array(holdback)
    holdback = []
    opts.writeToTerm(data)
  }

  /** ws 二进制输出帧统一入口:正常态嗅探签名,命中后交给 Sentry */
  function feedOutput(bytes: Uint8Array) {
    if (inZmodem) {
      try {
        sentry?.consume(bytes)
      } catch (e) {
        // 协议引擎异常:退出传输回到正常路径,残余输出原样上屏
        exitZmodem()
        toast("error", "failed", { error: String(e) })
        opts.writeToTerm(bytes)
        return
      } finally {
        // 本次 consume 若触发了重新检测,其回显吞没到此为止
        suppressEcho = false
      }
      return
    }

    // 冷却期(用户刚取消):签名原样上屏,不重新接管,防对话框反复弹出
    const cooling = cooldownUntil > 0
    const combined = holdback.length > 0 ? new Uint8Array([...holdback, ...bytes]) : bytes
    if (!cooling) {
      const sigLen = ZS_START.length
      let found = -1
      const last = combined.length - sigLen
      for (let i = 0; i <= last; i++) {
        let ok = true
        for (let j = 0; j < sigLen; j++) {
          if (combined[i + j] !== ZS_START[j]) {
            ok = false
            break
          }
        }
        if (ok) {
          found = i
          break
        }
      }
      if (found >= 0) {
        if (found > 0) opts.writeToTerm(combined.slice(0, found))
        enterZmodem(combined.slice(found))
        return
      }
    }

    // 未命中:放行除前缀尾部外的全部字节,前缀扣留待下一 chunk
    const hold = cooling ? 0 : tailPrefixLen(combined)
    const outLen = combined.length - hold
    if (outLen > 0) opts.writeToTerm(combined.slice(0, outLen))
    holdback = hold > 0 ? Array.from(combined.slice(outLen)) : []
    // 流静默时定时放行扣留字节,避免普通输出(如以 * 结尾的行)滞留
    if (holdback.length > 0) {
      if (holdFlushTimer !== null) window.clearTimeout(holdFlushTimer)
      holdFlushTimer = window.setTimeout(flushHoldback, HOLD_FLUSH_MS)
    } else if (holdFlushTimer !== null) {
      window.clearTimeout(holdFlushTimer)
      holdFlushTimer = null
    }
  }

  /** zmodem 模式期间丢弃用户键盘输入(协议传输不接受键入);
   *  冷却期里任意键入解除冷却。返回 true 表示输入已被处理(丢弃) */
  function handleUserInput(): boolean {
    if (inZmodem) return true
    if (cooldownUntil > 0) {
      cooldownUntil = 0
    }
    return false
  }

  /** ws 断开/组件销毁时清理:不打扰、不留悬挂定时器 */
  function reset() {
    aborting = true
    try {
      session?.abort()
    } catch {
      // ignore
    }
    exitZmodem()
    items.value = []
    cooldownUntil = 0
    if (holdFlushTimer !== null) {
      window.clearTimeout(holdFlushTimer)
      holdFlushTimer = null
    }
    holdback = []
  }

  /** 面板手动清空已完成的历史条目 */
  function clearFinished() {
    items.value = items.value.filter((it) => it.status === "running")
  }

  return {
    items,
    busy,
    confirmed,
    feedOutput,
    handleUserInput,
    cancelSession,
    clearFinished,
    reset,
  }
}
