import { LIST_FETCH_LIMIT } from "@/lib/constants";
import type { ClipboardItem } from "@/stores/clipboard";

function compareItems(a: ClipboardItem, b: ClipboardItem): number {
  if (a.is_pinned !== b.is_pinned) {
    return a.is_pinned ? -1 : 1;
  }
  if (a.sort_order !== b.sort_order) {
    return b.sort_order - a.sort_order;
  }
  return new Date(b.created_at).getTime() - new Date(a.created_at).getTime();
}

export function mergeCaptureItem(
  items: ClipboardItem[],
  incoming: ClipboardItem,
  maxItems = LIST_FETCH_LIMIT,
): ClipboardItem[] {
  const merged = [incoming, ...items.filter((item) => item.id !== incoming.id)];
  merged.sort(compareItems);
  if (merged.length > maxItems) {
    merged.length = maxItems;
  }
  return merged;
}

export function matchesListFilter(
  item: ClipboardItem,
  selectedGroup: string | null,
  selectedGroupId: number | null,
): boolean {
  if (selectedGroupId !== null) {
    // 自定义分组视图：新捕获条目写入当前活动分组
    return true;
  }

  if (!selectedGroup) {
    return true;
  }

  if (selectedGroup === "__favorites__") {
    return item.is_favorite;
  }

  const types = selectedGroup.split(",").map((t) => t.trim());
  return types.includes(item.content_type);
}
