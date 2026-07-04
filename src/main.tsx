import React, { lazy, Suspense } from "react";
import ReactDOM from "react-dom/client";
import { Toaster } from "@/components/ui/toast";
import { TooltipProvider } from "@/components/ui/tooltip";
import { initLocale } from "@/i18n";
import { initPluginAvailability } from "@/stores/plugin-availability";
import { useTranslateSettings } from "@/stores/translate-settings";
import { initUISettingsStore } from "@/stores/ui-settings";
import App from "./App";
import "overlayscrollbars/overlayscrollbars.css";
import "./index.css";

const Settings = lazy(() =>
  import("./pages/Settings").then((m) => ({ default: m.Settings })),
);
const TextEditor = lazy(() =>
  import("./pages/TextEditor").then((m) => ({ default: m.TextEditor })),
);
const TranslateResult = lazy(() =>
  import("./pages/TranslateResult").then((m) => ({ default: m.TranslateResult })),
);

// 禁用右键菜单
document.addEventListener("contextmenu", (e) => {
  e.preventDefault();
});

// 禁用 WebView2 浏览器快捷键
document.addEventListener("keydown", (e) => {
  // 拦截 Ctrl+字母浏览器快捷键，保留 Ctrl+Backspace/Arrow 等
  if (e.ctrlKey && !e.altKey && e.key.length === 1) {
    const allowed = new Set(["a", "c", "v", "x", "z", "y"]);
    if (!allowed.has(e.key.toLowerCase())) {
      e.preventDefault();
    }
  }
  // 拦截 Tab 导航、F5 刷新、F7 光标浏览
  if (e.key === "Tab" || e.key === "F1" || e.key === "F3" || e.key === "F5" || e.key === "F6" || e.key === "F7" || e.key === "F11" || e.key === "F12") {
    e.preventDefault();
  }
});

function RouteFallback() {
  return (
    <div className="h-screen flex items-center justify-center bg-page-shell">
      <div className="w-8 h-8 border-2 border-primary border-t-transparent rounded-full animate-spin" />
    </div>
  );
}

// 基于 URL 路径的简单路由（主窗口同步加载，其它窗口 lazy）
function Router() {
  const path = window.location.pathname;

  if (path === "/settings" || path === "/settings.html") {
    return (
      <Suspense fallback={<RouteFallback />}>
        <Settings />
      </Suspense>
    );
  }
  if (path === "/editor" || path === "/editor.html") {
    return (
      <Suspense fallback={<RouteFallback />}>
        <TextEditor />
      </Suspense>
    );
  }
  if (path === "/translate-result") {
    return (
      <Suspense fallback={<RouteFallback />}>
        <TranslateResult />
      </Suspense>
    );
  }

  return <App />;
}

/** WebDAV / 翻译配置：主窗口首屏不需要，render 后再加载 */
function deferSecondaryInit() {
  void (async () => {
    try {
      await initPluginAvailability();
      await useTranslateSettings.getState().loadSettings();
    } catch (error) {
      console.error("Deferred bootstrap init failed:", error);
    }
  })();
}

async function bootstrap() {
  try {
    await Promise.all([initLocale(), initUISettingsStore()]);
  } catch (error) {
    console.error("Bootstrap init failed:", error);
  }

  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <TooltipProvider delayDuration={300}>
        <Router />
        <Toaster />
      </TooltipProvider>
    </React.StrictMode>,
  );

  deferSecondaryInit();
}

void bootstrap();
