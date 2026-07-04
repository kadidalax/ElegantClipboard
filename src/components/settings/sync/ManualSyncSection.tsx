import type { RefObject } from "react";
import {
  ArrowDown16Regular,
  ArrowSync16Regular,
  ArrowUp16Regular,
} from "@fluentui/react-icons";
import { SettingsCard, SettingsCardHeader } from "@/components/settings/SettingSection";
import { Button } from "@/components/ui/button";
import type { SyncStatusType } from "@/hooks/useWebDAVActions";
import { useTranslation } from "@/i18n";

type ManualSyncSectionProps = {
  syncing: boolean;
  url: string;
  lastSyncTime: string;
  statusMsg: string;
  statusType: SyncStatusType;
  statusMsgRef: RefObject<HTMLDivElement | null>;
  onUpload: () => void;
  onDownload: () => void;
};

export function ManualSyncSection({
  syncing,
  url,
  lastSyncTime,
  statusMsg,
  statusType,
  statusMsgRef,
  onUpload,
  onDownload,
}: ManualSyncSectionProps) {
  const { t } = useTranslation();

  return (
    <SettingsCard>
      <SettingsCardHeader
        title={t("settings.sync.manualTitle")}
        description={t("settings.sync.manualDesc")}
      />
      <div className="space-y-3">
        <div className="flex items-center gap-2">
          <Button
            size="sm"
            className="h-7 text-xs"
            onClick={onUpload}
            disabled={syncing || !url}
          >
            {syncing ? (
              <ArrowSync16Regular className="w-3.5 h-3.5 mr-1 animate-spin" />
            ) : (
              <ArrowUp16Regular className="w-3.5 h-3.5 mr-1" />
            )}
            {t("settings.sync.upload")}
          </Button>
          <Button
            size="sm"
            className="h-7 text-xs"
            onClick={onDownload}
            disabled={syncing || !url}
          >
            {syncing ? (
              <ArrowSync16Regular className="w-3.5 h-3.5 mr-1 animate-spin" />
            ) : (
              <ArrowDown16Regular className="w-3.5 h-3.5 mr-1" />
            )}
            {t("settings.sync.download")}
          </Button>
        </div>

        {lastSyncTime && (
          <p className="text-xs text-muted-foreground">
            {t("settings.sync.lastSync", { time: lastSyncTime })}
          </p>
        )}

        {statusMsg && (
          <div
            ref={statusMsgRef}
            className={`text-xs px-3 py-2 rounded-md whitespace-pre-line ${
              statusType === "success"
                ? "bg-green-500/10 text-green-600 dark:text-green-400"
                : statusType === "error"
                  ? "bg-destructive/10 text-destructive"
                  : "bg-muted text-muted-foreground"
            }`}
          >
            {statusMsg}
          </div>
        )}
      </div>
    </SettingsCard>
  );
}
