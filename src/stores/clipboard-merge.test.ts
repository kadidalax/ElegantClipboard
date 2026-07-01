import { describe, expect, it } from "vitest";
import type { ClipboardItem } from "@/stores/clipboard";
import { mergeCaptureItem, matchesListFilter } from "@/stores/clipboard-merge";

function makeItem(overrides: Partial<ClipboardItem> & { id: number }): ClipboardItem {
  return {
    content_type: "text",
    text_content: null,
    html_content: null,
    rtf_content: null,
    image_path: null,
    file_paths: null,
    content_hash: "hash",
    preview: "preview",
    byte_size: 10,
    image_width: null,
    image_height: null,
    is_pinned: false,
    is_favorite: false,
    favorite_order: 0,
    sort_order: overrides.id,
    created_at: "2026-01-01T00:00:00",
    updated_at: "2026-01-01T00:00:00",
    access_count: 0,
    last_accessed_at: null,
    char_count: null,
    source_app_name: null,
    source_app_icon: null,
    ...overrides,
  };
}

describe("mergeCaptureItem", () => {
  it("prepends new item and removes duplicate id", () => {
    const existing = [makeItem({ id: 1, sort_order: 1 }), makeItem({ id: 2, sort_order: 2 })];
    const incoming = makeItem({ id: 3, sort_order: 99 });
    const merged = mergeCaptureItem(existing, incoming, 10);
    expect(merged.map((i) => i.id)).toEqual([3, 2, 1]);
  });

  it("updates existing item on dedup touch", () => {
    const existing = [makeItem({ id: 1, sort_order: 1, access_count: 0 })];
    const incoming = makeItem({ id: 1, sort_order: 50, access_count: 3 });
    const merged = mergeCaptureItem(existing, incoming, 10);
    expect(merged).toHaveLength(1);
    expect(merged[0].sort_order).toBe(50);
    expect(merged[0].access_count).toBe(3);
  });
});

describe("matchesListFilter", () => {
  it("filters by content type group", () => {
    const image = makeItem({ id: 1, content_type: "image" });
    expect(matchesListFilter(image, "image,files,url", null)).toBe(true);
    expect(matchesListFilter(image, "text,html,rtf", null)).toBe(false);
  });
});
