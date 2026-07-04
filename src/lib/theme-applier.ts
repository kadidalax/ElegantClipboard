/**
 * 模块级主题应用器，零 React 开销。
 *
 * 每个窗口调用一次 `initTheme()`：
 * - 应用颜色主题 class 到 <html>
 * - 获取系统强调色并设置 --system-accent-h
 * - 监听后端 WM_SETTINGCHANGE 事件
 * - 订阅 zustand store 主题切换
 * - 通过 matchMedia 应用深色模式
 *
 * 返回 Promise，主题完全应用后 resolve。
 */
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import {
  DEFAULT_FONT_STACK,
  resolveCardFontFamilyCss,
  resolveUiFontFamilyCss,
} from "@/lib/fonts";
import { logError } from "@/lib/logger";
import { getPreviewPresentation, previewPresentationChanged } from "@/lib/preview-presentation";
import { isUISettingsInitialized, useUISettings, whenUISettingsReady } from "@/stores/ui-settings";

const THEME_CLASSES = ["theme-emerald", "theme-cyan", "theme-system"];

let _initialized = false;
let _accentColor: string | null = null;
let _readyResolved = false;
let _readyResolve: (() => void) | null = null;
let _lastPreviewPresentation: ReturnType<typeof getPreviewPresentation> | null = null;
const _readyPromise = new Promise<void>((resolve) => {
  _readyResolve = resolve;
});

function resolveThemeReady() {
  if (_readyResolved) return;
  _readyResolved = true;
  _readyResolve?.();
}

// 强调色变更订阅者（主题预览用）
const _accentSubscribers = new Set<(color: string | null) => void>();

function notifyAccentSubscribers() {
  _accentSubscribers.forEach((fn) => fn(_accentColor));
}

function applySharpCorners() {
  const { sharpCorners } = useUISettings.getState();
  document.documentElement.classList.toggle("sharp-corners", sharpCorners);
}

function getIsDark(): boolean {
  const { darkMode } = useUISettings.getState();
  if (darkMode === "dark") return true;
  if (darkMode === "light") return false;
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

function applyWindowEffect() {
  const { windowEffect } = useUISettings.getState();
  if (windowEffect === "none") {
    // 移除特效：立即清除 CSS 透明
    document.documentElement.setAttribute("data-window-effect", "none");
    invoke("set_window_effect", { effect: "none", dark: null }).catch((error) => {
      logError("Failed to disable window effect:", error);
    });
  } else {
    // 应用特效：先设 DWM 背景，再激活 CSS 透明
    const dark = getIsDark();
    invoke("set_window_effect", { effect: windowEffect, dark })
      .then(() => {
        document.documentElement.setAttribute("data-window-effect", windowEffect);
      })
      .catch((error) => {
        // 特效不支持（如 Win10 不支持 Mica/Tabbed），回退 CSS 但不重置持久化偏好，
        // 下次启动仍会尝试应用（可能是窗口尚未就绪的临时失败）
        logError(`Failed to apply window effect '${windowEffect}', fallback to none:`, error);
        document.documentElement.setAttribute("data-window-effect", "none");
      });
  }
}

function applyFontSettings() {
  const { customFont, uiFontSize, cardFont, cardFontSize, previewFont, previewFontSize } =
    useUISettings.getState();
  const root = document.documentElement;
  const uiFontFamily = resolveUiFontFamilyCss(customFont);
  const cardFontFamily = resolveCardFontFamilyCss(cardFont, customFont);
  const previewFontFamily = previewFont
    ? `"${previewFont}", ${DEFAULT_FONT_STACK}`
    : cardFontFamily;

  // --- UI 字体 ---
  root.style.setProperty("--ui-font-family", uiFontFamily);
  if (customFont) {
    document.body.style.fontFamily = uiFontFamily;
  } else {
    document.body.style.removeProperty("font-family");
  }

  // --- UI 字号：缩放 html 根字号 ---
  const rootPx = (uiFontSize / 14) * 16;
  if (Math.abs(rootPx - 16) < 0.01) {
    root.style.removeProperty("font-size");
  } else {
    root.style.fontSize = `${rootPx}px`;
  }

  // --- 卡片 / 预览字体（CSS 变量供 .clipboard-content 与悬浮预览 IPC 使用） ---
  root.style.setProperty("--card-font-family", cardFontFamily);
  root.style.setProperty("--preview-font-family", previewFontFamily);

  if (cardFontSize !== 14) {
    root.style.setProperty("--card-font-size", `${cardFontSize}px`);
  } else {
    root.style.removeProperty("--card-font-size");
  }

  if (previewFontSize !== 13) {
    root.style.setProperty("--preview-font-size", `${previewFontSize}px`);
  } else {
    root.style.removeProperty("--preview-font-size");
  }
}

function apply() {
  const { colorTheme } = useUISettings.getState();
  const root = document.documentElement;

  root.classList.remove(...THEME_CLASSES);
  root.style.removeProperty("--system-accent-h");
  root.style.removeProperty("--system-accent-s");
  root.style.removeProperty("--system-accent-l");

  if (colorTheme === "system" && _accentColor) {
    const parts = _accentColor.split(" ");
    root.classList.add("theme-system");
    root.style.setProperty("--system-accent-h", parts[0]);
    root.style.setProperty("--system-accent-s", parts[1] || "65%");
    root.style.setProperty("--system-accent-l", parts[2] || "50%");
  } else if (colorTheme !== "default" && colorTheme !== "system") {
    root.classList.add(`theme-${colorTheme}`);
  }
}

function isMainThemeWindow(): boolean {
  const path = window.location.pathname;
  return path === "/" || path === "/index.html" || path.endsWith("/index.html");
}

function syncPreviewPresentation() {
  if (!isMainThemeWindow()) return;
  const payload = getPreviewPresentation();
  void emitTo("text-preview", "text-preview-theme", payload).catch(() => {});
  void emitTo("image-preview", "image-preview-theme", payload).catch(() => {});
}

function syncPreviewWindowEffects(windowEffect: string) {
  if (!isMainThemeWindow()) return;
  void invoke("sync_preview_window_effects", { windowEffect }).catch((error) => {
    logError("Failed to sync preview window effects:", error);
  });
}

/** 初始化主题系统，可安全多次调用，每个窗口仅执行一次 */
export function initTheme(): Promise<void> {
  if (_initialized) return _readyPromise;
  _initialized = true;

  // --- 深色模式 ---
  const mq = window.matchMedia("(prefers-color-scheme: dark)");

  function applyDarkMode() {
    const { darkMode } = useUISettings.getState();
    const isDark =
      darkMode === "dark" ? true : darkMode === "light" ? false : mq.matches;
    document.documentElement.classList.toggle("dark", isDark);
    const nextPresentation = getPreviewPresentation();
    if (previewPresentationChanged(_lastPreviewPresentation, nextPresentation)) {
      _lastPreviewPresentation = nextPresentation;
      syncPreviewPresentation();
    }
  }

  applyDarkMode();
  mq.addEventListener("change", () => applyDarkMode());

  // --- 订阅 store 变更：主题/圆角/深色模式变化时重新应用 ---
  useUISettings.subscribe((state, prev) => {
    if (state.sharpCorners !== prev.sharpCorners) {
      applySharpCorners();
      _lastPreviewPresentation = getPreviewPresentation();
      syncPreviewPresentation();
    }
    if (state.colorTheme !== prev.colorTheme) {
      if (state.colorTheme === "system" && !_accentColor) {
        // 切换到系统主题但还未获取强调色
        invoke<string | null>("get_system_accent_color").then((color) => {
          _accentColor = color;
          apply();
          _lastPreviewPresentation = getPreviewPresentation();
          syncPreviewPresentation();
        }).catch((error) => {
          logError("Failed to fetch system accent color on theme switch:", error);
          apply();
          _lastPreviewPresentation = getPreviewPresentation();
          syncPreviewPresentation();
        });
      } else {
        apply();
        _lastPreviewPresentation = getPreviewPresentation();
        syncPreviewPresentation();
      }
    }
    if (state.windowEffect !== prev.windowEffect) {
      applyWindowEffect();
      _lastPreviewPresentation = getPreviewPresentation();
      syncPreviewWindowEffects(state.windowEffect);
      syncPreviewPresentation();
    }
    if (state.darkMode !== prev.darkMode) {
      applyDarkMode();
      // 重新应用窗口特效以匹配深色模式
      if (state.windowEffect !== "none") {
        applyWindowEffect();
      }
    }
    if (
      state.customFont !== prev.customFont ||
      state.uiFontSize !== prev.uiFontSize ||
      state.cardFont !== prev.cardFont ||
      state.cardFontSize !== prev.cardFontSize ||
      state.previewFont !== prev.previewFont ||
      state.previewFontSize !== prev.previewFontSize
    ) {
      applyFontSettings();
      _lastPreviewPresentation = getPreviewPresentation();
      syncPreviewPresentation();
    }
  });

  // --- 后端推送新强调色（无需重新 IPC） ---
  listen<string | null>("system-accent-color-changed", (event) => {
    _accentColor = event.payload;
    notifyAccentSubscribers();
    apply();
    if (useUISettings.getState().colorTheme === "system") {
      _lastPreviewPresentation = getPreviewPresentation();
      syncPreviewPresentation();
    }
  });

  // --- 初始化应用 ---
  applySharpCorners();
  // 窗口特效和字体依赖后端设置值，初始化完成后再应用
  if (isUISettingsInitialized()) {
    applyWindowEffect();
    applyFontSettings();
  }
  void whenUISettingsReady().then(() => {
    applyWindowEffect();
    applyFontSettings();
    apply();
  });

  // 先用 store 默认值上色，系统强调色异步补齐，不阻塞首屏
  apply();
  resolveThemeReady();

  invoke<string | null>("get_system_accent_color")
    .then((color) => {
      _accentColor = color;
      notifyAccentSubscribers();
      apply();
    })
    .catch((error) => {
      logError("Failed to fetch initial system accent color:", error);
    });

  return _readyPromise;
}

/** 读取缓存的强调色（主题预览用） */
export function getAccentColor(): string | null {
  return _accentColor;
}

/** 订阅强调色变更，返回取消函数 */
export function subscribeAccentColor(
  fn: (color: string | null) => void,
): () => void {
  _accentSubscribers.add(fn);
  return () => _accentSubscribers.delete(fn);
}
