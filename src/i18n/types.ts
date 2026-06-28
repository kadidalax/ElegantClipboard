/** Deep string record type for locale message trees */
export type DeepStringRecord<T> = {
  [K in keyof T]: T[K] extends string ? string : DeepStringRecord<T[K]>;
};

export type Locale = "zh-CN" | "en" | "zh-TW";

export type TranslationTree = DeepStringRecord<
  typeof import("./messages/zh-CN").zhCN
>;
