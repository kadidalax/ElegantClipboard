import { t } from "@/i18n";
import type { ToolbarButton } from "@/stores/ui-settings";

/** 工具栏按钮注册表 */
export function getToolbarButtonRegistry(): Record<
  ToolbarButton,
  { label: string; description: string }
> {
  return {
    clear: { label: t("toolbar.clearHistory"), description: t("toolbar.clearHistoryDesc") },
    pin: { label: t("toolbar.pinWindow"), description: t("toolbar.pinWindowDesc") },
    batch: { label: t("toolbar.batchSelect"), description: t("toolbar.batchSelectDesc") },
    settings: { label: t("toolbar.settings"), description: t("toolbar.settingsDesc") },
  };
}

/** 分类分组值（App 标签页和键盘导航共用） */
export const GROUP_VALUES = [
  { value: null, labelKey: "groups.all" },
  { value: "__favorites__", labelKey: "groups.favorites" },
  { value: "text,html,rtf", labelKey: "groups.text" },
  { value: "image,files,url", labelKey: "groups.other" },
] as const;

export type GroupValue = (typeof GROUP_VALUES)[number]["value"];

export function getGroups() {
  return GROUP_VALUES.map((group) => ({
    label: t(group.labelKey),
    value: group.value,
  }));
}

export function getContentTypeLabel(type: string): string {
  const key = `contentType.${type}`;
  const label = t(key);
  return label === key ? t("contentType.text") : label;
}
