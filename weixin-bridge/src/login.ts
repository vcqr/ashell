/**
 * 扫码登录流程（CLI）。
 * 对齐 @tencent-weixin/openclaw-weixin 的 src/auth/login-qr.ts：
 * get_bot_qrcode 取码 → 终端展示 → 长轮询 get_qrcode_status，支持
 * 配对数字校验（need_verifycode）、过期刷新、IDC 重定向与已绑定判定。
 */

import { apiGetFetch, apiPostFetch, DEFAULT_BASE_URL } from "./protocol/api.js";
import { loadCredentials, saveCredentials } from "./store.js";
import { logger } from "./logger.js";

/** 插件当前渠道构建使用的 bot_type。 */
const DEFAULT_ILINK_BOT_TYPE = "3";

/** 二维码状态长轮询超时。 */
const QR_LONG_POLL_TIMEOUT_MS = 35_000;
/** 总登录时限。 */
const LOGIN_TIMEOUT_MS = 480_000;
/** 二维码最多自动刷新次数。 */
const MAX_QR_REFRESH_COUNT = 3;

const FIXED_BASE_URL = DEFAULT_BASE_URL;

interface QRCodeResponse {
  qrcode: string;
  /** 二维码内容（URL），终端渲染或手机扫码均用它。 */
  qrcode_img_content: string;
}

type QRStatus =
  | "wait"
  | "scaned"
  | "confirmed"
  | "expired"
  | "scaned_but_redirect"
  | "need_verifycode"
  | "verify_code_blocked"
  | "binded_redirect";

interface StatusResponse {
  status: QRStatus;
  bot_token?: string;
  ilink_bot_id?: string;
  baseurl?: string;
  /** 扫码者的用户 ID。 */
  ilink_user_id?: string;
  /** scaned_but_redirect 时要求切换的轮询主机。 */
  redirect_host?: string;
}

/** 已登录 token 随取码请求上送，服务端据此返回 binded_redirect（已绑定过）。 */
function localTokenList(): string[] {
  const cred = loadCredentials();
  const token = cred?.token?.trim();
  return token ? [token] : [];
}

async function fetchQRCode(apiBaseUrl: string): Promise<QRCodeResponse> {
  logger.info(`获取二维码: ${apiBaseUrl} bot_type=${DEFAULT_ILINK_BOT_TYPE}`);
  const rawText = await apiPostFetch({
    baseUrl: apiBaseUrl,
    endpoint: `ilink/bot/get_bot_qrcode?bot_type=${encodeURIComponent(DEFAULT_ILINK_BOT_TYPE)}`,
    body: JSON.stringify({ local_token_list: localTokenList() }),
    label: "fetchQRCode",
  });
  return JSON.parse(rawText) as QRCodeResponse;
}

async function pollQRStatus(
  apiBaseUrl: string,
  qrcode: string,
  verifyCode?: string,
): Promise<StatusResponse> {
  try {
    let endpoint = `ilink/bot/get_qrcode_status?qrcode=${encodeURIComponent(qrcode)}`;
    if (verifyCode) {
      endpoint += `&verify_code=${encodeURIComponent(verifyCode)}`;
    }
    const rawText = await apiGetFetch({
      baseUrl: apiBaseUrl,
      endpoint,
      timeoutMs: QR_LONG_POLL_TIMEOUT_MS,
      label: "pollQRStatus",
    });
    return JSON.parse(rawText) as StatusResponse;
  } catch (err) {
    if (err instanceof Error && err.name === "AbortError") {
      return { status: "wait" };
    }
    // 网关超时（如 Cloudflare 524）或其他网络错误，视为等待继续轮询。
    logger.warn(`pollQRStatus: 网络/网关错误，继续重试: ${String(err)}`);
    return { status: "wait" };
  }
}

async function displayQRCode(qrcodeUrl: string): Promise<void> {
  try {
    const qrterm = await import("qrcode-terminal");
    // CJS 包经 ESM 动态导入后挂在 default 上。
    qrterm.default.generate(qrcodeUrl, { small: true });
  } catch {
    // 依赖缺失或渲染失败时退化为纯链接展示。
  }
  process.stdout.write(`若二维码无法显示或扫不出来，用手机微信访问以下链接：\n${qrcodeUrl}\n`);
}

/** 从 stdin 读一行，返回 trim 后的字符串。 */
function readLine(prompt: string): Promise<string> {
  process.stdout.write(prompt);
  return new Promise((resolve) => {
    let input = "";
    const onData = (chunk: Buffer | string) => {
      input += chunk.toString();
      if (input.includes("\n")) {
        process.stdin.removeListener("data", onData);
        process.stdin.pause();
        resolve(input.trim());
      }
    };
    process.stdin.resume();
    process.stdin.setEncoding("utf-8");
    process.stdin.on("data", onData);
  });
}

export interface LoginResult {
  botToken: string;
  botId: string;
  baseUrl?: string;
  userId?: string;
}

/** 执行完整扫码登录，成功返回凭证并已写入本地存储。 */
export async function loginWithQr(): Promise<LoginResult> {
  let qrcode = "";
  let qrcodeUrl = "";
  let currentBaseUrl = FIXED_BASE_URL;
  let pendingVerifyCode: string | undefined;
  let qrRefreshCount = 1;
  let scannedPrinted = false;

  const first = await fetchQRCode(FIXED_BASE_URL);
  qrcode = first.qrcode;
  qrcodeUrl = first.qrcode_img_content;
  process.stdout.write("\n请用手机微信扫描以下二维码完成连接：\n\n");
  await displayQRCode(qrcodeUrl);

  const deadline = Date.now() + LOGIN_TIMEOUT_MS;
  while (Date.now() < deadline) {
    const status = await pollQRStatus(currentBaseUrl, qrcode, pendingVerifyCode);

    switch (status.status) {
      case "wait":
        break;
      case "scaned": {
        if (pendingVerifyCode) {
          // 带上配对码后返回 scaned，说明校验通过。
          logger.info("配对码校验通过，继续轮询");
          pendingVerifyCode = undefined;
        }
        if (!scannedPrinted) {
          process.stdout.write("\n已扫码，正在验证…\n");
          scannedPrinted = true;
        }
        break;
      }
      case "need_verifycode": {
        const prompt = pendingVerifyCode
          ? "❌ 数字不匹配，请重新输入："
          : "请输入手机微信上显示的数字以继续：";
        pendingVerifyCode = (await readLine(prompt)) || undefined;
        continue;
      }
      case "expired": {
        qrRefreshCount++;
        if (qrRefreshCount > MAX_QR_REFRESH_COUNT) {
          throw new Error("二维码多次失效，登录中止。请稍后重试。");
        }
        process.stdout.write(`\n⏳ 二维码已过期，正在刷新…(${qrRefreshCount}/${MAX_QR_REFRESH_COUNT})\n`);
        const refreshed = await fetchQRCode(FIXED_BASE_URL);
        qrcode = refreshed.qrcode;
        qrcodeUrl = refreshed.qrcode_img_content;
        scannedPrinted = false;
        process.stdout.write("\n🔄 二维码已更新，请重新扫描：\n\n");
        await displayQRCode(qrcodeUrl);
        break;
      }
      case "verify_code_blocked": {
        process.stdout.write("\n⛔ 配对码多次输入错误，请稍后再试。\n");
        pendingVerifyCode = undefined;
        qrRefreshCount++;
        if (qrRefreshCount > MAX_QR_REFRESH_COUNT) {
          throw new Error("配对码多次错误，登录中止。请稍后重试。");
        }
        const refreshed = await fetchQRCode(FIXED_BASE_URL);
        qrcode = refreshed.qrcode;
        qrcodeUrl = refreshed.qrcode_img_content;
        scannedPrinted = false;
        process.stdout.write(`\n🔄 二维码已更新，请重新扫描(${qrRefreshCount}/${MAX_QR_REFRESH_COUNT})：\n\n`);
        await displayQRCode(qrcodeUrl);
        break;
      }
      case "binded_redirect": {
        throw new Error(
          "该微信已绑定过此机器人且未返回新凭证。若之前登录过本机，直接 `npm start` 即可；否则请删除凭证后重试。",
        );
      }
      case "scaned_but_redirect": {
        if (status.redirect_host) {
          currentBaseUrl = `https://${status.redirect_host}`;
          logger.info(`IDC 重定向，切换轮询主机: ${status.redirect_host}`);
        }
        break;
      }
      case "confirmed": {
        if (!status.bot_token || !status.ilink_bot_id) {
          throw new Error("登录确认失败：服务端未返回 bot_token 或 ilink_bot_id。");
        }
        saveCredentials({
          token: status.bot_token,
          botId: status.ilink_bot_id,
          baseUrl: status.baseurl?.trim() || FIXED_BASE_URL,
          userId: status.ilink_user_id?.trim() || undefined,
        });
        return {
          botToken: status.bot_token,
          botId: status.ilink_bot_id,
          baseUrl: status.baseurl,
          userId: status.ilink_user_id,
        };
      }
    }

    await new Promise((r) => setTimeout(r, 1000));
  }

  throw new Error("登录超时，请重试。");
}
