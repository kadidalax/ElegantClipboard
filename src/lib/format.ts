// 剪贴板条目格式化与解析工具

import { getLocale, t } from "@/i18n";
import { getContentTypeLabel } from "@/lib/constants";

export function getContentTypeConfig(type: string) {
  return { label: getContentTypeLabel(type) };
}

/** @deprecated use getContentTypeConfig() */
export const contentTypeConfig: Record<string, { label: string }> = new Proxy(
  {},
  {
    get(_target, prop: string) {
      return { label: getContentTypeLabel(prop) };
    },
  },
);

export function formatTime(dateStr: string, format: "absolute" | "relative" = "absolute"): string {
  const date = new Date(dateStr);
  if (format === "relative") return formatRelativeTime(date);

  const now = new Date();
  const isToday = date.toDateString() === now.toDateString();

  const hours = date.getHours().toString().padStart(2, "0");
  const minutes = date.getMinutes().toString().padStart(2, "0");
  const time = `${hours}:${minutes}`;

  if (isToday) return t("format.timeToday", { time });

  const yesterday = new Date(now);
  yesterday.setDate(yesterday.getDate() - 1);
  if (date.toDateString() === yesterday.toDateString()) return t("format.timeYesterday", { time });

  const month = (date.getMonth() + 1).toString().padStart(2, "0");
  const day = date.getDate().toString().padStart(2, "0");
  return `${month}-${day} ${time}`;
}

function formatRelativeTime(date: Date): string {
  const now = Date.now();
  const diffMs = now - date.getTime();
  const diffSec = Math.floor(diffMs / 1000);
  if (diffSec < 60) return t("format.relativeJustNow");
  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) return t("format.relativeMinutesAgo", { count: diffMin });
  const diffHour = Math.floor(diffMin / 60);
  if (diffHour < 24) return t("format.relativeHoursAgo", { count: diffHour });
  const diffDay = Math.floor(diffHour / 24);
  if (diffDay < 30) return t("format.relativeDaysAgo", { count: diffDay });
  const diffMonth = Math.floor(diffDay / 30);
  if (diffMonth < 12) return t("format.relativeMonthsAgo", { count: diffMonth });
  return t("format.relativeYearsAgo", { count: Math.floor(diffMonth / 12) });
}

export function formatCharCount(count: number | null): string {
  if (!count) return t("format.charCountZero");
  const locale = getLocale();
  const localeTag = locale === "en" ? "en-US" : locale;
  return count >= 10000
    ? t("format.charCountTenThousands", { count: (count / 10000).toFixed(1) })
    : t("format.charCount", { count: count.toLocaleString(localeTag) });
}

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

export function getFileNameFromPath(path: string): string {
  const parts = path.replace(/\\/g, "/").split("/");
  return parts[parts.length - 1] || path;
}

export function parseFilePaths(filePathsJson: string | null): string[] {
  if (!filePathsJson) return [];
  try {
    const paths = JSON.parse(filePathsJson);
    return Array.isArray(paths) ? paths : [];
  } catch {
    return [];
  }
}

const IMAGE_EXTENSIONS = new Set([
  "png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "ico", "tiff", "tif",
]);

export function isImageFile(path: string): boolean {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return IMAGE_EXTENSIONS.has(ext);
}
