import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";

export const TRANSLATE_AVAILABILITY_EVENT = "translate-availability-changed";

export async function fetchTranslateAvailable(): Promise<boolean> {
  const settings = await invoke<Record<string, string>>("get_settings_batch", {
    keys: ["plugin_translate_enabled", "translate_enabled"],
  });
  return (
    settings["plugin_translate_enabled"] === "true"
    && settings["translate_enabled"] === "true"
  );
}

export function notifyTranslateAvailabilityChanged() {
  void emit(TRANSLATE_AVAILABILITY_EVENT);
}
