import type { NotificationApi } from "naive-ui"

/**
 * 全局通知 API 桥接：NNotificationProvider 下的 NotifyBridge 组件在
 * setup 中调用 setNotificationApi 注入实例；非组件上下文（如 useTabs
 * 组合式函数）通过 notify() 取用。未注入（如独立 SFTP/AI 窗口）时返回
 * null，调用方需自行降级为不提示。
 */
let notificationApi: NotificationApi | null = null

export function setNotificationApi(api: NotificationApi) {
  notificationApi = api
}

export function notify(): NotificationApi | null {
  return notificationApi
}
