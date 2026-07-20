import { openUrl } from "@tauri-apps/plugin-opener";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { showToast } from "@/components/ui/toast";
import { TooltipProvider } from "@/components/ui/tooltip";
import type { ClipboardItem } from "@/stores/clipboard";
import { ClipboardItemCard } from "./ClipboardItemCard";

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn(() => Promise.resolve()),
}));
vi.mock("@/components/ui/toast", () => ({ showToast: vi.fn() }));

afterEach(() => vi.clearAllMocks());

const sourceItem = {
  id: 1,
  content_type: "text",
  text_content: "项目计划",
  html_content: null,
  rtf_content: null,
  image_path: null,
  file_paths: null,
  content_hash: "hash",
  preview: "项目计划",
  byte_size: 12,
  image_width: null,
  image_height: null,
  is_pinned: false,
  is_favorite: false,
  is_locked: false,
  favorite_order: 0,
  sort_order: 0,
  created_at: "2026-07-20T00:00:00Z",
  updated_at: "2026-07-20T00:00:00Z",
  access_count: 0,
  last_accessed_at: null,
  char_count: 4,
  source_app_name: "Microsoft Edge",
  source_app_icon: null,
  source_title: "项目计划 - Microsoft Edge",
  source_url: "https://example.com/project",
  source_file_name: "项目计划.docx",
  group_id: null,
} as ClipboardItem & {
  source_title: string;
  source_url: string;
  source_file_name: string;
};

describe("ClipboardItemCard source details", () => {
  it("shows source details from the existing footer source control", () => {
    render(
      <TooltipProvider>
        <ClipboardItemCard item={sourceItem} index={0} />
      </TooltipProvider>,
    );

    fireEvent.click(screen.getByText("Microsoft Edge"));

    expect(screen.getByText("项目计划 - Microsoft Edge")).toBeInTheDocument();
    expect(screen.getByText("https://example.com/project")).toBeInTheDocument();
    expect(screen.getByText("项目计划.docx")).toBeInTheDocument();
  });

  it("opens a valid source URL with the existing opener", () => {
    render(
      <TooltipProvider>
        <ClipboardItemCard item={sourceItem} index={0} />
      </TooltipProvider>,
    );

    fireEvent.click(screen.getByText("Microsoft Edge"));
    fireEvent.click(screen.getByRole("button", { name: sourceItem.source_url }));

    expect(openUrl).toHaveBeenCalledWith(sourceItem.source_url);
  });

  it("locates the item from the source details dialog", async () => {
    const onLocateInTimeline = vi.fn();
    render(
      <TooltipProvider>
        <ClipboardItemCard
          item={sourceItem}
          index={0}
          onLocateInTimeline={onLocateInTimeline}
        />
      </TooltipProvider>,
    );

    fireEvent.click(screen.getByText("Microsoft Edge"));
    fireEvent.click(screen.getByRole("button", { name: "在时间线中定位" }));

    expect(onLocateInTimeline).toHaveBeenCalledWith(sourceItem.id);
    await waitFor(() => {
      expect(screen.queryByRole("dialog", { name: "来源详情" })).not.toBeInTheDocument();
    });
  });

  it("does not make an invalid source URL actionable", () => {
    render(
      <TooltipProvider>
        <ClipboardItemCard
          item={{ ...sourceItem, source_url: "javascript:alert(1)" }}
          index={0}
        />
      </TooltipProvider>,
    );

    fireEvent.click(screen.getByText("Microsoft Edge"));

    expect(screen.queryByRole("button", { name: "javascript:alert(1)" })).not.toBeInTheDocument();
    expect(openUrl).not.toHaveBeenCalled();
  });

  it("shows a toast when opening the source URL fails", async () => {
    vi.mocked(openUrl).mockRejectedValueOnce(new Error("opener failed"));
    render(
      <TooltipProvider>
        <ClipboardItemCard item={sourceItem} index={0} />
      </TooltipProvider>,
    );

    fireEvent.click(screen.getByText("Microsoft Edge"));
    fireEvent.click(screen.getByRole("button", { name: sourceItem.source_url }));

    await waitFor(() => {
      expect(showToast).toHaveBeenCalledWith("打开来源网址失败", "error");
    });
  });
});
