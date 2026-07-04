const heightByItemId = new Map<number, { height: number; settingsKey: string }>();

export function getCachedMasonryHeight(
  itemId: number,
  settingsKey: string,
): number | null {
  const entry = heightByItemId.get(itemId);
  if (!entry || entry.settingsKey !== settingsKey) return null;
  return entry.height;
}

/** 返回 true 表示高度有变化，需要重建布局 */
export function setCachedMasonryHeight(
  itemId: number,
  height: number,
  settingsKey: string,
): boolean {
  const rounded = Math.round(height);
  if (rounded <= 0) return false;
  const prev = heightByItemId.get(itemId);
  if (prev?.settingsKey === settingsKey && prev.height === rounded) return false;
  heightByItemId.set(itemId, { height: rounded, settingsKey });
  return true;
}

export function clearMasonryHeightCacheForTest(): void {
  heightByItemId.clear();
}
