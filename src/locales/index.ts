import { createI18n } from "vue-i18n"
import type { Ref } from "vue"
import { ref } from "vue"

import zhCNCommon from "./zh-CN/common"
import zhCNApp from "./zh-CN/app"
import zhCNSettings from "./zh-CN/settings"
import zhCNHosts from "./zh-CN/hosts"
import zhCNTerminal from "./zh-CN/terminal"
import zhCNSftp from "./zh-CN/sftp"
import zhCNHostInfo from "./zh-CN/hostInfo"
import zhCNForward from "./zh-CN/forward"
import zhCNAi from "./zh-CN/ai"
import zhCNBroadcast from "./zh-CN/broadcast"

import enUSCommon from "./en-US/common"
import enUSApp from "./en-US/app"
import enUSSettings from "./en-US/settings"
import enUSHosts from "./en-US/hosts"
import enUSTerminal from "./en-US/terminal"
import enUSSftp from "./en-US/sftp"
import enUSHostInfo from "./en-US/hostInfo"
import enUSForward from "./en-US/forward"
import enUSAi from "./en-US/ai"
import enUSBroadcast from "./en-US/broadcast"

export type AppLocale = "zh-CN" | "en-US"
export type LocalePreference = AppLocale | "auto"

const STORAGE_KEY = "ashell:locale"

const messages = {
  "zh-CN": {
    common: zhCNCommon,
    app: zhCNApp,
    settings: zhCNSettings,
    hosts: zhCNHosts,
    terminal: zhCNTerminal,
    sftp: zhCNSftp,
    hostInfo: zhCNHostInfo,
    forward: zhCNForward,
    ai: zhCNAi,
    broadcast: zhCNBroadcast,
  },
  "en-US": {
    common: enUSCommon,
    app: enUSApp,
    settings: enUSSettings,
    hosts: enUSHosts,
    terminal: enUSTerminal,
    sftp: enUSSftp,
    hostInfo: enUSHostInfo,
    forward: enUSForward,
    ai: enUSAi,
    broadcast: enUSBroadcast,
  },
}

function detectSystemLocale(): AppLocale {
  if (typeof navigator === "undefined") return "en-US"
  const lang = navigator.language || navigator.languages?.[0] || "en-US"
  return lang.toLowerCase().startsWith("zh") ? "zh-CN" : "en-US"
}

function loadPreference(): LocalePreference {
  if (typeof localStorage === "undefined") return "auto"
  const raw = localStorage.getItem(STORAGE_KEY)
  if (raw === "zh-CN" || raw === "en-US" || raw === "auto") return raw
  return "auto"
}

function resolveLocale(pref: LocalePreference): AppLocale {
  return pref === "auto" ? detectSystemLocale() : pref
}

export const localePreference = ref<LocalePreference>(loadPreference())

export const currentLocale: Ref<AppLocale> = ref<AppLocale>(
  resolveLocale(localePreference.value),
)

export function setLocalePreference(pref: LocalePreference) {
  localePreference.value = pref
  try {
    localStorage.setItem(STORAGE_KEY, pref)
  } catch {
    // ignore
  }
  const resolved = resolveLocale(pref)
  currentLocale.value = resolved
  i18n.global.locale.value = resolved
}

const i18n = createI18n({
  legacy: false,
  locale: currentLocale.value,
  fallbackLocale: "en-US",
  messages,
})

export default i18n
