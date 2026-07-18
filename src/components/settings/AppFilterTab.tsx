import { useState, useEffect, useCallback, useRef } from "react";
import { Delete16Regular } from "@fluentui/react-icons";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { SettingsCard, SettingsCardHeader } from "@/components/settings/SettingSection";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { useTranslation } from "@/i18n";
import { getContentTypeLabel } from "@/lib/constants";
import { logError } from "@/lib/logger";

interface AppMeta { name: string; icon: string | null }
type RunningApp = { name: string; process: string; icon: string | null };

const ALL_MONITOR_TYPES = ["text", "html", "rtf", "image", "files", "url"] as const;

export function AppFilterTab() {
  const { t } = useTranslation();
  const [appFilterEnabled, setAppFilterEnabled] = useState(false);
  const [appFilterMode, setAppFilterMode] = useState<"blacklist" | "whitelist">("blacklist");
  const [appFilterList, setAppFilterList] = useState<string[]>([]);
  const [excludeInput, setExcludeInput] = useState("");
  const [runningApps, setRunningApps] = useState<RunningApp[]>([]);
  const [showAppPicker, setShowAppPicker] = useState(false);
  const [monitorTypes, setMonitorTypes] = useState<Set<string>>(new Set(ALL_MONITOR_TYPES));
  // 缓存进程名 → 应用信息（名称+图标），从选择器中获取
  const appMetaCache = useRef<Map<string, AppMeta>>(new Map());

  useEffect(() => {
    void (async () => {
      const results = await Promise.allSettled([
        invoke<string | null>("get_setting", { key: "app_filter_enabled" }),
        invoke<string | null>("get_setting", { key: "app_filter_mode" }),
        invoke<string | null>("get_setting", { key: "app_filter_list" }),
        invoke<string | null>("get_setting", { key: "monitor_types" }),
        invoke<RunningApp[]>("get_running_apps"),
      ]);

      const [enabledR, modeR, listR, typesR, appsR] = results;

      if (enabledR.status === "fulfilled") {
        setAppFilterEnabled(enabledR.value === "true");
      } else {
        logError("Failed to load app_filter_enabled:", enabledR.reason);
      }
      if (modeR.status === "fulfilled") {
        if (modeR.value === "whitelist") setAppFilterMode("whitelist");
      } else {
        logError("Failed to load app_filter_mode:", modeR.reason);
      }
      if (listR.status === "fulfilled") {
        const v = listR.value;
        if (v && v.length > 0) {
          setAppFilterList(v.split(",").map((t) => t.trim()).filter(Boolean));
        }
      } else {
        logError("Failed to load app_filter_list:", listR.reason);
      }
      if (typesR.status === "fulfilled") {
        const v = typesR.value;
        if (v && v.length > 0) {
          setMonitorTypes(new Set(v.split(",").map((t) => t.trim()).filter(Boolean)));
        }
      } else {
        logError("Failed to load monitor_types:", typesR.reason);
      }
      if (appsR.status === "fulfilled") {
        for (const app of appsR.value) {
          appMetaCache.current.set(app.process.toLowerCase(), { name: app.name, icon: app.icon });
        }
      } else {
        logError("Failed to preload running apps:", appsR.reason);
      }
    })();
  }, []);

  const toggleMonitorType = useCallback((type: string) => {
    setMonitorTypes((prev) => {
      const next = new Set(prev);
      if (next.has(type)) {
        // 至少保留一种类型
        if (next.size <= 1) return prev;
        next.delete(type);
      } else {
        next.add(type);
      }
      const value = Array.from(next).join(",");
      const rollback = new Set(prev);
      invoke("set_setting", { key: "monitor_types", value }).catch((error) => {
        logError("Failed to save monitor_types:", error);
        setMonitorTypes(rollback);
      });
      return next;
    });
  }, []);

  const toggleAppFilter = useCallback((enabled: boolean) => {
    const previous = appFilterEnabled;
    setAppFilterEnabled(enabled);
    invoke("set_setting", { key: "app_filter_enabled", value: String(enabled) }).catch((error) => {
      logError("Failed to save app_filter_enabled:", error);
      setAppFilterEnabled(previous);
    });
  }, [appFilterEnabled]);

  const switchAppFilterMode = useCallback((mode: "blacklist" | "whitelist") => {
    const previous = appFilterMode;
    setAppFilterMode(mode);
    invoke("set_setting", { key: "app_filter_mode", value: mode }).catch((error) => {
      logError("Failed to save app_filter_mode:", error);
      setAppFilterMode(previous);
    });
  }, [appFilterMode]);

  const addFilterApp = useCallback((name: string, meta?: AppMeta) => {
    const trimmed = name.trim();
    if (!trimmed) return;
    if (meta) {
      appMetaCache.current.set(trimmed.toLowerCase(), meta);
    }
    setAppFilterList((prev) => {
      if (prev.some((a) => a.toLowerCase() === trimmed.toLowerCase())) return prev;
      const next = [...prev, trimmed];
      invoke("set_setting", { key: "app_filter_list", value: next.join(",") }).catch((error) => {
        logError("Failed to save app_filter_list (add):", error);
        setAppFilterList(prev);
      });
      return next;
    });
  }, []);

  const removeFilterApp = useCallback((name: string) => {
    setAppFilterList((prev) => {
      const next = prev.filter((a) => a !== name);
      invoke("set_setting", { key: "app_filter_list", value: next.join(",") }).catch((error) => {
        logError("Failed to save app_filter_list (remove):", error);
        setAppFilterList(prev);
      });
      return next;
    });
  }, []);

  const loadRunningApps = useCallback(async () => {
    try {
      const apps = await invoke<RunningApp[]>("get_running_apps");
      setRunningApps(apps);
      for (const app of apps) {
        appMetaCache.current.set(app.process.toLowerCase(), { name: app.name, icon: app.icon });
      }
      setShowAppPicker(true);
    } catch (error) {
      logError("Failed to load running apps:", error);
    }
  }, []);

  const getMeta = (process: string): AppMeta | undefined =>
    appMetaCache.current.get(process.toLowerCase());

  return (
    <div className="space-y-3">
      {/* 监听内容类型 */}
      <SettingsCard>
        <SettingsCardHeader
          title={t("settings.appFilter.monitorTypesTitle")}
          description={t("settings.appFilter.monitorTypesDesc")}
        />
        <div className="flex flex-wrap gap-2">
          {ALL_MONITOR_TYPES.map((type) => {
            const active = monitorTypes.has(type);
            const label = type === "text" ? t("settings.appFilter.plainText") : getContentTypeLabel(type);
            return (
              <button
                key={type}
                type="button"
                onClick={() => toggleMonitorType(type)}
                className={`px-3 py-1.5 text-xs font-medium rounded-md border transition-surface ${
                  active
                    ? "bg-primary text-primary-foreground border-primary"
                    : "bg-muted-surface-subtle text-muted-foreground border-transparent hover:bg-muted"
                }`}
              >
                {label}
              </button>
            );
          })}
        </div>
      </SettingsCard>

      {/* 开关 + 模式 */}
      <SettingsCard>
        <SettingsCardHeader
          title={t("settings.appFilter.filterTitle")}
          description={t("settings.appFilter.filterDesc")}
          action={<Switch checked={appFilterEnabled} onCheckedChange={toggleAppFilter} />}
        />

        <div className="flex gap-1.5">
          {(["blacklist", "whitelist"] as const).map((mode) => (
            <button
              key={mode}
              type="button"
              onClick={() => switchAppFilterMode(mode)}
              className={`px-3 py-1.5 text-xs font-medium rounded-md border transition-surface ${
                appFilterMode === mode
                  ? "bg-primary text-primary-foreground border-primary"
                  : "bg-muted-surface-subtle text-muted-foreground border-transparent hover:bg-muted"
              }`}
            >
              {mode === "blacklist" ? t("settings.appFilter.blacklist") : t("settings.appFilter.whitelist")}
            </button>
          ))}
        </div>

        <p className="text-xs text-muted-foreground mt-3">
          {appFilterMode === "blacklist"
            ? t("settings.appFilter.blacklistDesc")
            : t("settings.appFilter.whitelistDesc")}
        </p>
      </SettingsCard>

      {/* 规则列表 */}
      <SettingsCard>
        <SettingsCardHeader
          title={t("settings.appFilter.rulesTitle")}
          action={
            <Button variant="outline" size="sm" onClick={loadRunningApps} className="h-7 text-xs">
              {t("settings.appFilter.selectApp")}
            </Button>
          }
        />

        <div className="flex gap-2 mb-4">
          <Input
            value={excludeInput}
            onChange={(e) => setExcludeInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                addFilterApp(excludeInput);
                setExcludeInput("");
              }
            }}
            placeholder={t("settings.appFilter.inputPlaceholder")}
            className="flex-1 h-8 text-xs"
          />
          <Button
            variant="outline"
            size="sm"
            onClick={() => { addFilterApp(excludeInput); setExcludeInput(""); }}
            disabled={!excludeInput.trim()}
            className="h-8 text-xs"
          >
            {t("common.add")}
          </Button>
        </div>

        {appFilterList.length > 0 ? (
          <div className="space-y-1">
            {appFilterList.map((rule) => {
              const meta = getMeta(rule);
              return (
                <div
                  key={rule}
                  className="group flex items-center gap-2.5 px-2.5 py-2 rounded-md bg-muted-surface-subtle hover:bg-muted-surface-strong transition-surface"
                >
                  {meta?.icon ? (
                    <img
                      src={convertFileSrc(meta.icon)}
                      alt=""
                      className="w-5 h-5 shrink-0 object-contain"
                      onError={(e) => { (e.target as HTMLImageElement).style.display = "none"; }}
                    />
                  ) : (
                    <div className="w-5 h-5 shrink-0 rounded-md bg-muted flex items-center justify-center text-micro text-muted-foreground">
                      {rule.includes("*") || rule.includes("?") ? "*" : "?"}
                    </div>
                  )}
                  <div className="flex-1 min-w-0">
                    {meta ? (
                      <>
                        <div className="text-xs font-medium truncate">{meta.name}</div>
                        <div className="text-micro text-muted-foreground truncate">{rule}</div>
                      </>
                    ) : (
                      <div className="text-xs font-medium truncate">{rule}</div>
                    )}
                  </div>
                  <button
                    type="button"
                    onClick={() => removeFilterApp(rule)}
                    className="shrink-0 w-6 h-6 rounded-md flex items-center justify-center text-muted-foreground opacity-0 group-hover:opacity-100 hover:text-destructive transition-all"
                    aria-label={t("clipboard.contextMenu.removeRule", { rule })}
                  >
                    <Delete16Regular className="w-3.5 h-3.5" />
                  </button>
                </div>
              );
            })}
          </div>
        ) : (
          <div className="text-center py-6">
            <p className="text-xs text-muted-foreground/60">{t("settings.appFilter.noRules")}</p>
            <p className="text-micro text-muted-foreground/40 mt-1">
              {t("settings.appFilter.noRulesHint")}
            </p>
          </div>
        )}
      </SettingsCard>

      {/* Running Apps Picker Dialog */}
      <Dialog open={showAppPicker} onOpenChange={setShowAppPicker}>
        <DialogContent className="max-w-md max-h-[70vh] flex flex-col">
          <DialogHeader>
            <DialogTitle className="text-sm">{t("settings.appFilter.pickerTitle")}</DialogTitle>
            <DialogDescription className="text-xs">
              {appFilterMode === "blacklist"
                ? t("settings.appFilter.pickerDescBlack")
                : t("settings.appFilter.pickerDescWhite")}
            </DialogDescription>
          </DialogHeader>
          <div className="flex-1 overflow-y-auto -mx-1 px-1 space-y-0.5">
            {runningApps.map((app) => {
              const alreadyAdded = appFilterList.some(
                (f) => f.toLowerCase() === app.process.toLowerCase()
              );
              return (
                <button
                  key={app.process}
                  type="button"
                  disabled={alreadyAdded}
                  onClick={() => addFilterApp(app.process, { name: app.name, icon: app.icon })}
                  className={`w-full flex items-center gap-2.5 px-2.5 py-2 rounded-md text-left transition-surface ${
                    alreadyAdded
                      ? "opacity-40 cursor-not-allowed"
                      : "hover:bg-accent"
                  }`}
                >
                  {app.icon ? (
                    <img
                      src={convertFileSrc(app.icon)}
                      alt=""
                      className="w-5 h-5 shrink-0 object-contain"
                      onError={(e) => { (e.target as HTMLImageElement).style.display = "none"; }}
                    />
                  ) : (
                    <div className="w-5 h-5 shrink-0 rounded-md bg-muted flex items-center justify-center text-micro text-muted-foreground">
                      ?
                    </div>
                  )}
                  <div className="flex-1 min-w-0">
                    <div className="text-xs font-medium truncate">{app.name}</div>
                    <div className="text-micro text-muted-foreground truncate">{app.process}</div>
                  </div>
                  {alreadyAdded && (
                    <span className="text-micro text-muted-foreground shrink-0">{t("settings.appFilter.alreadyAdded")}</span>
                  )}
                </button>
              );
            })}
            {runningApps.length === 0 && (
              <p className="text-xs text-muted-foreground text-center py-8">{t("common.loading")}</p>
            )}
          </div>
          <DialogFooter>
            <Button variant="outline" size="sm" onClick={() => setShowAppPicker(false)} className="text-xs">
              {t("common.close")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
