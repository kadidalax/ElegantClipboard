import type { Dispatch, SetStateAction } from "react";
import { useMemo } from "react";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useTranslation } from "@/i18n";

type SyncTypesSectionProps = {
  syncTypes: Set<string>;
  setSyncTypes: Dispatch<SetStateAction<Set<string>>>;
  maxImageSizeKb: string;
  setMaxImageSizeKb: (value: string) => void;
  maxFileSizeKb: string;
  setMaxFileSizeKb: (value: string) => void;
  maxVideoSizeKb: string;
  setMaxVideoSizeKb: (value: string) => void;
};

const SYNC_TYPES = ["text", "image", "files"] as const;

const COMMON_SIZE_OPTIONS = [
  ["1024", "1 MB"],
  ["2048", "2 MB"],
  ["5120", "5 MB"],
  ["10240", "10 MB"],
  ["20480", "20 MB"],
  ["51200", "50 MB"],
  ["0", "noLimit"],
] as const;

const LARGE_SIZE_OPTIONS = [
  ...COMMON_SIZE_OPTIONS.slice(0, -1),
  ["102400", "100 MB"],
  COMMON_SIZE_OPTIONS[COMMON_SIZE_OPTIONS.length - 1],
] as const;

function SizeLimitRow({
  label,
  typeLabel,
  value,
  onChange,
  includeLargeOption = false,
  noLimitLabel,
  syncOnlyTemplate,
}: {
  label: string;
  typeLabel: string;
  value: string;
  onChange: (value: string) => void;
  includeLargeOption?: boolean;
  noLimitLabel: string;
  syncOnlyTemplate: (size: string, type: string) => string;
}) {
  const options = includeLargeOption ? LARGE_SIZE_OPTIONS : COMMON_SIZE_OPTIONS;
  const sizeMb = Math.round(parseInt(value || "0") / 1024);
  const sizeLabel = sizeMb > 0 ? `${sizeMb} MB` : noLimitLabel;

  return (
    <div className="flex items-center justify-between">
      <div className="space-y-0.5">
        <Label className="text-xs">{label}</Label>
        <p className="text-xs text-muted-foreground">
          {syncOnlyTemplate(sizeLabel, typeLabel)}
        </p>
      </div>
      <Select value={value} onValueChange={onChange}>
        <SelectTrigger className="w-[120px] h-8 text-xs"><SelectValue /></SelectTrigger>
        <SelectContent>
          {options.map(([optionValue, optionLabel]) => (
            <SelectItem key={optionValue} value={optionValue}>
              {optionLabel === "noLimit" ? noLimitLabel : optionLabel}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  );
}

export function SyncTypesSection({
  syncTypes,
  setSyncTypes,
  maxImageSizeKb,
  setMaxImageSizeKb,
  maxFileSizeKb,
  setMaxFileSizeKb,
  maxVideoSizeKb,
  setMaxVideoSizeKb,
}: SyncTypesSectionProps) {
  const { t } = useTranslation();

  const typeLabels = useMemo(() => ({
    text: t("settings.sync.typeText"),
    image: t("settings.sync.typeImage"),
    files: t("settings.sync.typeFiles"),
    video: t("settings.sync.typeVideo"),
  }), [t]);

  const syncOnly = (size: string, type: string) =>
    t("settings.sync.syncOnly", { size, type });

  return (
    <div className="rounded-lg border bg-card p-4">
      <h3 className="text-sm font-medium mb-3">{t("settings.sync.typesTitle")}</h3>
      <p className="text-xs text-muted-foreground mb-4">
        {t("settings.sync.typesDesc")}
      </p>
      <div className="space-y-3">
        <div className="flex flex-wrap gap-2">
          {SYNC_TYPES.map((type) => {
            const active = syncTypes.has(type);
            return (
              <button
                key={type}
                type="button"
                onClick={() => {
                  setSyncTypes((prev) => {
                    const next = new Set(prev);
                    if (next.has(type)) {
                      next.delete(type);
                      if (next.size === 0) return prev;
                    } else {
                      next.add(type);
                    }
                    return next;
                  });
                }}
                className={`px-3 py-1.5 text-xs font-medium rounded-md border transition-colors ${
                  active
                    ? "bg-primary text-primary-foreground border-primary"
                    : "bg-muted/40 text-muted-foreground border-transparent hover:bg-muted"
                }`}
              >
                {typeLabels[type]}
              </button>
            );
          })}
        </div>
        {syncTypes.has("image") && (
          <SizeLimitRow
            label={t("settings.sync.imageSizeLimit")}
            typeLabel={typeLabels.image}
            value={maxImageSizeKb}
            onChange={setMaxImageSizeKb}
            noLimitLabel={t("settings.sync.noLimit")}
            syncOnlyTemplate={syncOnly}
          />
        )}
        {syncTypes.has("files") && (
          <SizeLimitRow
            label={t("settings.sync.fileSizeLimit")}
            typeLabel={typeLabels.files}
            value={maxFileSizeKb}
            onChange={setMaxFileSizeKb}
            includeLargeOption
            noLimitLabel={t("settings.sync.noLimit")}
            syncOnlyTemplate={syncOnly}
          />
        )}
        {syncTypes.has("video") && (
          <SizeLimitRow
            label={t("settings.sync.videoSizeLimit")}
            typeLabel={typeLabels.video}
            value={maxVideoSizeKb}
            onChange={setMaxVideoSizeKb}
            includeLargeOption
            noLimitLabel={t("settings.sync.noLimit")}
            syncOnlyTemplate={syncOnly}
          />
        )}
      </div>
    </div>
  );
}
