import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AnimatePresence, motion } from "motion/react";
import { type MouseEvent, useEffect, useRef, useState } from "react";
import { PingToast } from "./components/PingToast";
import { Sidebar } from "./components/Sidebar";
import { AppSettingsPage } from "./pages/AppSettingsPage";
import { DevicePage } from "./pages/DevicePage";
import { PairDevicePage } from "./pages/PairDevicePage";
import type {
  ActiveView,
  AppSnapshot,
  DeviceView,
  PingToastData,
  TransferItem,
} from "./types";
import { itunesArtCache } from "./utils/format";

export default function App() {
  const [devices, setDevices] = useState<DeviceView[]>([]);
  const [transfers, setTransfers] = useState<TransferItem[]>([]);
  const [activeView, setActiveView] = useState<ActiveView>({
    kind: "device",
    deviceId: "",
    subview: "control",
  });
  const [isDraggingOver, setIsDraggingOver] = useState<boolean>(false);
  const [justDroppedCount, setJustDroppedCount] = useState<number>(0);
  const [pingToast, setPingToast] = useState<PingToastData | null>(null);
  const [fallbackArtworkUrl, setFallbackArtworkUrl] = useState<string | null>(
    null
  );
  const [fileSaveLocation, setFileSaveLocation] = useState<string>("");

  const dropZoneRef = useRef<HTMLDivElement | null>(null);
  const selectedDeviceRef = useRef<DeviceView | null>(null);
  const activeViewRef = useRef<ActiveView>(activeView);
  activeViewRef.current = activeView;
  const prevPairedIdsRef = useRef<Set<string>>(new Set());

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

        const paired = snap.devices.filter((d) => d.pairState === "Paired");
        const newlyPaired = paired.find(
          (d) => !prevPairedIdsRef.current.has(d.deviceId)
        );
        prevPairedIdsRef.current = new Set(paired.map((d) => d.deviceId));

        setActiveView((prev) => {
          // If the user is on the Pair New Device page and a device just finished pairing, open it immediately!
          if (prev.kind === "pair-device" && newlyPaired) {
            return {
              kind: "device",
              deviceId: newlyPaired.deviceId,
              subview: "control",
            };
          }
          if (prev.kind !== "device") {
            return prev;
          }
          if (
            prev.deviceId &&
            paired.some((d) => d.deviceId === prev.deviceId)
          ) {
            return prev;
          }
          if (paired.length > 0) {
            return {
              kind: "device",
              deviceId: paired[0].deviceId,
              subview: "control",
            };
          }
          return { kind: "pair-device" };
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

  const pairedDevices = devices.filter((d) => d.pairState === "Paired");
  const unpairedDevices = devices.filter(
    (d) => d.pairState !== "Paired" && d.reachable
  );

  const selectedDevice =
    activeView.kind === "device"
      ? (pairedDevices.find((d) => d.deviceId === activeView.deviceId) ??
        pairedDevices[0] ??
        null)
      : null;
  selectedDeviceRef.current = selectedDevice;

  // Native Tauri drag-and-drop file listener scoped strictly to the drop zone bounding rect
  useEffect(() => {
    let unlistenDrop: (() => void) | undefined;

    const isInsideDropZone = (pos: { x: number; y: number }): boolean => {
      const currentView = activeViewRef.current;
      if (currentView.kind !== "device" || currentView.subview !== "control") {
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

  const nowPlaying = selectedDevice?.nowPlaying ?? null;
  const nativeArtData = nowPlaying?.albumArtData ?? null;
  const trackTitle = nowPlaying?.title ?? "";
  const trackArtist = nowPlaying?.artist ?? "";
  const trackAlbum = nowPlaying?.album ?? "";

  // Fallback to iTunes Search API if KDE Connect hasn't provided native album art
  useEffect(() => {
    if (!trackTitle) {
      setFallbackArtworkUrl(null);
      return;
    }

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
    activeView.kind === "device" &&
      activeView.subview === "control" &&
      selectedDevice?.reachable &&
      selectedDevice?.nowPlaying?.title
  );
  const hasArtworkBackdrop = Boolean(hasActiveMedia && artworkSrc);

  const handleDragMouseDown = (e: MouseEvent<HTMLDivElement>) => {
    if (e.buttons === 1) {
      getCurrentWindow().startDragging();
    }
  };

  const handleFilesPicked = (count: number) => {
    setJustDroppedCount(count);
    setTimeout(() => setJustDroppedCount(0), 2200);
  };

  const handleUnpairDevice = async (deviceId: string) => {
    try {
      await invoke("unpair_device", { deviceId });
      const remaining = pairedDevices.filter((d) => d.deviceId !== deviceId);
      if (remaining.length > 0) {
        setActiveView({
          kind: "device",
          deviceId: remaining[0].deviceId,
          subview: "control",
        });
      } else {
        setActiveView({ kind: "pair-device" });
      }
    } catch (err) {
      console.error("Failed to unpair device:", err);
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
      <PingToast pingToast={pingToast} onDismiss={() => setPingToast(null)} />

      {/* Left transparent/vibrant sidebar */}
      <Sidebar
        devices={pairedDevices}
        activeView={activeView}
        onSelectDevice={(deviceId) =>
          setActiveView({
            kind: "device",
            deviceId,
            subview: "control",
          })
        }
        onSelectAppSettings={() => setActiveView({ kind: "app-settings" })}
        onSelectPairDevice={() => setActiveView({ kind: "pair-device" })}
      />

      {/* Right main content area */}
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
          {activeView.kind === "device" && selectedDevice && (
            <DevicePage
              key={`device-${selectedDevice.deviceId}`}
              device={selectedDevice}
              transfers={transfers}
              subview={activeView.subview}
              onToggleSubview={() =>
                setActiveView((prev) =>
                  prev.kind === "device"
                    ? {
                        ...prev,
                        subview:
                          prev.subview === "control" ? "settings" : "control",
                      }
                    : prev
                )
              }
              hasArtworkBackdrop={hasArtworkBackdrop}
              artworkSrc={artworkSrc}
              dropZoneRef={dropZoneRef}
              isDraggingOver={isDraggingOver}
              justDroppedCount={justDroppedCount}
              onFilesPicked={handleFilesPicked}
              fileSaveLocation={fileSaveLocation}
              onChangeFileSaveLocation={setFileSaveLocation}
              syncClipboard={selectedDevice.syncClipboard}
              onToggleSyncClipboard={() =>
                invoke("set_device_clipboard_sync", {
                  deviceId: selectedDevice.deviceId,
                  enabled: !selectedDevice.syncClipboard,
                })
              }
              onUnpair={() => handleUnpairDevice(selectedDevice.deviceId)}
            />
          )}

          {activeView.kind === "app-settings" && (
            <AppSettingsPage key="app-settings" />
          )}

          {activeView.kind === "pair-device" && (
            <PairDevicePage
              key="pair-device"
              unpairedDevices={unpairedDevices}
            />
          )}
        </AnimatePresence>
      </main>
    </div>
  );
}
