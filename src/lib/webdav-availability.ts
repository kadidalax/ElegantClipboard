import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import type { ToolbarButton } from "@/stores/ui-settings";

export const WEBDAV_AVAILABILITY_EVENT = "webdav-availability-changed";

export const WEBDAV_TOOLBAR_BUTTONS = ["webdav-upload", "webdav-download"] as const;

export function isWebDAVToolbarButton(id: ToolbarButton): boolean {
  return id === "webdav-upload" || id === "webdav-download";
}

export function filterToolbarButtonsForWebDAV(
  buttons: ToolbarButton[],
  webdavAvailable: boolean,
): ToolbarButton[] {
  if (webdavAvailable) return buttons;
  return buttons.filter((button) => !isWebDAVToolbarButton(button));
}

export async function fetchWebDAVAvailable(): Promise<boolean> {
  const settings = await invoke<Record<string, string>>("get_settings_batch", {
    keys: ["plugin_webdav_enabled", "webdav_enabled"],
  });
  return (
    settings["plugin_webdav_enabled"] === "true"
    && settings["webdav_enabled"] === "true"
  );
}

export function notifyWebDAVAvailabilityChanged() {
  void emit(WEBDAV_AVAILABILITY_EVENT);
}
