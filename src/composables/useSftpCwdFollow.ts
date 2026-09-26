import { ref, watch } from "vue";

const KEY = "ashell:sftp-cwd-follow";

function load(): boolean {
  if (typeof localStorage === "undefined") return false;
  return localStorage.getItem(KEY) === "1";
}

/**
 * SFTP 远程栏「目录跟随」开关（模块级单例，localStorage 持久化）。
 * 开关只控制远程栏是否跳转；SSH 会话是否上报 cwd 由连接时的
 * cwd_report 参数决定（TerminalView 建连时读取本状态），
 * 会话中途开启跟随需重连终端才能收到上报。
 */
const sftpCwdFollow = ref(load());

watch(sftpCwdFollow, (v) => {
  try {
    localStorage.setItem(KEY, String(v));
  } catch {
    // ignore
  }
});

export function useSftpCwdFollow() {
  return sftpCwdFollow;
}
