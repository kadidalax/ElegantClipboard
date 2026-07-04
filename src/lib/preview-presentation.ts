import { resolveUiFontFamilyCss } from "@/lib/fonts";
import { useUISettings } from "@/stores/ui-settings";

export type PreviewPresentation = {
  theme: "dark" | "light";
  sharpCorners: boolean;
  colorTheme: string;
  systemAccent: string | null;
  windowEffect: string;
  uiFontFamily: string;
};

function readSystemAccentFromDom(): string | null {
  const root = document.documentElement;
  const h = root.style.getPropertyValue("--system-accent-h");
  if (!h.trim()) return null;
  const s = root.style.getPropertyValue("--system-accent-s") || "65%";
  const l = root.style.getPropertyValue("--system-accent-l") || "50%";
  return `${h.trim()} ${s.trim()} ${l.trim()}`;
}

export function getPreviewPresentation(): PreviewPresentation {
  const { sharpCorners, colorTheme, windowEffect, customFont } = useUISettings.getState();
  const theme = document.documentElement.classList.contains("dark") ? "dark" : "light";
  return {
    theme,
    sharpCorners,
    colorTheme,
    systemAccent: colorTheme === "system" ? readSystemAccentFromDom() : null,
    windowEffect,
    uiFontFamily: resolveUiFontFamilyCss(customFont),
  };
}

export function previewPresentationChanged(
  a: PreviewPresentation | null,
  b: PreviewPresentation,
): boolean {
  if (!a) return true;
  return (
    a.theme !== b.theme ||
    a.sharpCorners !== b.sharpCorners ||
    a.colorTheme !== b.colorTheme ||
    a.systemAccent !== b.systemAccent ||
    a.windowEffect !== b.windowEffect ||
    a.uiFontFamily !== b.uiFontFamily
  );
}
