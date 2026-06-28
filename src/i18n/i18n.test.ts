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
});
