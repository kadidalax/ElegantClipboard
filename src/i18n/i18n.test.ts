import { describe, it, expect, beforeEach } from "vitest";
import { t, useLocaleStore } from "@/i18n";

describe("i18n", () => {
  beforeEach(() => {
    useLocaleStore.setState({ locale: "zh-CN", loaded: true });
  });

  it("defaults to Simplified Chinese", () => {
    expect(t("groups.all")).toBe("全部");
  });

  it("switches to English", () => {
    useLocaleStore.setState({ locale: "en" });
    expect(t("groups.all")).toBe("All");
    expect(t("settings.title")).toBe("Settings");
  });

  it("switches to Traditional Chinese", () => {
    useLocaleStore.setState({ locale: "zh-TW" });
    expect(t("groups.text")).toBe("文字");
    expect(t("groups.other")).toBe("其他");
  });

  it("interpolates params", () => {
    expect(t("app.batchSelected", { count: 3 })).toBe("已选择 3 项");
  });

  it.each([
    [
      "zh-CN",
      [
        "确定要清理当前分组内未受保护的历史记录吗？置顶、收藏和锁定条目将保留。此操作不可撤销。",
        "确定要清理当前分组内未受保护的文本历史记录吗？置顶、收藏和锁定条目将保留。此操作不可撤销。",
        "确定要清理当前分组内未受保护的其它历史记录吗？置顶、收藏和锁定条目将保留。此操作不可撤销。",
        "清理当前分组内未受保护的历史记录；置顶、收藏和锁定条目将保留",
        "此操作将删除包括置顶、收藏和锁定条目在内的所有剪贴板记录，且不可恢复。",
      ],
    ],
    [
      "zh-TW",
      [
        "確定要清理目前分組內未受保護的歷史記錄嗎？置頂、收藏和鎖定條目將保留。此操作不可撤銷。",
        "確定要清理目前分組內未受保護的文字歷史記錄嗎？置頂、收藏和鎖定條目將保留。此操作不可撤銷。",
        "確定要清理目前分組內未受保護的其他歷史記錄嗎？置頂、收藏和鎖定條目將保留。此操作不可撤銷。",
        "清理目前分組內未受保護的歷史記錄；置頂、收藏和鎖定條目將保留",
        "此操作將刪除包括置頂、收藏和鎖定條目在內的所有剪貼簿記錄，且不可恢復。",
      ],
    ],
    [
      "en",
      [
        "Clear unprotected history in the current group? Pinned, favorite, and locked items will be kept. This cannot be undone.",
        "Clear unprotected text history in the current group? Pinned, favorite, and locked items will be kept. This cannot be undone.",
        "Clear unprotected other history in the current group? Pinned, favorite, and locked items will be kept. This cannot be undone.",
        "Clear unprotected history in the current group; pinned, favorite, and locked items will be kept",
        "This deletes all clipboard records, including pinned, favorite, and locked items. This cannot be undone.",
      ],
    ],
  ] as const)("describes protected items accurately in %s", (locale, messages) => {
    useLocaleStore.setState({ locale });

    expect(t("app.clearHistoryConfirmAll")).toBe(messages[0]);
    expect(t("app.clearHistoryConfirmText")).toBe(messages[1]);
    expect(t("app.clearHistoryConfirmOther")).toBe(messages[2]);
    expect(t("toolbar.clearHistoryDesc")).toBe(messages[3]);
    expect(t("settings.data.clearHistoryDialogWarning")).toBe(messages[4]);
  });
});
