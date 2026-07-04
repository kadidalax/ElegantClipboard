import type { ClipboardItem } from "@/stores/clipboard";

const MASONRY_COLUMN_COUNT = 2;
const CARD_META_HEIGHT = 36;
const CARD_PADDING = 20;
const FILE_CARD_HEIGHT = 88;

export const MASONRY_COLUMN_GAP_PX = 8;
export const MASONRY_HORIZONTAL_INSET_PX = 8;
export const MASONRY_SEPARATOR_HEIGHT_PX = 17;
export const MASONRY_VIRTUAL_OVERSCAN_PX = 400;

export interface MasonryEntry<T> {
  item: T;
  index: number;
}

export interface MasonryPlacedItem<T> {
  item: T;
  index: number;
  column: number;
  top: number;
  height: number;
}

export interface MasonrySectionLayout<T> {
  placed: MasonryPlacedItem<T>[];
  totalHeight: number;
}

export function estimateClipboardItemHeight(
  item: ClipboardItem,
  cardMaxLines: number,
  imageMaxHeight: number,
  imageAutoHeight: boolean,
  columnWidth = 180,
): number {
  switch (item.content_type) {
    case "image": {
      if (
        imageAutoHeight &&
        item.image_width &&
        item.image_height &&
        item.image_width > 0
      ) {
        const scaled = (item.image_height / item.image_width) * columnWidth;
        return Math.min(imageMaxHeight, scaled) + CARD_META_HEIGHT + CARD_PADDING;
      }
      return Math.min(imageMaxHeight, 160) + CARD_META_HEIGHT + CARD_PADDING;
    }
    case "files":
      return FILE_CARD_HEIGHT + CARD_META_HEIGHT + CARD_PADDING;
    default:
      return 20 + cardMaxLines * 20 + CARD_META_HEIGHT + CARD_PADDING;
  }
}

export function masonryRowGapPx(cardDensity: string): number {
  const gaps: Record<string, number> = {
    compact: 6,
    standard: 8,
    spacious: 12,
  };
  return gaps[cardDensity] ?? 8;
}

export function masonryColumnWidth(containerWidth: number): number {
  if (containerWidth <= 0) return 180;
  const available =
    containerWidth - MASONRY_HORIZONTAL_INSET_PX * 2 - MASONRY_COLUMN_GAP_PX;
  return Math.max(0, available) / MASONRY_COLUMN_COUNT;
}

/** 按全局顺序贪心分配到最短列；列分配在条目顺序变化前保持稳定 */
export function buildMasonrySection<T>(
  items: T[],
  startIndex: number,
  getHeight: (item: T) => number,
  rowGapPx: number,
  columnCount = MASONRY_COLUMN_COUNT,
): MasonrySectionLayout<T> {
  const colHeights = new Array<number>(columnCount).fill(0);
  const placed: MasonryPlacedItem<T>[] = [];

  items.forEach((item, offset) => {
    let shortestCol = 0;
    for (let col = 1; col < columnCount; col += 1) {
      if (colHeights[col] < colHeights[shortestCol]) shortestCol = col;
    }
    const height = getHeight(item);
    const top = colHeights[shortestCol];
    placed.push({
      item,
      index: startIndex + offset,
      column: shortestCol,
      top,
      height,
    });
    colHeights[shortestCol] = top + height + rowGapPx;
  });

  const totalHeight =
    colHeights.length === 0
      ? 0
      : Math.max(
          ...colHeights.map((height) => (height > 0 ? height - rowGapPx : 0)),
          0,
        );

  return { placed, totalHeight };
}

export function getVisiblePlacedItems<T>(
  placed: MasonryPlacedItem<T>[],
  scrollTop: number,
  viewportHeight: number,
  overscan = MASONRY_VIRTUAL_OVERSCAN_PX,
): MasonryPlacedItem<T>[] {
  if (placed.length === 0 || viewportHeight <= 0) return [];
  const start = scrollTop - overscan;
  const end = scrollTop + viewportHeight + overscan;
  return placed.filter(
    (entry) => entry.top + entry.height >= start && entry.top <= end,
  );
}

export function findPlacedItemByIndex<T>(
  pinnedLayout: MasonrySectionLayout<T> | null,
  mainLayout: MasonrySectionLayout<T>,
  pinnedBlockHeight: number,
  index: number,
): { entry: MasonryPlacedItem<T>; absoluteTop: number } | null {
  if (pinnedLayout) {
    const pinned = pinnedLayout.placed.find((entry) => entry.index === index);
    if (pinned) return { entry: pinned, absoluteTop: pinned.top };
  }
  const main = mainLayout.placed.find((entry) => entry.index === index);
  if (main) return { entry: main, absoluteTop: pinnedBlockHeight + main.top };
  return null;
}

/** @deprecated 使用 buildMasonrySection；保留供测试兼容 */
export function distributeToMasonryColumns<T>(
  items: T[],
  startIndex: number,
  estimateHeight: (item: T) => number,
  columnCount = MASONRY_COLUMN_COUNT,
): MasonryEntry<T>[][] {
  const layout = buildMasonrySection(items, startIndex, estimateHeight, 8, columnCount);
  const columns: MasonryEntry<T>[][] = Array.from({ length: columnCount }, () => []);
  layout.placed.forEach(({ item, index, column }) => {
    columns[column].push({ item, index });
  });
  return columns;
}

export { MASONRY_COLUMN_COUNT };
