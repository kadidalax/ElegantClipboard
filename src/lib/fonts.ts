/** 与 index.css @font-face / body 默认栈一致 */
export const DEFAULT_FONT_STACK =
  '"CustomFont", -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif';

export function resolveUiFontFamilyCss(customFont: string): string {
  return customFont ? `"${customFont}", ${DEFAULT_FONT_STACK}` : DEFAULT_FONT_STACK;
}

export function resolveCardFontFamilyCss(cardFont: string, customFont: string): string {
  if (cardFont) return `"${cardFont}", ${DEFAULT_FONT_STACK}`;
  return resolveUiFontFamilyCss(customFont);
}

export function resolvePreviewFontFamilyCss(
  previewFont: string,
  cardFont: string,
  customFont: string,
): string {
  if (previewFont) return `"${previewFont}", ${DEFAULT_FONT_STACK}`;
  return resolveCardFontFamilyCss(cardFont, customFont);
}
