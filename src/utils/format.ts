import type { DeviceView, SimSignalInfo } from "../types";

// In-memory cache for iTunes fallback artwork lookups
export const itunesArtCache = new Map<string, string | null>();

export function formatLastSeen(
  reachable: boolean,
  lastSeenSecs: number
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

export function formatSimSignal(sim: SimSignalInfo): string {
  if (sim.networkType && sim.signalStrength >= 0) {
    return `${sim.networkType} • ${sim.signalStrength}/4`;
  }
  if (sim.signalStrength >= 0) {
    return `${sim.signalStrength}/4 bars`;
  }
  return sim.networkType || "—";
}

export function formatSignal(device: DeviceView): string {
  if (
    device.cellularNetworkType &&
    device.cellularSignalStrength !== null &&
    device.cellularSignalStrength >= 0
  ) {
    return `${device.cellularNetworkType} • ${device.cellularSignalStrength}/4`;
  }
  if (
    device.cellularSignalStrength !== null &&
    device.cellularSignalStrength >= 0
  ) {
    return `${device.cellularSignalStrength}/4 bars`;
  }
  if (device.cellularNetworkType) {
    return device.cellularNetworkType;
  }
  return device.reachable ? "Wi-Fi" : "—";
}

export function formatDurationMs(ms: number): string {
  if (!ms || ms <= 0) return "0:00";
  const totalSecs = Math.floor(ms / 1000);
  const mins = Math.floor(totalSecs / 60);
  const secs = totalSecs % 60;
  return `${mins}:${secs.toString().padStart(2, "0")}`;
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
