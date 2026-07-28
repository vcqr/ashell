import { request } from "./client"
import type { SysInfo } from "@/types"

/** 拉取当前 sid 关联主机的实时系统信息 */
export function getSysInfo(sid: string): Promise<SysInfo> {
  return request<SysInfo>("/api/ssh/sysinfo", { params: { sid } })
}
