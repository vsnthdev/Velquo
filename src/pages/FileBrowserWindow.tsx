import { motion } from "motion/react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { formatFileSize, formatShortDate } from "../utils/format";
import type {
    DeviceView,
    AppSnapshot,
    SftpFileEntry,
    SftpBrowseResponse,
} from "../types";
import {
    useRef,
    useMemo,
    useState,
    useEffect,
    useCallback,
    type MouseEvent,
} from "react";
import {
    FileIcon,
    TrashIcon,
    CheckIcon,
    FolderIcon,
    FilePdfIcon,
    FileTextIcon,
    FileVideoIcon,
    FileImageIcon,
    FileAudioIcon,
    ArrowLeftIcon,
    FolderOpenIcon,
    FolderLockIcon,
    CaretRightIcon,
    FileArchiveIcon,
    CircleNotchIcon,
    WarningCircleIcon,
    DownloadSimpleIcon,
    ArrowsClockwiseIcon,
} from "@phosphor-icons/react";

export function FileBrowserWindow() {
    const deviceId = useMemo(
        () => new URLSearchParams(window.location.search).get("deviceId") ?? "",
        [],
    );

    const [device, setDevice] = useState<DeviceView | null>(null);
    const [sftpData, setSftpData] = useState<SftpBrowseResponse | null>(null);
    const [isSftpLoading, setIsSftpLoading] = useState<boolean>(false);
    const [sftpError, setSftpError] = useState<string | null>(null);
    const [downloadingPaths, setDownloadingPaths] = useState<Set<string>>(
        () => new Set(),
    );
    const [downloadedPaths, setDownloadedPaths] = useState<Map<string, string>>(
        () => new Map(),
    );
    const [downloadErrors, setDownloadErrors] = useState<Map<string, string>>(
        () => new Map(),
    );
    const [deletingPaths, setDeletingPaths] = useState<Set<string>>(
        () => new Set(),
    );

    const breadcrumbScrollRef = useRef<HTMLDivElement | null>(null);
    const sftpReqSeqRef = useRef<number>(0);
    const initialLoadedRef = useRef<boolean>(false);
    const isFetchingSnapRef = useRef<boolean>(false);
    const pendingSnapRef = useRef<boolean>(false);

    const refreshDeviceState = useCallback(async () => {
        if (!deviceId) return;
        if (isFetchingSnapRef.current) {
            pendingSnapRef.current = true;
            return;
        }
        isFetchingSnapRef.current = true;
        try {
            do {
                pendingSnapRef.current = false;
                const snap = await invoke<AppSnapshot>("get_app_snapshot");
                const found =
                    snap.devices.find((d) => d.deviceId == deviceId) ?? null;
                setDevice(found);
            } while (pendingSnapRef.current);
        } catch (err) {
            console.error("Failed to fetch snapshot in FileBrowserWindow:", err);
        } finally {
            isFetchingSnapRef.current = false;
        }
    }, [deviceId]);

    useEffect(() => {
        refreshDeviceState();
        const unlistenPromise = listen("velquo://state-updated", () => {
            refreshDeviceState();
        });
        return () => {
            unlistenPromise.then((unlisten) => unlisten());
        };
    }, [refreshDeviceState]);

    const loadSftpDirectory = useCallback(
        async (targetPath?: string) => {
            if (!deviceId) return;
            const reqId = ++sftpReqSeqRef.current;
            setIsSftpLoading(true);
            setSftpError(null);
            try {
                const res = await invoke<SftpBrowseResponse>("sftp_browse", {
                    deviceId,
                    path: targetPath ?? null,
                });
                if (sftpReqSeqRef.current != reqId) return;
                setSftpData(res);
                requestAnimationFrame(() => {
                    if (breadcrumbScrollRef.current) {
                        breadcrumbScrollRef.current.scrollLeft =
                            breadcrumbScrollRef.current.scrollWidth;
                    }
                });
            } catch (err) {
                if (sftpReqSeqRef.current != reqId) return;
                setSftpError(typeof err == "string" ? err : String(err));
            } finally {
                if (sftpReqSeqRef.current == reqId) {
                    setIsSftpLoading(false);
                }
            }
        },
        [deviceId],
    );

    useEffect(() => {
        if (!initialLoadedRef.current && deviceId) {
            initialLoadedRef.current = true;
            loadSftpDirectory();
        }
    }, [deviceId, loadSftpDirectory]);

    const sftpBreadcrumbs = useMemo(() => {
        if (!sftpData) return [];
        const current = sftpData.currentPath || "/";
        const roots = sftpData.roots || [];

        if (current == "/" && roots.length > 1) {
            return [{ path: "/", label: "Storage locations" }];
        }

        const matchedRoot = [...roots]
            .sort((a, b) => b.path.length - a.path.length)
            .find(
                (r) =>
                    current == r.path ||
          (r.path != "/" && current.startsWith(`${r.path}/`)),
            );

        const crumbs: Array<{ label: string; path: string }> = [];
        if (roots.length > 1) {
            crumbs.push({ path: "/", label: "Storage" });
        }

        if (matchedRoot) {
            crumbs.push({ path: matchedRoot.path, label: matchedRoot.name });
            const rel =
                matchedRoot.path == "/"
                    ? current.slice(1)
                    : current.slice(matchedRoot.path.length);
            const parts = rel.split("/").filter(Boolean);
            let accum = matchedRoot.path == "/" ? "" : matchedRoot.path;
            for (const seg of parts) {
                accum = `${accum}/${seg}`;
                crumbs.push({ label: seg, path: accum });
            }
            return crumbs;
        }

        crumbs.push({
            path: "/",
            label: roots[0]?.name || "Internal storage",
        });
        const parts = current.split("/").filter(Boolean);
        let accum = "";
        for (const seg of parts) {
            accum = `${accum}/${seg}`;
            crumbs.push({ label: seg, path: accum });
        }
        return crumbs;
    }, [sftpData]);

    const canSftpGoBack = sftpBreadcrumbs.length > 1;

    const handleSftpGoBack = () => {
        if (sftpBreadcrumbs.length <= 1) return;
        const parentCrumb = sftpBreadcrumbs[sftpBreadcrumbs.length - 2];
        if (parentCrumb) {
            loadSftpDirectory(parentCrumb.path);
        }
    };

    const handleSftpDownload = async (entry: SftpFileEntry) => {
        if (entry.isDir || downloadingPaths.has(entry.path)) return;

        setDownloadingPaths((prev) => {
            const next = new Set(prev);
            next.add(entry.path);
            return next;
        });
        setDownloadErrors((prev) => {
            if (!prev.has(entry.path)) return prev;
            const next = new Map(prev);
            next.delete(entry.path);
            return next;
        });

        try {
            const savedLocalPath = await invoke<string>("sftp_download_file", {
                deviceId,
                fileName: entry.name,
                remotePath: entry.path,
            });
            setDownloadedPaths((prev) => {
                const next = new Map(prev);
                next.set(entry.path, savedLocalPath);
                return next;
            });
        } catch (err) {
            setDownloadErrors((prev) => {
                const next = new Map(prev);
                next.set(entry.path, typeof err == "string" ? err : String(err));
                return next;
            });
        } finally {
            setDownloadingPaths((prev) => {
                const next = new Set(prev);
                next.delete(entry.path);
                return next;
            });
        }
    };

    const handleSftpDelete = async (entry: SftpFileEntry) => {
        if (deletingPaths.has(entry.path) || downloadingPaths.has(entry.path)) {
            return;
        }

        setDeletingPaths((prev) => {
            const next = new Set(prev);
            next.add(entry.path);
            return next;
        });
        setDownloadErrors((prev) => {
            if (!prev.has(entry.path)) return prev;
            const next = new Map(prev);
            next.delete(entry.path);
            return next;
        });

        try {
            await invoke("sftp_delete_entry", {
                deviceId,
                isDir: entry.isDir,
                remotePath: entry.path,
            });
            setSftpData((prev) => {
                if (!prev) return prev;
                return {
                    ...prev,
                    entries: prev.entries.filter((item) => item.path != entry.path),
                };
            });
            setDownloadedPaths((prev) => {
                if (!prev.has(entry.path)) return prev;
                const next = new Map(prev);
                next.delete(entry.path);
                return next;
            });
        } catch (err) {
            setDownloadErrors((prev) => {
                const next = new Map(prev);
                next.set(entry.path, typeof err == "string" ? err : String(err));
                return next;
            });
        } finally {
            setDeletingPaths((prev) => {
                const next = new Set(prev);
                next.delete(entry.path);
                return next;
            });
        }
    };

    const handleHeaderDrag = (e: MouseEvent<HTMLElement>) => {
        if (e.button != 0) return;
        const target = e.target as HTMLElement;
        if (target.closest("button, a, input")) return;
        getCurrentWindow()
            .startDragging()
            .catch(() => {});
    };

    const renderFileEntryIcon = (entry: SftpFileEntry) => {
        if (entry.isDir) {
            return (
                <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-blue-500/12 text-blue-600 dark:bg-blue-500/15 dark:text-blue-400">
                    <FolderIcon size={17} weight="fill" />
                </div>
            );
        }
        const ext = entry.name.includes(".")
            ? entry.name.split(".").pop()?.toLowerCase() ?? ""
            : "";

        if (
            [
                "jpg",
                "jpeg",
                "png",
                "gif",
                "webp",
                "heic",
                "heif",
                "avif",
                "svg",
                "bmp",
                "dng",
                "raw",
            ].includes(ext)
        ) {
            return (
                <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-purple-500/12 text-purple-600 dark:bg-purple-500/15 dark:text-purple-400">
                    <FileImageIcon size={17} weight="duotone" />
                </div>
            );
        }
        if (
            ["mp4", "mov", "mkv", "webm", "avi", "3gp", "m4v", "ts"].includes(ext)
        ) {
            return (
                <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-amber-500/12 text-amber-600 dark:bg-amber-500/15 dark:text-amber-400">
                    <FileVideoIcon size={17} weight="duotone" />
                </div>
            );
        }
        if (
            ["mp3", "flac", "wav", "m4a", "aac", "ogg", "opus", "wma"].includes(ext)
        ) {
            return (
                <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-rose-500/12 text-rose-600 dark:bg-rose-500/15 dark:text-rose-400">
                    <FileAudioIcon size={17} weight="duotone" />
                </div>
            );
        }
        if (ext == "pdf") {
            return (
                <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-red-500/12 text-red-600 dark:bg-red-500/15 dark:text-red-400">
                    <FilePdfIcon size={17} weight="duotone" />
                </div>
            );
        }
        if (
            [
                "zip",
                "tar",
                "gz",
                "tgz",
                "7z",
                "rar",
                "xz",
                "apk",
                "aab",
                "dmg",
            ].includes(ext)
        ) {
            return (
                <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-orange-500/12 text-orange-600 dark:bg-orange-500/15 dark:text-orange-400">
                    <FileArchiveIcon size={17} weight="duotone" />
                </div>
            );
        }
        if (
            [
                "txt",
                "md",
                "json",
                "csv",
                "xml",
                "yaml",
                "yml",
                "log",
                "doc",
                "docx",
                "rtf",
            ].includes(ext)
        ) {
            return (
                <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-sky-500/12 text-sky-600 dark:bg-sky-500/15 dark:text-sky-400">
                    <FileTextIcon size={17} weight="duotone" />
                </div>
            );
        }
        return (
            <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-black/[0.05] text-zinc-600 dark:bg-white/[0.07] dark:text-zinc-300">
                <FileIcon size={17} weight="duotone" />
            </div>
        );
    };

    const deviceDisplayName = device?.deviceName || "Phone";
    const saveFolderName =
        device?.saveLocation?.split("/").filter(Boolean).pop() || "Downloads";

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
                className="relative z-10 flex shrink-0 items-center justify-between gap-2 border-b border-black/8 bg-white/80 py-2.5 pr-3.5 pl-[78px] backdrop-blur-md dark:border-white/10 dark:bg-zinc-900/80"
            >
                <div className="flex min-w-0 flex-1 items-center gap-1.5">
                    <motion.button
                        type="button"
                        disabled={!canSftpGoBack || isSftpLoading}
                        whileHover={
                            canSftpGoBack && !isSftpLoading ? { scale: 1.06 } : undefined
                        }
                        whileTap={
                            canSftpGoBack && !isSftpLoading ? { scale: 0.94 } : undefined
                        }
                        onClick={handleSftpGoBack}
                        className={`flex h-7 w-7 shrink-0 items-center justify-center rounded-lg transition-colors ${
                            canSftpGoBack && !isSftpLoading
                                ? "cursor-pointer text-zinc-700 hover:bg-black/[0.06] dark:text-zinc-200 dark:hover:bg-white/[0.08]"
                                : "cursor-default text-zinc-300 dark:text-zinc-600"
                        }`}
                        title="Back"
                    >
                        <ArrowLeftIcon size={15} weight="bold" />
                    </motion.button>

                    {/* Scrollable Breadcrumb Bar */}
                    <div
                        ref={breadcrumbScrollRef}
                        onMouseDown={handleHeaderDrag}
                        className="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto whitespace-nowrap [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
                    >
                        {sftpBreadcrumbs.length > 0 ? (
                            sftpBreadcrumbs.map((crumb, idx) => {
                                const isLast = idx == sftpBreadcrumbs.length - 1;
                                return (
                                    <div
                                        key={`${crumb.path}-${idx}`}
                                        className="flex shrink-0 items-center gap-1"
                                    >
                                        {idx > 0 && (
                                            <CaretRightIcon
                                                size={11}
                                                weight="bold"
                                                className="text-zinc-400 dark:text-zinc-500"
                                            />
                                        )}
                                        <button
                                            type="button"
                                            disabled={isLast || isSftpLoading}
                                            onClick={() => {
                                                if (!isLast) {
                                                    loadSftpDirectory(crumb.path);
                                                }
                                            }}
                                            className={`rounded-md px-1.5 py-0.5 text-[13px] transition-colors ${
                                                isLast
                                                    ? "cursor-default font-semibold text-zinc-900 dark:text-zinc-100"
                                                    : "cursor-pointer font-medium text-zinc-500 hover:bg-black/[0.05] hover:text-zinc-800 dark:text-zinc-400 dark:hover:bg-white/[0.07] dark:hover:text-zinc-200"
                                            }`}
                                        >
                                            {crumb.label}
                                        </button>
                                    </div>
                                );
                            })
                        ) : (
                            <span className="px-1.5 text-[13.5px] font-semibold text-zinc-900 dark:text-zinc-100">
                                {deviceDisplayName} files
                            </span>
                        )}
                    </div>
                </div>

                <div className="flex shrink-0 items-center gap-1">
                    <motion.button
                        type="button"
                        disabled={isSftpLoading}
                        whileHover={!isSftpLoading ? { scale: 1.06 } : undefined}
                        whileTap={!isSftpLoading ? { scale: 0.94 } : undefined}
                        onClick={() => loadSftpDirectory(sftpData?.currentPath)}
                        className={`flex h-7 w-7 items-center justify-center rounded-lg text-zinc-500 transition-colors hover:bg-black/[0.06] hover:text-zinc-800 dark:text-zinc-400 dark:hover:bg-white/[0.08] dark:hover:text-zinc-200 ${
                            isSftpLoading ? "cursor-default opacity-60" : "cursor-pointer"
                        }`}
                        title="Refresh folder"
                    >
                        <ArrowsClockwiseIcon
                            size={15}
                            weight="bold"
                            className={isSftpLoading ? "animate-spin" : ""}
                        />
                    </motion.button>
                </div>
            </div>

            {/* File Browser Body */}
            <div className="relative z-10 flex-1 overflow-y-auto p-2.5">
                {isSftpLoading && !sftpData ? (
                    <div className="flex h-full flex-col items-center justify-center gap-2.5 px-6 text-center">
                        <CircleNotchIcon
                            size={24}
                            weight="bold"
                            className="animate-spin text-blue-600 dark:text-blue-400"
                        />
                        <p className="text-[13px] font-medium text-zinc-700 dark:text-zinc-200">
                            Connecting to {deviceDisplayName} storage...
                        </p>
                    </div>
                ) : sftpData?.unconfiguredMessage ? (
                    <div className="flex h-full flex-col items-center justify-center px-6 text-center">
                        <div className="flex h-12 w-12 items-center justify-center rounded-2xl bg-amber-500/12 text-amber-600 dark:bg-amber-500/15 dark:text-amber-400">
                            <FolderLockIcon size={24} weight="duotone" />
                        </div>
                        <p className="mt-3 text-[13.5px] font-semibold text-zinc-800 dark:text-zinc-100">
                            Storage access not configured
                        </p>
                        <p className="mt-1.5 max-w-xs text-xs leading-relaxed text-zinc-500 dark:text-zinc-400">
                            Open{" "}
                            <span className="font-medium text-zinc-700 dark:text-zinc-200">
                                KDE Connect
                            </span>{" "}
                            on {deviceDisplayName} →{" "}
                            <span className="font-medium text-zinc-700 dark:text-zinc-200">
                                Plugin settings
                            </span>{" "}
                            →{" "}
                            <span className="font-medium text-zinc-700 dark:text-zinc-200">
                                Filesystem expose
                            </span>{" "}
                            and add a storage location.
                        </p>
                        <motion.button
                            type="button"
                            whileHover={{ scale: 1.02 }}
                            whileTap={{ scale: 0.98 }}
                            onClick={() => loadSftpDirectory()}
                            className="mt-4 cursor-pointer rounded-xl border border-black/12 bg-black/[0.03] px-4 py-1.5 text-xs font-medium text-zinc-800 transition-colors hover:bg-black/[0.06] dark:border-white/12 dark:bg-white/[0.05] dark:text-zinc-100 dark:hover:bg-white/[0.09]"
                        >
                            Retry
                        </motion.button>
                    </div>
                ) : sftpError ? (
                    <div className="flex h-full flex-col items-center justify-center px-6 text-center">
                        <div className="flex h-12 w-12 items-center justify-center rounded-2xl bg-red-500/12 text-red-600 dark:bg-red-500/15 dark:text-red-400">
                            <WarningCircleIcon size={24} weight="duotone" />
                        </div>
                        <p className="mt-3 text-[13.5px] font-semibold text-zinc-800 dark:text-zinc-100">
                            Couldn&apos;t load files
                        </p>
                        <p className="mt-1 max-w-xs text-xs leading-relaxed text-zinc-500 dark:text-zinc-400">
                            {sftpError}
                        </p>
                        <motion.button
                            type="button"
                            whileHover={{ scale: 1.02 }}
                            whileTap={{ scale: 0.98 }}
                            onClick={() => loadSftpDirectory(sftpData?.currentPath)}
                            className="mt-4 cursor-pointer rounded-xl border border-black/12 bg-black/[0.03] px-4 py-1.5 text-xs font-medium text-zinc-800 transition-colors hover:bg-black/[0.06] dark:border-white/12 dark:bg-white/[0.05] dark:text-zinc-100 dark:hover:bg-white/[0.09]"
                        >
                            Retry
                        </motion.button>
                    </div>
                ) : sftpData && sftpData.entries.length == 0 ? (
                    <div className="flex h-full flex-col items-center justify-center px-6 text-center">
                        <div className="flex h-11 w-11 items-center justify-center rounded-2xl bg-black/[0.04] text-zinc-400 dark:bg-white/[0.05] dark:text-zinc-500">
                            <FolderOpenIcon size={22} weight="regular" />
                        </div>
                        <p className="mt-2.5 text-[13px] font-medium text-zinc-500 dark:text-zinc-400">
                            This folder is empty
                        </p>
                    </div>
                ) : (
                    <div
                        className={`flex flex-col gap-0.5 transition-opacity duration-150 ${
                            isSftpLoading ? "pointer-events-none opacity-60" : ""
                        }`}
                    >
                        {sftpData?.entries.map((entry) => {
                            const isDownloading = downloadingPaths.has(entry.path);
                            const isDeleting = deletingPaths.has(entry.path);
                            const savedLocalPath = downloadedPaths.get(entry.path);
                            const dlError = downloadErrors.get(entry.path);
                            const formattedDate = formatShortDate(entry.modifiedSecs);
                            const isRootStorageEntry =
                                sftpData.currentPath == "/" &&
                sftpData.roots.some((r) => r.path == entry.path);

                            if (entry.isDir) {
                                return (
                                    <div
                                        key={entry.path}
                                        className="group flex w-full items-center justify-between gap-2 rounded-xl px-2.5 py-2 transition-colors hover:bg-black/[0.04] dark:hover:bg-white/[0.05]"
                                    >
                                        <button
                                            type="button"
                                            disabled={isDeleting}
                                            onClick={() => loadSftpDirectory(entry.path)}
                                            className="flex min-w-0 flex-1 cursor-pointer items-center gap-2.5 text-left"
                                        >
                                            {renderFileEntryIcon(entry)}
                                            <div className="flex min-w-0 flex-1 flex-col">
                                                <span className="truncate text-[13px] font-medium text-zinc-800 dark:text-zinc-100">
                                                    {entry.name}
                                                </span>
                                                {(dlError || formattedDate) && (
                                                    <span
                                                        className={`truncate text-[11px] ${
                                                            dlError
                                                                ? "text-red-500 dark:text-red-400"
                                                                : "text-zinc-400 dark:text-zinc-500"
                                                        }`}
                                                    >
                                                        {dlError
                                                            ? `Delete failed: ${dlError}`
                                                            : formattedDate}
                                                    </span>
                                                )}
                                            </div>
                                        </button>

                                        <div className="flex shrink-0 items-center gap-1.5">
                                            {!isRootStorageEntry &&
                        (isDeleting ? (
                            <div
                                className="flex h-7 w-7 items-center justify-center rounded-lg bg-red-500/10 text-red-600 dark:bg-red-500/15 dark:text-red-400"
                                title="Deleting folder..."
                            >
                                <CircleNotchIcon
                                    size={15}
                                    weight="bold"
                                    className="animate-spin"
                                />
                            </div>
                        ) : (
                            <motion.button
                                type="button"
                                whileHover={{ scale: 1.06 }}
                                whileTap={{ scale: 0.94 }}
                                onClick={() => handleSftpDelete(entry)}
                                className="flex h-7 w-7 cursor-pointer items-center justify-center rounded-lg border border-black/10 bg-black/[0.02] text-zinc-500 transition-colors hover:border-red-500/30 hover:bg-red-500/10 hover:text-red-600 dark:border-white/10 dark:bg-white/[0.03] dark:text-zinc-400 dark:hover:border-red-400/30 dark:hover:bg-red-500/15 dark:hover:text-red-400"
                                title="Delete folder"
                            >
                                <TrashIcon size={14} weight="regular" />
                            </motion.button>
                        ))}

                                            <button
                                                type="button"
                                                disabled={isDeleting}
                                                onClick={() => loadSftpDirectory(entry.path)}
                                                className="flex h-7 w-5 cursor-pointer items-center justify-center"
                                                aria-label={`Open ${entry.name}`}
                                            >
                                                <CaretRightIcon
                                                    size={14}
                                                    weight="bold"
                                                    className="shrink-0 text-zinc-300 transition-transform group-hover:translate-x-0.5 group-hover:text-zinc-500 dark:text-zinc-600 dark:group-hover:text-zinc-400"
                                                />
                                            </button>
                                        </div>
                                    </div>
                                );
                            }

                            return (
                                <div
                                    key={entry.path}
                                    className="flex items-center justify-between gap-2 rounded-xl px-2.5 py-2 transition-colors hover:bg-black/[0.03] dark:hover:bg-white/[0.04]"
                                >
                                    <div className="flex min-w-0 flex-1 items-center gap-2.5">
                                        {renderFileEntryIcon(entry)}
                                        <div className="flex min-w-0 flex-1 flex-col">
                                            <span className="truncate text-[13px] font-medium text-zinc-800 dark:text-zinc-100">
                                                {entry.name}
                                            </span>
                                            <span
                                                className={`truncate text-[11px] ${
                                                    dlError
                                                        ? "text-red-500 dark:text-red-400"
                                                        : "text-zinc-400 dark:text-zinc-500"
                                                }`}
                                            >
                                                {dlError
                                                    ? dlError
                                                    : `${formatFileSize(entry.size)}${
                                                        formattedDate ? ` · ${formattedDate}` : ""
                                                    }`}
                                            </span>
                                        </div>
                                    </div>

                                    <div className="flex shrink-0 items-center gap-1.5">
                                        {isDownloading ? (
                                            <div
                                                className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-blue-500/10 text-blue-600 dark:bg-blue-500/15 dark:text-blue-400"
                                                title="Downloading..."
                                            >
                                                <CircleNotchIcon
                                                    size={15}
                                                    weight="bold"
                                                    className="animate-spin"
                                                />
                                            </div>
                                        ) : savedLocalPath ? (
                                            <motion.button
                                                type="button"
                                                whileHover={{ scale: 1.06 }}
                                                whileTap={{ scale: 0.94 }}
                                                onClick={() => {
                                                    revealItemInDir(savedLocalPath).catch(() => {});
                                                }}
                                                className="flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded-lg bg-emerald-500/12 text-emerald-600 transition-colors hover:bg-emerald-500/20 dark:bg-emerald-500/15 dark:text-emerald-400 dark:hover:bg-emerald-500/25"
                                                title="Saved — click to reveal in Finder"
                                            >
                                                <CheckIcon size={15} weight="bold" />
                                            </motion.button>
                                        ) : (
                                            <motion.button
                                                type="button"
                                                disabled={isDeleting}
                                                whileHover={!isDeleting ? { scale: 1.06 } : undefined}
                                                whileTap={!isDeleting ? { scale: 0.94 } : undefined}
                                                onClick={() => handleSftpDownload(entry)}
                                                className="flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded-lg border border-black/10 bg-black/[0.02] text-zinc-600 transition-colors hover:border-black/20 hover:bg-black/[0.06] hover:text-zinc-900 dark:border-white/10 dark:bg-white/[0.03] dark:text-zinc-300 dark:hover:border-white/20 dark:hover:bg-white/[0.08] dark:hover:text-zinc-100"
                                                title="Download file"
                                            >
                                                <DownloadSimpleIcon size={15} weight="regular" />
                                            </motion.button>
                                        )}

                                        {isDeleting ? (
                                            <div
                                                className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-red-500/10 text-red-600 dark:bg-red-500/15 dark:text-red-400"
                                                title="Deleting file..."
                                            >
                                                <CircleNotchIcon
                                                    size={15}
                                                    weight="bold"
                                                    className="animate-spin"
                                                />
                                            </div>
                                        ) : (
                                            <motion.button
                                                type="button"
                                                disabled={isDownloading}
                                                whileHover={
                                                    !isDownloading ? { scale: 1.06 } : undefined
                                                }
                                                whileTap={!isDownloading ? { scale: 0.94 } : undefined}
                                                onClick={() => handleSftpDelete(entry)}
                                                className="flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded-lg border border-black/10 bg-black/[0.02] text-zinc-500 transition-colors hover:border-red-500/30 hover:bg-red-500/10 hover:text-red-600 dark:border-white/10 dark:bg-white/[0.03] dark:text-zinc-400 dark:hover:border-red-400/30 dark:hover:bg-red-500/15 dark:hover:text-red-400"
                                                title="Delete file"
                                            >
                                                <TrashIcon size={14} weight="regular" />
                                            </motion.button>
                                        )}
                                    </div>
                                </div>
                            );
                        })}
                    </div>
                )}
            </div>

            {/* Footer Status Bar */}
            <div className="relative z-10 flex shrink-0 items-center justify-between gap-2 border-t border-black/8 bg-black/[0.015] px-4 py-2 text-[11.5px] text-zinc-400 dark:border-white/10 dark:bg-white/[0.015] dark:text-zinc-500">
                <span>
                    {sftpData && !sftpData.unconfiguredMessage
                        ? `${sftpData.entries.length} ${
                            sftpData.entries.length == 1 ? "item" : "items"
                        }`
                        : "Remote SFTP"}
                </span>
                <span className="truncate">
                    Saves to{" "}
                    <span className="font-medium text-zinc-600 dark:text-zinc-300">
                        {saveFolderName}
                    </span>
                </span>
            </div>
        </div>
    );
}
