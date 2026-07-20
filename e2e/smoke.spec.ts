import { expect, test } from "@playwright/test";

// Local rendered runs require Playwright Chromium: `npx playwright install chromium`.
// The command is setup guidance only; CI/developers should install it explicitly.
const clipboardItem = {
  id: 42,
  content_type: "text",
  text_content: "项目计划",
  html_content: null,
  rtf_content: null,
  image_path: null,
  file_paths: null,
  content_hash: "e2e-hash",
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
};

test.beforeEach(async ({ page }) => {
  await page.addInitScript(({ item }) => {
    let callbackId = 0;
    let locked = false;
    const callbacks = new Map<number, (...args: unknown[]) => unknown>();
    const commands: { command: string; args: unknown }[] = [];

    Object.assign(window, {
      __E2E_COMMANDS__: commands,
      __TAURI_EVENT_PLUGIN_INTERNALS__: {
        unregisterListener: () => undefined,
      },
      __TAURI_INTERNALS__: {
        metadata: { currentWindow: { label: "e2e" } },
        convertFileSrc: (path: string) => path,
        transformCallback: (callback: (...args: unknown[]) => unknown, once = false) => {
          const id = ++callbackId;
          callbacks.set(id, (...args: unknown[]) => {
            const result = callback(...args);
            if (once) callbacks.delete(id);
            return result;
          });
          return id;
        },
        invoke: async (command: string, args: unknown) => {
          commands.push({ command, args });
          switch (command) {
            case "get_clipboard_items":
              return [{ ...item, is_locked: locked }];
            case "toggle_lock":
              locked = !locked;
              return locked;
            case "get_active_database_stats":
              return localStorage.getItem("e2e-active-database") === "archive"
                ? { id: "archive", name: "归档数据库", item_count: 7, db_size: 8192 }
                : { id: "main", name: "默认数据库", item_count: 1, db_size: 4096 };
            case "list_databases": {
              const activeId = localStorage.getItem("e2e-active-database") ?? "main";
              return [
                { id: "main", name: "默认数据库", path: "C:\\Clipboard\\main", is_active: activeId === "main" },
                { id: "archive", name: "归档数据库", path: "C:\\Clipboard\\archive", is_active: activeId === "archive" },
              ];
            }
            case "switch_database": {
              const id = (args as { id?: string } | undefined)?.id;
              if (id !== "archive" && id !== "main") throw new Error(`Unexpected database id: ${id}`);
              localStorage.setItem("e2e-active-database", id);
              return null;
            }
            case "get_data_size":
              return { db_size: 4096, images_size: 0, images_count: 0, total_size: 4096 };
            case "get_settings_batch":
              return {};
            case "get_groups":
            case "get_toolbar_buttons":
              return [];
            case "get_app_version":
              return "e2e";
            case "get_build_time":
              return "e2e";
            case "get_default_data_path":
              return localStorage.getItem("e2e-active-database") === "archive"
                ? "C:\\Clipboard\\archive"
                : "C:\\Clipboard\\main";
            case "get_current_shortcut":
              return "Alt+C";
            case "get_system_accent_color":
              return "#0078d4";
            case "get_setting":
              return null;
            case "plugin:event|listen":
              return ++callbackId;
            case "plugin:event|emit":
            case "plugin:event|unlisten":
            case "plugin:window|show":
            case "plugin:window|set_focus":
            case "set_keyboard_nav_enabled":
            case "set_window_effect":
            case "sync_preview_window_effects":
              return null;
            case "is_window_pinned":
            case "is_autostart_enabled":
            case "is_admin_launch_enabled":
            case "is_running_as_admin":
            case "is_portable_mode":
            case "is_winv_replacement_enabled":
            case "is_log_to_file_enabled":
            case "take_pending_update_dialog":
              return false;
            case "get_log_file_path":
              return "";
            default:
              throw new Error(`Unhandled E2E IPC command: ${command}`);
          }
        },
      },
    });
  }, { item: clipboardItem });
});

test("main window exposes stats, lock action, and timeline return", async ({ page }) => {
  await page.goto("/");

  await expect(page.getByText("默认数据库 · 1 条")).toBeVisible();
  await expect(page.getByText("项目计划", { exact: true })).toBeVisible();

  const lockButton = page.getByRole("button", { name: "锁定" });
  await lockButton.click();
  await expect(page.getByRole("button", { name: "解锁" })).toBeVisible();

  await page.getByPlaceholder("搜索剪贴板内容...").fill("项目");
  await expect.poll(() => page.evaluate(() => (
    window as typeof window & { __E2E_COMMANDS__: { command: string; args: { search?: string } }[] }
  ).__E2E_COMMANDS__.some(({ command, args }) => command === "get_clipboard_items" && args?.search === "项目"))).toBe(true);
  await page.getByText("项目计划", { exact: true }).click({ button: "right" });
  await page.getByText("在时间线中定位", { exact: true }).click();
  await expect(page.getByRole("button", { name: "返回搜索结果" })).toBeVisible();
});

test("settings data management confirms and hot-switches the active database", async ({ page }) => {
  await page.setViewportSize({ width: 1100, height: 760 });
  await page.goto("/settings.html");

  await page.getByRole("button", { name: "数据管理" }).click();
  await expect(page.getByText("数据库管理", { exact: true })).toBeVisible();
  await expect(page.getByText("当前", { exact: true })).toBeVisible();

  await page.getByRole("button", { name: "切换", exact: true }).click();
  await expect(page.getByRole("heading", { name: "确认切换数据库" })).toBeVisible();
  await expect(page.getByText("确定切换到“归档数据库”吗？无需重启软件。", { exact: true })).toBeVisible();
  await page.getByRole("dialog").getByRole("button", { name: "切换", exact: true }).click();

  await expect.poll(() => page.evaluate(() => (
    window as typeof window & { __E2E_COMMANDS__: { command: string; args: unknown }[] }
  ).__E2E_COMMANDS__)).toContainEqual({ command: "switch_database", args: { id: "archive" } });
  expect(await page.evaluate(() => (
    window as typeof window & { __E2E_COMMANDS__: { command: string }[] }
  ).__E2E_COMMANDS__.some(({ command }) => command === "restart_app"))).toBe(false);

  await expect(page.getByText("归档数据库", { exact: true }).locator("..").getByText("当前", { exact: true })).toBeVisible();
  await page.goto("/");
  await expect(page.getByText("归档数据库 · 7 条")).toBeVisible();
});
