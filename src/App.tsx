import {
  ArrowLeft,
  BellRinging,
  CheckCircle,
  Desktop,
  DeviceMobile,
  DeviceTablet,
  GearSix,
  Laptop,
  Link,
  MusicNotes,
  PaperPlaneRight,
  Pause,
  Play,
  PlusCircle,
  SkipBack,
  SkipForward,
  SlidersHorizontal,
  Television,
  Trash,
  UploadSimple,
} from "@phosphor-icons/react";
import * as Popover from "@radix-ui/react-popover";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import {
  animate,
  AnimatePresence,
  motion,
  useMotionValue,
  useTransform,
} from "motion/react";
import { type MouseEvent, useEffect, useRef, useState } from "react";

interface NowPlayingInfo {
  player: string;
  title: string;
  artist: string;
  album: string;
  isPlaying: boolean;
  positionMs: number;
  positionUpdatedAtMs: number;
  lengthMs: number;
  canPlay: boolean;
  canPause: boolean;
  canGoNext: boolean;
  canGoPrevious: boolean;
  canSeek: boolean;
  albumArtUrl: string | null;
  albumArtData: string | null;
}

interface SimSignalInfo {
  slotId: string;
  networkType: string;
  signalStrength: number;
}

interface DeviceView {
  deviceId: string;
  deviceName: string;
  deviceType: string;
  protocolVersion: number;
  address: string;
  tcpPort: number;
  reachable: boolean;
  pairState: "NotPaired" | "Requested" | "RequestedByPeer" | "Paired";
  verificationKey: string | null;
  certFingerprint: string | null;
  incomingCapabilities: string[];
  outgoingCapabilities: string[];
  lastSeenSecs: number;
  batteryCharge: number | null;
  batteryCharging: boolean | null;
  cellularNetworkType: string | null;
  cellularSignalStrength: number | null;
  simSignals: SimSignalInfo[];
  storageFreeGb: number | null;
  nowPlaying: NowPlayingInfo | null;
}

interface TransferItem {
  id: string;
  deviceId: string;
  deviceName: string;
  direction: "Incoming" | "Outgoing";
  kind: "File" | "Text" | "Url";
  filename: string;
  totalBytes: number;
  transferredBytes: number;
  status: "InProgress" | "Completed" | "Failed";
  savedPath: string | null;
  error: string | null;
  timestampMs: number;
}

interface AppSnapshot {
  host?: {
    downloadsDir?: string;
  };
  devices: DeviceView[];
  transfers: TransferItem[];
}

// In-memory cache for iTunes fallback artwork lookups
const itunesArtCache = new Map<string, string | null>();

function DeviceIcon({
  type,
  size = 18,
}: {
  type: string;
  size?: number;
}) {
  const normalized = type.toLowerCase();
  if (normalized === "phone" || normalized === "smartphone") {
    return <DeviceMobile size={size} weight="regular" className="shrink-0" />;
  }
  if (normalized === "laptop") {
    return <Laptop size={size} weight="regular" className="shrink-0" />;
  }
  if (normalized === "tablet") {
    return <DeviceTablet size={size} weight="regular" className="shrink-0" />;
  }
  if (normalized === "tv") {
    return <Television size={size} weight="regular" className="shrink-0" />;
  }
  return <Desktop size={size} weight="regular" className="shrink-0" />;
}

function formatLastSeen(reachable: boolean, lastSeenSecs: number): string {
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

function formatSimSignal(sim: SimSignalInfo): string {
  if (sim.networkType && sim.signalStrength >= 0) {
    return `${sim.networkType} • ${sim.signalStrength}/4`;
  }
  if (sim.signalStrength >= 0) {
    return `${sim.signalStrength}/4 bars`;
  }
  return sim.networkType || "—";
}

function formatSignal(device: DeviceView): string {
  if (
    device.cellularNetworkType &&
    device.cellularSignalStrength !== null &&
    device.cellularSignalStrength >= 0
  ) {
    return `${device.cellularNetworkType} • ${device.cellularSignalStrength}/4`;
  }
  if (device.cellularSignalStrength !== null && device.cellularSignalStrength >= 0) {
    return `${device.cellularSignalStrength}/4 bars`;
  }
  if (device.cellularNetworkType) {
    return device.cellularNetworkType;
  }
  return device.reachable ? "Wi-Fi" : "—";
}

function formatDurationMs(ms: number): string {
  if (!ms || ms <= 0) return "0:00";
  const totalSecs = Math.floor(ms / 1000);
  const mins = Math.floor(totalSecs / 60);
  const secs = totalSecs % 60;
  return `${mins}:${secs.toString().padStart(2, "0")}`;
}

function isValidHttpUrl(raw: string): boolean {
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

export default function App() {
  const [devices, setDevices] = useState<DeviceView[]>([]);
  const [transfers, setTransfers] = useState<TransferItem[]>([]);
  const [selectedDeviceId, setSelectedDeviceId] = useState<string | null>(null);
  const [hoveredDeviceId, setHoveredDeviceId] = useState<string | null>(null);
  const [isDraggingOver, setIsDraggingOver] = useState<boolean>(false);
  const [justDroppedCount, setJustDroppedCount] = useState<number>(0);
  const [pingToast, setPingToast] = useState<{
    deviceName: string;
    message: string;
  } | null>(null);
  const [fallbackArtworkUrl, setFallbackArtworkUrl] = useState<string | null>(
    null
  );
  const [displaySecs, setDisplaySecs] = useState<number>(0);
  const [isShareLinkOpen, setIsShareLinkOpen] = useState<boolean>(false);
  const [shareLinkInput, setShareLinkInput] = useState<string>("");
  const [shareLinkTouchedInvalid, setShareLinkTouchedInvalid] =
    useState<boolean>(false);
  const [isSettingsOpen, setIsSettingsOpen] = useState<boolean>(false);
  const [fileSaveLocation, setFileSaveLocation] = useState<string>("");
  const [syncClipboard, setSyncClipboard] = useState<boolean>(true);
  const seekBarRef = useRef<HTMLDivElement | null>(null);
  const dropZoneRef = useRef<HTMLDivElement | null>(null);
  const selectedDeviceRef = useRef<DeviceView | null>(null);
  const isSettingsOpenRef = useRef<boolean>(false);
  isSettingsOpenRef.current = isSettingsOpen;
  const isAnimatingSeekRef = useRef<boolean>(false);
  const optimisticSeekRef = useRef<{
    player: string;
    title: string;
    targetMs: number;
    atMs: number;
  } | null>(null);

  // Single shared MotionValue (0..100) driving both the bar fill and the dot at 60fps without React re-renders
  const progressPctMv = useMotionValue(0);
  const progressPctStyle = useTransform(progressPctMv, (v) => `${v}%`);

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    async function loadDevices() {
      try {
        const snap = await invoke<AppSnapshot>("get_app_snapshot");
        setDevices(snap.devices);
        setTransfers(snap.transfers || []);
        if (snap.host?.downloadsDir) {
          const defaultDir = snap.host.downloadsDir;
          setFileSaveLocation((prev) => (prev ? prev : defaultDir));
        }
        setSelectedDeviceId((prev) => {
          if (prev && snap.devices.some((d) => d.deviceId === prev)) {
            return prev;
          }
          return snap.devices[0]?.deviceId ?? null;
        });
      } catch (err) {
        console.error("Failed to load devices:", err);
      }
    }

    loadDevices();

    listen("velquo://state-updated", () => {
      loadDevices();
    }).then((fn) => {
      unlisten = fn;
    });

    let unlistenPing: (() => void) | undefined;
    let pingTimer: ReturnType<typeof setTimeout> | undefined;
    listen<{ deviceId: string; deviceName: string; message: string }>(
      "velquo://ping",
      (event) => {
        setPingToast({
          deviceName: event.payload.deviceName,
          message: event.payload.message,
        });
        if (pingTimer) clearTimeout(pingTimer);
        pingTimer = setTimeout(() => setPingToast(null), 4000);
      }
    ).then((fn) => {
      unlistenPing = fn;
    });

    return () => {
      if (unlisten) unlisten();
      if (unlistenPing) unlistenPing();
      if (pingTimer) clearTimeout(pingTimer);
    };
  }, []);

  const selectedDevice =
    devices.find((d) => d.deviceId === selectedDeviceId) ?? devices[0] ?? null;
  selectedDeviceRef.current = selectedDevice;

  // Native Tauri drag-and-drop file listener scoped strictly to the drop zone bounding rect
  useEffect(() => {
    let unlistenDrop: (() => void) | undefined;

    const isInsideDropZone = (pos: { x: number; y: number }): boolean => {
      if (isSettingsOpenRef.current) {
        return false;
      }
      const targetDevice = selectedDeviceRef.current;
      if (
        !targetDevice ||
        !targetDevice.reachable ||
        targetDevice.pairState !== "Paired"
      ) {
        return false;
      }
      const el = dropZoneRef.current;
      if (!el) return false;
      const rect = el.getBoundingClientRect();
      return (
        pos.x >= rect.left &&
        pos.x <= rect.right &&
        pos.y >= rect.top &&
        pos.y <= rect.bottom
      );
    };

    getCurrentWebview()
      .onDragDropEvent(async (event) => {
        if (event.payload.type === "enter" || event.payload.type === "over") {
          setIsDraggingOver(isInsideDropZone(event.payload.position));
        } else if (event.payload.type === "leave") {
          setIsDraggingOver(false);
        } else if (event.payload.type === "drop") {
          const droppedInside = isInsideDropZone(event.payload.position);
          setIsDraggingOver(false);
          if (!droppedInside) {
            return;
          }
          const targetDevice = selectedDeviceRef.current;
          if (
            targetDevice &&
            targetDevice.reachable &&
            targetDevice.pairState === "Paired" &&
            event.payload.paths.length > 0
          ) {
            setJustDroppedCount(event.payload.paths.length);
            setTimeout(() => setJustDroppedCount(0), 2200);
            for (const filePath of event.payload.paths) {
              try {
                await invoke("send_file", {
                  deviceId: targetDevice.deviceId,
                  filePath,
                });
              } catch (err) {
                console.error("Failed to send dropped file:", err);
              }
            }
          }
        }
      })
      .then((fn) => {
        unlistenDrop = fn;
      });

    return () => {
      if (unlistenDrop) unlistenDrop();
    };
  }, []);

  const handlePickFilesToSend = async () => {
    if (
      !selectedDevice ||
      !selectedDevice.reachable ||
      selectedDevice.pairState !== "Paired"
    ) {
      return;
    }
    const selected = await open({
      multiple: true,
      directory: false,
      title: `Send file(s) to ${selectedDevice.deviceName}`,
    });
    if (selected) {
      const paths = Array.isArray(selected) ? selected : [selected];
      if (paths.length > 0) {
        setJustDroppedCount(paths.length);
        setTimeout(() => setJustDroppedCount(0), 2200);
      }
      for (const filePath of paths) {
        try {
          await invoke("send_file", {
            deviceId: selectedDevice.deviceId,
            filePath,
          });
        } catch (err) {
          console.error("Failed to send selected file:", err);
        }
      }
    }
  };

  const nowPlaying = selectedDevice?.nowPlaying ?? null;
  const nativeArtData = nowPlaying?.albumArtData ?? null;
  const trackTitle = nowPlaying?.title ?? "";
  const trackArtist = nowPlaying?.artist ?? "";
  const trackAlbum = nowPlaying?.album ?? "";

  // 60fps requestAnimationFrame loop updating the MotionValue directly (never interrupting a seek spring)
  useEffect(() => {
    if (!nowPlaying || nowPlaying.lengthMs <= 0) {
      progressPctMv.set(0);
      setDisplaySecs(0);
      optimisticSeekRef.current = null;
      return;
    }

    if (
      optimisticSeekRef.current &&
      (optimisticSeekRef.current.player !== nowPlaying.player ||
        optimisticSeekRef.current.title !== nowPlaying.title)
    ) {
      optimisticSeekRef.current = null;
    }

    const computePosMs = () => {
      const now = Date.now();
      const opt = optimisticSeekRef.current;

      // Hold optimistic seek position for 1500ms so stale pre-seek packets from the phone never pull the bar back
      if (opt && now - opt.atMs < 1500) {
        const elapsed = nowPlaying.isPlaying ? Math.max(0, now - opt.atMs) : 0;
        return Math.min(
          nowPlaying.lengthMs,
          Math.max(0, opt.targetMs + elapsed)
        );
      }

      if (!nowPlaying.isPlaying) {
        return Math.min(nowPlaying.lengthMs, Math.max(0, nowPlaying.positionMs));
      }
      const baseTime = nowPlaying.positionUpdatedAtMs || now;
      const elapsed = Math.max(0, now - baseTime);
      return Math.min(
        nowPlaying.lengthMs,
        Math.max(0, nowPlaying.positionMs + elapsed)
      );
    };

    const syncNow = () => {
      const posMs = computePosMs();
      const pct = Math.min(100, Math.max(0, (posMs / nowPlaying.lengthMs) * 100));
      if (!isAnimatingSeekRef.current) {
        progressPctMv.set(pct);
      }
      const secs = Math.floor(posMs / 1000);
      setDisplaySecs((prev) => (prev !== secs ? secs : prev));
    };

    syncNow();

    if (!nowPlaying.isPlaying) {
      return;
    }

    let rafId: number;
    const tick = () => {
      syncNow();
      rafId = requestAnimationFrame(tick);
    };
    rafId = requestAnimationFrame(tick);

    return () => cancelAnimationFrame(rafId);
  }, [
    nowPlaying?.player,
    nowPlaying?.title,
    nowPlaying?.positionMs,
    nowPlaying?.positionUpdatedAtMs,
    nowPlaying?.isPlaying,
    nowPlaying?.lengthMs,
    progressPctMv,
  ]);

  // Fallback to iTunes Search API if KDE Connect hasn't provided native album art
  useEffect(() => {
    if (!trackTitle) {
      setFallbackArtworkUrl(null);
      return;
    }

    // If native KDE Connect artwork is already present, no fallback needed
    if (nativeArtData) {
      setFallbackArtworkUrl(null);
      return;
    }

    const queryKey = `${trackArtist} - ${trackTitle} - ${trackAlbum}`.trim();
    if (itunesArtCache.has(queryKey)) {
      setFallbackArtworkUrl(itunesArtCache.get(queryKey) ?? null);
      return;
    }

    let cancelled = false;
    // Brief grace window so native KDE Connect TLS payload transfer can finish first
    const timer = setTimeout(async () => {
      try {
        const searchTerm = [trackArtist, trackTitle]
          .filter(Boolean)
          .join(" ")
          .trim();
        if (!searchTerm) return;

        const resp = await fetch(
          `https://itunes.apple.com/search?term=${encodeURIComponent(
            searchTerm
          )}&entity=song&limit=1`
        );
        if (!resp.ok) return;
        const data = await resp.json();
        const rawUrl: string | undefined = data?.results?.[0]?.artworkUrl100;
        const hiResUrl = rawUrl
          ? rawUrl.replace("100x100bb", "300x300bb")
          : null;
        itunesArtCache.set(queryKey, hiResUrl);
        if (!cancelled) {
          setFallbackArtworkUrl(hiResUrl);
        }
      } catch {
        // Ignore network errors on fallback lookup
      }
    }, 550);

    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [nativeArtData, trackTitle, trackArtist, trackAlbum]);

  const artworkSrc = nativeArtData || fallbackArtworkUrl;
  const hasActiveMedia = Boolean(
    !isSettingsOpen &&
      selectedDevice?.reachable &&
      selectedDevice?.nowPlaying?.title
  );
  const hasArtworkBackdrop = Boolean(hasActiveMedia && artworkSrc);

  const isShareLinkInvalid =
    (shareLinkInput.trim().length > 0 && !isValidHttpUrl(shareLinkInput)) ||
    shareLinkTouchedInvalid;

  const handleSendShareLink = async () => {
    if (
      !selectedDevice ||
      !selectedDevice.reachable ||
      selectedDevice.pairState !== "Paired"
    ) {
      return;
    }
    const trimmed = shareLinkInput.trim();
    if (!isValidHttpUrl(trimmed)) {
      setShareLinkTouchedInvalid(true);
      return;
    }
    try {
      await invoke("send_url", {
        deviceId: selectedDevice.deviceId,
        url: trimmed,
      });
      setShareLinkInput("");
      setShareLinkTouchedInvalid(false);
      setIsShareLinkOpen(false);
    } catch (err) {
      console.error("Failed to share URL:", err);
      setShareLinkTouchedInvalid(true);
    }
  };

  const handleChooseSaveLocation = async () => {
    const selected = await open({
      directory: true,
      multiple: false,
      defaultPath: fileSaveLocation || undefined,
      title: "Select File Save Location",
    });
    if (typeof selected === "string" && selected.trim()) {
      setFileSaveLocation(selected);
    }
  };

  const handleDragMouseDown = (e: MouseEvent<HTMLDivElement>) => {
    if (e.buttons === 1) {
      getCurrentWindow().startDragging();
    }
  };

  const handleMediaAction = async (action: string) => {
    if (!selectedDevice || !selectedDevice.nowPlaying) return;
    try {
      await invoke("send_media_action", {
        deviceId: selectedDevice.deviceId,
        player: selectedDevice.nowPlaying.player,
        action,
      });
    } catch (err) {
      console.error("Failed to send media action:", err);
    }
  };

  const handleSeekClick = async (e: MouseEvent<HTMLDivElement>) => {
    if (
      !selectedDevice ||
      !selectedDevice.nowPlaying ||
      selectedDevice.nowPlaying.lengthMs <= 0
    ) {
      return;
    }
    const bar = seekBarRef.current;
    if (!bar) return;

    const rect = bar.getBoundingClientRect();
    const ratio = Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width));
    const targetMs = Math.round(ratio * selectedDevice.nowPlaying.lengthMs);
    const targetPct = ratio * 100;

    // Lock optimistic seek position for 1.5s so phone catch-up packets never cause bounce-back
    optimisticSeekRef.current = {
      player: selectedDevice.nowPlaying.player,
      title: selectedDevice.nowPlaying.title,
      targetMs,
      atMs: Date.now(),
    };
    setDisplaySecs(Math.floor(targetMs / 1000));

    // Smooth uninterrupted spring animation on the shared MotionValue
    isAnimatingSeekRef.current = true;
    animate(progressPctMv, targetPct, {
      type: "spring",
      stiffness: 280,
      damping: 30,
      onComplete: () => {
        isAnimatingSeekRef.current = false;
      },
    });

    try {
      await invoke("seek_media", {
        deviceId: selectedDevice.deviceId,
        player: selectedDevice.nowPlaying.player,
        positionMs: targetMs,
      });
    } catch (err) {
      console.error("Failed to seek media:", err);
    }
  };

  return (
    <div className="relative flex h-screen w-screen select-none overflow-hidden bg-transparent text-zinc-900 dark:text-zinc-100">
      {/* Top window drag region aligned with macOS traffic lights */}
      <div
        data-tauri-drag-region
        onMouseDown={handleDragMouseDown}
        className="fixed top-0 left-0 z-50 h-11 w-full"
      />

      {/* Animated Ping Notification Toast */}
      <AnimatePresence>
        {pingToast && (
          <motion.div
            key="ping-toast"
            initial={{ opacity: 0, y: -16, scale: 0.94 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: -12, scale: 0.95 }}
            transition={{ type: "spring", stiffness: 520, damping: 32 }}
            onClick={() => setPingToast(null)}
            className="fixed top-4 right-5 z-50 flex cursor-pointer items-center gap-3 rounded-2xl border border-black/10 bg-white/90 px-4 py-2.5 shadow-lg backdrop-blur-xl dark:border-white/15 dark:bg-zinc-800/90"
          >
            <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-xl bg-blue-500/15 text-blue-600 dark:bg-blue-500/20 dark:text-blue-400">
              <BellRinging size={18} weight="fill" />
            </div>
            <div className="flex flex-col pr-1">
              <span className="text-[12.5px] font-semibold text-zinc-900 dark:text-zinc-100">
                {pingToast.deviceName}
              </span>
              <span className="text-[11.5px] text-zinc-500 dark:text-zinc-400">
                {pingToast.message}
              </span>
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      {/* Left transparent/vibrant sidebar showing devices */}
      <aside className="flex h-full w-60 shrink-0 select-none flex-col border-r border-black/10 bg-transparent px-2 pt-12 pb-4 dark:border-white/10">
        <div className="mb-2 select-none px-3.5 text-xs font-medium text-zinc-500 dark:text-zinc-400">
          Devices
        </div>

        <div
          className="flex flex-1 flex-col gap-1 overflow-y-auto px-1.5 py-1"
          onMouseLeave={() => setHoveredDeviceId(null)}
        >
          <AnimatePresence initial={false}>
            {devices.map((device, index) => {
              const isSelected = device.deviceId === selectedDevice?.deviceId;
              const isHovered = device.deviceId === hoveredDeviceId;

              return (
                <motion.div
                  key={device.deviceId}
                  layout
                  initial={{ opacity: 0, y: 6 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, scale: 0.96 }}
                  transition={{
                    type: "spring",
                    stiffness: 520,
                    damping: 34,
                    delay: index * 0.03,
                  }}
                  whileHover={{ x: 2 }}
                  whileTap={{ scale: 0.975 }}
                  onMouseEnter={() => setHoveredDeviceId(device.deviceId)}
                  onClick={() => {
                    if (device.deviceId !== selectedDevice?.deviceId) {
                      setIsSettingsOpen(false);
                      setIsShareLinkOpen(false);
                    }
                    setSelectedDeviceId(device.deviceId);
                  }}
                  className={`relative flex cursor-pointer select-none items-center justify-between rounded-xl px-3 py-2 transition-colors duration-150 ${
                    isSelected
                      ? "text-zinc-900 dark:text-zinc-100"
                      : "text-zinc-600 hover:text-zinc-900 dark:text-zinc-400 dark:hover:text-zinc-200"
                  }`}
                >
                  {/* Animated active selection pill */}
                  {isSelected && (
                    <motion.div
                      layoutId="active-device-pill"
                      transition={{
                        type: "spring",
                        stiffness: 500,
                        damping: 35,
                      }}
                      className="absolute inset-0 rounded-xl bg-black/[0.08] dark:bg-white/[0.11]"
                    />
                  )}

                  {/* Subtle animated hover pill when not selected */}
                  {!isSelected && isHovered && (
                    <motion.div
                      layoutId="hover-device-pill"
                      initial={{ opacity: 0 }}
                      animate={{ opacity: 1 }}
                      exit={{ opacity: 0 }}
                      transition={{
                        type: "spring",
                        stiffness: 500,
                        damping: 38,
                      }}
                      className="absolute inset-0 rounded-xl bg-black/[0.04] dark:bg-white/[0.05]"
                    />
                  )}

                  <div className="relative z-10 flex min-w-0 select-none items-center gap-2.5">
                    <motion.span
                      animate={{
                        scale: isSelected || isHovered ? 1.06 : 1,
                      }}
                      transition={{ type: "spring", stiffness: 450, damping: 25 }}
                      className="flex items-center"
                    >
                      <DeviceIcon type={device.deviceType} />
                    </motion.span>
                    <span className="truncate select-none text-[13.5px] font-medium">
                      {device.deviceName}
                    </span>
                  </div>

                  <motion.span
                    layout
                    initial={{ scale: 0.8 }}
                    animate={{
                      scale: device.reachable ? 1 : 0.9,
                    }}
                    transition={{ type: "spring", stiffness: 500, damping: 28 }}
                    className={`relative z-10 ml-2 h-2 w-2 shrink-0 select-none rounded-full ${
                      device.reachable
                        ? "bg-[#30d158]"
                        : "bg-zinc-400 dark:bg-zinc-600"
                    }`}
                  />
                </motion.div>
              );
            })}
          </AnimatePresence>
        </div>

        {/* Bottom sidebar action buttons */}
        <div className="mt-2 flex flex-col gap-1 border-t border-black/8 px-1.5 pt-2.5 dark:border-white/8">
          <motion.button
            type="button"
            whileHover={{ x: 2 }}
            whileTap={{ scale: 0.975 }}
            transition={{ type: "spring", stiffness: 520, damping: 34 }}
            className="flex w-full cursor-pointer select-none items-center gap-2.5 rounded-xl px-3 py-2 text-[13.5px] font-medium text-zinc-600 transition-colors duration-150 hover:bg-black/[0.04] hover:text-zinc-900 dark:text-zinc-400 dark:hover:bg-white/[0.05] dark:hover:text-zinc-200"
          >
            <GearSix size={18} weight="regular" className="shrink-0" />
            <span className="truncate">Settings</span>
          </motion.button>

          <motion.button
            type="button"
            whileHover={{ x: 2 }}
            whileTap={{ scale: 0.975 }}
            transition={{ type: "spring", stiffness: 520, damping: 34 }}
            className="flex w-full cursor-pointer select-none items-center gap-2.5 rounded-xl px-3 py-2 text-[13.5px] font-medium text-zinc-600 transition-colors duration-150 hover:bg-black/[0.04] hover:text-zinc-900 dark:text-zinc-400 dark:hover:bg-white/[0.05] dark:hover:text-zinc-200"
          >
            <PlusCircle size={18} weight="regular" className="shrink-0" />
            <span className="truncate">Pair new device</span>
          </motion.button>
        </div>
      </aside>

      {/* Right main content area: selected device overview */}
      <main className="relative h-full flex-1 overflow-x-hidden overflow-y-auto bg-white px-8 pt-10 pb-8 dark:bg-zinc-900">
        {/* Top-to-50% Ambient Blurred Album Artwork Gradient for the Entire Right Section */}
        <AnimatePresence>
          {hasArtworkBackdrop && artworkSrc && (
            <motion.div
              key={artworkSrc}
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.5, ease: "easeOut" }}
              style={{
                WebkitMaskImage:
                  "linear-gradient(to bottom, rgba(0,0,0,1) 0%, rgba(0,0,0,0.85) 42%, rgba(0,0,0,0.35) 75%, rgba(0,0,0,0) 100%)",
                maskImage:
                  "linear-gradient(to bottom, rgba(0,0,0,1) 0%, rgba(0,0,0,0.85) 42%, rgba(0,0,0,0.35) 75%, rgba(0,0,0,0) 100%)",
              }}
              className="pointer-events-none absolute inset-x-0 top-0 z-0 h-[54%] overflow-hidden"
            >
              <img
                src={artworkSrc}
                alt=""
                className="h-full w-full scale-125 object-cover opacity-90 brightness-95 blur-2xl saturate-150 dark:opacity-75 dark:brightness-100"
              />
              {/* Subtle top scrim so pure white text always pops even on light album covers */}
              <div className="absolute inset-0 bg-gradient-to-b from-black/45 via-black/20 to-transparent dark:from-black/30 dark:via-transparent" />
            </motion.div>
          )}
        </AnimatePresence>

        {/* Subtle bottom gray blur glow in Dark Theme ONLY when no music is playing */}
        <AnimatePresence>
          {!hasActiveMedia && (
            <motion.div
              key="idle-bottom-blur-blob"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.45, ease: "easeOut" }}
              style={{
                background:
                  "radial-gradient(ellipse 75% 80% at 82% 105%, rgba(113, 113, 122, 0.22) 0%, rgba(82, 82, 91, 0.10) 48%, rgba(24, 24, 27, 0) 100%)",
              }}
              className="pointer-events-none absolute inset-x-0 bottom-0 z-0 hidden h-[55%] dark:block"
            />
          )}
        </AnimatePresence>

        <AnimatePresence mode="wait">
          {selectedDevice && (
            <motion.div
              key={selectedDevice.deviceId}
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -6 }}
              transition={{ type: "spring", stiffness: 460, damping: 34 }}
              className="relative z-10 flex flex-col"
            >
              {/* Top Device Header */}
              <div className="flex items-center justify-between gap-4">
                <div className="flex items-center gap-4">
                  <div
                    className={`flex h-12 w-12 shrink-0 items-center justify-center rounded-2xl transition-colors duration-300 ${
                      hasArtworkBackdrop
                        ? "bg-white/15 text-white ring-1 ring-white/20 backdrop-blur-md"
                        : "bg-blue-500/12 text-blue-600 dark:bg-[#102648] dark:text-[#60a5fa]"
                    }`}
                  >
                    <DeviceIcon type={selectedDevice.deviceType} size={24} />
                  </div>

                  <div className="flex flex-col">
                    <h1
                      className={`text-[18px] font-semibold tracking-tight transition-colors duration-300 ${
                        hasArtworkBackdrop
                          ? "text-white drop-shadow-xs"
                          : "text-zinc-900 dark:text-zinc-100"
                      }`}
                    >
                      {selectedDevice.deviceName}
                    </h1>
                    <p
                      className={`mt-0.5 text-[13.5px] transition-colors duration-300 ${
                        hasArtworkBackdrop
                          ? "text-white/85 drop-shadow-xs"
                          : "text-zinc-500 dark:text-zinc-400"
                      }`}
                    >
                      {formatLastSeen(
                        selectedDevice.reachable,
                        selectedDevice.lastSeenSecs
                      )}
                    </p>
                  </div>
                </div>

                <motion.button
                  type="button"
                  whileHover={{ scale: 1.04 }}
                  whileTap={{ scale: 0.96 }}
                  onClick={() => {
                    setIsShareLinkOpen(false);
                    setIsSettingsOpen((prev) => !prev);
                  }}
                  className={`relative flex h-9 w-11 cursor-pointer items-center justify-center overflow-hidden rounded-xl border transition-colors duration-300 ${
                    hasArtworkBackdrop
                      ? "border-white/25 bg-white/10 text-white backdrop-blur-md hover:bg-white/20"
                      : "border-black/10 bg-black/[0.02] text-zinc-600 hover:bg-black/[0.05] dark:border-white/10 dark:bg-white/[0.02] dark:text-zinc-300 dark:hover:bg-white/[0.06]"
                  }`}
                  title={isSettingsOpen ? "Back to Device" : "Device Settings"}
                >
                  <AnimatePresence mode="wait" initial={false}>
                    {isSettingsOpen ? (
                      <motion.span
                        key="back-arrow-icon"
                        initial={{ opacity: 0, rotate: 45, scale: 0.7 }}
                        animate={{ opacity: 1, rotate: 0, scale: 1 }}
                        exit={{ opacity: 0, rotate: -45, scale: 0.7 }}
                        transition={{
                          type: "spring",
                          stiffness: 520,
                          damping: 30,
                        }}
                        className="flex items-center justify-center"
                      >
                        <ArrowLeft size={18} weight="regular" />
                      </motion.span>
                    ) : (
                      <motion.span
                        key="settings-sliders-icon"
                        initial={{ opacity: 0, scale: 0.7 }}
                        animate={{ opacity: 1, scale: 1 }}
                        exit={{ opacity: 0, scale: 0.7 }}
                        transition={{
                          type: "spring",
                          stiffness: 520,
                          damping: 30,
                        }}
                        className="flex items-center justify-center"
                      >
                        <SlidersHorizontal size={18} weight="regular" />
                      </motion.span>
                    )}
                  </AnimatePresence>
                </motion.button>
              </div>

              {/* Animated view swap between Device Control Page and Device Settings Page */}
              <AnimatePresence mode="wait" initial={false}>
                {isSettingsOpen ? (
                  <motion.div
                    key="device-settings-view"
                    initial={{ opacity: 0, y: 10, filter: "blur(4px)" }}
                    animate={{ opacity: 1, y: 0, filter: "blur(0px)" }}
                    exit={{ opacity: 0, y: -8, filter: "blur(4px)" }}
                    transition={{
                      type: "spring",
                      stiffness: 460,
                      damping: 34,
                    }}
                    className="mt-6 flex flex-col gap-5"
                  >
                    {/* File save location setting */}
                    <div className="flex flex-col gap-2">
                      <label className="text-[12.5px] font-medium text-zinc-500 dark:text-zinc-400">
                        File save location
                      </label>

                      <div className="flex items-center overflow-hidden rounded-xl border border-black/12 bg-black/[0.02] focus-within:border-black/25 dark:border-white/12 dark:bg-white/[0.02] dark:focus-within:border-white/25">
                        <input
                          type="text"
                          value={fileSaveLocation}
                          onChange={(e) => setFileSaveLocation(e.target.value)}
                          readOnly
                          className="min-w-0 flex-1 select-text bg-transparent px-3.5 py-2.5 text-[13px] text-zinc-800 focus:outline-none dark:text-zinc-200"
                        />

                        <motion.button
                          type="button"
                          onClick={handleChooseSaveLocation}
                          whileHover={{ scale: 1.01 }}
                          whileTap={{ scale: 0.98 }}
                          className="shrink-0 cursor-pointer border-l border-black/12 bg-black/[0.04] px-4 py-2.5 text-[13px] font-medium text-zinc-800 transition-colors hover:bg-black/[0.08] dark:border-white/12 dark:bg-white/[0.05] dark:text-zinc-100 dark:hover:bg-white/[0.10]"
                        >
                          Change
                        </motion.button>
                      </div>
                    </div>

                    {/* Sync clipboard toggle */}
                    <div
                      onClick={() => setSyncClipboard((prev) => !prev)}
                      className="flex cursor-pointer items-center justify-between rounded-xl border border-black/12 bg-black/[0.02] px-3.5 py-2.5 transition-colors hover:border-black/20 dark:border-white/12 dark:bg-white/[0.02] dark:hover:border-white/20"
                    >
                      <span className="text-[13.5px] font-medium text-zinc-800 dark:text-zinc-100">
                        Sync clipboard
                      </span>

                      <button
                        type="button"
                        role="switch"
                        aria-checked={syncClipboard}
                        onClick={(e) => {
                          e.stopPropagation();
                          setSyncClipboard((prev) => !prev);
                        }}
                        className={`relative flex h-6 w-11 shrink-0 cursor-pointer items-center rounded-full p-0.5 transition-colors duration-200 ${
                          syncClipboard
                            ? "bg-blue-600 dark:bg-blue-500"
                            : "bg-zinc-300 dark:bg-zinc-700"
                        }`}
                      >
                        <motion.span
                          layout
                          transition={{
                            type: "spring",
                            stiffness: 560,
                            damping: 34,
                          }}
                          animate={{
                            x: syncClipboard ? 20 : 0,
                          }}
                          className="h-5 w-5 rounded-full bg-white shadow-sm"
                        />
                      </button>
                    </div>

                    <motion.button
                      type="button"
                      whileHover={{ scale: 1.008 }}
                      whileTap={{ scale: 0.99 }}
                      transition={{
                        type: "spring",
                        stiffness: 450,
                        damping: 28,
                      }}
                      className="flex w-full cursor-pointer items-center justify-center gap-2 rounded-xl border border-red-500/25 bg-red-500/[0.08] px-4 py-2.5 text-[13.5px] font-medium text-red-600 transition-colors hover:border-red-500/40 hover:bg-red-500/[0.14] dark:border-red-500/25 dark:bg-red-500/[0.10] dark:text-red-400 dark:hover:border-red-400/40 dark:hover:bg-red-500/[0.16]"
                    >
                      <Trash size={16} weight="regular" />
                      <span>Unpair</span>
                    </motion.button>
                  </motion.div>
                ) : (
                  <motion.div
                    key="device-control-view"
                    initial={{ opacity: 0, y: 10, filter: "blur(4px)" }}
                    animate={{ opacity: 1, y: 0, filter: "blur(0px)" }}
                    exit={{ opacity: 0, y: -8, filter: "blur(4px)" }}
                    transition={{
                      type: "spring",
                      stiffness: 460,
                      damping: 34,
                    }}
                    className="flex flex-col"
                  >
              {/* Pairing Banner when device is not yet paired */}
              {selectedDevice.pairState !== "Paired" && (
                <div className="mt-6 flex items-center justify-between gap-4 rounded-2xl border border-amber-500/25 bg-amber-500/[0.08] px-4 py-3 text-sm">
                  <div className="flex flex-col">
                    <span className="font-medium text-amber-600 dark:text-amber-300">
                      {selectedDevice.pairState === "Requested"
                        ? "Pairing request sent — confirm on your device"
                        : selectedDevice.pairState === "RequestedByPeer"
                        ? "Incoming pairing request from this device"
                        : "Device is not paired yet"}
                    </span>
                    {selectedDevice.verificationKey &&
                      (selectedDevice.pairState === "Requested" ||
                        selectedDevice.pairState === "RequestedByPeer") && (
                        <span className="mt-0.5 font-mono text-xs text-amber-700/80 dark:text-amber-200/80">
                          Verification Key: {selectedDevice.verificationKey}
                        </span>
                      )}
                  </div>

                  <div className="flex shrink-0 items-center gap-2">
                    {selectedDevice.pairState === "NotPaired" &&
                      selectedDevice.reachable && (
                        <motion.button
                          type="button"
                          whileHover={{ scale: 1.03 }}
                          whileTap={{ scale: 0.97 }}
                          onClick={() =>
                            invoke("request_pairing", {
                              deviceId: selectedDevice.deviceId,
                            })
                          }
                          className="cursor-pointer rounded-xl bg-blue-600 px-3.5 py-1.5 text-xs font-medium text-white transition-colors hover:bg-blue-500"
                        >
                          Pair Device
                        </motion.button>
                      )}
                    {selectedDevice.pairState === "Requested" && (
                      <motion.button
                        type="button"
                        whileHover={{ scale: 1.03 }}
                        whileTap={{ scale: 0.97 }}
                        onClick={() =>
                          invoke("reject_pairing", {
                            deviceId: selectedDevice.deviceId,
                          })
                        }
                        className="cursor-pointer rounded-xl bg-red-500/20 px-3.5 py-1.5 text-xs font-medium text-red-600 dark:text-red-300"
                      >
                        Cancel
                      </motion.button>
                    )}
                    {selectedDevice.pairState === "RequestedByPeer" && (
                      <>
                        <motion.button
                          type="button"
                          whileHover={{ scale: 1.03 }}
                          whileTap={{ scale: 0.97 }}
                          onClick={() =>
                            invoke("accept_pairing", {
                              deviceId: selectedDevice.deviceId,
                            })
                          }
                          className="cursor-pointer rounded-xl bg-emerald-600 px-3.5 py-1.5 text-xs font-medium text-white"
                        >
                          Accept
                        </motion.button>
                        <motion.button
                          type="button"
                          whileHover={{ scale: 1.03 }}
                          whileTap={{ scale: 0.97 }}
                          onClick={() =>
                            invoke("reject_pairing", {
                              deviceId: selectedDevice.deviceId,
                            })
                          }
                          className="cursor-pointer rounded-xl bg-red-500/20 px-3.5 py-1.5 text-xs font-medium text-red-600 dark:text-red-300"
                        >
                          Reject
                        </motion.button>
                      </>
                    )}
                  </div>
                </div>
              )}

              {/* Conditionally Rendered Now Playing Music Section */}
              <AnimatePresence initial={false}>
                {selectedDevice.reachable &&
                  selectedDevice.nowPlaying &&
                  selectedDevice.nowPlaying.title && (
                    <motion.div
                      key="now-playing-card"
                      initial={{
                        opacity: 0,
                        height: 0,
                        marginTop: 0,
                        y: -6,
                        scale: 0.98,
                      }}
                      animate={{
                        opacity: 1,
                        height: "auto",
                        marginTop: 24,
                        y: 0,
                        scale: 1,
                      }}
                      exit={{
                        opacity: 0,
                        height: 0,
                        marginTop: 0,
                        y: -6,
                        scale: 0.98,
                      }}
                      transition={{
                        height: {
                          type: "spring",
                          stiffness: 380,
                          damping: 34,
                        },
                        marginTop: {
                          type: "spring",
                          stiffness: 380,
                          damping: 34,
                        },
                        opacity: { duration: 0.2, ease: "easeOut" },
                        y: {
                          type: "spring",
                          stiffness: 420,
                          damping: 32,
                        },
                        scale: {
                          type: "spring",
                          stiffness: 420,
                          damping: 32,
                        },
                      }}
                      className="overflow-hidden"
                    >
                      <div className="relative flex flex-col gap-3 overflow-hidden rounded-2xl border border-black/10 bg-white/60 p-4 backdrop-blur-md dark:border-white/10 dark:bg-white/[0.04]">
                      <div className="relative z-10 flex items-center justify-between gap-4">
                        <div className="flex min-w-0 items-center gap-3.5">
                          {/* Crisp Album Artwork Thumbnail (or MusicNotes fallback) */}
                          <div className="relative flex h-12 w-12 shrink-0 items-center justify-center overflow-hidden rounded-xl bg-emerald-500/12 text-emerald-600 shadow-sm ring-1 ring-black/10 dark:bg-emerald-500/15 dark:text-emerald-400 dark:ring-white/15">
                            {artworkSrc ? (
                              <motion.img
                                key={artworkSrc}
                                src={artworkSrc}
                                alt={selectedDevice.nowPlaying.title}
                                initial={{ opacity: 0, scale: 0.92 }}
                                animate={{ opacity: 1, scale: 1 }}
                                transition={{ duration: 0.25 }}
                                className="h-full w-full object-cover"
                              />
                            ) : (
                              <MusicNotes size={20} weight="fill" />
                            )}
                          </div>

                          <div className="flex min-w-0 flex-col">
                            <div className="flex items-center gap-2">
                              <span className="truncate text-[14px] font-semibold text-zinc-900 dark:text-zinc-100">
                                {selectedDevice.nowPlaying.title}
                              </span>
                              {selectedDevice.nowPlaying.player && (
                                <span className="shrink-0 rounded-md bg-black/[0.07] px-1.5 py-0.5 text-[10.5px] font-medium text-zinc-700 backdrop-blur-md dark:bg-white/[0.10] dark:text-zinc-300">
                                  {selectedDevice.nowPlaying.player}
                                </span>
                              )}
                            </div>

                            <span className="mt-0.5 truncate text-[12.5px] text-zinc-600 dark:text-zinc-400">
                              {[
                                selectedDevice.nowPlaying.artist,
                                selectedDevice.nowPlaying.album,
                              ]
                                .filter(Boolean)
                                .join(" — ") || "Playing on device"}
                            </span>
                          </div>
                        </div>

                        {/* Media Playback Controls */}
                        <div className="flex shrink-0 items-center gap-1.5">
                          <motion.button
                            type="button"
                            whileHover={{ scale: 1.08 }}
                            whileTap={{ scale: 0.92 }}
                            disabled={!selectedDevice.nowPlaying.canGoPrevious}
                            onClick={() => handleMediaAction("Previous")}
                            className="flex h-8 w-8 cursor-pointer items-center justify-center rounded-lg text-zinc-700 transition-colors hover:bg-black/[0.08] hover:text-zinc-950 disabled:opacity-40 dark:text-zinc-200 dark:hover:bg-white/[0.10] dark:hover:text-white"
                            title="Previous Track"
                          >
                            <SkipBack size={16} weight="fill" />
                          </motion.button>

                          <motion.button
                            type="button"
                            whileHover={{ scale: 1.08 }}
                            whileTap={{ scale: 0.92 }}
                            onClick={() => handleMediaAction("PlayPause")}
                            className="flex h-9 w-9 cursor-pointer items-center justify-center rounded-xl bg-zinc-900 text-white shadow-sm transition-colors hover:bg-zinc-800 dark:bg-zinc-100 dark:text-zinc-900 dark:hover:bg-white"
                            title={
                              selectedDevice.nowPlaying.isPlaying
                                ? "Pause"
                                : "Play"
                            }
                          >
                            {selectedDevice.nowPlaying.isPlaying ? (
                              <Pause size={16} weight="fill" />
                            ) : (
                              <Play size={16} weight="fill" />
                            )}
                          </motion.button>

                          <motion.button
                            type="button"
                            whileHover={{ scale: 1.08 }}
                            whileTap={{ scale: 0.92 }}
                            disabled={!selectedDevice.nowPlaying.canGoNext}
                            onClick={() => handleMediaAction("Next")}
                            className="flex h-8 w-8 cursor-pointer items-center justify-center rounded-lg text-zinc-700 transition-colors hover:bg-black/[0.08] hover:text-zinc-950 disabled:opacity-40 dark:text-zinc-200 dark:hover:bg-white/[0.10] dark:hover:text-white"
                            title="Next Track"
                          >
                            <SkipForward size={16} weight="fill" />
                          </motion.button>
                        </div>
                      </div>

                      {/* Interactive Seek Progress Bar (shown if duration is reported) */}
                      {selectedDevice.nowPlaying.lengthMs > 0 && (
                        <div className="relative z-10 flex items-center gap-2.5 pt-0.5 text-[11px] font-medium tabular-nums text-zinc-500 dark:text-zinc-400">
                          <span className="w-8 text-left">
                            {formatDurationMs(displaySecs * 1000)}
                          </span>

                          <div
                            ref={seekBarRef}
                            onClick={handleSeekClick}
                            className="group relative flex flex-1 cursor-pointer items-center py-2"
                          >
                            {/* Track background */}
                            <div className="relative h-1 w-full overflow-hidden rounded-full bg-black/[0.10] dark:bg-white/[0.12]">
                              {/* Animated progress fill driven directly by progressPctStyle MotionValue */}
                              <motion.div
                                style={{ width: progressPctStyle }}
                                className="h-full rounded-full bg-zinc-800 dark:bg-zinc-200"
                              />
                            </div>

                            {/* Simple dot knob on hover driven by the exact same MotionValue */}
                            <motion.div
                              style={{ left: progressPctStyle }}
                              className="pointer-events-none absolute top-1/2 h-2.5 w-2.5 -translate-x-1/2 -translate-y-1/2 rounded-full bg-zinc-800 opacity-0 transition-opacity duration-150 group-hover:opacity-100 dark:bg-zinc-200"
                            />
                          </div>

                          <span className="w-8 text-right">
                            {formatDurationMs(
                              selectedDevice.nowPlaying.lengthMs
                            )}
                          </span>
                        </div>
                      )}
                      </div>
                    </motion.div>
                  )}
              </AnimatePresence>

              {/* Sleek File Drop Zone */}
              <motion.div
                ref={dropZoneRef}
                onClick={
                  selectedDevice.reachable ? handlePickFilesToSend : undefined
                }
                animate={{
                  scale:
                    selectedDevice.reachable && isDraggingOver ? 1.015 : 1,
                }}
                whileHover={
                  selectedDevice.reachable ? { scale: 1.005 } : undefined
                }
                whileTap={
                  selectedDevice.reachable ? { scale: 0.99 } : undefined
                }
                transition={{
                  type: "spring",
                  stiffness: 420,
                  damping: 30,
                }}
                className={`group relative mt-6 flex flex-col items-center justify-center overflow-hidden rounded-2xl border-2 border-dotted px-6 py-8 text-center backdrop-blur-md transition-all duration-200 ${
                  !selectedDevice.reachable
                    ? "pointer-events-none cursor-not-allowed border-black/10 bg-zinc-100/40 opacity-45 grayscale dark:border-white/10 dark:bg-white/[0.015]"
                    : isDraggingOver
                      ? "cursor-pointer border-blue-500 bg-blue-500/[0.08] dark:border-blue-400 dark:bg-blue-500/[0.12]"
                      : "cursor-pointer border-black/20 bg-white/50 hover:border-black/35 hover:bg-white/70 dark:border-white/20 dark:bg-white/[0.03] dark:hover:border-white/35 dark:hover:bg-white/[0.05]"
                }`}
              >
                <AnimatePresence mode="wait">
                  {justDroppedCount > 0 ? (
                    <motion.div
                      key="dropped-feedback"
                      initial={{ opacity: 0, y: 8, scale: 0.92 }}
                      animate={{ opacity: 1, y: 0, scale: 1 }}
                      exit={{ opacity: 0, y: -6, scale: 0.95 }}
                      transition={{
                        type: "spring",
                        stiffness: 500,
                        damping: 30,
                      }}
                      className="flex flex-col items-center gap-2"
                    >
                      <div className="flex h-11 w-11 items-center justify-center rounded-2xl bg-emerald-500/15 text-emerald-600 dark:bg-emerald-500/20 dark:text-emerald-400">
                        <CheckCircle size={24} weight="fill" />
                      </div>
                      <span className="text-[14px] font-semibold text-zinc-900 dark:text-zinc-100">
                        Sending {justDroppedCount}{" "}
                        {justDroppedCount === 1 ? "file" : "files"} to{" "}
                        {selectedDevice.deviceName}...
                      </span>
                    </motion.div>
                  ) : (
                    <motion.div
                      key="drop-prompt"
                      initial={{ opacity: 0, y: 6 }}
                      animate={{ opacity: 1, y: 0 }}
                      exit={{ opacity: 0, y: -6 }}
                      transition={{
                        type: "spring",
                        stiffness: 480,
                        damping: 32,
                      }}
                      className="flex flex-col items-center"
                    >
                      <motion.div
                        animate={{
                          y: isDraggingOver ? -5 : 0,
                          scale: isDraggingOver ? 1.12 : 1,
                        }}
                        transition={{
                          type: "spring",
                          stiffness: 450,
                          damping: 22,
                        }}
                        className={`flex h-11 w-11 items-center justify-center rounded-2xl transition-colors duration-200 ${
                          isDraggingOver
                            ? "bg-blue-500 text-white shadow-md shadow-blue-500/25"
                            : "bg-black/[0.06] text-zinc-700 group-hover:bg-black/[0.10] dark:bg-white/[0.08] dark:text-zinc-200 dark:group-hover:bg-white/[0.12]"
                        }`}
                      >
                        <UploadSimple size={22} weight="bold" />
                      </motion.div>

                      <span className="mt-3 text-[14px] font-semibold text-zinc-900 dark:text-zinc-100">
                        {isDraggingOver
                          ? `Drop to send to ${selectedDevice.deviceName}`
                          : `Drop files to send to ${selectedDevice.deviceName}`}
                      </span>
                      <span className="mt-1 text-[12.5px] text-zinc-500 dark:text-zinc-400">
                        or click to browse from your Mac
                      </span>
                    </motion.div>
                  )}
                </AnimatePresence>

                {/* Active file transfer progress bar inside the drop card if a transfer is currently running */}
                {(() => {
                  const activeTransfer = transfers.find(
                    (t) =>
                      t.deviceId === selectedDevice.deviceId &&
                      t.status === "InProgress" &&
                      t.kind === "File"
                  );
                  if (!activeTransfer) return null;
                  const pct =
                    activeTransfer.totalBytes > 0
                      ? Math.min(
                          100,
                          Math.round(
                            (activeTransfer.transferredBytes /
                              activeTransfer.totalBytes) *
                              100
                          )
                        )
                      : 50;
                  return (
                    <div className="mt-4 flex w-full max-w-sm flex-col gap-1.5">
                      <div className="flex items-center justify-between text-[11.5px] font-medium text-zinc-600 dark:text-zinc-300">
                        <span className="truncate">
                          {activeTransfer.direction === "Outgoing"
                            ? "Sending"
                            : "Receiving"}{" "}
                          {activeTransfer.filename}
                        </span>
                        <span className="tabular-nums">{pct}%</span>
                      </div>
                      <div className="h-1.5 w-full overflow-hidden rounded-full bg-black/10 dark:bg-white/10">
                        <motion.div
                          className="h-full rounded-full bg-blue-500"
                          animate={{ width: `${pct}%` }}
                          transition={{ ease: "easeOut", duration: 0.2 }}
                        />
                      </div>
                    </div>
                  );
                })()}
              </motion.div>

              {/* Quick actions */}
              <div
                className={`mt-6 flex flex-col gap-2.5 transition-opacity duration-200 ${
                  !selectedDevice.reachable
                    ? "pointer-events-none opacity-45 grayscale"
                    : ""
                }`}
              >
                <span className="text-[12.5px] font-medium text-zinc-500 dark:text-zinc-400">
                  Quick actions
                </span>

                <div className="grid grid-cols-3 gap-3">
                  <Popover.Root
                    open={selectedDevice.reachable && isShareLinkOpen}
                    onOpenChange={(open) => {
                      setIsShareLinkOpen(open);
                      if (!open) {
                        setShareLinkTouchedInvalid(false);
                      }
                    }}
                  >
                    <Popover.Trigger asChild>
                      <motion.button
                        type="button"
                        whileHover={
                          selectedDevice.reachable ? { scale: 1.015 } : undefined
                        }
                        whileTap={
                          selectedDevice.reachable ? { scale: 0.985 } : undefined
                        }
                        transition={{
                          type: "spring",
                          stiffness: 450,
                          damping: 28,
                        }}
                        className="flex cursor-pointer items-center justify-center gap-2 rounded-xl border border-black/12 bg-black/[0.02] px-4 py-2.5 text-[13.5px] font-medium text-zinc-800 transition-colors hover:border-black/20 hover:bg-black/[0.05] data-[state=open]:border-black/25 data-[state=open]:bg-black/[0.06] dark:border-white/12 dark:bg-white/[0.02] dark:text-zinc-100 dark:hover:border-white/20 dark:hover:bg-white/[0.05] dark:data-[state=open]:border-white/25 dark:data-[state=open]:bg-white/[0.07]"
                      >
                        <Link size={16} weight="regular" />
                        <span>Share link</span>
                      </motion.button>
                    </Popover.Trigger>

                    <AnimatePresence>
                      {selectedDevice.reachable && isShareLinkOpen && (
                        <Popover.Portal forceMount>
                          <Popover.Content
                            asChild
                            forceMount
                            side="top"
                            align="start"
                            sideOffset={8}
                          >
                            <motion.div
                              initial={{ opacity: 0, y: 6, scale: 0.95 }}
                              animate={{ opacity: 1, y: 0, scale: 1 }}
                              exit={{ opacity: 0, y: 4, scale: 0.96 }}
                              transition={{
                                type: "spring",
                                stiffness: 480,
                                damping: 32,
                              }}
                              style={{
                                transformOrigin:
                                  "var(--radix-popover-content-transform-origin)",
                              }}
                              className="z-50 flex w-72 flex-col gap-2 rounded-2xl border border-black/12 bg-white/95 p-3 shadow-xl backdrop-blur-xl outline-none dark:border-white/15 dark:bg-zinc-800/95"
                            >
                              <span className="text-xs font-medium text-zinc-500 dark:text-zinc-400">
                                Paste your link here
                              </span>

                              <div
                                className={`flex items-center gap-1.5 rounded-xl border bg-black/[0.03] p-1 transition-colors dark:bg-zinc-900/80 ${
                                  isShareLinkInvalid
                                    ? "border-red-500/80 focus-within:border-red-500 dark:border-red-400/80 dark:focus-within:border-red-400"
                                    : "border-black/12 focus-within:border-blue-500/60 dark:border-white/12 dark:focus-within:border-blue-400/60"
                                }`}
                              >
                                <input
                                  type="url"
                                  value={shareLinkInput}
                                  onChange={(e) => {
                                    setShareLinkInput(e.target.value);
                                    if (shareLinkTouchedInvalid) {
                                      setShareLinkTouchedInvalid(false);
                                    }
                                  }}
                                  onKeyDown={(e) => {
                                    if (e.key === "Enter") {
                                      e.preventDefault();
                                      handleSendShareLink();
                                    }
                                  }}
                                  placeholder="https://example.com"
                                  autoFocus
                                  className="min-w-0 flex-1 select-text bg-transparent px-2.5 py-1 text-[13px] text-zinc-900 placeholder:text-zinc-400 focus:outline-none dark:text-zinc-100 dark:placeholder:text-zinc-500"
                                />
                                <motion.button
                                  type="button"
                                  onClick={handleSendShareLink}
                                  whileHover={{ scale: 1.05 }}
                                  whileTap={{ scale: 0.94 }}
                                  className="flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded-lg bg-blue-600 text-white shadow-xs transition-colors hover:bg-blue-500"
                                  title="Send link"
                                >
                                  <PaperPlaneRight size={14} weight="fill" />
                                </motion.button>
                              </div>
                            </motion.div>
                          </Popover.Content>
                        </Popover.Portal>
                      )}
                    </AnimatePresence>
                  </Popover.Root>
                </div>
              </div>
                  </motion.div>
                )}
              </AnimatePresence>
            </motion.div>
          )}
        </AnimatePresence>
      </main>
    </div>
  );
}
