/** zmodem.js 0.1.10 最小类型声明(包未随附 TS 类型,按 README 与源码提炼)。
 *  从浏览器聚合入口 src/zmodem_browser.js 导入(含 Browser 工具)。 */

declare module "zmodem.js/src/zmodem_browser.js" {
  /** ZMODEM 文件描述(send_offer 参数 / offer 的 get_details 返回) */
  export interface ZmodemFileDetails {
    name: string
    size?: number
    mtime?: number | Date
    mode?: number
    files_remaining?: number
    bytes_remaining?: number
    [key: string]: unknown
  }

  /** 单个文件的传输句柄:接收侧由 "offer" 事件给出,发送侧由 send_offer 兑现 */
  export interface ZmodemTransfer {
    on(event: "input", handler: (payload: Uint8Array<ArrayBuffer>) => void): void
    on(event: string, handler: (...args: unknown[]) => void): void
    /** 文件名/大小等元信息 */
    get_details(): ZmodemFileDetails
    /** 当前已传输字节偏移 */
    get_offset(): number
    /** 接收侧:接受本次传输,Promise 在文件收完时兑现 */
    accept(): Promise<void>
    /** 接收侧:跳过本文件(发送方继续下一个) */
    skip(): void
    /** 发送侧:发送一块数据 */
    send(chunk: Uint8Array): void
    /** 发送侧:终结本文件,Promise 在对端确认后兑现 */
    end(chunk?: Uint8Array): Promise<void>
  }

  /** 已确认的 ZMODEM 会话(receive = 对端发给我们 / send = 我们发给对端) */
  export interface ZmodemSession {
    type: "receive" | "send"
    on(event: "offer", handler: (xfer: ZmodemTransfer) => void): void
    on(event: "garbage", handler: (octets: number[]) => void): void
    on(event: "session_end", handler: () => void): void
    on(event: string, handler: (...args: unknown[]) => void): void
    /** 仅 receive 会话:发出 ZRINIT 启动握手(注册 offer 监听后调用) */
    start(): void
    /** 发送侧:发起一个文件 offer,对端接受返回 Transfer、跳过返回 undefined */
    send_offer(details: ZmodemFileDetails): Promise<ZmodemTransfer | undefined>
    /** 发送 ZMODEM abort 序列,强制终止会话 */
    abort(): void
    /** 发送侧:正常收尾(ZFIN 握手) */
    close(): void
    has_ended(): boolean
    aborted(): boolean
    /** 会话结束后残留的线路字节(接收侧) */
    get_trailing_bytes(): number[]
  }

  /** Sentry 解析出的"疑似 ZMODEM 起始";须 confirm/deny 二选一 */
  export interface ZmodemDetection {
    /** 确认会话:注册事件、接管线路,返回 Session */
    confirm(): ZmodemSession
    /** 拒绝:向线路发 abort 序列 */
    deny(): void
    /** 该检测是否仍有效(可能已被后续字节作废) */
    is_valid(): boolean
    /** "receive" = 对端 sz 发文件给我们;"send" = 对端 rz 收我们的文件 */
    get_session_role(): "receive" | "send"
  }

  export interface ZmodemSentryOptions {
    /** 线路字节解析后要上屏的部分(八位字节数组) */
    to_terminal: (octets: number[]) => void
    /** 要发往线路的字节(协议帧与 abort 序列) */
    sender: (octets: number[] | Uint8Array) => void
    on_detect: (detection: ZmodemDetection) => void
    on_retract: () => void
  }

  export interface ZmodemSentry {
    /** 喂入线路方向字节(数组或 ArrayBuffer/TypedArray) */
    consume(input: ArrayBuffer | Uint8Array | number[]): void
    get_confirmed_session(): ZmodemSession | null
  }

  export const Sentry: new (options: ZmodemSentryOptions) => ZmodemSentry

  export const Browser: {
    /** 批量发送文件(rz 上传),顺序逐个 offer;跳过不算错误 */
    send_files(
      session: ZmodemSession,
      files: File[] | FileList,
      options?: {
        on_offer_response?: (file: File, xfer: ZmodemTransfer | undefined) => void
        on_progress?: (file: File, xfer: ZmodemTransfer, piece: Uint8Array) => void
        on_file_complete?: (file: File, xfer: ZmodemTransfer) => void
      },
    ): Promise<void>
    /** 注入隐藏 <a download> 触发浏览器保存(Web 端下载兜底) */
    save_to_disk(packets: BlobPart[], name: string): void
  }

  /** 包入口为 CommonJS(Vite 预打包后以默认导出消费) */
  const Zmodem: {
    Sentry: new (options: ZmodemSentryOptions) => ZmodemSentry
    /** 数值编码工具(lrzsZ 互操作补丁构造 ZACK 帧用) */
    ENCODELIB: {
      pack_u32_le(n: number): number[]
    }
    Browser: {
      send_files(
        session: ZmodemSession,
        files: File[] | FileList,
        options?: {
          on_offer_response?: (file: File, xfer: ZmodemTransfer | undefined) => void
          on_progress?: (file: File, xfer: ZmodemTransfer, piece: Uint8Array) => void
          on_file_complete?: (file: File, xfer: ZmodemTransfer) => void
        },
      ): Promise<void>
      save_to_disk(packets: BlobPart[], name: string): void
    }
  }
  export default Zmodem
}
