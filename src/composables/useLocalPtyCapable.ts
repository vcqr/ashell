import { ref } from "vue";
import { terminalCapable } from "@/api/local";

/**
 * 本地终端可用性（模块级单例）：Windows 上探测 ConPTY 能力（老系统 /
 * Server 2016 / Wine 为 false），TabBar + 菜单、欢迎页入口据此置灰，
 * 避免点开只见 fatal 断连。未知/查询完成前默认 true（乐观放行），
 * 查询失败也按可用处理——真不可用时有连接侧 fatal 兜底。
 */
const capable = ref(true);

let started = false;

export function useLocalPtyCapable() {
  if (!started) {
    started = true;
    void terminalCapable().then((v) => {
      capable.value = v;
    });
  }
  return capable;
}
