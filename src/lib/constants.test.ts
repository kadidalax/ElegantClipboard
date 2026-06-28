import { describe, it, expect } from "vitest";
import { t } from "@/i18n";
import { getGroups, getToolbarButtonRegistry, GROUP_VALUES } from "./constants";

describe("TOOLBAR_BUTTON_REGISTRY", () => {
  it("has clear button", () => {
    const registry = getToolbarButtonRegistry();
    expect(registry.clear).toBeDefined();
    expect(registry.clear.label).toBe(t("toolbar.clearHistory"));
  });

  it("has pin button", () => {
    const registry = getToolbarButtonRegistry();
    expect(registry.pin).toBeDefined();
    expect(registry.pin.label).toBe(t("toolbar.pinWindow"));
  });

  it("has batch button", () => {
    const registry = getToolbarButtonRegistry();
    expect(registry.batch).toBeDefined();
    expect(registry.batch.label).toBe(t("toolbar.batchSelect"));
  });

  it("has settings button", () => {
    const registry = getToolbarButtonRegistry();
    expect(registry.settings).toBeDefined();
    expect(registry.settings.label).toBe(t("toolbar.settings"));
  });

  it("all buttons have descriptions", () => {
    for (const [key, value] of Object.entries(getToolbarButtonRegistry())) {
      expect(value.description).toBeTruthy();
      expect(value.label).toBeTruthy();
      void key;
    }
  });
});

describe("GROUPS", () => {
  it("has 4 groups", () => {
    expect(GROUP_VALUES).toHaveLength(4);
  });

  it("first group is all", () => {
    const groups = getGroups();
    expect(groups[0].label).toBe(t("groups.all"));
    expect(groups[0].value).toBeNull();
  });

  it("second group is favorites", () => {
    const groups = getGroups();
    expect(groups[1].label).toBe(t("groups.favorites"));
    expect(groups[1].value).toBe("__favorites__");
  });

  it("third group is text", () => {
    const groups = getGroups();
    expect(groups[2].label).toBe(t("groups.text"));
    expect(groups[2].value).toContain("text");
  });

  it("fourth group is other", () => {
    const groups = getGroups();
    expect(groups[3].label).toBe(t("groups.other"));
    expect(groups[3].value).toContain("image");
    expect(groups[3].value).toContain("url");
  });
});
