import { en } from "../messages/en";
import { zhCN } from "../messages/zh-CN";
import { zhTW } from "../messages/zh-TW";
import type { Locale, TranslationTree } from "../types";

export const DEFAULT_LOCALE: Locale = "zh-CN";

export const LOCALE_OPTIONS: {
  value: Locale;
  labelKey: "language.zhCN" | "language.en" | "language.zhTW";
}[] = [
  { value: "zh-CN", labelKey: "language.zhCN" },
  { value: "en", labelKey: "language.en" },
  { value: "zh-TW", labelKey: "language.zhTW" },
];

const LOCALES: Record<Locale, TranslationTree> = {
  "zh-CN": zhCN as TranslationTree,
  en: en as TranslationTree,
  "zh-TW": zhTW as TranslationTree,
};

function resolvePath(tree: TranslationTree, path: string): string | undefined {
  let current: unknown = tree;
  for (const part of path.split(".")) {
    if (!current || typeof current !== "object" || !(part in current)) {
      return undefined;
    }
    current = (current as Record<string, unknown>)[part];
  }
  return typeof current === "string" ? current : undefined;
}

export function createTranslator(locale: Locale) {
  const tree = LOCALES[locale] ?? LOCALES[DEFAULT_LOCALE];
  return (key: string, params?: Record<string, string | number>) => {
    let text = resolvePath(tree, key) ?? resolvePath(LOCALES[DEFAULT_LOCALE], key) ?? key;
    if (params) {
      for (const [name, value] of Object.entries(params)) {
        text = text.split(`{{${name}}}`).join(String(value));
      }
    }
    return text;
  };
}

export function normalizeLocale(raw: string | null | undefined): Locale {
  if (raw === "en" || raw === "zh-TW" || raw === "zh-CN") {
    return raw;
  }
  return DEFAULT_LOCALE;
}

export function applyDocumentLocale(locale: Locale) {
  if (typeof document !== "undefined") {
    document.documentElement.lang = locale;
  }
}
