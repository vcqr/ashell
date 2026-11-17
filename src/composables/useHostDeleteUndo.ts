import { h, ref, type Ref } from "vue";
import { NButton, useMessage, useNotification, type NotificationReactive } from "naive-ui";
import i18n from "@/locales";
import { useHostStore } from "@/stores/hosts";

/** 撤销窗口时长（秒）：与断线保留通知（useTabs）同款的倒计时通知形态 */
const UNDO_SECONDS = 6;

interface PendingUndo {
  ids: number[];
  firstName: string;
  secLeft: Ref<number>;
  timer: number | null;
  notif: NotificationReactive | null;
}

/**
 * 主机删除撤销：删除是即时的软删（is_del=1），本 composable 只负责
 * 6 秒撤销窗口的通知与恢复调用。窗口过期仅关通知（软删行留在库里，
 * 是未来回收站的天然基础）；撤销调 POST /api/hosts/batch-restore。
 * 同一时间只保留一条：新删除会替换上一个未决窗口。
 */
export function useHostDeleteUndo() {
  const notification = useNotification();
  const message = useMessage();
  const hostsStore = useHostStore();

  let pending: PendingUndo | null = null;

  function stopTimerOf(p: PendingUndo) {
    if (p.timer !== null) {
      window.clearInterval(p.timer);
      p.timer = null;
    }
  }

  function dismiss(p: PendingUndo) {
    stopTimerOf(p);
    p.notif?.destroy();
    if (pending === p) pending = null;
  }

  /** 撤销：恢复软删主机，按结果提示（全部恢复 / 部分恢复含跳过名单） */
  async function undo(p: PendingUndo) {
    dismiss(p);
    const gt = i18n.global.t;
    try {
      const r = await hostsStore.restoreHosts(p.ids);
      if (r.skipped.length === 0) {
        message.success(gt("hosts.message.restored"));
        return;
      }
      const reasonText = (reason: string) =>
        reason === "orphan"
          ? gt("hosts.message.restoreSkipOrphan")
          : gt("hosts.message.restoreSkipJumpTarget");
      const names = r.skipped
        .map((s) => `${s.name}（${reasonText(s.reason)}）`)
        .join("、");
      message.warning(
        gt("hosts.message.restoredPartial", {
          restored: r.restored,
          names,
        }),
      );
    } catch (e) {
      message.error(gt("hosts.message.restoreFailed", { error: String(e) }));
    }
  }

  /**
   * 删除成功后调用：弹出 6 秒可撤销通知。
   * ids 为本次软删的主机 id 集合，firstName 用于单台文案。
   */
  function notifyDeleted(ids: number[], firstName: string) {
    // 新删除替换上一个未决窗口（旧窗口的软删行留在库里，不再可撤销）
    if (pending) dismiss(pending);
    if (ids.length === 0) return;
    if (!notification) return;
    const gt = i18n.global.t;

    const p: PendingUndo = {
      ids,
      firstName,
      secLeft: ref(UNDO_SECONDS),
      timer: null,
      notif: null,
    };
    p.notif = notification.create({
      title:
        ids.length === 1
          ? gt("hosts.message.deletedToastTitle", { name: firstName })
          : gt("hosts.message.deletedToastTitleBatch", { n: ids.length }),
      content: () =>
        h("span", null, gt("hosts.message.deletedToastHint", { sec: p.secLeft.value })),
      action: () =>
        h(
          NButton,
          { size: "tiny", secondary: true, onClick: () => void undo(p) },
          { default: () => gt("hosts.message.undo") },
        ),
      duration: 0,
      closable: false,
    });
    pending = p;
    p.timer = window.setInterval(() => {
      p.secLeft.value -= 1;
      if (p.secLeft.value <= 0) dismiss(p);
    }, 1000);
  }

  return { notifyDeleted };
}
