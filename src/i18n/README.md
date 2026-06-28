# i18n 模块

```
src/i18n/
  index.ts          # 对外 API：useTranslation、t、initLocale
  runtime.ts        # Zustand store + 多窗口同步
  types.ts          # Locale、TranslationTree 类型
  core/
    merge.ts        # 深度合并 locale 树
    translator.ts   # createTranslator、LOCALE_OPTIONS
  messages/
    zh-CN/
      core.ts       # 基础文案（app、toolbar、groups…）
      extended.ts   # 扩展文案（settings 各 Tab、卡片 UI…）
      index.ts      # mergeDeep(core, extended) → zhCN
    en/
    zh-TW/
  locales/          # extended 源文件（*-ext.ts），core 已迁入 messages
```

## 使用

```tsx
import { useTranslation } from "@/i18n";

function MyComponent() {
  const { t } = useTranslation();
  return <span>{t("app.searchPlaceholder")}</span>;
}
```

带参数：`t("app.batchSelected", { count: 3 })`

## 新增文案

1. 在 `messages/{locale}/core.ts` 或 `locales/{locale}-ext.ts` 中添加 key
2. 同步更新 zh-CN、en、zh-TW 三份
3. 组件中使用 `t("your.key")`
