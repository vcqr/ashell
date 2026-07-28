import { computed, onBeforeUnmount, ref, watch, watchEffect } from "vue";
import { darkTheme, type GlobalThemeOverrides } from "naive-ui";
import { useI18n } from "vue-i18n";
import { useTerminalStore } from "@/stores/terminal";

export type ThemeMode = "system" | "dark" | "light";
export type ResolvedTheme = "dark" | "light";

const THEME_KEY = "ashell:theme-mode";

const sharedThemeOverrides: GlobalThemeOverrides = {
  common: {
    primaryColor: "#7c5cff",
    primaryColorHover: "#9277ff",
    primaryColorPressed: "#6a4ae6",
    primaryColorSuppl: "#7c5cff",
    borderRadius: "8px",
    fontFamily:
      "'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif",
    fontFamilyMono: "'Fira Code', 'JetBrains Mono', Menlo, Consolas, monospace",
  },
};

function loadThemeMode(): ThemeMode {
  if (typeof localStorage === "undefined") return "system";
  const raw = localStorage.getItem(THEME_KEY);
  return raw === "dark" || raw === "light" || raw === "system" ? raw : "system";
}

function getSystemTheme(): ResolvedTheme {
  if (typeof window === "undefined") return "dark";
  return window.matchMedia("(prefers-color-scheme: light)").matches
    ? "light"
    : "dark";
}

/**
 * 主题管理：主题模式（system/dark/light）、系统主题监听、Naive UI overrides。
 * themeMode 为可写 ref，供 SettingsModal 通过 v-model 修改。
 */
export function useTheme() {
  const terminalStore = useTerminalStore();
  const { t } = useI18n();

  const darkThemeOverrides = computed<GlobalThemeOverrides>(() => ({
    ...sharedThemeOverrides,
    common: {
      ...sharedThemeOverrides.common,
      bodyColor: `rgba(15, 17, 21, ${terminalStore.windowOpacity})`,
      cardColor: `rgba(22, 25, 31, ${terminalStore.windowOpacity})`,
      modalColor: `rgba(22, 25, 31, ${terminalStore.windowOpacity})`,
      popoverColor: `rgba(28, 32, 39, ${terminalStore.windowOpacity})`,
    },
    Layout: {
      color: `rgba(15, 17, 21, ${terminalStore.windowOpacity})`,
      siderColor: `rgba(19, 22, 28, ${terminalStore.windowOpacity})`,
      headerColor: `rgba(19, 22, 28, ${terminalStore.windowOpacity})`,
    },
    Tree: {
      nodeTextColor: "rgba(255,255,255,0.85)",
    },
  }));

  const lightThemeOverrides = computed<GlobalThemeOverrides>(() => ({
    ...sharedThemeOverrides,
    common: {
      ...sharedThemeOverrides.common,
      bodyColor: `rgba(244, 246, 251, ${terminalStore.windowOpacity})`,
      cardColor: `rgba(255, 255, 255, ${terminalStore.windowOpacity})`,
      modalColor: `rgba(255, 255, 255, ${terminalStore.windowOpacity})`,
      popoverColor: `rgba(255, 255, 255, ${terminalStore.windowOpacity})`,
    },
    Layout: {
      color: `rgba(244, 246, 251, ${terminalStore.windowOpacity})`,
      siderColor: `rgba(255, 255, 255, ${terminalStore.windowOpacity})`,
      headerColor: `rgba(255, 255, 255, ${terminalStore.windowOpacity})`,
    },
    Tree: {
      nodeTextColor: "rgba(20,24,33,0.86)",
    },
  }));

  const themeMode = ref<ThemeMode>(loadThemeMode());
  const systemTheme = ref<ResolvedTheme>(getSystemTheme());

  const resolvedTheme = computed<ResolvedTheme>(() =>
    themeMode.value === "system" ? systemTheme.value : themeMode.value,
  );
  const naiveTheme = computed(() =>
    resolvedTheme.value === "dark" ? darkTheme : null,
  );
  const themeOverrides = computed(() =>
    resolvedTheme.value === "dark"
      ? darkThemeOverrides.value
      : lightThemeOverrides.value,
  );
  const themeTitle = computed(() => {
    if (themeMode.value === "system") {
      const resolved =
        resolvedTheme.value === "dark" ? t("app.theme.dark") : t("app.theme.light");
      return t("app.theme.followSystem", { resolved });
    }
    const mode =
      themeMode.value === "dark" ? t("app.theme.dark") : t("app.theme.light");
    return t("app.theme.current", { mode });
  });

  watch(themeMode, (mode) => {
    try {
      localStorage.setItem(THEME_KEY, mode);
    } catch {
      // ignore
    }
  });

  watchEffect(() => {
    if (typeof document === "undefined") return;
    document.documentElement.dataset.ashellTheme = resolvedTheme.value;
  });

  if (typeof window !== "undefined") {
    const media = window.matchMedia("(prefers-color-scheme: light)");
    const onSystemThemeChange = () => {
      systemTheme.value = getSystemTheme();
    };
    media.addEventListener("change", onSystemThemeChange);
    onBeforeUnmount(() =>
      media.removeEventListener("change", onSystemThemeChange),
    );
  }

  return { themeMode, resolvedTheme, naiveTheme, themeOverrides, themeTitle };
}
