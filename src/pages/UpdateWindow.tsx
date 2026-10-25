import pkg from "../../package.json";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import { Update } from "@tauri-apps/plugin-updater";
import { motion, AnimatePresence } from "motion/react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
    useRef,
    useState,
    useEffect,
    useCallback,
    type MouseEvent,
} from "react";
import {
    SparkleIcon,
    CheckCircleIcon,
    CircleNotchIcon,
    CloudArrowDownIcon,
    ArrowClockwiseIcon,
} from "@phosphor-icons/react";

type UpdateStep = "prompt" | "downloading" | "ready" | "error";

interface UpdateMetadataDto {
    body?: string;
    currentVersion: string;
    date?: string;
    rawJson: Record<string, unknown>;
    rid: number;
    version: string;
}

export function UpdateWindow() {
    const [update, setUpdate] = useState<Update | null>(null);
    const [appVersion, setAppVersion] = useState<string>(pkg.version);
    const [isLoadingUpdate, setIsLoadingUpdate] = useState<boolean>(true);
    const [step, setStep] = useState<UpdateStep>("prompt");
    const [progressPct, setProgressPct] = useState<number>(0);
    const [errorMessage, setErrorMessage] = useState<string | null>(null);
    const [releaseNotes, setReleaseNotes] = useState<string | null>(null);
    const [isFetchingNotes, setIsFetchingNotes] = useState<boolean>(false);

    useEffect(() => {
        getVersion()
            .then((v) => {
                if (v) setAppVersion(v);
            })
            .catch(() => {});
    }, []);

    const stepRef = useRef<UpdateStep>(step);
    useEffect(() => {
        stepRef.current = step;
    }, [step]);

    const loadPendingUpdate = useCallback(async () => {
        setIsLoadingUpdate(true);
        try {
            const metadata = await invoke<UpdateMetadataDto | null>(
                "get_pending_update",
            );
            setUpdate(metadata ? new Update(metadata) : null);
        } catch (err) {
            console.error("Failed to load pending update:", err);
            setUpdate(null);
        } finally {
            setIsLoadingUpdate(false);
        }
    }, []);

    useEffect(() => {
        loadPendingUpdate();
        const unlistenPromise = listen("velquo://update-available", () => {
            if (stepRef.current != "downloading") {
                loadPendingUpdate();
            }
        });
        return () => {
            unlistenPromise.then((fn) => fn());
        };
    }, [loadPendingUpdate]);

    const targetVersion = update?.version ?? "";

    useEffect(() => {
        if (!targetVersion) return;

        setStep("prompt");
        setProgressPct(0);
        setErrorMessage(null);
        setReleaseNotes(null);
        setIsFetchingNotes(true);

        const controller = new AbortController();
        const tag = targetVersion.startsWith("v")
            ? targetVersion
            : `v${targetVersion}`;

        fetch(
            `https://api.github.com/repos/vsnthdev/Velquo/releases/tags/${encodeURIComponent(tag)}`,
            {
                signal: controller.signal,
                headers: {
                    Accept: "application/vnd.github+json",
                },
            },
        )
            .then((res) => (res.ok ? res.json() : null))
            .then((data: { body?: string | null } | null) => {
                if (typeof data?.body == "string" && data.body.trim().length > 0) {
                    setReleaseNotes(data.body.replace(/\r\n/g, "\n").trim());
                }
            })
            .catch(() => {
                // Ignore network errors fetching release notes
            })
            .finally(() => {
                if (!controller.signal.aborted) {
                    setIsFetchingNotes(false);
                }
            });

        return () => controller.abort();
    }, [targetVersion]);

    const handleHeaderDrag = (e: MouseEvent<HTMLElement>) => {
        if (e.button != 0) return;
        const target = e.target as HTMLElement;
        if (target.closest("button, a, input")) return;
        getCurrentWindow()
            .startDragging()
            .catch(() => {});
    };

    const handleCloseWindow = () => {
        if (step == "downloading") return;
        invoke("close_updater_window").catch(() => {
            getCurrentWindow()
                .close()
                .catch(() => {});
        });
    };

    const handleSkipVersion = () => {
        if (!update || step == "downloading") return;
        localStorage.setItem("velquo.skippedUpdateVersion", update.version);
        handleCloseWindow();
    };

    const handleUpdateNow = async () => {
        if (!update || step == "downloading") return;
        setStep("downloading");
        setProgressPct(0);
        setErrorMessage(null);

        let downloadedBytes = 0;
        let totalBytes = 0;

        try {
            await update.downloadAndInstall((event) => {
                if (event.event == "Started") {
                    totalBytes = event.data.contentLength ?? 0;
                    setProgressPct(2);
                } else if (event.event == "Progress") {
                    downloadedBytes += event.data.chunkLength;
                    if (totalBytes > 0) {
                        const pct = Math.min(
                            99,
                            Math.max(2, Math.round((downloadedBytes / totalBytes) * 100)),
                        );
                        setProgressPct(pct);
                    } else {
                        setProgressPct((prev) => Math.min(95, prev + 5));
                    }
                } else if (event.event == "Finished") {
                    setProgressPct(100);
                }
            });

            setProgressPct(100);
            setStep("ready");
        } catch (err) {
            console.error("Failed to download and install update:", err);
            setErrorMessage(
                typeof err == "string"
                    ? err
                    : err instanceof Error
                        ? err.message
                        : "Failed to download and install the update.",
            );
            setStep("error");
        }
    };

    const handleRestartApp = async () => {
        try {
            await invoke("restart_app");
        } catch (err) {
            console.error("Failed to restart application:", err);
        }
    };

    return (
        <div className="relative flex h-screen w-screen flex-col overflow-hidden bg-white text-zinc-900 select-none dark:bg-zinc-900 dark:text-zinc-100">
            {/* Subtle bottom-right dark mode ambient glow matching the main window */}
            <div
                className="pointer-events-none absolute inset-0 z-0 hidden dark:block"
                style={{
                    background:
                        "radial-gradient(circle at 96% 96%, rgba(113, 113, 122, 0.22) 0%, rgba(113, 113, 122, 0.08) 34%, transparent 65%)",
                }}
            />

            {/* Overlay Title Bar Header: Left padding leaves room for macOS traffic lights */}
            <div
                onMouseDown={handleHeaderDrag}
                className="relative z-10 flex shrink-0 items-center justify-between gap-2 border-b border-black/8 bg-white/80 py-2 pr-3.5 pl-[76px] backdrop-blur-md dark:border-white/10 dark:bg-zinc-900/80"
            >
                <span className="text-[13px] font-semibold text-zinc-900 dark:text-zinc-100">
                    Software update
                </span>
                {update && (
                    <span className="rounded-md bg-black/[0.04] px-2 py-0.5 font-mono text-[11px] text-zinc-500 dark:bg-white/[0.06] dark:text-zinc-400">
                        v{update.currentVersion} → v{update.version}
                    </span>
                )}
            </div>

            {/* Body */}
            <div className="relative z-10 flex min-h-0 flex-1 flex-col px-5 pt-4 pb-4">
                {isLoadingUpdate && !update ? (
                    <div className="flex flex-1 flex-col items-center justify-center gap-2.5 text-zinc-500 dark:text-zinc-400">
                        <CircleNotchIcon size={24} weight="bold" className="animate-spin" />
                        <span className="text-[13px] font-medium">
                            Checking for updates...
                        </span>
                    </div>
                ) : !update ? (
                    <div className="flex flex-1 flex-col items-center justify-center gap-2.5 text-center">
                        <div className="flex h-11 w-11 items-center justify-center rounded-2xl bg-emerald-500/12 text-emerald-600 dark:bg-emerald-500/15 dark:text-emerald-400">
                            <CheckCircleIcon size={24} weight="duotone" />
                        </div>
                        <div className="flex flex-col gap-0.5">
                            <span className="text-[15px] font-semibold text-zinc-900 dark:text-zinc-100">
                                You have the latest version
                            </span>
                            <span className="text-[12.5px] text-zinc-500 dark:text-zinc-400">
                                Velquo{" "}
                                <span className="font-mono font-medium text-zinc-700 dark:text-zinc-300">
                                    v{appVersion}
                                </span>{" "}
                                is currently the newest version available.
                            </span>
                        </div>
                        <button
                            type="button"
                            onClick={handleCloseWindow}
                            className="mt-1.5 cursor-pointer rounded-xl border border-black/12 bg-black/[0.03] px-5 py-1.5 text-[12.5px] font-medium text-zinc-700 transition-colors hover:bg-black/[0.06] dark:border-white/12 dark:bg-white/[0.04] dark:text-zinc-200 dark:hover:bg-white/[0.08]"
                        >
                            Close
                        </button>
                    </div>
                ) : (
                    <>
                        {/* Hero Header */}
                        <div className="flex shrink-0 items-center gap-3">
                            <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-blue-500/12 text-blue-600 dark:bg-blue-500/15 dark:text-blue-400">
                                <SparkleIcon size={20} weight="duotone" />
                            </div>
                            <div className="flex flex-col">
                                <h1 className="text-[15px] font-semibold tracking-tight text-zinc-900 dark:text-zinc-100">
                                    Velquo{" "}
                                    <span className="font-mono text-blue-600 dark:text-blue-400">
                                        v{update.version}
                                    </span>{" "}
                                    is available
                                </h1>
                                <p className="text-[12px] text-zinc-500 dark:text-zinc-400">
                                    Ready to install (current:{" "}
                                    <span className="font-mono font-medium text-zinc-700 dark:text-zinc-300">
                                        v{update.currentVersion}
                                    </span>
                                    )
                                </p>
                            </div>
                        </div>

                        {/* Release Notes Section */}
                        <div className="mt-3.5 flex min-h-0 flex-1 flex-col overflow-hidden rounded-xl border border-black/10 bg-black/[0.02] dark:border-white/10 dark:bg-white/[0.02]">
                            <div className="flex-1 overflow-y-auto px-3.5 py-2.5 text-[12.5px] leading-relaxed whitespace-pre-wrap text-zinc-700 select-text dark:text-zinc-300">
                                {isFetchingNotes ? (
                                    <div className="flex h-full items-center justify-center gap-2 text-zinc-400 dark:text-zinc-500">
                                        <CircleNotchIcon
                                            size={15}
                                            weight="bold"
                                            className="animate-spin"
                                        />
                                        <span className="text-[12px]">
                                            Loading release notes...
                                        </span>
                                    </div>
                                ) : releaseNotes ? (
                                    releaseNotes
                                ) : (
                                    <span className="text-zinc-400 dark:text-zinc-500">
                                        No release notes provided for this version.
                                    </span>
                                )}
                            </div>
                        </div>

                        {errorMessage && (
                            <div className="mt-2.5 shrink-0 rounded-xl border border-red-500/30 bg-red-500/10 px-3 py-1.5 text-[11.5px] text-red-600 dark:text-red-300">
                                {errorMessage}
                            </div>
                        )}

                        {/* Footer Actions */}
                        <div className="mt-3.5 flex shrink-0 items-center">
                            <AnimatePresence mode="wait" initial={false}>
                                {(step == "prompt" || step == "error") && (
                                    <motion.div
                                        key="prompt-buttons"
                                        initial={{ y: 4, opacity: 0 }}
                                        animate={{ y: 0, opacity: 1 }}
                                        exit={{ y: -4, opacity: 0 }}
                                        transition={{ duration: 0.16 }}
                                        className="flex w-full items-center gap-2.5"
                                    >
                                        <motion.button
                                            type="button"
                                            whileHover={{ scale: 1.015 }}
                                            whileTap={{ scale: 0.985 }}
                                            onClick={handleSkipVersion}
                                            className="flex flex-1 cursor-pointer items-center justify-center rounded-xl border border-black/12 bg-black/[0.02] px-3.5 py-2 text-[12.5px] font-medium text-zinc-700 transition-colors hover:border-black/20 hover:bg-black/[0.05] dark:border-white/12 dark:bg-white/[0.03] dark:text-zinc-300 dark:hover:border-white/20 dark:hover:bg-white/[0.06]"
                                        >
                                            Skip this version
                                        </motion.button>

                                        <motion.button
                                            type="button"
                                            whileHover={{ scale: 1.015 }}
                                            whileTap={{ scale: 0.985 }}
                                            onClick={handleUpdateNow}
                                            className="flex flex-1 cursor-pointer items-center justify-center gap-1.5 rounded-xl bg-blue-600 px-3.5 py-2 text-[12.5px] font-medium text-white shadow-xs transition-colors hover:bg-blue-500"
                                        >
                                            <CloudArrowDownIcon size={15} weight="bold" />
                                            <span>
                                                {step == "error" ? "Retry update" : "Update now"}
                                            </span>
                                        </motion.button>
                                    </motion.div>
                                )}

                                {step == "downloading" && (
                                    <motion.div
                                        key="downloading-progress-button"
                                        initial={{ opacity: 0, scale: 0.98 }}
                                        animate={{ scale: 1, opacity: 1 }}
                                        exit={{ opacity: 0, scale: 0.98 }}
                                        transition={{ duration: 0.18 }}
                                        className="relative flex h-9 w-full items-center justify-center overflow-hidden rounded-xl border border-blue-500/30 bg-blue-500/12 text-[12.5px] font-medium text-blue-700 dark:border-blue-400/30 dark:bg-blue-500/15 dark:text-blue-200"
                                    >
                                        <motion.div
                                            className="absolute inset-y-0 left-0 bg-blue-600/25 dark:bg-blue-500/35"
                                            initial={{ width: "0%" }}
                                            animate={{ width: `${progressPct}%` }}
                                            transition={{ duration: 0.2, ease: "easeOut" }}
                                        />
                                        <span className="relative z-10 flex items-center gap-2">
                                            <span>Downloading & installing...</span>
                                            <span className="font-mono text-[11.5px] tabular-nums">
                                                {progressPct}%
                                            </span>
                                        </span>
                                    </motion.div>
                                )}

                                {step == "ready" && (
                                    <motion.button
                                        key="restart-button"
                                        type="button"
                                        initial={{ opacity: 0, scale: 0.96 }}
                                        animate={{ scale: 1, opacity: 1 }}
                                        exit={{ opacity: 0, scale: 0.96 }}
                                        whileHover={{ scale: 1.012 }}
                                        whileTap={{ scale: 0.988 }}
                                        transition={{
                                            damping: 30,
                                            type: "spring",
                                            stiffness: 480,
                                        }}
                                        onClick={handleRestartApp}
                                        className="flex w-full cursor-pointer items-center justify-center gap-2 rounded-xl bg-red-600 px-4 py-2 text-[13px] font-semibold text-white shadow-sm transition-colors hover:bg-red-500"
                                    >
                                        <ArrowClockwiseIcon size={15} weight="bold" />
                                        <span>Restart</span>
                                    </motion.button>
                                )}
                            </AnimatePresence>
                        </div>
                    </>
                )}
            </div>
        </div>
    );
}
