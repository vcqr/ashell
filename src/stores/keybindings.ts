import { defineStore } from "pinia";
import { reactive, ref, watch } from "vue";
import { detectMac } from "@/utils/platform";

export interface KeyBinding {
  key: string;
  ctrl: boolean;
  meta: boolean;
  shift: boolean;
  alt: boolean;
}

export type ShortcutActionId =
  | "tab.new"
  | "tab.close"
  | "tab.next"
  | "tab.prev"
  | "tab.jump"
  | "search.toggle"
  | "panel.hosts"
  | "panel.settings"
  | "panel.ai"
  | "panel.sftp"
  | "panel.aiProviders"
  | "panel.hostInfo"
  | "panel.forward"
  | "panel.template"
  | "panel.activityBar";

export type ShortcutCategory = "tabs" | "search" | "panels";

export interface ShortcutActionDef {
  id: ShortcutActionId;
  category: ShortcutCategory;
  defaults: { mac: KeyBinding; win: KeyBinding };
}

export const isMac = detectMac();

const STORAGE_KEY = "ashell:keybindings";

export const SHORTCUT_ACTIONS: ShortcutActionDef[] = [
  {
    id: "tab.new",
    category: "tabs",
    defaults: {
      mac: { key: "t", ctrl: false, meta: true, shift: false, alt: false },
      win: { key: "t", ctrl: true, meta: false, shift: false, alt: false },
    },
  },
  {
    id: "tab.close",
    category: "tabs",
    defaults: {
      mac: { key: "w", ctrl: false, meta: true, shift: true, alt: false },
      win: { key: "w", ctrl: true, meta: false, shift: true, alt: false },
    },
  },
  {
    id: "tab.next",
    category: "tabs",
    defaults: {
      mac: { key: "tab", ctrl: true, meta: false, shift: false, alt: false },
      win: { key: "tab", ctrl: true, meta: false, shift: false, alt: false },
    },
  },
  {
    id: "tab.prev",
    category: "tabs",
    defaults: {
      mac: { key: "tab", ctrl: true, meta: false, shift: true, alt: false },
      win: { key: "tab", ctrl: true, meta: false, shift: true, alt: false },
    },
  },
  {
    id: "tab.jump",
    category: "tabs",
    defaults: {
      mac: { key: "digit", ctrl: false, meta: true, shift: false, alt: false },
      win: { key: "digit", ctrl: true, meta: false, shift: false, alt: true },
    },
  },
  {
    id: "search.toggle",
    category: "search",
    defaults: {
      mac: { key: "f", ctrl: false, meta: true, shift: false, alt: false },
      win: { key: "f", ctrl: true, meta: false, shift: false, alt: false },
    },
  },
  {
    id: "panel.hosts",
    category: "panels",
    defaults: {
      mac: { key: "h", ctrl: false, meta: true, shift: true, alt: false },
      win: { key: "h", ctrl: true, meta: false, shift: true, alt: false },
    },
  },
  {
    id: "panel.settings",
    category: "panels",
    defaults: {
      mac: { key: ",", ctrl: false, meta: true, shift: false, alt: false },
      win: { key: ",", ctrl: true, meta: false, shift: false, alt: false },
    },
  },
  {
    id: "panel.ai",
    category: "panels",
    defaults: {
      mac: { key: "a", ctrl: false, meta: true, shift: true, alt: false },
      win: { key: "a", ctrl: true, meta: false, shift: true, alt: false },
    },
  },
  {
    id: "panel.sftp",
    category: "panels",
    defaults: {
      mac: { key: "s", ctrl: false, meta: true, shift: true, alt: false },
      win: { key: "s", ctrl: true, meta: false, shift: true, alt: false },
    },
  },
  {
    id: "panel.aiProviders",
    category: "panels",
    defaults: {
      mac: { key: "m", ctrl: false, meta: true, shift: true, alt: false },
      win: { key: "m", ctrl: true, meta: false, shift: true, alt: false },
    },
  },
  {
    id: "panel.hostInfo",
    category: "panels",
    defaults: {
      mac: { key: "i", ctrl: false, meta: true, shift: true, alt: false },
      win: { key: "i", ctrl: true, meta: false, shift: true, alt: false },
    },
  },
  {
    id: "panel.forward",
    category: "panels",
    defaults: {
      mac: { key: "p", ctrl: false, meta: true, shift: true, alt: false },
      win: { key: "p", ctrl: true, meta: false, shift: true, alt: false },
    },
  },
  {
    id: "panel.template",
    category: "panels",
    defaults: {
      mac: { key: "l", ctrl: false, meta: true, shift: true, alt: false },
      win: { key: "l", ctrl: true, meta: false, shift: true, alt: false },
    },
  },
  {
    id: "panel.activityBar",
    category: "panels",
    defaults: {
      mac: { key: "b", ctrl: false, meta: true, shift: false, alt: false },
      win: { key: "b", ctrl: true, meta: false, shift: false, alt: false },
    },
  },
];

const ACTION_MAP = Object.fromEntries(
  SHORTCUT_ACTIONS.map((a) => [a.id, a]),
) as Record<ShortcutActionId, ShortcutActionDef>;

type KeybindingsMap = Record<ShortcutActionId, KeyBinding | null>;

function defaultBindings(): KeybindingsMap {
  const result = {} as KeybindingsMap;
  for (const def of SHORTCUT_ACTIONS) {
    result[def.id] = isMac ? { ...def.defaults.mac } : { ...def.defaults.win };
  }
  return result;
}

function isValidBinding(v: unknown): v is KeyBinding {
  if (typeof v !== "object" || v === null) return false;
  const b = v as Record<string, unknown>;
  return (
    typeof b.key === "string" &&
    b.key.length > 0 &&
    typeof b.ctrl === "boolean" &&
    typeof b.meta === "boolean" &&
    typeof b.shift === "boolean" &&
    typeof b.alt === "boolean"
  );
}

function loadConfig(): KeybindingsMap {
  const defaults = defaultBindings();
  if (typeof localStorage === "undefined") return defaults;
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return defaults;
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    const result = { ...defaults };
    for (const def of SHORTCUT_ACTIONS) {
      const v = parsed[def.id];
      if (v === null) {
        result[def.id] = null;
      } else if (isValidBinding(v)) {
        result[def.id] = v;
      }
    }
    return result;
  } catch {
    return defaults;
  }
}

/** Match a KeyboardEvent against a binding. Returns false if binding is null/empty. */
export function matchesBinding(
  binding: KeyBinding | null,
  e: KeyboardEvent,
): boolean {
  if (!binding || !binding.key) return false;
  if (e.ctrlKey !== binding.ctrl) return false;
  if (e.metaKey !== binding.meta) return false;
  if (e.shiftKey !== binding.shift) return false;
  if (e.altKey !== binding.alt) return false;
  const key = e.key.toLowerCase();
  const bk = binding.key.toLowerCase();
  if (bk === "digit") return /^[1-9]$/.test(key);
  return key === bk;
}

/** Format a binding into display segments (e.g., ["⌘", "T"] or ["Ctrl", "Shift", "W"]). */
export function formatBinding(binding: KeyBinding | null): string[] {
  if (!binding || !binding.key) return [];
  const parts: string[] = [];
  if (isMac) {
    if (binding.ctrl) parts.push("⌃");
    if (binding.alt) parts.push("⌥");
    if (binding.shift) parts.push("⇧");
    if (binding.meta) parts.push("⌘");
  } else {
    if (binding.ctrl) parts.push("Ctrl");
    if (binding.alt) parts.push("Alt");
    if (binding.shift) parts.push("Shift");
    if (binding.meta) parts.push("Win");
  }
  let keyDisplay: string;
  switch (binding.key.toLowerCase()) {
    case "digit":
      keyDisplay = "1-9";
      break;
    case "tab":
      keyDisplay = "Tab";
      break;
    case " ":
      keyDisplay = "Space";
      break;
    case "enter":
      keyDisplay = "Enter";
      break;
    case "escape":
      keyDisplay = "Esc";
      break;
    case "backspace":
      keyDisplay = "⌫";
      break;
    case "delete":
      keyDisplay = "Del";
      break;
    case "arrowup":
      keyDisplay = "↑";
      break;
    case "arrowdown":
      keyDisplay = "↓";
      break;
    case "arrowleft":
      keyDisplay = "←";
      break;
    case "arrowright":
      keyDisplay = "→";
      break;
    default:
      keyDisplay =
        binding.key.length === 1 ? binding.key.toUpperCase() : binding.key;
  }
  parts.push(keyDisplay);
  return parts;
}

export const useKeybindingStore = defineStore("keybindings", () => {
  const initial = loadConfig();
  const bindings = reactive<KeybindingsMap>({ ...initial });

  /** 录制中标志——为 true 时所有快捷键处理器跳过拦截，让录制器独占按键 */
  const recording = ref(false);

  function persist() {
    if (typeof localStorage === "undefined") return;
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(bindings));
    } catch {
      // ignore
    }
  }

  watch(bindings, persist, { deep: true });

  function getBinding(id: ShortcutActionId): KeyBinding | null {
    return bindings[id];
  }

  function setBinding(id: ShortcutActionId, binding: KeyBinding | null) {
    if (binding && id === "tab.jump") {
      binding = { ...binding, key: "digit" };
    }
    bindings[id] = binding;
  }

  function resetBinding(id: ShortcutActionId) {
    const def = ACTION_MAP[id];
    bindings[id] = isMac
      ? { ...def.defaults.mac }
      : { ...def.defaults.win };
  }

  function resetAll() {
    const defaults = defaultBindings();
    for (const def of SHORTCUT_ACTIONS) {
      bindings[def.id] = defaults[def.id];
    }
  }

  function getDefaultBinding(id: ShortcutActionId): KeyBinding {
    const def = ACTION_MAP[id];
    return isMac
      ? { ...def.defaults.mac }
      : { ...def.defaults.win };
  }

  return {
    bindings,
    recording,
    getBinding,
    setBinding,
    resetBinding,
    resetAll,
    getDefaultBinding,
  };
});
