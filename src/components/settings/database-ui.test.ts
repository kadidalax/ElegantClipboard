import { emit } from "@tauri-apps/api/event";
import { describe, expect, it, vi } from "vitest";
import {
  getMigrationAvailability,
  renameDatabaseAndNotify,
} from "./DataTab";

vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn(() => Promise.resolve()) }));

describe("database migration UI", () => {
  it("only offers migration for an empty target", () => {
    expect(getMigrationAvailability(false)).toBe("migrate");
    expect(getMigrationAvailability(true)).toBe("blocked");
  });

  it("notifies the main window after a successful database rename", async () => {
    const renameDatabase = vi.fn(() => Promise.resolve());

    await renameDatabaseAndNotify(renameDatabase, "db-1", "工作库");

    expect(renameDatabase).toHaveBeenCalledWith("db-1", "工作库");
    expect(emit).toHaveBeenCalledWith("database-stats-changed");
  });
});
