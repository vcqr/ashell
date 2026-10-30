import { computed, onBeforeUnmount, ref, watch, watchEffect } from "vue";
import { darkTheme, type GlobalThemeOverrides } from "naive-ui";
import { useI18n } from "vue-i18n";
import { useTerminalStore } from "@/stores/terminal";

export type ThemeMode = "system" | "dark" | "light";
export type ResolvedTheme = "dark" | "light";

const THEME_KEY = "ashell:theme-mode";

// 主色按主题分档：暗色用亮档，亮色主题底是白/浅灰，压白字的实心按钮
// 需要深一档才能过 WCAG AA（4.5:1），浅色下直接沿用亮档只有 ~4.3:1
const darkPrimaryOverrides = {
  primaryColor: "#6d5ae8",
  primaryColorHover: "#8272f0",
  primaryColorPressed: "#5a47cf",
  primaryColorSuppl: "#6d5ae8",
};

const lightPrimaryOverrides = {
  primaryColor: "#5f4fd9",
  primaryColorHover: "#6d5ae8",
  primaryColorPressed: "#5342c2",
  primaryColorSuppl: "#5f4fd9",
};

const sharedThemeOverrides: GlobalThemeOverrides = {
  common: {
    borderRadius: "8px",
    fontFamily:
      "'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif",
    fontFamilyMono: "'Fira Code', 'JetBrains Mono', Menlo, Consolas, monospace",
  },
  Button: {
    // 品牌紫明度偏低，naive 暗色主题的实心主色按钮默认黑字（baseColor=#000，
    // 为其亮薄荷绿设计），观感差；统一改为白字
    textColorPrimary: "#fff",
    textColorHoverPrimary: "#fff",
    textColorPressedPrimary: "#fff",
    textColorFocusPrimary: "#fff",
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
      ...darkPrimaryOverrides,
      bodyColor: `rgba(19, 19, 22, ${terminalStore.windowOpacity})`,
      cardColor: `rgba(24, 24, 29, ${terminalStore.windowOpacity})`,
      modalColor: `rgba(24, 24, 29, ${terminalStore.windowOpacity})`,
      popoverColor: `rgba(33, 33, 40, ${terminalStore.windowOpacity})`,
    },
    Layout: {
      color: `rgba(19, 19, 22, ${terminalStore.windowOpacity})`,
      siderColor: `rgba(24, 24, 29, ${terminalStore.windowOpacity})`,
      headerColor: `rgba(24, 24, 29, ${terminalStore.windowOpacity})`,
    },
    Tree: {
      nodeTextColor: "rgba(255,255,255,0.85)",
    },
  }));

  const lightThemeOverrides = computed<GlobalThemeOverrides>(() => ({
    ...sharedThemeOverrides,
    common: {
      ...sharedThemeOverrides.common,
      ...lightPrimaryOverrides,
      // naive 浅色默认 hoverColor 是实色 #f3f3f5，在应用浅色头部（#f5f5f8）上
      // 几乎同亮度，TabBar/ActivityBar 悬停胶囊会隐形；换成半透明墨色，
      // 强度与暗色主题的白色 9% 对称
      hoverColor: "rgba(20, 24, 33, 0.08)",
      bodyColor: `rgba(245, 245, 248, ${terminalStore.windowOpacity})`,
      cardColor: `rgba(255, 255, 255, ${terminalStore.windowOpacity})`,
      modalColor: `rgba(255, 255, 255, ${terminalStore.windowOpacity})`,
      popoverColor: `rgba(255, 255, 255, ${terminalStore.windowOpacity})`,
    },
    Layout: {
      color: `rgba(245, 245, 248, ${terminalStore.windowOpacity})`,
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
