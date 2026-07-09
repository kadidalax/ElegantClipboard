import { useEffect, useRef } from "react";
import { AutoSyncSection } from "@/components/settings/sync/AutoSyncSection";
import { ConnectionSection } from "@/components/settings/sync/ConnectionSection";
import { ManualSyncSection } from "@/components/settings/sync/ManualSyncSection";
import { SyncTypesSection } from "@/components/settings/sync/SyncTypesSection";
import { useWebDAVActions } from "@/hooks/useWebDAVActions";
import { useWebDAVSettings } from "@/hooks/useWebDAVSettings";
import { onWebDAVLastSyncUpdated } from "@/stores/webdav-sync";

export function SyncTab() {
  const settings = useWebDAVSettings();
  const {
    testing,
    syncing,
    statusMsg,
    statusType,
    handleTestConnection,
    handleUpload,
    handleDownload,
  } = useWebDAVActions();
  const statusMsgRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (statusMsg && statusMsgRef.current) {
      statusMsgRef.current.scrollIntoView({ behavior: "smooth", block: "nearest" });
    }
  }, [statusMsg]);

  useEffect(() => {
    return onWebDAVLastSyncUpdated(() => {
      void settings.loadSettings();
    });
  }, [settings.loadSettings]);

  return (
    <>
      <ConnectionSection
        enabled={settings.enabled}
        setEnabled={settings.setEnabled}
        url={settings.url}
        setUrl={settings.setUrl}
        username={settings.username}
        setUsername={settings.setUsername}
        password={settings.password}
        setPassword={settings.setPassword}
        remoteDir={settings.remoteDir}
        setRemoteDir={settings.setRemoteDir}
        proxyMode={settings.proxyMode}
        setProxyMode={settings.setProxyMode}
        proxyUrl={settings.proxyUrl}
        setProxyUrl={settings.setProxyUrl}
        acceptInvalidCerts={settings.acceptInvalidCerts}
        setAcceptInvalidCerts={settings.setAcceptInvalidCerts}
        testing={testing}
        onTestConnection={handleTestConnection}
      />

      {settings.enabled && (
        <>
          <SyncTypesSection
            syncTypes={settings.syncTypes}
            setSyncTypes={settings.setSyncTypes}
            maxImageSizeKb={settings.maxImageSizeKb}
            setMaxImageSizeKb={settings.setMaxImageSizeKb}
            maxFileSizeKb={settings.maxFileSizeKb}
            setMaxFileSizeKb={settings.setMaxFileSizeKb}
            maxVideoSizeKb={settings.maxVideoSizeKb}
            setMaxVideoSizeKb={settings.setMaxVideoSizeKb}
          />

          <AutoSyncSection
            autoSync={settings.autoSync}
            setAutoSync={settings.setAutoSync}
            syncInterval={settings.syncInterval}
            setSyncInterval={settings.setSyncInterval}
          />

          <ManualSyncSection
            syncing={syncing}
            url={settings.url}
            lastSyncTime={settings.lastSyncTime}
            statusMsg={statusMsg}
            statusType={statusType}
            statusMsgRef={statusMsgRef}
            onUpload={handleUpload}
            onDownload={handleDownload}
          />
        </>
      )}
    </>
  );
}
