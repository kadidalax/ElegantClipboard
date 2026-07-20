import { describe, it, expect, vi, beforeEach } from "vitest";
import {
  canDeleteClipboardItem,
  hasLockedSelection,
  useClipboardStore,
  type ActiveDatabaseStats,
  type ClipboardItem,
} from "./clipboard";

const databaseStats: ActiveDatabaseStats = {
  id: "default",
  name: "默认数据库",
  item_count: 3,
  db_size: 4096,
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}

// Reset store before each test
beforeEach(async () => {
  const { invoke } = await import("@tauri-apps/api/core");
  const { listen } = await import("@tauri-apps/api/event");
  vi.mocked(invoke).mockReset().mockResolvedValue([]);
  vi.mocked(listen).mockClear().mockResolvedValue(() => {});
  useClipboardStore.setState({
    items: [],
    isLoading: false,
    searchQuery: "",
    selectedGroup: null,
    selectedGroupId: null,
    activeIndex: -1,
    batchMode: false,
    selectedIds: new Set(),
    lastSelectedIndex: -1,
    _fetchId: 0,
    _resetToken: 0,
    _timelineGeneration: 0,
    timelineSnapshot: null,
    timelineHighlightId: null,
    timelineRestoreRequest: null,
    timelineError: null,
    activeDatabaseStats: null,
    databaseStatsLoading: false,
    databaseStatsError: null,
    _databaseStatsFetchId: 0,
  });
});

describe("clipboard store", () => {
  describe("delete guards", () => {
    it("detects locked items and locked selections", () => {
      const items = [
        { id: 1, is_locked: false },
        { id: 2, is_locked: true },
      ] as ClipboardItem[];

      expect(canDeleteClipboardItem(items[0])).toBe(true);
      expect(canDeleteClipboardItem(items[1])).toBe(false);
      expect(canDeleteClipboardItem(undefined)).toBe(false);
      expect(hasLockedSelection(items, new Set([1]))).toBe(false);
      expect(hasLockedSelection(items, new Set([1, 2]))).toBe(true);
      expect(hasLockedSelection(items, new Set([99]))).toBe(true);
    });
  });

  describe("initial state", () => {
    it("has correct defaults", () => {
      const state = useClipboardStore.getState();
      expect(state.items).toEqual([]);
      expect(state.isLoading).toBe(false);
      expect(state.searchQuery).toBe("");
      expect(state.selectedGroup).toBeNull();
      expect(state.selectedGroupId).toBeNull();
      expect(state.activeIndex).toBe(-1);
      expect(state.batchMode).toBe(false);
      expect(state.selectedIds.size).toBe(0);
      expect(state.activeDatabaseStats).toBeNull();
    });
  });

  describe("active database stats", () => {
    it("loads the combined active database stats payload", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      vi.mocked(invoke).mockResolvedValueOnce(databaseStats);

      await useClipboardStore.getState().refreshActiveDatabaseStats();

      expect(invoke).toHaveBeenCalledWith("get_active_database_stats");
      expect(useClipboardStore.getState().activeDatabaseStats).toEqual(databaseStats);
    });

    it("refreshes stats after deleting an item", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      useClipboardStore.setState({
        items: [{ id: 1, is_locked: false }] as ClipboardItem[],
      });
      vi.mocked(invoke).mockImplementation((command) =>
        Promise.resolve(command === "get_active_database_stats" ? databaseStats : undefined),
      );

      await useClipboardStore.getState().deleteItem(1);

      expect(invoke).toHaveBeenCalledWith("get_active_database_stats");
    });

    it("refreshes stats after clearing history", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      vi.mocked(invoke).mockImplementation((command) => {
        if (command === "clear_history") return Promise.resolve(2);
        if (command === "get_clipboard_items") return Promise.resolve([]);
        if (command === "get_active_database_stats") return Promise.resolve(databaseStats);
        return Promise.resolve(undefined);
      });

      await useClipboardStore.getState().clearHistory();

      expect(invoke).toHaveBeenCalledWith("get_active_database_stats");
    });

    it("clears stale stats and records an error when refresh fails", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      useClipboardStore.setState({ activeDatabaseStats: databaseStats });
      vi.mocked(invoke).mockRejectedValueOnce(new Error("stats unavailable"));

      await useClipboardStore.getState().refreshActiveDatabaseStats();

      expect(useClipboardStore.getState()).toMatchObject({
        activeDatabaseStats: null,
        databaseStatsLoading: false,
        databaseStatsError: "Error: stats unavailable",
      });
    });

    it("clears stale stats before refreshing after a database switch", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const pendingStats = deferred<ActiveDatabaseStats>();
      useClipboardStore.setState({
        activeDatabaseStats: databaseStats,
        databaseStatsError: "old error",
      });
      vi.mocked(invoke).mockImplementation((command) => {
        if (command === "get_active_database_stats") return pendingStats.promise;
        return Promise.resolve([]);
      });
      const cleanup = await useClipboardStore.getState().setupListener();
      const { listen } = await import("@tauri-apps/api/event");
      const databaseListener = vi
        .mocked(listen)
        .mock.calls.find(([event]) => event === "database-switched")?.[1];

      databaseListener?.({ event: "database-switched", id: 0, payload: undefined });

      expect(useClipboardStore.getState()).toMatchObject({
        activeDatabaseStats: null,
        databaseStatsError: null,
      });
      expect(invoke).toHaveBeenCalledWith("get_active_database_stats");
      pendingStats.resolve({ ...databaseStats, id: "next", name: "新库" });
      await vi.waitFor(() => {
        expect(useClipboardStore.getState().activeDatabaseStats?.id).toBe("next");
      });
      cleanup();
    });

    it("ignores an older stats response after switching databases", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const oldStats = deferred<ActiveDatabaseStats>();
      const newStats = deferred<ActiveDatabaseStats>();
      let statsCalls = 0;
      vi.mocked(invoke).mockImplementation((command) => {
        if (command === "get_active_database_stats") {
          statsCalls += 1;
          return statsCalls === 1 ? oldStats.promise : newStats.promise;
        }
        return Promise.resolve([]);
      });

      const oldRequest = useClipboardStore.getState().refreshActiveDatabaseStats();
      const cleanup = await useClipboardStore.getState().setupListener();
      const { listen } = await import("@tauri-apps/api/event");
      const databaseListener = vi
        .mocked(listen)
        .mock.calls.find(([event]) => event === "database-switched")?.[1];
      databaseListener?.({ event: "database-switched", id: 0, payload: undefined });

      newStats.resolve({ ...databaseStats, id: "next", name: "新库" });
      await vi.waitFor(() => {
        expect(useClipboardStore.getState().activeDatabaseStats?.id).toBe("next");
      });
      oldStats.resolve(databaseStats);
      await oldRequest;

      expect(useClipboardStore.getState().activeDatabaseStats?.id).toBe("next");
      cleanup();
    });
  });

  describe("setSearchQuery", () => {
    it("updates search query", () => {
      useClipboardStore.getState().setSearchQuery("test query");
      expect(useClipboardStore.getState().searchQuery).toBe("test query");
    });

    it("handles empty string", () => {
      useClipboardStore.getState().setSearchQuery("something");
      useClipboardStore.getState().setSearchQuery("");
      expect(useClipboardStore.getState().searchQuery).toBe("");
    });
  });

  describe("setSelectedGroup", () => {
    it("updates selected group and resets batch state", () => {
      useClipboardStore.setState({
        batchMode: true,
        selectedIds: new Set([1, 2, 3]),
        lastSelectedIndex: 5,
      });

      useClipboardStore.getState().setSelectedGroup("text");

      const state = useClipboardStore.getState();
      expect(state.selectedGroup).toBe("text");
      expect(state.batchMode).toBe(false);
      expect(state.selectedIds.size).toBe(0);
      expect(state.lastSelectedIndex).toBe(-1);
    });

    it("sets group to null", () => {
      useClipboardStore.getState().setSelectedGroup("image");
      useClipboardStore.getState().setSelectedGroup(null);
      expect(useClipboardStore.getState().selectedGroup).toBeNull();
    });
  });

  describe("setSelectedGroupId", () => {
    it("updates group id and persists", () => {
      useClipboardStore.getState().setSelectedGroupId(42);
      expect(useClipboardStore.getState().selectedGroupId).toBe(42);
    });

    it("resets batch state", () => {
      useClipboardStore.setState({
        batchMode: true,
        selectedIds: new Set([1]),
      });
      useClipboardStore.getState().setSelectedGroupId(1);
      expect(useClipboardStore.getState().batchMode).toBe(false);
      expect(useClipboardStore.getState().selectedIds.size).toBe(0);
    });
  });

  describe("setActiveIndex", () => {
    it("updates active index", () => {
      useClipboardStore.getState().setActiveIndex(5);
      expect(useClipboardStore.getState().activeIndex).toBe(5);
    });

    it("resets to -1", () => {
      useClipboardStore.getState().setActiveIndex(3);
      useClipboardStore.getState().setActiveIndex(-1);
      expect(useClipboardStore.getState().activeIndex).toBe(-1);
    });
  });

  describe("timeline locate", () => {
    it("restores search state after leaving timeline", async () => {
      const searchItems = [
        { id: 42, group_id: 7 },
        { id: 11, group_id: 7 },
      ] as ClipboardItem[];
      const timelineItems = [
        { id: 42, group_id: 7 },
        { id: 41, group_id: 7 },
      ] as ClipboardItem[];
      useClipboardStore.setState({
        items: searchItems,
        searchQuery: "invoice",
        selectedGroup: "text",
        selectedGroupId: 7,
        activeIndex: 1,
      });
      const { invoke } = await import("@tauri-apps/api/core");
      vi.mocked(invoke).mockResolvedValueOnce(timelineItems);

      const located = await useClipboardStore.getState().enterTimeline(42, 640);

      expect(located).toBe(true);
      expect(invoke).toHaveBeenLastCalledWith(
        "get_clipboard_items",
        expect.objectContaining({
          search: null,
          contentType: null,
          favoriteOnly: false,
          groupId: null,
          timeline: true,
        }),
      );
      expect(useClipboardStore.getState().timelineSnapshot).toEqual({
        searchQuery: "invoice",
        selectedGroup: "text",
        selectedGroupId: 7,
        activeItemId: 11,
        scrollTop: 640,
      });
      expect(useClipboardStore.getState().activeIndex).toBe(0);
      expect(useClipboardStore.getState().timelineHighlightId).toBe(42);

      vi.mocked(invoke).mockResolvedValueOnce(searchItems);
      await useClipboardStore.getState().leaveTimeline();

      const state = useClipboardStore.getState();
      expect(state.searchQuery).toBe("invoice");
      expect(state.selectedGroup).toBe("text");
      expect(state.selectedGroupId).toBe(7);
      expect(state.activeIndex).toBe(1);
      expect(state.timelineSnapshot).toBeNull();
      expect(state.timelineRestoreRequest).toEqual({
        activeItemId: 11,
        scrollTop: 640,
      });
    });

    it("keeps the search view when the target disappeared", async () => {
      const searchItems = [{ id: 42, group_id: null }] as ClipboardItem[];
      useClipboardStore.setState({
        items: searchItems,
        searchQuery: "invoice",
      });
      const { invoke } = await import("@tauri-apps/api/core");
      vi.mocked(invoke)
        .mockResolvedValueOnce([{ id: 41, group_id: null }] as ClipboardItem[])
        .mockResolvedValueOnce(searchItems);

      const located = await useClipboardStore.getState().enterTimeline(42, 120);

      expect(located).toBe(false);
      expect(useClipboardStore.getState().searchQuery).toBe("invoice");
      expect(useClipboardStore.getState().items).toEqual(searchItems);
      expect(useClipboardStore.getState().timelineSnapshot).toBeNull();
    });

    it("clears timeline state on reset and database switch", async () => {
      useClipboardStore.setState({
        timelineSnapshot: {
          searchQuery: "invoice",
          selectedGroup: "text",
          selectedGroupId: null,
          activeItemId: 42,
          scrollTop: 120,
        },
        timelineHighlightId: 42,
        timelineRestoreRequest: { activeItemId: 42, scrollTop: 120 },
      });

      await useClipboardStore.getState().resetView();
      expect(useClipboardStore.getState().timelineSnapshot).toBeNull();

      useClipboardStore.setState({
        timelineSnapshot: {
          searchQuery: "again",
          selectedGroup: null,
          selectedGroupId: null,
          activeItemId: null,
          scrollTop: 0,
        },
        timelineHighlightId: 1,
      });
      const cleanup = await useClipboardStore.getState().setupListener();
      const { listen } = await import("@tauri-apps/api/event");
      const databaseListener = vi
        .mocked(listen)
        .mock.calls.find(([event]) => event === "database-switched")?.[1];
      expect(databaseListener).toBeDefined();
      databaseListener?.({ event: "database-switched", id: 0, payload: undefined });

      expect(useClipboardStore.getState().timelineSnapshot).toBeNull();
      expect(useClipboardStore.getState().timelineHighlightId).toBeNull();
      cleanup();
    });

    it("keeps timeline ordering when a capture update arrives", async () => {
      const timelineItems = [{ id: 42, group_id: 7 }] as ClipboardItem[];
      useClipboardStore.setState({
        items: timelineItems,
        timelineSnapshot: {
          searchQuery: "invoice",
          selectedGroup: null,
          selectedGroupId: 7,
          activeItemId: 42,
          scrollTop: 0,
        },
      });
      const { invoke } = await import("@tauri-apps/api/core");
      vi.mocked(invoke).mockResolvedValueOnce(timelineItems);

      await useClipboardStore.getState().applyCaptureUpdate(99);

      expect(invoke).toHaveBeenCalledWith(
        "get_clipboard_items",
        expect.objectContaining({ groupId: null, timeline: true }),
      );
      expect(invoke).not.toHaveBeenCalledWith("get_clipboard_item", { id: 99 });
    });

    it("does not revive a pending timeline entry after reset", async () => {
      const pendingTimeline = deferred<ClipboardItem[]>();
      const resetItems = [{ id: 7, group_id: null }] as ClipboardItem[];
      useClipboardStore.setState({
        items: [{ id: 42, group_id: null }] as ClipboardItem[],
        searchQuery: "invoice",
      });
      const { invoke } = await import("@tauri-apps/api/core");
      vi.mocked(invoke)
        .mockImplementationOnce(() => pendingTimeline.promise)
        .mockResolvedValueOnce(resetItems)
        .mockResolvedValueOnce([]);

      const enterPromise = useClipboardStore.getState().enterTimeline(42, 320);
      await useClipboardStore.getState().resetView();
      pendingTimeline.resolve([{ id: 42, group_id: null }] as ClipboardItem[]);

      expect(await enterPromise).toBe(false);
      expect(useClipboardStore.getState()).toMatchObject({
        items: resetItems,
        searchQuery: "",
        selectedGroup: null,
        timelineSnapshot: null,
        timelineHighlightId: null,
        timelineRestoreRequest: null,
      });
      expect(invoke).toHaveBeenCalledTimes(2);
    });

    it("does not restore stale scroll state after a database switch", async () => {
      const pendingRestore = deferred<ClipboardItem[]>();
      const newDatabaseItems = [{ id: 99, group_id: null }] as ClipboardItem[];
      useClipboardStore.setState({
        items: [{ id: 42, group_id: 7 }] as ClipboardItem[],
        timelineSnapshot: {
          searchQuery: "invoice",
          selectedGroup: "text",
          selectedGroupId: 7,
          activeItemId: 42,
          scrollTop: 640,
        },
      });
      const { invoke } = await import("@tauri-apps/api/core");
      vi.mocked(invoke)
        .mockImplementationOnce(() => pendingRestore.promise)
        .mockResolvedValueOnce(newDatabaseItems);
      const cleanup = await useClipboardStore.getState().setupListener();

      const leavePromise = useClipboardStore.getState().leaveTimeline();
      const { listen } = await import("@tauri-apps/api/event");
      const databaseListener = vi
        .mocked(listen)
        .mock.calls.find(([event]) => event === "database-switched")?.[1];
      databaseListener?.({ event: "database-switched", id: 0, payload: undefined });
      await vi.waitFor(() => {
        expect(useClipboardStore.getState().items).toEqual(newDatabaseItems);
      });
      pendingRestore.resolve([{ id: 42, group_id: 7 }] as ClipboardItem[]);
      await leavePromise;

      expect(useClipboardStore.getState().items).toEqual(newDatabaseItems);
      expect(useClipboardStore.getState().timelineSnapshot).toBeNull();
      expect(useClipboardStore.getState().timelineRestoreRequest).toBeNull();
      cleanup();
    });

    it("blocks cleanup while timeline navigation is active", async () => {
      useClipboardStore.setState({
        selectedGroupId: null,
        timelineSnapshot: {
          searchQuery: "invoice",
          selectedGroup: "text",
          selectedGroupId: 7,
          activeItemId: 42,
          scrollTop: 0,
        },
      });
      const { invoke } = await import("@tauri-apps/api/core");

      expect(await useClipboardStore.getState().clearHistory(null)).toBeNull();
      expect(invoke).not.toHaveBeenCalled();
    });

    it("keeps timeline state when leaving fails", async () => {
      const snapshot = {
        searchQuery: "invoice",
        selectedGroup: "text",
        selectedGroupId: 7,
        activeItemId: 42,
        scrollTop: 640,
      };
      const timelineItems = [{ id: 42, group_id: 7 }] as ClipboardItem[];
      useClipboardStore.setState({
        items: timelineItems,
        searchQuery: "",
        selectedGroup: null,
        selectedGroupId: null,
        timelineSnapshot: snapshot,
      });
      const { invoke } = await import("@tauri-apps/api/core");
      vi.mocked(invoke).mockRejectedValueOnce(new Error("restore failed"));

      await useClipboardStore.getState().leaveTimeline();

      expect(useClipboardStore.getState()).toMatchObject({
        items: timelineItems,
        searchQuery: "",
        selectedGroup: null,
        selectedGroupId: null,
        timelineSnapshot: snapshot,
        timelineRestoreRequest: null,
      });
      expect(useClipboardStore.getState().timelineError).toBeTruthy();
    });

    it("keeps timeline state when target fallback restoration fails", async () => {
      const timelineItems = [{ id: 41, group_id: 7 }] as ClipboardItem[];
      useClipboardStore.setState({
        items: [{ id: 42, group_id: 7 }] as ClipboardItem[],
        searchQuery: "invoice",
        selectedGroup: "text",
        selectedGroupId: 7,
      });
      const { invoke } = await import("@tauri-apps/api/core");
      vi.mocked(invoke)
        .mockResolvedValueOnce(timelineItems)
        .mockRejectedValueOnce(new Error("fallback failed"));

      expect(await useClipboardStore.getState().enterTimeline(42, 320)).toBe(false);

      expect(useClipboardStore.getState()).toMatchObject({
        items: timelineItems,
        searchQuery: "",
        selectedGroup: null,
        selectedGroupId: null,
        timelineRestoreRequest: null,
      });
      expect(useClipboardStore.getState().timelineSnapshot).not.toBeNull();
      expect(useClipboardStore.getState().timelineError).toBeTruthy();
    });

    it("blocks category switching while timeline navigation is active", async () => {
      useClipboardStore.setState({
        selectedGroup: null,
        selectedGroupId: null,
        timelineSnapshot: {
          searchQuery: "invoice",
          selectedGroup: "text",
          selectedGroupId: 7,
          activeItemId: 42,
          scrollTop: 0,
        },
      });
      const { invoke } = await import("@tauri-apps/api/core");

      useClipboardStore.getState().setSelectedGroup("image");
      useClipboardStore.getState().setSelectedGroupId(9);

      expect(useClipboardStore.getState().selectedGroup).toBeNull();
      expect(useClipboardStore.getState().selectedGroupId).toBeNull();
      expect(invoke).not.toHaveBeenCalled();
    });

    it("keeps the pure timeline query when an empty timeline refreshes again", async () => {
      useClipboardStore.setState({
        items: [],
        timelineSnapshot: {
          searchQuery: "invoice",
          selectedGroup: "text",
          selectedGroupId: 7,
          activeItemId: 42,
          scrollTop: 0,
        },
      });
      const { invoke } = await import("@tauri-apps/api/core");
      vi.mocked(invoke).mockResolvedValue([]);

      await useClipboardStore.getState().refresh();
      await useClipboardStore.getState().refresh();

      expect(vi.mocked(invoke).mock.calls).toEqual([
        ["get_clipboard_items", expect.objectContaining({ groupId: null, timeline: true })],
        ["get_clipboard_items", expect.objectContaining({ groupId: null, timeline: true })],
      ]);
    });

    it("clears database-scoped selections before loading the switched database", async () => {
      const newItems = [{ id: 1, group_id: null }] as ClipboardItem[];
      useClipboardStore.setState({
        items: [{ id: 1, group_id: 7 }] as ClipboardItem[],
        searchQuery: "invoice",
        selectedGroup: "text",
        selectedGroupId: 7,
        activeIndex: 3,
        batchMode: true,
        selectedIds: new Set([1]),
        lastSelectedIndex: 0,
      });
      const { invoke } = await import("@tauri-apps/api/core");
      vi.mocked(invoke).mockImplementation((command) => {
        if (command === "get_clipboard_items") return Promise.resolve(newItems);
        if (command === "get_active_database_stats") return Promise.resolve(databaseStats);
        return Promise.resolve(undefined);
      });
      const cleanup = await useClipboardStore.getState().setupListener();
      const { listen } = await import("@tauri-apps/api/event");
      const databaseListener = vi
        .mocked(listen)
        .mock.calls.find(([event]) => event === "database-switched")?.[1];

      databaseListener?.({ event: "database-switched", id: 0, payload: undefined });

      expect(useClipboardStore.getState()).toMatchObject({
        items: [],
        searchQuery: "",
        selectedGroup: null,
        selectedGroupId: null,
        activeIndex: -1,
        batchMode: false,
        lastSelectedIndex: -1,
      });
      expect(useClipboardStore.getState().selectedIds.size).toBe(0);
      await vi.waitFor(() => expect(useClipboardStore.getState().items).toEqual(newItems));
      expect(invoke).toHaveBeenCalledWith(
        "get_clipboard_items",
        expect.objectContaining({ search: null, contentType: null, groupId: null }),
      );

      vi.mocked(invoke).mockClear();
      await useClipboardStore.getState().batchDelete();
      expect(invoke).not.toHaveBeenCalledWith("batch_delete_clipboard_items", expect.anything());
      cleanup();
    });

    it("cancels a delayed capture update when the database switches", async () => {
      vi.useFakeTimers();
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        vi.mocked(invoke).mockImplementation((command) =>
          Promise.resolve(command === "get_active_database_stats" ? databaseStats : []),
        );
        const cleanup = await useClipboardStore.getState().setupListener();
        const { listen } = await import("@tauri-apps/api/event");
        const clipboardListener = vi
          .mocked(listen)
          .mock.calls.find(([event]) => event === "clipboard-updated")?.[1];
        const databaseListener = vi
          .mocked(listen)
          .mock.calls.find(([event]) => event === "database-switched")?.[1];

        clipboardListener?.({ event: "clipboard-updated", id: 0, payload: 42 });
        databaseListener?.({ event: "database-switched", id: 0, payload: undefined });
        await vi.runAllTimersAsync();

        expect(invoke).not.toHaveBeenCalledWith("get_clipboard_item", { id: 42 });
        cleanup();
      } finally {
        vi.useRealTimers();
      }
    });
  });

  describe("deleteItem", () => {
    it("removes item from list after invoke", async () => {
      const mockItems: ClipboardItem[] = [
        { id: 1, content_type: "text", text_content: "a", preview: "a" } as ClipboardItem,
        { id: 2, content_type: "text", text_content: "b", preview: "b" } as ClipboardItem,
        { id: 3, content_type: "text", text_content: "c", preview: "c" } as ClipboardItem,
      ];
      useClipboardStore.setState({ items: mockItems });

      const { invoke } = await import("@tauri-apps/api/core");
      vi.mocked(invoke).mockResolvedValueOnce(undefined);

      await useClipboardStore.getState().deleteItem(2);

      const items = useClipboardStore.getState().items;
      expect(items).toHaveLength(2);
      expect(items.find((i) => i.id === 2)).toBeUndefined();
    });

    it("does not invoke backend for a locked item", async () => {
      useClipboardStore.setState({
        items: [{ id: 1, is_locked: true } as ClipboardItem],
      });
      const { invoke } = await import("@tauri-apps/api/core");

      await useClipboardStore.getState().deleteItem(1);

      expect(invoke).not.toHaveBeenCalled();
      expect(useClipboardStore.getState().items).toHaveLength(1);
    });
  });

  describe("toggleLock", () => {
    it("invokes backend and updates the target item", async () => {
      const items = [
        { id: 1, is_locked: false },
        { id: 2, is_locked: false },
      ] as ClipboardItem[];
      useClipboardStore.setState({ items });
      const { invoke } = await import("@tauri-apps/api/core");
      vi.mocked(invoke).mockResolvedValueOnce(true);

      await useClipboardStore.getState().toggleLock(2);

      expect(invoke).toHaveBeenCalledWith("toggle_lock", { id: 2 });
      expect(useClipboardStore.getState().items).toEqual([
        { id: 1, is_locked: false },
        { id: 2, is_locked: true },
      ]);
    });
  });

  describe("clearHistory", () => {
    it("returns deleted count from backend and refreshes", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      vi.mocked(invoke)
        .mockResolvedValueOnce(5) // clear_history
        .mockResolvedValueOnce([]); // refresh -> get_clipboard_items

      const deleted = await useClipboardStore.getState().clearHistory(null);

      expect(deleted).toBe(5);
      expect(invoke).toHaveBeenCalledWith("clear_history", {
        groupId: null,
        contentType: null,
      });
    });

    it("returns null on failure", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      vi.mocked(invoke).mockRejectedValueOnce(new Error("db error"));

      const deleted = await useClipboardStore.getState().clearHistory(null);

      expect(deleted).toBeNull();
    });
  });

  describe("batch selection", () => {
    it("toggleSelect adds item to selection", () => {
      const mockItems: ClipboardItem[] = [
        { id: 1 } as ClipboardItem,
        { id: 2 } as ClipboardItem,
        { id: 3 } as ClipboardItem,
      ];
      useClipboardStore.setState({ items: mockItems, batchMode: true });

      useClipboardStore.getState().toggleSelect(2, 1, false);

      const state = useClipboardStore.getState();
      expect(state.selectedIds.has(2)).toBe(true);
      expect(state.lastSelectedIndex).toBe(1);
    });

    it("toggleSelect removes item if already selected", () => {
      useClipboardStore.setState({
        batchMode: true,
        selectedIds: new Set([1, 2]),
      });

      useClipboardStore.getState().toggleSelect(2, 1, false);

      expect(useClipboardStore.getState().selectedIds.has(2)).toBe(false);
    });

    it("shift+click selects range", () => {
      const mockItems: ClipboardItem[] = Array.from({ length: 10 }, (_, i) => ({
        id: i + 1,
      })) as ClipboardItem[];
      useClipboardStore.setState({
        items: mockItems,
        batchMode: true,
        lastSelectedIndex: 2,
      });

      useClipboardStore.getState().toggleSelect(6, 5, true);

      const selected = useClipboardStore.getState().selectedIds;
      // Should select items at indices 2-5 (ids 3-6)
      expect(selected.has(3)).toBe(true);
      expect(selected.has(4)).toBe(true);
      expect(selected.has(5)).toBe(true);
      expect(selected.has(6)).toBe(true);
      expect(selected.has(1)).toBe(false);
      expect(selected.has(7)).toBe(false);
    });

    it("selectAll selects all items", () => {
      const mockItems: ClipboardItem[] = Array.from({ length: 5 }, (_, i) => ({
        id: i + 1,
      })) as ClipboardItem[];
      useClipboardStore.setState({ items: mockItems, batchMode: true });

      useClipboardStore.getState().selectAll();

      expect(useClipboardStore.getState().selectedIds.size).toBe(5);
    });

    it("deselectAll clears selection", () => {
      useClipboardStore.setState({
        batchMode: true,
        selectedIds: new Set([1, 2, 3]),
      });

      useClipboardStore.getState().deselectAll();

      expect(useClipboardStore.getState().selectedIds.size).toBe(0);
    });

    it("setBatchMode resets selection", () => {
      useClipboardStore.setState({
        batchMode: true,
        selectedIds: new Set([1, 2]),
        lastSelectedIndex: 3,
      });

      useClipboardStore.getState().setBatchMode(false);

      const state = useClipboardStore.getState();
      expect(state.batchMode).toBe(false);
      expect(state.selectedIds.size).toBe(0);
      expect(state.lastSelectedIndex).toBe(-1);
    });

    it("batchDelete does not invoke backend when selection contains a locked item", async () => {
      useClipboardStore.setState({
        items: [
          { id: 1, is_locked: false },
          { id: 2, is_locked: true },
        ] as ClipboardItem[],
        batchMode: true,
        selectedIds: new Set([1, 2]),
      });
      const { invoke } = await import("@tauri-apps/api/core");

      await useClipboardStore.getState().batchDelete();

      expect(invoke).not.toHaveBeenCalled();
      expect(useClipboardStore.getState().selectedIds).toEqual(new Set([1, 2]));
    });
  });

  describe("resetView", () => {
    it("resets search and group, increments resetToken", () => {
      useClipboardStore.setState({
        searchQuery: "test",
        selectedGroup: "text",
        batchMode: true,
        selectedIds: new Set([1]),
        lastSelectedIndex: 5,
        _resetToken: 3,
      });

      useClipboardStore.getState().resetView();

      const state = useClipboardStore.getState();
      expect(state.searchQuery).toBe("");
      expect(state.selectedGroup).toBeNull();
      expect(state.batchMode).toBe(false);
      expect(state.selectedIds.size).toBe(0);
      expect(state._resetToken).toBe(4);
    });
  });

  describe("fetchItems deduplication", () => {
    it("discards stale responses using _fetchId", async () => {
      const { invoke } = await import("@tauri-apps/api/core");

      // First call returns slow
      let resolveFirst: (value: ClipboardItem[]) => void;
      const firstPromise = new Promise<ClipboardItem[]>((r) => {
        resolveFirst = r;
      });
      vi.mocked(invoke).mockImplementationOnce(() => firstPromise);

      // Second call returns fast
      const fastItems = [{ id: 99 } as ClipboardItem];
      vi.mocked(invoke).mockResolvedValueOnce(fastItems);

      // Start both fetches
      const fetch1 = useClipboardStore.getState().fetchItems();
      const fetch2 = useClipboardStore.getState().fetchItems();

      // Resolve fast one first
      resolveFirst!([]);
      await Promise.all([fetch1, fetch2]);

      // Should have the fast result (fetch2), not the stale one
      expect(useClipboardStore.getState().items).toEqual(fastItems);
    });
  });
});
