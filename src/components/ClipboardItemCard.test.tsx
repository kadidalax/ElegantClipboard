import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { showToast } from "@/components/ui/toast";
import { TooltipProvider } from "@/components/ui/tooltip";
import type { ClipboardItem } from "@/stores/clipboard";
import { ClipboardItemCard } from "./ClipboardItemCard";

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn(() => Promise.resolve()),
}));
vi.mock("@/components/ui/toast", () => ({ showToast: vi.fn() }));
vi.mock("@/components/CardContentRenderers", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/components/CardContentRenderers")>();
  return {
    ...actual,
    getPreviewBounds: vi.fn(() => Promise.resolve({
      maxW: 600,
      maxH: 400,
      anchorX: 100,
      cardCenterY: 200,
      monY: 0,
      monBottom: 400,
      scale: 1,
      side: "right",
    })),
  };
});

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.clearAllMocks();
});

afterEach(() => {
  vi.useRealTimers();
});

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

const filePaths = [
  "C:\\Users\\Administrator\\Desktop\\report.txt",
  "D:\\Documents\\notes.md",
];
const stagedFilePaths = [
  "C:\\Z_Software\\ElegantClipboard\\staged\\report.txt",
  "C:\\Z_Software\\ElegantClipboard\\staged\\notes.md",
];
const fileItem = {
  ...sourceItem,
  id: 2,
  content_type: "files",
  text_content: null,
  preview: "2 files",
  file_paths: JSON.stringify(filePaths),
  source_app_name: "Windows 资源管理器",
  source_title: null,
  source_url: null,
  source_file_name: null,
  files_valid: undefined,
} as ClipboardItem;

const imageFilePath = "C:\\Users\\Administrator\\Desktop\\photo.png";
const imageFileItem = {
  ...fileItem,
  id: 3,
  preview: imageFilePath,
  file_paths: JSON.stringify([imageFilePath]),
} as ClipboardItem;

describe("ClipboardItemCard source details", () => {
  it("shows a copied image file path in the text hover preview", async () => {
    vi.useFakeTimers();
    vi.mocked(invoke).mockImplementation((command) => {
      if (command === "allocate_text_preview_lease") return Promise.resolve(30);
      return Promise.resolve(null);
    });
    render(
      <TooltipProvider>
        <ClipboardItemCard item={imageFileItem} index={0} />
      </TooltipProvider>,
    );

    const image = screen.getByAltText("photo.png");
    fireEvent.mouseEnter(image.closest(".px-3")!);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(128);
    });

    expect(invoke).toHaveBeenCalledWith("show_text_preview", expect.objectContaining({
      text: imageFilePath,
    }));
    expect(invoke).not.toHaveBeenCalledWith("show_image_preview", expect.anything());
  });

  it("shows full copied file paths in the hover preview", async () => {
    vi.useFakeTimers();
    vi.mocked(invoke).mockImplementation((command) => {
      if (command === "allocate_text_preview_lease") return Promise.resolve(40);
      if (command === "batch_get_item_file_status") {
        return Promise.resolve({
          2: {
            all_exist: false,
            resolved_paths: stagedFilePaths,
            checks: {},
          },
        });
      }
      return Promise.resolve(null);
    });
    render(
      <TooltipProvider>
        <ClipboardItemCard item={fileItem} index={0} />
      </TooltipProvider>,
    );

    await act(async () => {
      await vi.advanceTimersByTimeAsync(60);
    });

    const previewAnchor = screen.getByText("2 个文件").closest(".px-3");
    expect(previewAnchor).not.toBeNull();
    fireEvent.mouseEnter(previewAnchor!);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(128);
    });

    expect(invoke).toHaveBeenCalledWith("show_text_preview", expect.objectContaining({
      text: filePaths.join("\n"),
    }));
  });

  it("passes source details to the text hover preview", async () => {
    vi.useFakeTimers();
    vi.mocked(invoke).mockResolvedValueOnce(50);
    render(
      <TooltipProvider>
        <ClipboardItemCard item={sourceItem} index={0} />
      </TooltipProvider>,
    );

    const previewAnchor = screen.getByText(sourceItem.preview).closest(".flex-1");
    expect(previewAnchor).not.toBeNull();
    fireEvent.mouseEnter(previewAnchor!);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(128);
    });

    expect(invoke).toHaveBeenCalledWith("show_text_preview", expect.objectContaining({
      sourceAppName: sourceItem.source_app_name,
      sourceTitle: sourceItem.source_title,
      sourceUrl: sourceItem.source_url,
      sourceFileName: sourceItem.source_file_name,
    }));
  });

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
