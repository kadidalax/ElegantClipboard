import { useMemo } from "react";
import { ArrowSync16Regular, Translate16Regular } from "@fluentui/react-icons";
import { SettingRow, SettingSection } from "@/components/settings/SettingSection";
import { Switch } from "@/components/ui/switch";
import { useTranslation } from "@/i18n";

const PLUGIN_ICONS: Record<string, React.ComponentType<{ className?: string }>> = {
  webdav: ArrowSync16Regular,
  translate: Translate16Regular,
};

const PLUGIN_IDS = ["webdav", "translate"] as const;

type PluginsTabProps = {
  enabledMap: Record<string, boolean>;
  onToggle: (id: string, value: boolean) => void;
};

export function PluginsTab({ enabledMap, onToggle }: PluginsTabProps) {
  const { t } = useTranslation();

  const plugins = useMemo(() => [
    {
      id: "webdav",
      name: t("settings.plugins.webdavName"),
      description: t("settings.plugins.webdavDesc"),
    },
    {
      id: "translate",
      name: t("settings.plugins.translateName"),
      description: t("settings.plugins.translateDesc"),
    },
  ], [t]);

  return (
    <div className="space-y-3">
      {PLUGIN_IDS.map((id) => {
        const plugin = plugins.find((p) => p.id === id)!;
        const Icon = PLUGIN_ICONS[plugin.id] ?? ArrowSync16Regular;

        return (
          <SettingSection key={plugin.id}>
            <SettingRow
              icon={<Icon className="w-4 h-4 text-muted-foreground" />}
              title={plugin.name}
              description={plugin.description}
              action={
                <Switch
                  checked={!!enabledMap[plugin.id]}
                  onCheckedChange={(v) => onToggle(plugin.id, v)}
                  aria-label={t("settings.plugins.toggleAria", { name: plugin.name })}
                />
              }
            />
          </SettingSection>
        );
      })}
    </div>
  );
}
