import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useDatabaseStore } from "./databases";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const first = { id: "one", name: "默认", path: "C:/one", is_active: true };
const second = { id: "two", name: "工作", path: "C:/two", is_active: false };

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  useDatabaseStore.setState({ databases: [], activeId: null, loading: false, error: null });
});

describe("database store", () => {
  it("fetches registrations and derives the active id", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([first, second]);

    await useDatabaseStore.getState().fetchDatabases();

    expect(invoke).toHaveBeenCalledWith("list_databases");
    expect(useDatabaseStore.getState()).toMatchObject({
      databases: [first, second], activeId: "one", loading: false, error: null,
    });
  });

  it("adds created and existing databases", async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce(second)
      .mockResolvedValueOnce({ ...second, id: "three", path: "C:/three" });

    await useDatabaseStore.getState().createDatabase("工作", "C:/two");
    await useDatabaseStore.getState().addExistingDatabase("归档", "C:/three");

    expect(invoke).toHaveBeenNthCalledWith(1, "create_database", { name: "工作", path: "C:/two" });
    expect(invoke).toHaveBeenNthCalledWith(2, "add_existing_database", { name: "归档", path: "C:/three" });
    expect(useDatabaseStore.getState().databases.map((database) => database.id)).toEqual(["two", "three"]);
  });

  it("renames and removes registrations locally after commands succeed", async () => {
    useDatabaseStore.setState({ databases: [first, second], activeId: "one" });
    vi.mocked(invoke).mockResolvedValue(undefined);

    await useDatabaseStore.getState().renameDatabase("two", "项目");
    await useDatabaseStore.getState().removeRegistration("two");

    expect(useDatabaseStore.getState().databases).toEqual([first]);
  });

  it("switches the active database without restarting", async () => {
    useDatabaseStore.setState({ databases: [first, second], activeId: "one" });
    vi.mocked(invoke).mockResolvedValue({ id: "two" });

    await useDatabaseStore.getState().switchDatabase("two");

    expect(invoke).toHaveBeenCalledWith("switch_database", { id: "two" });
    expect(useDatabaseStore.getState()).toMatchObject({
      activeId: "two",
      databases: [{ ...first, is_active: false }, { ...second, is_active: true }],
    });
  });

  it("stores and rethrows command errors", async () => {
    vi.mocked(invoke).mockRejectedValue(new Error("busy"));

    await expect(useDatabaseStore.getState().fetchDatabases()).rejects.toThrow("busy");
    expect(useDatabaseStore.getState()).toMatchObject({ loading: false, error: "Error: busy" });
  });
});
