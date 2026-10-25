import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Sidebar } from "./components/Sidebar";
import { DevicePage } from "./pages/DevicePage";
import { itunesArtCache } from "./utils/format";
import { motion, AnimatePresence } from "motion/react";
import { PairDevicePage } from "./pages/PairDevicePage";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AppSettingsPage } from "./pages/AppSettingsPage";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type {
    ActiveView,
    DeviceView,
    AppSnapshot,
    TransferItem,
} from "./types";
import {
    useRef,
    useState,
    useEffect,
    useCallback,
    type MouseEvent,
} from "react";

export default function App() {
    const [devices, setDevices] = useState<DeviceView[]>([]);
    const [transfers, setTransfers] = useState<TransferItem[]>([]);
    const [activeView, setActiveView] = useState<ActiveView>({
        deviceId: "",
        kind: "device",
        subview: "control",
    });
    const [isDraggingOver, setIsDraggingOver] = useState<boolean>(false);
    const [justDroppedCount, setJustDroppedCount] = useState<number>(0);
    const [fallbackArtworkUrl, setFallbackArtworkUrl] = useState<string | null>(
        null,
    );
    const [ignoreWhatsappConversations, setIgnoreWhatsappConversations] =
        useState<boolean>(true);
    const [rightPaneEl, setRightPaneEl] = useState<HTMLDivElement | null>(null);
    const [openAddCommandSignal, setOpenAddCommandSignal] = useState<number>(0);
    const [scrollToAboutSignal, setScrollToAboutSignal] = useState<number>(0);
    const [highlightedNotification, setHighlightedNotification] = useState<{
        deviceId: string;
        notificationId: string;
        seq: number;
    } | null>(null);

    const dropZoneRef = useRef<HTMLDivElement | null>(null);
    const selectedDeviceRef = useRef<DeviceView | null>(null);
    const activeViewRef = useRef<ActiveView>(activeView);
    const prevPairedIdsRef = useRef<Set<string>>(new Set());

    const runUpdateCheck = useCallback(
        async (
            force: boolean,
        ): Promise<"update-found" | "up-to-date" | "error"> => {
            try {
                const metadata = await invoke<{
                    body?: string;
                    currentVersion: string;
                    date?: string;
                    rawJson: Record<string, unknown>;
                    rid: number;
                    version: string;
                } | null>("check_for_update", { timeout: 20000 });
                if (!metadata) {
                    return "up-to-date";
                }

                const skippedVersion = localStorage.getItem(
                    "velquo.skippedUpdateVersion",
                );
                if (!force && skippedVersion == metadata.version) {
                    return "up-to-date";
                }

                await invoke("open_updater_window");
                return "update-found";
            } catch (err) {
                console.debug("[updater] Update check finished:", err);
                return "error";
            }
        },
        [],
    );

    // Automatic background update check shortly after launch and every 8 hours
    useEffect(() => {
        const EIGHT_HOURS_MS = 8 * 60 * 60 * 1000;
        const initialTimer = setTimeout(() => {
            runUpdateCheck(false);
        }, 10000);

        const intervalTimer = setInterval(() => {
            runUpdateCheck(false);
        }, EIGHT_HOURS_MS);

        return () => {
            clearTimeout(initialTimer);
            clearInterval(intervalTimer);
        };
    }, [runUpdateCheck]);

    useEffect(() => {
        let disposed = false;
        let isFetching = false;
        let hasPendingFetch = false;
        let unlisten: (() => void) | undefined;

        async function loadDevices() {
            if (isFetching) {
                hasPendingFetch = true;
                return;
            }
            isFetching = true;

            try {
                do {
                    hasPendingFetch = false;
                    const snap = await invoke<AppSnapshot>("get_app_snapshot");
                    if (disposed) return;
                    setDevices(snap.devices);
                    setTransfers(snap.transfers || []);
                    if (typeof snap.host?.ignoreWhatsappConversations == "boolean") {
                        setIgnoreWhatsappConversations(snap.host.ignoreWhatsappConversations);
                    }

                    const paired = snap.devices.filter((d) => d.pairState == "Paired");
                    const newlyPaired = paired.find(
                        (d) => !prevPairedIdsRef.current.has(d.deviceId),
                    );
                    prevPairedIdsRef.current = new Set(paired.map((d) => d.deviceId));

                    setActiveView((prev) => {
                        // If the user is on the Pair New Device page and a device just finished pairing, open it immediately!
                        if (prev.kind == "pair-device" && newlyPaired) {
                            return {
                                kind: "device",
                                subview: "control",
                                deviceId: newlyPaired.deviceId,
                            };
                        }
                        if (prev.kind != "device") {
                            return prev;
                        }
                        if (
                            prev.deviceId &&
                            paired.some((d) => d.deviceId == prev.deviceId)
                        ) {
                            return prev;
                        }
                        if (paired.length > 0) {
                            return {
                                kind: "device",
                                subview: "control",
                                deviceId: paired[0].deviceId,
                            };
                        }
                        return { kind: "pair-device" };
                    });
                } while (hasPendingFetch && !disposed);
            } catch (err) {
                console.error("Failed to load devices:", err);
            } finally {
                isFetching = false;
            }
        }

        loadDevices();

        listen("velquo://state-updated", () => {
            loadDevices();
        }).then((fn) => {
            if (disposed) {
                fn();
            } else {
                unlisten = fn;
            }
        });

        let unlistenSetup: (() => void) | undefined;
        listen<{ deviceId: string }>("velquo://open-runcommand-setup", (event) => {
            if (disposed) return;
            setActiveView({
                kind: "device",
                subview: "settings",
                deviceId: event.payload.deviceId,
            });
            setOpenAddCommandSignal((prev) => prev + 1);
        }).then((fn) => {
            if (disposed) {
                fn();
            } else {
                unlistenSetup = fn;
            }
        });

        let unlistenTrayNav: (() => void) | undefined;
        listen<{ view: string; deviceId?: string }>(
            "velquo://tray-navigate",
            (event) => {
                if (disposed) return;
                if (event.payload.view == "pair-device") {
                    setActiveView({ kind: "pair-device" });
                } else if (event.payload.view == "about") {
                    setActiveView({ kind: "app-settings" });
                    setScrollToAboutSignal((prev) => prev + 1);
                } else if (event.payload.view == "device" && event.payload.deviceId) {
                    setActiveView({
                        kind: "device",
                        subview: "control",
                        deviceId: event.payload.deviceId,
                    });
                }
            },
        ).then((fn) => {
            if (disposed) {
                fn();
            } else {
                unlistenTrayNav = fn;
            }
        });

        let unlistenFocusNotif: (() => void) | undefined;
        listen<{ deviceId: string; notificationId?: string | null }>(
            "velquo://focus-notification",
            (event) => {
                if (disposed) return;
                const { deviceId, notificationId } = event.payload;
                if (!deviceId) return;
                setActiveView({
                    deviceId,
                    kind: "device",
                    subview: "control",
                });
                if (notificationId) {
                    setHighlightedNotification((prev) => ({
                        deviceId,
                        notificationId,
                        seq: (prev?.seq ?? 0) + 1,
                    }));
                }
            },
        ).then((fn) => {
            if (disposed) {
                fn();
            } else {
                unlistenFocusNotif = fn;
            }
        });

        return () => {
            disposed = true;
            if (unlisten) unlisten();
            if (unlistenSetup) unlistenSetup();
            if (unlistenTrayNav) unlistenTrayNav();
            if (unlistenFocusNotif) unlistenFocusNotif();
        };
    }, []);

    const pairedDevices = devices.filter((d) => d.pairState == "Paired");
    const unpairedDevices = devices.filter(
        (d) => d.pairState != "Paired" && d.reachable,
    );

    const selectedDevice =
        activeView.kind == "device"
            ? (pairedDevices.find((d) => d.deviceId == activeView.deviceId) ??
        pairedDevices[0] ??
        null)
            : null;

    useEffect(() => {
        activeViewRef.current = activeView;
        selectedDeviceRef.current = selectedDevice;
    }, [activeView, selectedDevice]);

    // Native Tauri drag-and-drop file listener scoped strictly to the drop zone bounding rect
    useEffect(() => {
        let disposed = false;
        let unlistenDrop: (() => void) | undefined;
        let lastDropAtMs = 0;

        const isInsideDropZone = (pos: { x: number; y: number }): boolean => {
            const currentView = activeViewRef.current;
            if (currentView.kind != "device" || currentView.subview != "control") {
                return false;
            }
            const targetDevice = selectedDeviceRef.current;
            if (
                !targetDevice ||
        !targetDevice.reachable ||
        targetDevice.pairState != "Paired"
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
                if (disposed) return;
                if (event.payload.type == "enter" || event.payload.type == "over") {
                    setIsDraggingOver(isInsideDropZone(event.payload.position));
                } else if (event.payload.type == "leave") {
                    setIsDraggingOver(false);
                } else if (event.payload.type == "drop") {
                    const droppedInside = isInsideDropZone(event.payload.position);
                    setIsDraggingOver(false);
                    if (!droppedInside) {
                        return;
                    }
                    const now = Date.now();
                    if (now - lastDropAtMs < 250) {
                        return;
                    }
                    lastDropAtMs = now;

                    const targetDevice = selectedDeviceRef.current;
                    if (
                        targetDevice &&
            targetDevice.reachable &&
            targetDevice.pairState == "Paired" &&
            event.payload.paths.length > 0
                    ) {
                        setJustDroppedCount(event.payload.paths.length);
                        setTimeout(() => setJustDroppedCount(0), 2200);
                        try {
                            await invoke("send_files", {
                                filePaths: event.payload.paths,
                                deviceId: targetDevice.deviceId,
                            });
                        } catch (err) {
                            console.error("Failed to send dropped files:", err);
                        }
                    }
                }
            })
            .then((fn) => {
                if (disposed) {
                    fn();
                } else {
                    unlistenDrop = fn;
                }
            });

        return () => {
            disposed = true;
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
            itunesArtCache.clear();
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
                        searchTerm,
                    )}&entity=song&limit=1`,
                );
                if (!resp.ok) return;
                const data = await resp.json();
                const rawUrl: string | undefined = data?.results?.[0]?.artworkUrl100;
                const hiResUrl = rawUrl
                    ? rawUrl.replace("100x100bb", "300x300bb")
                    : null;
                itunesArtCache.clear();
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
        activeView.kind == "device" &&
    activeView.subview == "control" &&
    selectedDevice?.reachable &&
    selectedDevice?.nowPlaying?.title,
    );
    const hasArtworkBackdrop = Boolean(hasActiveMedia && artworkSrc);

    const handleDragMouseDown = (e: MouseEvent<HTMLDivElement>) => {
        if (e.buttons == 1) {
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
            const remaining = pairedDevices.filter((d) => d.deviceId != deviceId);
            if (remaining.length > 0) {
                setActiveView({
                    kind: "device",
                    subview: "control",
                    deviceId: remaining[0].deviceId,
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

            {/* Left transparent/vibrant sidebar */}
            <Sidebar
                devices={pairedDevices}
                activeView={activeView}
                onSelectDevice={(deviceId) =>
                    setActiveView({
                        deviceId,
                        kind: "device",
                        subview: "control",
                    })
                }
                onSelectAppSettings={() => setActiveView({ kind: "app-settings" })}
                onSelectPairDevice={() => setActiveView({ kind: "pair-device" })}
            />

            {/* Right main content area */}
            <div
                ref={setRightPaneEl}
                className="relative h-full flex-1 overflow-hidden bg-white dark:bg-zinc-900"
            >
                {/* Subtle bottom gray blur glow in Dark Theme ONLY when no music is playing (anchored to viewport bottom so scrolling never clips it) */}
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

                <main className="relative z-10 h-full w-full overscroll-none overflow-x-hidden overflow-y-auto px-8 pt-10 pb-8">
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
                                    maskImage:
                    "linear-gradient(to bottom, rgba(0,0,0,1) 0%, rgba(0,0,0,0.85) 42%, rgba(0,0,0,0.35) 75%, rgba(0,0,0,0) 100%)",
                                    WebkitMaskImage:
                    "linear-gradient(to bottom, rgba(0,0,0,1) 0%, rgba(0,0,0,0.85) 42%, rgba(0,0,0,0.35) 75%, rgba(0,0,0,0) 100%)",
                                }}
                                className="pointer-events-none absolute inset-x-0 top-0 z-0 h-[300px] overflow-hidden"
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

                    <AnimatePresence mode="wait">
                        {activeView.kind == "device" && selectedDevice && (
                            <DevicePage
                                key={`device-${selectedDevice.deviceId}`}
                                device={selectedDevice}
                                transfers={transfers}
                                subview={activeView.subview}
                                onToggleSubview={() =>
                                    setActiveView((prev) =>
                                        prev.kind == "device"
                                            ? {
                                                ...prev,
                                                subview:
                            prev.subview == "control" ? "settings" : "control",
                                            }
                                            : prev,
                                    )
                                }
                                hasArtworkBackdrop={hasArtworkBackdrop}
                                artworkSrc={artworkSrc}
                                dropZoneRef={dropZoneRef}
                                isDraggingOver={isDraggingOver}
                                justDroppedCount={justDroppedCount}
                                onFilesPicked={handleFilesPicked}
                                fileSaveLocation={selectedDevice.saveLocation}
                                onChangeFileSaveLocation={(nextPath) =>
                                    invoke("set_device_save_location", {
                                        saveLocation: nextPath,
                                        deviceId: selectedDevice.deviceId,
                                    })
                                }
                                syncClipboard={selectedDevice.syncClipboard}
                                onToggleSyncClipboard={() =>
                                    invoke("set_device_clipboard_sync", {
                                        deviceId: selectedDevice.deviceId,
                                        enabled: !selectedDevice.syncClipboard,
                                    })
                                }
                                allowPresenter={selectedDevice.allowPresenter}
                                onTogglePresenter={() =>
                                    invoke("set_device_presenter", {
                                        deviceId: selectedDevice.deviceId,
                                        enabled: !selectedDevice.allowPresenter,
                                    })
                                }
                                syncNotifications={selectedDevice.syncNotifications}
                                onToggleNotifications={() =>
                                    invoke("set_device_notifications_sync", {
                                        deviceId: selectedDevice.deviceId,
                                        enabled: !selectedDevice.syncNotifications,
                                    })
                                }
                                autoCopyOtp={selectedDevice.autoCopyOtp}
                                onToggleAutoCopyOtp={() =>
                                    invoke("set_device_auto_copy_otp", {
                                        deviceId: selectedDevice.deviceId,
                                        enabled: !selectedDevice.autoCopyOtp,
                                    })
                                }
                                onAddRunCommand={async (name, command) => {
                                    await invoke("add_device_run_command", {
                                        name,
                                        command,
                                        deviceId: selectedDevice.deviceId,
                                    });
                                }}
                                onRemoveRunCommand={async (key) => {
                                    await invoke("remove_device_run_command", {
                                        key,
                                        deviceId: selectedDevice.deviceId,
                                    });
                                }}
                                rightPaneContainer={rightPaneEl}
                                openAddCommandSignal={openAddCommandSignal}
                                highlightedNotification={
                                    highlightedNotification?.deviceId == selectedDevice.deviceId
                                        ? highlightedNotification
                                        : null
                                }
                                onUnpair={() => handleUnpairDevice(selectedDevice.deviceId)}
                            />
                        )}

                        {activeView.kind == "app-settings" && (
                            <AppSettingsPage
                                key="app-settings"
                                ignoreWhatsappConversations={ignoreWhatsappConversations}
                                onToggleIgnoreWhatsappConversations={() =>
                                    invoke("set_ignore_whatsapp_conversations", {
                                        enabled: !ignoreWhatsappConversations,
                                    })
                                }
                                onCheckForUpdates={() => runUpdateCheck(true)}
                                scrollToAboutSignal={scrollToAboutSignal}
                            />
                        )}

                        {activeView.kind == "pair-device" && (
                            <PairDevicePage
                                key="pair-device"
                                unpairedDevices={unpairedDevices}
                            />
                        )}
                    </AnimatePresence>
                </main>
            </div>
        </div>
    );
}
