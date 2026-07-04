import { useEffect, useState, useMemo } from "react";
import {
  Checkmark16Filled,
  Desktop16Regular,
} from "@fluentui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import { SettingsCard, SettingsCardHeader } from "@/components/settings/SettingSection";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger } from "@/components/ui/select";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { useTranslation } from "@/i18n";
import { logError } from "@/lib/logger";
import { getAccentColor, subscribeAccentColor } from "@/lib/theme-applier";
import { cn } from "@/lib/utils";
import { useUISettings, ColorTheme, WindowEffect } from "@/stores/ui-settings";

// 过滤艺术字体的关键词（大小写不敏感）
const ART_FONT_PATTERNS = /^(Webdings|Wingdings|MT Extra|Symbol|Bookshelf Symbol|MS Outlook|High Tower Text|Pristina|Jokerman|Vivaldi|Kristen IT|French Script|Playbill|Mistral|Papyrus)/i;

const DARK_MODE_OPTIONS = [
  { value: "auto" as const, labelKey: "settings.theme.darkAuto" as const },
  { value: "light" as const, labelKey: "settings.theme.darkLight" as const },
  { value: "dark" as const, labelKey: "settings.theme.darkDark" as const },
];

function ThemeColorSwatch({
  themeId,
  systemAccent,
}: {
  themeId: ColorTheme;
  systemAccent: string | null;
}) {
  const themeClass =
    themeId === "default" ? undefined : themeId === "system" ? "theme-system" : `theme-${themeId}`;
  const accentStyle =
    themeId === "system"
      ? (() => {
          const parts = (systemAccent ?? "210 65% 50%").split(" ");
          return {
            "--system-accent-h": parts[0],
            "--system-accent-s": parts[1] || "65%",
            "--system-accent-l": parts[2] || "50%",
          } as React.CSSProperties;
        })()
      : undefined;

  return (
    <div className={cn("flex gap-1.5 shrink-0", themeClass)} style={accentStyle}>
      <div className="w-8 h-8 rounded-md elevation-control bg-primary" />
      <div className="w-8 h-8 rounded-md border elevation-control bg-secondary" />
    </div>
  );
}

function FontSettingGroup({ label, fonts, font, onFontChange, fontSize, onFontSizeChange, min, max, defaultFontLabel, fontSizeLabel }: {
  label: string;
  fonts: string[];
  font: string;
  onFontChange: (v: string) => void;
  fontSize: number;
  onFontSizeChange: (v: number) => void;
  min: number;
  max: number;
  defaultFontLabel: string;
  fontSizeLabel: string;
}) {
  const [open, setOpen] = useState(false);

  return (
    <div className="space-y-3">
      <Label className="text-xs font-medium">{label}</Label>
      <Select
        value={font || "__default__"}
        onValueChange={(v) => onFontChange(v === "__default__" ? "" : v)}
        onOpenChange={setOpen}
      >
        <SelectTrigger className="w-full h-8 text-xs">
          <span className="line-clamp-1">{font || defaultFontLabel}</span>
        </SelectTrigger>
        {open && (
          <SelectContent className="max-h-64 overflow-y-auto">
            <SelectItem value="__default__" className="text-xs">{defaultFontLabel}</SelectItem>
            {fonts.map((f) => (
              <SelectItem key={f} value={f} className="text-xs">{f}</SelectItem>
            ))}
          </SelectContent>
        )}
      </Select>
      <div className="flex items-center justify-between">
        <Label className="text-xs">{fontSizeLabel}</Label>
        <span className="text-xs font-medium tabular-nums">{fontSize}px</span>
      </div>
      <Slider value={[fontSize]} onValueChange={(v) => onFontSizeChange(v[0])} min={min} max={max} step={1} />
    </div>
  );
}

export function ThemeTab() {
  const { t } = useTranslation();
  const {
    colorTheme, setColorTheme, sharpCorners, setSharpCorners, darkMode, setDarkMode,
    windowEffect, setWindowEffect, customFont, setCustomFont, uiFontSize, setUIFontSize,
    cardFont, setCardFont, cardFontSize, setCardFontSize,
    previewFont, setPreviewFont, previewFontSize, setPreviewFontSize,
    resetFontSettings,
  } = useUISettings();
  const [systemAccentColor, setSystemAccentColor] = useState(getAccentColor);
  const [systemFonts, setSystemFonts] = useState<string[]>([]);

  // 强调色变化时重新渲染
  useEffect(() => subscribeAccentColor(setSystemAccentColor), []);

  // 加载系统字体列表（过滤掉艺术字体）
  useEffect(() => {
    invoke<string[]>("get_system_fonts")
      .then((fonts) => setSystemFonts(fonts.filter((f) => !ART_FONT_PATTERNS.test(f))))
      .catch((error) => {
        logError("Failed to load system fonts:", error);
      });
  }, []);

  const themes = useMemo(() => [
    {
      id: "system" as ColorTheme,
      name: t("settings.theme.system"),
      description: systemAccentColor
        ? t("settings.theme.systemAccentCurrent")
        : t("settings.theme.systemAccentAuto"),
      icon: Desktop16Regular,
    },
    {
      id: "default" as ColorTheme,
      name: t("settings.theme.default"),
      description: t("settings.theme.defaultDesc"),
    },
    {
      id: "emerald" as ColorTheme,
      name: t("settings.theme.emerald"),
      description: t("settings.theme.emeraldDesc"),
    },
    {
      id: "cyan" as ColorTheme,
      name: t("settings.theme.cyan"),
      description: t("settings.theme.cyanDesc"),
    },
  ], [t, systemAccentColor]);

  const windowEffects = useMemo(() => [
    { value: "none" as WindowEffect, label: t("settings.theme.effectNone"), desc: t("settings.theme.effectNoneDesc") },
    { value: "mica" as WindowEffect, label: t("settings.theme.effectMica"), desc: t("settings.theme.effectMicaDesc") },
    { value: "acrylic" as WindowEffect, label: t("settings.theme.effectAcrylic"), desc: t("settings.theme.effectAcrylicDesc") },
    { value: "tabbed" as WindowEffect, label: t("settings.theme.effectTabbed"), desc: t("settings.theme.effectTabbedDesc") },
  ], [t]);

  const fontLabels = useMemo(() => ({
    defaultFont: t("settings.theme.fontDefault"),
    fontSize: t("settings.theme.fontSize"),
    ui: t("settings.theme.fontUI"),
    card: t("settings.theme.fontCard"),
    preview: t("settings.theme.fontPreview"),
  }), [t]);

  const activeDarkModeIndex = Math.max(
    0,
    DARK_MODE_OPTIONS.findIndex((opt) => opt.value === darkMode),
  );

  return (
    <div className="space-y-3">
      <SettingsCard>
        <SettingsCardHeader
          title={t("settings.theme.colorTitle")}
          description={t("settings.theme.colorDesc")}
        />

        <div className="space-y-2">
          {themes.map((theme) => {
            const Icon = theme.icon;
            const isActive = colorTheme === theme.id;
            return (
              <button
                key={theme.id}
                onClick={() => setColorTheme(theme.id)}
                className={`
                  w-full flex items-center gap-3 p-3 rounded-md border transition-surface
                  ${isActive
                    ? "border-primary bg-primary-faint"
                    : "border-transparent hover:bg-accent"
                  }
                `}
              >
                <ThemeColorSwatch themeId={theme.id} systemAccent={systemAccentColor} />

                {/* Theme Info */}
                <div className="flex-1 text-left">
                  <div className="flex items-center gap-2">
                    {Icon && <Icon className="w-3.5 h-3.5 text-muted-foreground" />}
                    <span className="text-xs font-medium">{theme.name}</span>
                    {isActive && (
                      <Checkmark16Filled className="w-3.5 h-3.5 text-primary" />
                    )}
                  </div>
                  <span className="text-caption text-muted-foreground">
                    {theme.description}
                  </span>
                </div>
              </button>
            );
          })}
        </div>
      </SettingsCard>

      {/* Dark Mode */}
      <SettingsCard>
        <SettingsCardHeader
          title={t("settings.theme.darkModeTitle")}
          description={t("settings.theme.darkModeDesc")}
        />
        <div
          role="radiogroup"
          aria-label={t("settings.theme.darkModeAria")}
          className="relative rounded-md border bg-muted-surface-subtle p-1"
        >
          <div className="relative grid grid-cols-3">
            <div
              aria-hidden
              className="absolute inset-y-0 left-0 w-1/3 rounded-md bg-primary elevation-control will-change-transform transition-transform duration-200 ease-out"
              style={{ transform: `translateX(${activeDarkModeIndex * 100}%)` }}
            />
            {DARK_MODE_OPTIONS.map((opt) => {
              const isActive = darkMode === opt.value;
              return (
                <button
                  key={opt.value}
                  type="button"
                  role="radio"
                  aria-checked={isActive}
                  onClick={() => setDarkMode(opt.value)}
                  className={`relative z-1 rounded-md px-2.5 py-1.5 text-xs font-medium transition-surface ${
                    isActive
                      ? "text-primary-foreground"
                      : "text-foreground/80 hover:text-foreground"
                  }`}
                >
                  {t(opt.labelKey)}
                </button>
              );
            })}
          </div>
        </div>
      </SettingsCard>

      {/* Sharp Corners */}
      <SettingsCard>
        <SettingsCardHeader title={t("settings.theme.cornersTitle")} />
        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <Label className="text-xs">{t("settings.theme.sharpCorners")}</Label>
            <p className="text-xs text-muted-foreground">
              {t("settings.theme.sharpCornersDesc")}
            </p>
          </div>
          <Switch
            checked={sharpCorners}
            onCheckedChange={setSharpCorners}
          />
        </div>
      </SettingsCard>

      {/* Window Effect */}
      <SettingsCard>
        <SettingsCardHeader
          title={t("settings.theme.windowEffectTitle")}
          description={t("settings.theme.windowEffectDesc")}
        />
        <div className="grid grid-cols-2 gap-2">
          {windowEffects.map((opt) => (
            <button
              key={opt.value}
              onClick={() => setWindowEffect(opt.value)}
              className={`flex flex-col items-start p-3 rounded-md border transition-surface text-left ${
                windowEffect === opt.value
                  ? "border-primary bg-primary-faint"
                  : "border-transparent hover:bg-accent"
              }`}
            >
              <div className="flex items-center gap-2">
                <span className="text-xs font-medium">{opt.label}</span>
                {windowEffect === opt.value && (
                  <Checkmark16Filled className="w-3.5 h-3.5 text-primary" />
                )}
              </div>
              <span className="text-caption text-muted-foreground mt-0.5">
                {opt.desc}
              </span>
            </button>
          ))}
        </div>
      </SettingsCard>
      {/* Font Settings */}
      <SettingsCard className="space-y-5">
        <SettingsCardHeader
          className="mb-0"
          title={t("settings.theme.fontTitle")}
          description={t("settings.theme.fontDesc")}
          action={
            <button
              type="button"
              className="text-xs text-muted-foreground hover:text-foreground transition-surface shrink-0"
              onClick={resetFontSettings}
            >
              {t("settings.theme.fontReset")}
            </button>
          }
        />

        <FontSettingGroup label={fontLabels.ui} fonts={systemFonts} font={customFont} onFontChange={setCustomFont} fontSize={uiFontSize} onFontSizeChange={setUIFontSize} min={12} max={18} defaultFontLabel={fontLabels.defaultFont} fontSizeLabel={fontLabels.fontSize} />
        <hr className="border-border" />
        <FontSettingGroup label={fontLabels.card} fonts={systemFonts} font={cardFont} onFontChange={setCardFont} fontSize={cardFontSize} onFontSizeChange={setCardFontSize} min={12} max={18} defaultFontLabel={fontLabels.defaultFont} fontSizeLabel={fontLabels.fontSize} />
        <hr className="border-border" />
        <FontSettingGroup label={fontLabels.preview} fonts={systemFonts} font={previewFont} onFontChange={setPreviewFont} fontSize={previewFontSize} onFontSizeChange={setPreviewFontSize} min={11} max={18} defaultFontLabel={fontLabels.defaultFont} fontSizeLabel={fontLabels.fontSize} />
      </SettingsCard>
    </div>
  );
}
