/**
 * i18n module — public API
 *
 * Structure:
 *   core/       merge + translator utilities
 *   messages/   locale message trees (zh-CN, en, zh-TW)
 *   runtime.ts  locale store, hooks, init
 *   types.ts    shared types
 */

export {
  t,
  useTranslation,
  useLocaleStore,
  getLocale,
  initLocale,
  initLocaleListener,
} from "./runtime";

export type { Locale, TranslationTree } from "./types";

export {
  LOCALE_OPTIONS,
  DEFAULT_LOCALE,
  createTranslator,
  normalizeLocale,
  applyDocumentLocale,
} from "./core/translator";
