import { SettingsCard, SettingsCardHeader } from "@/components/settings/SettingSection";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { useTranslation } from "@/i18n";
import { previewCopySound, previewPasteSound } from "@/lib/sounds";
import { useUISettings, type SoundTiming } from "@/stores/ui-settings";

function SoundCard({
  title,
  desc,
  enabled,
  onToggle,
  timing,
  onTimingChange,
  onPreview,
}: {
  title: string;
  desc: string;
  enabled: boolean;
  onToggle: (v: boolean) => void;
  timing: SoundTiming;
  onTimingChange: (v: SoundTiming) => void;
  onPreview: () => void;
}) {
  const { t } = useTranslation();

  return (
    <SettingsCard>
      <SettingsCardHeader title={title} description={desc} />
      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <Label className="text-xs">{t("common.enable")}</Label>
          <Switch checked={enabled} onCheckedChange={onToggle} />
        </div>
        <div className="flex items-center justify-between gap-2">
          <Label className="text-xs shrink-0">{t("settings.audio.timing")}</Label>
          <div className="flex items-center gap-2">
            <Select value={timing} onValueChange={(v) => onTimingChange(v as SoundTiming)} disabled={!enabled}>
              <SelectTrigger className="w-[120px] h-8 text-xs"><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="immediate">{t("settings.audio.timingImmediate")}</SelectItem>
                <SelectItem value="after_success">{t("settings.audio.timingAfterSuccess")}</SelectItem>
              </SelectContent>
            </Select>
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="h-8 shrink-0 text-xs"
              onClick={onPreview}
            >
              {t("settings.audio.preview")}
            </Button>
          </div>
        </div>
      </div>
    </SettingsCard>
  );
}

export function AudioTab() {
  const { t } = useTranslation();
  const {
    copySound, setCopySound, copySoundTiming, setCopySoundTiming,
    pasteSound, setPasteSound, pasteSoundTiming, setPasteSoundTiming,
  } = useUISettings();

  return (
    <div className="space-y-3">
      <SoundCard
        title={t("settings.audio.copyTitle")}
        desc={t("settings.audio.copyDesc")}
        enabled={copySound}
        onToggle={setCopySound}
        timing={copySoundTiming}
        onTimingChange={setCopySoundTiming}
        onPreview={previewCopySound}
      />
      <SoundCard
        title={t("settings.audio.pasteTitle")}
        desc={t("settings.audio.pasteDesc")}
        enabled={pasteSound}
        onToggle={setPasteSound}
        timing={pasteSoundTiming}
        onTimingChange={setPasteSoundTiming}
        onPreview={previewPasteSound}
      />
    </div>
  );
}
