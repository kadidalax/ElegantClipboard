import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { useTranslation } from "@/i18n";
import { useUISettings, type SoundTiming } from "@/stores/ui-settings";

function SoundCard({ title, desc, enabled, onToggle, timing, onTimingChange }: {
  title: string; desc: string;
  enabled: boolean; onToggle: (v: boolean) => void;
  timing: SoundTiming; onTimingChange: (v: SoundTiming) => void;
}) {
  const { t } = useTranslation();

  return (
    <div className="rounded-lg border bg-card p-4">
      <h3 className="text-sm font-medium mb-3">{title}</h3>
      <p className="text-xs text-muted-foreground mb-4">{desc}</p>
      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <Label className="text-xs">{t("common.enable")}</Label>
          <Switch checked={enabled} onCheckedChange={onToggle} />
        </div>
        <div className="flex items-center justify-between">
          <Label className="text-xs">{t("settings.audio.timing")}</Label>
          <Select value={timing} onValueChange={(v) => onTimingChange(v as SoundTiming)} disabled={!enabled}>
            <SelectTrigger className="w-[120px] h-8 text-xs"><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="immediate">{t("settings.audio.timingImmediate")}</SelectItem>
              <SelectItem value="after_success">{t("settings.audio.timingAfterSuccess")}</SelectItem>
            </SelectContent>
          </Select>
        </div>
      </div>
    </div>
  );
}

export function AudioTab() {
  const { t } = useTranslation();
  const {
    copySound, setCopySound, copySoundTiming, setCopySoundTiming,
    pasteSound, setPasteSound, pasteSoundTiming, setPasteSoundTiming,
  } = useUISettings();

  return (
    <div className="space-y-4">
      <SoundCard title={t("settings.audio.copyTitle")} desc={t("settings.audio.copyDesc")}
        enabled={copySound} onToggle={setCopySound}
        timing={copySoundTiming} onTimingChange={setCopySoundTiming} />
      <SoundCard title={t("settings.audio.pasteTitle")} desc={t("settings.audio.pasteDesc")}
        enabled={pasteSound} onToggle={setPasteSound}
        timing={pasteSoundTiming} onTimingChange={setPasteSoundTiming} />
    </div>
  );
}
