import type { DateTimeFormatPreference } from "../types/monitoring";

/** Formats a byte count for Settings → Data. */
export function formatBytes(bytes: number | null | undefined): string {
  if (bytes == null || Number.isNaN(bytes)) {
    return "—";
  }
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  return `${value.toFixed(value >= 10 ? 0 : 1)} ${units[unitIndex]}`;
}

/** Formats an ISO timestamp according to the user's date/time preference. */
export function formatDateTime(
  iso: string | null | undefined,
  format: DateTimeFormatPreference = "iso",
): string {
  if (!iso) {
    return "—";
  }
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) {
    return iso;
  }
  if (format === "iso") {
    return iso;
  }
  const options: Intl.DateTimeFormatOptions =
    format === "locale_seconds"
      ? {
          year: "numeric",
          month: "short",
          day: "2-digit",
          hour: "2-digit",
          minute: "2-digit",
          second: "2-digit",
        }
      : {
          year: "numeric",
          month: "short",
          day: "2-digit",
          hour: "2-digit",
          minute: "2-digit",
        };
  return new Intl.DateTimeFormat(undefined, options).format(date);
}

/** Applies `system` / `light` / `dark` to the document root. */
export function applyTheme(theme: string): void {
  const root = document.documentElement;
  if (theme === "light" || theme === "dark") {
    root.dataset.theme = theme;
  } else {
    delete root.dataset.theme;
  }
}
