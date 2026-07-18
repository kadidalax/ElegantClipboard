import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { logError } from "@/lib/logger";
import {
  TRANSLATE_AVAILABILITY_EVENT,
  fetchTranslateAvailable,
} from "@/lib/translate-availability";
import {
  WEBDAV_AVAILABILITY_EVENT,
  fetchWebDAVAvailable,
} from "@/lib/webdav-availability";

type PluginAvailabilityState = {
  webdavAvailable: boolean;
  translateAvailable: boolean;
  refresh: () => Promise<void>;
};

export const usePluginAvailability = create<PluginAvailabilityState>((set) => ({
  webdavAvailable: false,
  translateAvailable: false,
  refresh: async () => {
    try {
      const [webdavAvailable, translateAvailable] = await Promise.all([
        fetchWebDAVAvailable(),
        fetchTranslateAvailable(),
      ]);
      set({ webdavAvailable, translateAvailable });
    } catch (error) {
      logError("Failed to refresh plugin availability:", error);
      set({ webdavAvailable: false, translateAvailable: false });
    }
  },
}));

let initialized = false;

export async function initPluginAvailability() {
  if (!initialized) {
    initialized = true;

    try {
      await listen(WEBDAV_AVAILABILITY_EVENT, () => {
        void usePluginAvailability.getState().refresh();
      });
      await listen(TRANSLATE_AVAILABILITY_EVENT, () => {
        void usePluginAvailability.getState().refresh();
      });
      await listen("translate-settings-changed", () => {
        void usePluginAvailability.getState().refresh();
      });
    } catch {
      // non-Tauri environments
    }
  }

  await usePluginAvailability.getState().refresh();
}

export function useWebDAVAvailable() {
  return usePluginAvailability((state) => state.webdavAvailable);
}

export function useTranslateAvailable() {
  return usePluginAvailability((state) => state.translateAvailable);
}
