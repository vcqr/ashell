/** 极简日志：默认 info 以上，WEIXIN_BRIDGE_DEBUG=1 时输出 debug。 */

function debugEnabled(): boolean {
  return process.env.WEIXIN_BRIDGE_DEBUG === "1";
}

function ts(): string {
  return new Date().toISOString();
}

export const logger = {
  debug(msg: string): void {
    if (debugEnabled()) console.log(`[debug ${ts()}] ${msg}`);
  },
  info(msg: string): void {
    console.log(`[info ${ts()}] ${msg}`);
  },
  warn(msg: string): void {
    console.warn(`[warn ${ts()}] ${msg}`);
  },
  error(msg: string): void {
    console.error(`[error ${ts()}] ${msg}`);
  },
};
