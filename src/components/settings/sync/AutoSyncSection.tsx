import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { useTranslation } from "@/i18n";

type AutoSyncSectionProps = {
  autoSync: boolean;
  setAutoSync: (value: boolean) => void;
  syncInterval: string;
  setSyncInterval: (value: string) => void;
};

export function AutoSyncSection({
  autoSync,
  setAutoSync,
  syncInterval,
  setSyncInterval,
}: AutoSyncSectionProps) {
  const { t } = useTranslation();

  return (
    <div className="rounded-lg border bg-card p-4">
      <h3 className="text-sm font-medium mb-3">{t("settings.sync.autoTitle")}</h3>
      <p className="text-xs text-muted-foreground mb-4">
        {t("settings.sync.autoDesc")}
      </p>
      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <Label className="text-xs">{t("settings.sync.autoEnable")}</Label>
            <p className="text-xs text-muted-foreground">
              {t("settings.sync.autoEnableDesc")}
            </p>
          </div>
          <Switch checked={autoSync} onCheckedChange={setAutoSync} />
        </div>
        {autoSync && (
          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label className="text-xs">{t("settings.sync.interval")}</Label>
              <p className="text-xs text-muted-foreground">
                {t("settings.sync.intervalDesc")}
              </p>
            </div>
            <Select value={syncInterval} onValueChange={setSyncInterval}>
              <SelectTrigger className="w-[120px] h-8 text-xs"><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="30">{t("settings.sync.interval30s")}</SelectItem>
                <SelectItem value="60">{t("settings.sync.interval1m")}</SelectItem>
                <SelectItem value="180">{t("settings.sync.interval3m")}</SelectItem>
                <SelectItem value="300">{t("settings.sync.interval5m")}</SelectItem>
                <SelectItem value="600">{t("settings.sync.interval10m")}</SelectItem>
                <SelectItem value="1200">{t("settings.sync.interval20m")}</SelectItem>
              </SelectContent>
            </Select>
          </div>
        )}
      </div>
    </div>
  );
}
