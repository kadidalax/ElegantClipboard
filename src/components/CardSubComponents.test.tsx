import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { TooltipProvider } from "@/components/ui/tooltip";
import type { ClipboardItem } from "@/stores/clipboard";
import { ActionToolbar } from "./CardSubComponents";

function renderToolbar(isLocked: boolean) {
  return render(
    <TooltipProvider>
      <ActionToolbar
        item={{ id: 1, is_locked: isLocked } as ClipboardItem}
        onTogglePin={vi.fn()}
        onToggleFavorite={vi.fn()}
        onToggleLock={vi.fn()}
        onCopy={vi.fn()}
        onDelete={vi.fn()}
      />
    </TooltipProvider>,
  );
}

describe("ActionToolbar", () => {
  it("labels the lock action and shows for keyboard focus", () => {
    const { container } = renderToolbar(false);

    expect(screen.getByRole("button", { name: "锁定" })).toBeInTheDocument();
    expect(container.firstElementChild).toHaveClass("group-focus-within:opacity-100");
  });

  it("labels the unlock action", () => {
    renderToolbar(true);
    expect(screen.getByRole("button", { name: "解锁" })).toBeInTheDocument();
  });
});
