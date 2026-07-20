import { describe, expect, it, vi } from "vitest";
import type { ClipboardItem } from "@/stores/clipboard";
import { deleteActiveClipboardItem } from "./ClipboardList";

describe("deleteActiveClipboardItem", () => {
  it("does not call delete for a locked active item", () => {
    const deleteItem = vi.fn();
    const setActiveIndex = vi.fn();
    const items = [{ id: 1, is_locked: true }] as ClipboardItem[];

    deleteActiveClipboardItem(items, 0, deleteItem, setActiveIndex);

    expect(deleteItem).not.toHaveBeenCalled();
    expect(setActiveIndex).not.toHaveBeenCalled();
  });

  it("deletes an unlocked active item", () => {
    const deleteItem = vi.fn();
    const setActiveIndex = vi.fn();
    const items = [{ id: 1, is_locked: false }] as ClipboardItem[];

    deleteActiveClipboardItem(items, 0, deleteItem, setActiveIndex);

    expect(deleteItem).toHaveBeenCalledWith(1);
    expect(setActiveIndex).toHaveBeenCalledWith(0);
  });
});
