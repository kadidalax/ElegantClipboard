import React from "react";
import ReactDOM from "react-dom/client";
import { Toaster } from "@/components/ui/toast";
import { TooltipProvider } from "@/components/ui/tooltip";
import { initLocale } from "@/i18n";
import { initTheme } from "@/lib/theme-applier";
import { initPluginAvailability } from "@/stores/plugin-availability";
import { useTranslateSettings } from "@/stores/translate-settings";
import { initUISettingsStore } from "@/stores/ui-settings";
import { Settings } from "./pages/Settings";
import "overlayscrollbars/overlayscrollbars.css";
import "./index.css";

document.addEventListener("contextmenu", (e) => {
  e.preventDefault();
});

document.addEventListener("keydown", (e) => {
  if (e.ctrlKey && !e.altKey && e.key.length === 1) {
    const allowed = new Set(["a", "c", "v", "x", "z", "y"]);
    if (!allowed.has(e.key.toLowerCase())) {
      e.preventDefault();
    }
  }
  if (
    e.key === "Tab" ||
    e.key === "F1" ||
    e.key === "F3" ||
    e.key === "F5" ||
    e.key === "F6" ||
    e.key === "F7" ||
    e.key === "F11" ||
    e.key === "F12"
  ) {
    e.preventDefault();
  }
});

function deferSecondaryInit() {
  void (async () => {
    try {
      await initPluginAvailability();
      await useTranslateSettings.getState().loadSettings();
    } catch (error) {
      console.error("Settings window deferred init failed:", error);
    }
  })();
}

void initTheme();

void Promise.all([initLocale(), initUISettingsStore()]).catch((error) => {
  console.error("Settings bootstrap init failed:", error);
});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <TooltipProvider delayDuration={300}>
      <Settings />
      <Toaster />
    </TooltipProvider>
  </React.StrictMode>,
);

deferSecondaryInit();
