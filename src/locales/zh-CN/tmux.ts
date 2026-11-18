export default {
  title: "tmux 会话",
  titleWithHost: "tmux 会话 · {host}",
  dragToResize: "拖动调整宽度",
  refresh: "刷新",
  close: "关闭",
  attach: "接入",
  needSession: "请先建立连接",
  empty: "远端没有 tmux 会话",
  killConfirm: "终止 tmux 会话「{name}」？其中未保存的工作将丢失。",
  columns: {
    name: "会话名",
    windows: "窗口",
    attached: "已接",
    created: "创建时间",
    actions: "操作",
  },
  message: {
    loadFailed: "tmux 会话加载失败",
    killed: "已终止会话「{name}」",
    killFailed: "终止会话失败：{error}",
  },
}
