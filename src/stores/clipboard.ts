import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import debounce from "lodash.debounce";
import { create } from "zustand";
import { cancelPendingFocusRestore } from "@/hooks/useInputFocus";
import { logError } from "@/lib/logger";
import { playCopySound, setupPasteSoundListeners } from "@/lib/sounds";
import { mergeCaptureItem, matchesListFilter } from "@/stores/clipboard-merge";
import { useUISettings } from "@/stores/ui-settings";

function batchResetState() {
  return { batchMode: false, selectedIds: new Set<number>(), lastSelectedIndex: -1 };
}

export interface ClipboardItem {
  id: number;
  content_type: "text" | "image" | "html" | "rtf" | "files" | "url";
  text_content: string | null;
  html_content: string | null;
  rtf_content: string | null;
  image_path: string | null;
  file_paths: string | null;
  content_hash: string;
  preview: string | null;
  byte_size: number;
  image_width: number | null;
  image_height: number | null;
  is_pinned: boolean;
  is_favorite: boolean;
  is_locked: boolean;
  favorite_order: number;
  sort_order: number;
  created_at: string;
  updated_at: string;
  access_count: number;
  last_accessed_at: string | null;
  char_count: number | null;
  source_app_name: string | null;
  source_app_icon: string | null;
  source_title: string | null;
  source_url: string | null;
  source_file_name: string | null;
  /** 所属自定义分组 ID（null = 默认分组） */
  group_id: number | null;
  /** 所有文件是否存在（仅 files 类型，查询时计算） */
  files_valid?: boolean;
}

export function canDeleteClipboardItem(item?: Pick<ClipboardItem, "is_locked">): boolean {
  return !!item && !item.is_locked;
}

export function hasLockedSelection(items: ClipboardItem[], selectedIds: Set<number>): boolean {
  let matched = 0;
  for (const item of items) {
    if (!selectedIds.has(item.id)) continue;
    matched++;
    if (item.is_locked) return true;
  }
  return matched !== selectedIds.size;
}

export interface TimelineSnapshot {
  searchQuery: string;
  selectedGroup: string | null;
  selectedGroupId: number | null;
  activeItemId: number | null;
  scrollTop: number;
}

export interface TimelineRestoreRequest {
  activeItemId: number | null;
  scrollTop: number;
}

export interface ActiveDatabaseStats {
  id: string;
  name: string;
  item_count: number;
  db_size: number;
}

interface ClipboardState {
  items: ClipboardItem[];
  isLoading: boolean;
  searchQuery: string;
  selectedGroup: string | null;
  /** 当前选中的自定义分组 id（与 selectedGroup 互斥） */
  selectedGroupId: number | null;
  /** 当前键盘高亮索引（-1 表示无） */
  activeIndex: number;
  /** 单调计数器，丢弃过期请求 */
  _fetchId: number;
  /** 视图重置计数（滚动到顶部等） */
  _resetToken: number;
  /** 使已离开的时间线异步请求失效 */
  _timelineGeneration: number;
  timelineSnapshot: TimelineSnapshot | null;
  timelineHighlightId: number | null;
  timelineRestoreRequest: TimelineRestoreRequest | null;
  timelineError: string | null;
  activeDatabaseStats: ActiveDatabaseStats | null;
  databaseStatsLoading: boolean;
  databaseStatsError: string | null;
  _databaseStatsFetchId: number;

  // 操作
  fetchItems: (options?: {
    search?: string | null;
    content_type?: string | null;
    groupId?: number | null;
    timeline?: boolean;
    commit?: boolean;
    limit?: number;
    offset?: number;
  }) => Promise<ClipboardItem[] | null>;
  setSearchQuery: (query: string) => void;
  setSelectedGroup: (group: string | null) => void;
  setSelectedGroupId: (groupId: number | null) => void;
  setActiveIndex: (index: number) => void;
  togglePin: (id: number) => Promise<void>;
  toggleFavorite: (id: number) => Promise<void>;
  toggleLock: (id: number) => Promise<void>;
  moveItem: (fromId: number, toId: number) => Promise<void>;
  moveFavoriteItem: (fromId: number, toId: number) => Promise<void>;
  deleteItem: (id: number) => Promise<void>;
  copyToClipboard: (id: number) => Promise<void>;
  pasteContent: (id: number) => Promise<void>;
  pasteAsPlainText: (id: number) => Promise<void>;
  /** 清空当前分组历史，返回删除条数；失败返回 null */
  clearHistory: (contentType?: string | null) => Promise<number | null>;
  refresh: () => Promise<void>;
  /** 剪贴板捕获后增量更新列表（单条 IPC） */
  applyCaptureUpdate: (id: number) => Promise<void>;
  /** 重置视图：清除搜索、类型筛选，滚动到顶部，刷新 */
  resetView: () => Promise<void>;
  enterTimeline: (targetId: number, scrollTop: number) => Promise<boolean>;
  leaveTimeline: () => Promise<void>;
  clearTimelineHighlight: () => void;
  consumeTimelineRestoreRequest: () => void;
  refreshActiveDatabaseStats: () => Promise<void>;
  setupListener: () => Promise<() => void>;

  // 批量选择
  batchMode: boolean;
  selectedIds: Set<number>;
  lastSelectedIndex: number;
  setBatchMode: (enabled: boolean) => void;
  toggleSelect: (id: number, index: number, shiftKey: boolean) => void;
  selectAll: () => void;
  deselectAll: () => void;
  batchDelete: () => Promise<void>;
}

async function doPaste(
  get: () => ClipboardState,
  id: number,
  command: "paste_content" | "paste_content_as_plain",
) {
  try {
    cancelPendingFocusRestore();
    const { pasteCloseWindow, pasteMoveToTop } = useUISettings.getState();
    await invoke(command, { id, closeWindow: pasteCloseWindow });
    if (pasteMoveToTop) {
      invoke("bump_item_to_top", { id }).then(() => get().refresh()).catch((e) => logError("Failed to bump item to top:", e));
    }
  } catch (error) {
    logError(`Failed to ${command}:`, error);
  }
}

export const useClipboardStore = create<ClipboardState>((set, get) => ({
  items: [],
  isLoading: false,
  searchQuery: "",
  selectedGroup: null,
  selectedGroupId: null,
  activeIndex: -1,
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

  fetchItems: async (options = {}) => {
    const state = get();
    const fetchId = state._fetchId + 1;
    set({ isLoading: true, _fetchId: fetchId });
    try {
      const group = "content_type" in options
        ? options.content_type ?? null
        : state.selectedGroup;
      const isFavoritesView = group === "__favorites__";
      const items = await invoke<ClipboardItem[]>("get_clipboard_items", {
        search: "search" in options
          ? options.search || null
          : state.searchQuery || null,
        contentType: isFavoritesView ? null : group,
        pinnedOnly: false,
        favoriteOnly: isFavoritesView,
        groupId: "groupId" in options
          ? options.groupId ?? null
          : state.selectedGroupId,
        timeline: options.timeline ?? false,
        limit: options.limit,
        offset: options.offset ?? 0,
      });
      if (get()._fetchId === fetchId) {
        set(options.commit === false
          ? { isLoading: false }
          : { items, isLoading: false, activeIndex: -1 });
        return items;
      }
      return null;
    } catch (error) {
      if (get()._fetchId === fetchId) {
        logError("Failed to fetch items:", error);
        set({ isLoading: false });
      }
      return null;
    }
  },

  setSearchQuery: (query: string) => {
    set({ searchQuery: query });
    // 仅更新查询状态，防抖在 App.tsx 中处理
  },

  setSelectedGroup: (group: string | null) => {
    if (get().timelineSnapshot) return;
    set({ selectedGroup: group, ...batchResetState() });
    get().fetchItems();
  },

  setSelectedGroupId: (groupId: number | null) => {
    if (get().timelineSnapshot) return;
    set({ selectedGroupId: groupId, ...batchResetState() });
    invoke("set_active_group", { groupId }).catch((error) => {
      logError("Failed to persist active group:", error);
    });
    get().fetchItems();
  },

  setActiveIndex: (index: number) => {
    set({ activeIndex: index });
  },

  togglePin: async (id: number) => {
    try {
      await invoke<boolean>("toggle_pin", { id });
      // 刷新以获取正确排序（置顶优先）
      await get().refresh();
    } catch (error) {
      logError("Failed to toggle pin:", error);
    }
  },

  toggleFavorite: async (id: number) => {
    try {
      const newState = await invoke<boolean>("toggle_favorite", { id });
      // 在收藏视图中取消收藏时，需要刷新列表以移除该条目
      if (!newState && get().selectedGroup === "__favorites__") {
        await get().refresh();
      } else {
        set((state) => ({
          items: state.items.map((item) =>
            item.id === id ? { ...item, is_favorite: newState } : item
          ),
        }));
      }
    } catch (error) {
      logError("Failed to toggle favorite:", error);
    }
  },

  toggleLock: async (id: number) => {
    try {
      const newState = await invoke<boolean>("toggle_lock", { id });
      set((state) => ({
        items: state.items.map((item) =>
          item.id === id ? { ...item, is_locked: newState } : item
        ),
      }));
    } catch (error) {
      logError("Failed to toggle lock:", error);
    }
  },

  moveItem: async (fromId: number, toId: number) => {
    try {
      await invoke("move_clipboard_item", { fromId, toId });
      // 刷新以获取更新后的顺序
      await get().refresh();
    } catch (error) {
      logError("Failed to move item:", error);
    }
  },

  moveFavoriteItem: async (fromId: number, toId: number) => {
    try {
      await invoke("move_favorite_clipboard_item", { fromId, toId });
      await get().refresh();
    } catch (error) {
      logError("Failed to move favorite item:", error);
    }
  },

  deleteItem: async (id: number) => {
    if (!canDeleteClipboardItem(get().items.find((item) => item.id === id))) return;
    try {
      await invoke("delete_clipboard_item", { id });
      set((state) => ({
        items: state.items.filter((item) => item.id !== id),
      }));
      await get().refreshActiveDatabaseStats();
    } catch (error) {
      logError("Failed to delete item:", error);
    }
  },

  copyToClipboard: async (id: number) => {
    try {
      await invoke("copy_to_clipboard", { id });
    } catch (error) {
      logError("Failed to copy to clipboard:", error);
    }
  },

  pasteContent: async (id: number) => {
    await doPaste(get, id, "paste_content");
  },

  pasteAsPlainText: async (id: number) => {
    await doPaste(get, id, "paste_content_as_plain");
  },

  // contentType=null 时后端 Option<String> 为 None，清除所有类型（正确行为）
  clearHistory: async (contentType = null) => {
    if (get().timelineSnapshot) return null;
    try {
      const deleted = await invoke<number>("clear_history", {
        groupId: get().selectedGroupId,
        contentType,
      });
      await get().refresh();
      await get().refreshActiveDatabaseStats();
      return deleted;
    } catch (error) {
      logError("Failed to clear history:", error);
      return null;
    }
  },

  refresh: async () => {
    const state = get();
    if (state.timelineSnapshot) {
      await get().fetchItems({
        search: null,
        content_type: null,
        groupId: null,
        timeline: true,
      });
      return;
    }
    await state.fetchItems();
  },

  applyCaptureUpdate: async (id: number) => {
    const state = get();
    if (state.timelineSnapshot) {
      await get().refresh();
      return;
    }
    if (state.searchQuery) {
      await get().fetchItems();
      return;
    }

    try {
      const item = await invoke<ClipboardItem | null>("get_clipboard_item", { id });
      if (!item) {
        return;
      }
      if (!matchesListFilter(item, state.selectedGroup, state.selectedGroupId)) {
        return;
      }
      set((s) => ({
        items: mergeCaptureItem(s.items, item),
        activeIndex: -1,
      }));
    } catch (error) {
      logError("Failed to apply capture update:", error);
      await get().fetchItems();
    }
  },

  resetView: async () => {
    // 仅重置搜索和类型筛选，保留分组选择
    set((state) => ({
      searchQuery: "",
      selectedGroup: null,
      timelineSnapshot: null,
      timelineHighlightId: null,
      timelineRestoreRequest: null,
      timelineError: null,
      ...batchResetState(),
      _resetToken: state._resetToken + 1,
      _timelineGeneration: state._timelineGeneration + 1,
    }));
    await get().fetchItems({ search: "" });
  },

  enterTimeline: async (targetId, scrollTop) => {
    const state = get();
    if (state.timelineSnapshot) {
      const targetIndex = state.items.findIndex((item) => item.id === targetId);
      if (targetIndex < 0) return false;
      set({ activeIndex: targetIndex, timelineHighlightId: targetId });
      return true;
    }

    const target = state.items.find((item) => item.id === targetId);
    if (!target) return false;
    const generation = state._timelineGeneration + 1;
    const snapshot: TimelineSnapshot = {
      searchQuery: state.searchQuery,
      selectedGroup: state.selectedGroup,
      selectedGroupId: state.selectedGroupId,
      activeItemId: state.items[state.activeIndex]?.id ?? null,
      scrollTop,
    };
    set({
      searchQuery: "",
      selectedGroup: null,
      selectedGroupId: null,
      timelineSnapshot: snapshot,
      timelineHighlightId: null,
      timelineRestoreRequest: null,
      timelineError: null,
      _timelineGeneration: generation,
      ...batchResetState(),
    });

    const timelineItems = await get().fetchItems({
      search: null,
      content_type: null,
      groupId: null,
      timeline: true,
    });
    if (get()._timelineGeneration !== generation) return false;
    const targetIndex = timelineItems?.findIndex((item) => item.id === targetId) ?? -1;
    if (targetIndex >= 0) {
      set({ activeIndex: targetIndex, timelineHighlightId: targetId });
      return true;
    }

    const restoredItems = await get().fetchItems({
      search: snapshot.searchQuery,
      content_type: snapshot.selectedGroup,
      groupId: snapshot.selectedGroupId,
      commit: false,
    });
    if (get()._timelineGeneration !== generation) return false;
    if (!restoredItems) {
      set({ timelineError: "restore_failed" });
      return false;
    }
    const activeIndex = snapshot.activeItemId == null
      ? -1
      : restoredItems.findIndex((item) => item.id === snapshot.activeItemId);
    set({
      items: restoredItems,
      searchQuery: snapshot.searchQuery,
      selectedGroup: snapshot.selectedGroup,
      selectedGroupId: snapshot.selectedGroupId,
      activeIndex,
      timelineSnapshot: null,
      timelineHighlightId: null,
      timelineError: null,
      timelineRestoreRequest: {
        activeItemId: snapshot.activeItemId,
        scrollTop: snapshot.scrollTop,
      },
    });
    return false;
  },

  leaveTimeline: async () => {
    const snapshot = get().timelineSnapshot;
    if (!snapshot) return;
    const generation = get()._timelineGeneration + 1;
    set({
      _timelineGeneration: generation,
      timelineError: null,
      ...batchResetState(),
    });
    const restoredItems = await get().fetchItems({
      search: snapshot.searchQuery,
      content_type: snapshot.selectedGroup,
      groupId: snapshot.selectedGroupId,
      commit: false,
    });
    if (get()._timelineGeneration !== generation) return;
    if (!restoredItems) {
      set({ timelineError: "restore_failed" });
      return;
    }
    const activeIndex = snapshot.activeItemId == null
      ? -1
      : restoredItems.findIndex((item) => item.id === snapshot.activeItemId);
    set({
      items: restoredItems,
      searchQuery: snapshot.searchQuery,
      selectedGroup: snapshot.selectedGroup,
      selectedGroupId: snapshot.selectedGroupId,
      activeIndex,
      timelineSnapshot: null,
      timelineHighlightId: null,
      timelineError: null,
      timelineRestoreRequest: {
        activeItemId: snapshot.activeItemId,
        scrollTop: snapshot.scrollTop,
      },
    });
  },

  clearTimelineHighlight: () => set({ timelineHighlightId: null }),
  consumeTimelineRestoreRequest: () => set({ timelineRestoreRequest: null }),

  refreshActiveDatabaseStats: async () => {
    const requestId = get()._databaseStatsFetchId + 1;
    set({
      databaseStatsLoading: true,
      databaseStatsError: null,
      _databaseStatsFetchId: requestId,
    });
    try {
      const stats = await invoke<ActiveDatabaseStats>("get_active_database_stats");
      if (get()._databaseStatsFetchId === requestId) {
        set({
          activeDatabaseStats: stats,
          databaseStatsLoading: false,
          databaseStatsError: null,
        });
      }
    } catch (error) {
      if (get()._databaseStatsFetchId === requestId) {
        set({
          activeDatabaseStats: null,
          databaseStatsLoading: false,
          databaseStatsError: String(error),
        });
      }
      logError("Failed to refresh active database stats:", error);
    }
  },

  setupListener: async () => {
    const unlistenPasteSound = await setupPasteSoundListeners();

    // 防抖合并快速连续的剪贴板变化事件，避免 IPC 风暴
    const debouncedCaptureUpdate = debounce(async (id: number) => {
      await get().applyCaptureUpdate(id);
      playCopySound("after_success");
    }, 50, { leading: false, trailing: true });

    const unlisten = await listen<number>("clipboard-updated", (event) => {
      const id = event.payload;
      if (typeof id !== "number" || !Number.isFinite(id)) {
        return;
      }
      playCopySound("immediate");
      void debouncedCaptureUpdate(id);
    });
    const unlistenDatabaseSwitched = await listen("database-switched", () => {
      debouncedCaptureUpdate.cancel();
      set((state) => ({
        items: [],
        searchQuery: "",
        selectedGroup: null,
        selectedGroupId: null,
        activeIndex: -1,
        ...batchResetState(),
        timelineSnapshot: null,
        timelineHighlightId: null,
        timelineRestoreRequest: null,
        timelineError: null,
        _timelineGeneration: state._timelineGeneration + 1,
        activeDatabaseStats: null,
        databaseStatsLoading: false,
        databaseStatsError: null,
        _databaseStatsFetchId: state._databaseStatsFetchId + 1,
      }));
      void get().fetchItems({ search: "", content_type: null, groupId: null });
      void get().refreshActiveDatabaseStats();
    });
    return () => {
      unlistenPasteSound();
      unlisten();
      unlistenDatabaseSwitched();
    };
  },

  // 批量选择
  batchMode: false,
  selectedIds: new Set<number>(),
  lastSelectedIndex: -1,

  setBatchMode: (enabled) => {
    set({ ...batchResetState(), batchMode: enabled });
  },

  toggleSelect: (id, index, shiftKey) => {
    const { selectedIds, lastSelectedIndex, items } = get();
    const next = new Set(selectedIds);

    if (shiftKey && lastSelectedIndex >= 0) {
      const from = Math.min(lastSelectedIndex, index);
      const to = Math.max(lastSelectedIndex, index);
      for (let i = from; i <= to; i++) {
        if (items[i]) next.add(items[i].id);
      }
    } else {
      if (next.has(id)) next.delete(id);
      else next.add(id);
    }
    set({ selectedIds: next, lastSelectedIndex: index });
  },

  selectAll: () => {
    const ids = new Set(get().items.map((item) => item.id));
    set({ selectedIds: ids });
  },

  deselectAll: () => {
    set({ selectedIds: new Set() });
  },

  batchDelete: async () => {
    const { items, selectedIds } = get();
    if (selectedIds.size === 0 || hasLockedSelection(items, selectedIds)) return;
    try {
      await invoke("batch_delete_clipboard_items", { ids: Array.from(selectedIds) });
      set({ ...batchResetState() });
      await get().refresh();
      await get().refreshActiveDatabaseStats();
    } catch (error) {
      logError("Failed to batch delete:", error);
    }
  },
}));
