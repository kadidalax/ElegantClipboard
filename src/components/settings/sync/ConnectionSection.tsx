import { useState } from "react";
import {
  ArrowSync16Regular,
  Checkmark16Regular,
  Eye16Regular,
  EyeOff16Regular,
} from "@fluentui/react-icons";
import { SettingsCard, SettingsCardHeader } from "@/components/settings/SettingSection";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import type { ProxyMode } from "@/hooks/useWebDAVSettings";
import { useTranslation } from "@/i18n";

type ConnectionSectionProps = {
  enabled: boolean;
  setEnabled: (value: boolean) => void;
  url: string;
  setUrl: (value: string) => void;
  username: string;
  setUsername: (value: string) => void;
  password: string;
  setPassword: (value: string) => void;
  remoteDir: string;
  setRemoteDir: (value: string) => void;
  proxyMode: ProxyMode;
  setProxyMode: (value: ProxyMode) => void;
  proxyUrl: string;
  setProxyUrl: (value: string) => void;
  acceptInvalidCerts: boolean;
  setAcceptInvalidCerts: (value: boolean) => void;
  testing: boolean;
  onTestConnection: () => void;
};

export function ConnectionSection({
  enabled,
  setEnabled,
  url,
  setUrl,
  username,
  setUsername,
  password,
  setPassword,
  remoteDir,
  setRemoteDir,
  proxyMode,
  setProxyMode,
  proxyUrl,
  setProxyUrl,
  acceptInvalidCerts,
  setAcceptInvalidCerts,
  testing,
  onTestConnection,
}: ConnectionSectionProps) {
  const { t } = useTranslation();
  const [showPassword, setShowPassword] = useState(false);

  return (
    <SettingsCard>
      <SettingsCardHeader
        title={t("settings.sync.connectionTitle")}
        description={t("settings.sync.connectionDesc")}
      />
      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <Label className="text-xs">{t("settings.sync.enable")}</Label>
            <p className="text-xs text-muted-foreground">
              {t("settings.sync.enableDesc")}
            </p>
          </div>
          <Switch checked={enabled} onCheckedChange={setEnabled} />
        </div>

        {enabled && (
          <>
            <div className="space-y-2 pt-1">
              <div className="space-y-1.5">
                <Label className="text-xs">{t("settings.sync.url")}</Label>
                <Input
                  className="h-8 text-xs"
                  placeholder="https://dav.example.com"
                  value={url}
                  onChange={(e) => setUrl(e.target.value)}
                />
              </div>
              <div className="grid grid-cols-2 gap-2">
                <div className="space-y-1.5">
                  <Label className="text-xs">{t("settings.sync.username")}</Label>
                  <Input
                    className="h-8 text-xs"
                    placeholder="username"
                    value={username}
                    onChange={(e) => setUsername(e.target.value)}
                  />
                </div>
                <div className="space-y-1.5">
                  <Label className="text-xs">{t("settings.sync.password")}</Label>
                  <div className="relative">
                    <Input
                      className="h-8 text-xs pr-8"
                      type={showPassword ? "text" : "password"}
                      placeholder="password"
                      value={password}
                      onChange={(e) => setPassword(e.target.value)}
                    />
                    <button
                      type="button"
                      className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground transition-colors"
                      onClick={() => setShowPassword(!showPassword)}
                    >
                      {showPassword ? <EyeOff16Regular className="w-3.5 h-3.5" /> : <Eye16Regular className="w-3.5 h-3.5" />}
                    </button>
                  </div>
                </div>
              </div>
              <div className="space-y-1.5">
                <Label className="text-xs">{t("settings.sync.remoteDir")}</Label>
                <Input
                  className="h-8 text-xs"
                  placeholder="/elegant-clipboard"
                  value={remoteDir}
                  onChange={(e) => setRemoteDir(e.target.value)}
                />
              </div>
              <div className="space-y-1.5">
                <Label className="text-xs">{t("settings.sync.proxy")}</Label>
                <div className="flex items-center gap-2">
                  <Select value={proxyMode} onValueChange={(value) => setProxyMode(value as ProxyMode)}>
                    <SelectTrigger className="w-[130px] h-8 text-xs shrink-0"><SelectValue /></SelectTrigger>
                    <SelectContent>
                      <SelectItem value="system">{t("settings.sync.proxySystem")}</SelectItem>
                      <SelectItem value="none">{t("settings.sync.proxyNone")}</SelectItem>
                      <SelectItem value="custom">{t("settings.sync.proxyCustom")}</SelectItem>
                    </SelectContent>
                  </Select>
                  {proxyMode === "custom" && (
                    <Input
                      className="h-8 text-xs flex-1"
                      placeholder={t("settings.sync.proxyPlaceholder")}
                      value={proxyUrl}
                      onChange={(e) => setProxyUrl(e.target.value)}
                    />
                  )}
                </div>
              </div>
              <div className="flex items-center justify-between rounded-md border border-amber-500/30 bg-amber-500/5 p-3">
                <div className="space-y-0.5 pr-4">
                  <Label className="text-xs">{t("settings.sync.acceptInvalidCert")}</Label>
                  <p className="text-xs text-muted-foreground">
                    {t("settings.sync.acceptInvalidCertDesc")}
                  </p>
                </div>
                <Switch checked={acceptInvalidCerts} onCheckedChange={setAcceptInvalidCerts} />
              </div>
            </div>

            <div className="flex items-center gap-2 pt-1">
              <Button
                variant="outline"
                size="sm"
                className="h-7 text-xs"
                onClick={onTestConnection}
                disabled={testing || !url}
              >
                {testing ? (
                  <ArrowSync16Regular className="w-3.5 h-3.5 mr-1 animate-spin" />
                ) : (
                  <Checkmark16Regular className="w-3.5 h-3.5 mr-1" />
                )}
                {t("settings.sync.testConnection")}
              </Button>
            </div>
          </>
        )}
      </div>
    </SettingsCard>
  );
}
