// In-memory cache for iTunes fallback artwork lookups
export const itunesArtCache = new Map<string, string | null>();

export function formatLastSeen(
  reachable: boolean,
  lastSeenSecs: number,
): string {
  if (reachable) {
    return "Connected over Wi-Fi, last seen just now";
  }
  if (!lastSeenSecs) {
    return "Offline";
  }
  const diffSecs = Math.max(0, Math.floor(Date.now() / 1000) - lastSeenSecs);
  if (diffSecs < 60) {
    return "Offline, last seen just now";
  }
  const mins = Math.floor(diffSecs / 60);
  if (mins < 60) {
    return `Offline, last seen ${mins}m ago`;
  }
  const hours = Math.floor(mins / 60);
  return `Offline, last seen ${hours}h ago`;
}

export function formatDurationMs(ms: number): string {
  if (!ms || ms <= 0) return "0:00";
  const totalSecs = Math.floor(ms / 1000);
  const mins = Math.floor(totalSecs / 60);
  const secs = totalSecs % 60;
  return `${mins}:${secs.toString().padStart(2, "0")}`;
}

export function formatFileSize(bytes: number): string {
  if (!bytes || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unitIdx = 0;
  while (value >= 1024 && unitIdx < units.length - 1) {
    value /= 1024;
    unitIdx++;
  }
  return `${value >= 10 || unitIdx === 0 ? Math.round(value) : value.toFixed(1)} ${units[unitIdx]}`;
}

export function formatShortDate(epochSecs: number | null): string {
  if (!epochSecs || epochSecs <= 0) return "";
  const date = new Date(epochSecs * 1000);
  if (Number.isNaN(date.getTime())) return "";
  const now = new Date();
  const sameYear = date.getFullYear() === now.getFullYear();
  return date.toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
    ...(sameYear ? {} : { year: "numeric" }),
  });
}

export function isValidHttpUrl(raw: string): boolean {
  const trimmed = raw.trim();
  if (!trimmed) return false;
  try {
    const parsed = new URL(trimmed);
    return (
      (parsed.protocol === "http:" || parsed.protocol === "https:") &&
      Boolean(parsed.hostname) &&
      (parsed.hostname.includes(".") || parsed.hostname === "localhost")
    );
  } catch {
    return false;
  }
}
