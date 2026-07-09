import {
  useWebDAVSyncStore,
  type SyncStatusType,
} from "@/stores/webdav-sync";

export type { SyncStatusType };

export function useWebDAVActions() {
  return useWebDAVSyncStore();
}
