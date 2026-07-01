import { describe, it, expect, vi, beforeEach } from "vitest";
import { translateText, getLanguages, getProviderOptions } from "./translate";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn((command: string, args?: Record<string, unknown>) => {
    if (command === "translate_text") {
      return Promise.resolve("Translated text");
    }
    return Promise.resolve();
  }),
}));

vi.mock("@/i18n", () => ({
  t: (key: string, params?: Record<string, string>) => {
    if (params) {
      return Object.entries(params).reduce((acc, [k, v]) => acc.replace(`{{${k}}}`, v), key);
    }
    return key;
  },
}));

vi.mock("@/lib/logger", () => ({
  logError: vi.fn(),
}));

const mockGetState = vi.fn().mockReturnValue({
  enabled: true,
  languageMode: "auto",
  sourceLanguage: null,
  targetLanguage: null,
  provider: "microsoft",
  proxyMode: false,
  proxyUrl: "",
  deeplxEndpoint: "",
  googleApiKey: "",
  baiduAppId: "",
  baiduSecretKey: "",
  openaiEndpoint: "",
  openaiApiKey: "",
  openaiModel: "",
});

vi.mock("@/stores/translate-settings", () => ({
  useTranslateSettings: {
    getState: (...args: unknown[]) => mockGetState(...args),
  },
}));

describe("translate", () => {
  describe("getLanguages", () => {
    it("has 13 languages", () => {
      expect(getLanguages()).toHaveLength(13);
    });

    it("has Chinese", () => {
      expect(getLanguages().find((l) => l.value === "zh")).toBeDefined();
    });

    it("has English", () => {
      expect(getLanguages().find((l) => l.value === "en")).toBeDefined();
    });
  });

  describe("getProviderOptions", () => {
    it("has 6 providers", () => {
      expect(getProviderOptions()).toHaveLength(6);
    });

    it("has Microsoft provider", () => {
      expect(getProviderOptions().find((p) => p.value === "microsoft")).toBeDefined();
    });

    it("has OpenAI provider", () => {
      expect(getProviderOptions().find((p) => p.value === "openai")).toBeDefined();
    });
  });

  describe("translateText", () => {
    beforeEach(() => {
      mockGetState.mockReturnValue({
        enabled: true,
        languageMode: "auto",
        sourceLanguage: null,
        targetLanguage: null,
        provider: "microsoft",
        proxyMode: false,
        proxyUrl: "",
        deeplxEndpoint: "",
        googleApiKey: "",
        baiduAppId: "",
        baiduSecretKey: "",
        openaiEndpoint: "",
        openaiApiKey: "",
        openaiModel: "",
      });
    });

    it("translates text successfully", async () => {
      const result = await translateText("Hello");
      expect(result).toBe("Translated text");
    });

    it("throws when disabled", async () => {
      mockGetState.mockReturnValue({
        ...mockGetState(),
        enabled: false,
      });
      await expect(translateText("Hello")).rejects.toThrow("translate.errors.FEATURE_DISABLED");
    });

    it("uses manual language mode", async () => {
      mockGetState.mockReturnValue({
        ...mockGetState(),
        languageMode: "manual",
        sourceLanguage: "en",
        targetLanguage: "zh",
      });
      const result = await translateText("Hello");
      expect(result).toBe("Translated text");
    });
  });
});
