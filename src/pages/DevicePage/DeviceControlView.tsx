import {
  ArrowLeft,
  ArrowsClockwise,
  ArrowSquareOut,
  BellRinging,
  CaretRight,
  Check,
  CheckCircle,
  CircleNotch,
  Copy,
  DownloadSimple,
  File,
  FileArchive,
  FileAudio,
  FileImage,
  FilePdf,
  FileText,
  FileVideo,
  Folder,
  FolderLock,
  FolderOpen,
  Keyboard,
  Link,
  MusicNotes,
  PaperPlaneRight,
  Pause,
  Play,
  SkipBack,
  SkipForward,
  Trash,
  UploadSimple,
  WarningCircle,
  X,
} from "@phosphor-icons/react";
import * as Dialog from "@radix-ui/react-dialog";
import * as Popover from "@radix-ui/react-popover";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import {
  animate,
  AnimatePresence,
  motion,
  useMotionValue,
  useTransform,
} from "motion/react";
import {
  type MouseEvent,
  type RefObject,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type {
  DeviceView,
  SftpBrowseResponse,
  SftpFileEntry,
  TransferItem,
} from "../../types";
import {
  formatDurationMs,
  formatFileSize,
  formatShortDate,
  isValidHttpUrl,
} from "../../utils/format";

const SPECIAL_KEY_MAP: Record<string, number> = {
  Backspace: 1,
  Tab: 2,
  ArrowLeft: 4,
  ArrowUp: 5,
  ArrowRight: 6,
  ArrowDown: 7,
  PageUp: 8,
  PageDown: 9,
  Home: 10,
  End: 11,
  Enter: 12,
  Delete: 13,
  Escape: 14,
  F1: 21,
  F2: 22,
  F3: 23,
  F4: 24,
  F5: 25,
  F6: 26,
  F7: 27,
  F8: 28,
  F9: 29,
  F10: 30,
  F11: 31,
  F12: 32,
};

interface DeviceControlViewProps {
  device: DeviceView;
  transfers: TransferItem[];
  artworkSrc: string | null;
  dropZoneRef: RefObject<HTMLDivElement | null>;
  isDraggingOver: boolean;
  justDroppedCount: number;
  onFilesPicked: (count: number) => void;
  highlightedNotification: {
    deviceId: string;
    notificationId: string;
    seq: number;
  } | null;
  rightPaneContainer: HTMLElement | null;
}

export function DeviceControlView({
  device,
  transfers,
  artworkSrc,
  dropZoneRef,
  isDraggingOver,
  justDroppedCount,
  onFilesPicked,
  highlightedNotification,
  rightPaneContainer,
}: DeviceControlViewProps) {
  const [displaySecs, setDisplaySecs] = useState<number>(0);
  const [isShareLinkOpen, setIsShareLinkOpen] = useState<boolean>(false);
  const [shareLinkInput, setShareLinkInput] = useState<string>("");
  const [shareLinkTouchedInvalid, setShareLinkTouchedInvalid] =
    useState<boolean>(false);
  const [isRemoteInputOpen, setIsRemoteInputOpen] = useState<boolean>(false);
  const [isFileBrowserOpen, setIsFileBrowserOpen] = useState<boolean>(false);
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
  const [nowTickMs, setNowTickMs] = useState<number>(() => Date.now());
  const [copiedNotifId, setCopiedNotifId] = useState<string | null>(null);
  const [activeHighlightNotifId, setActiveHighlightNotifId] = useState<
    string | null
  >(null);
  const notifRefs = useRef<Map<string, HTMLDivElement>>(new Map());
  const remoteInputZoneRef = useRef<HTMLDivElement | null>(null);
  const breadcrumbScrollRef = useRef<HTMLDivElement | null>(null);
  const sftpReqSeqRef = useRef<number>(0);

  const seekBarRef = useRef<HTMLDivElement | null>(null);
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

  const nowPlaying = device.nowPlaying;

  useEffect(() => {
    setIsShareLinkOpen(false);
    setShareLinkTouchedInvalid(false);
    setIsRemoteInputOpen(false);
    setIsFileBrowserOpen(false);
    setSftpData(null);
    setSftpError(null);
    setIsSftpLoading(false);
    setDownloadingPaths(new Set());
    setDownloadedPaths(new Map());
    setDownloadErrors(new Map());
    setDeletingPaths(new Set());
  }, [device.deviceId]);

  const loadSftpDirectory = useCallback(
    async (targetPath?: string) => {
      if (!device.reachable || device.pairState !== "Paired") return;
      const reqId = ++sftpReqSeqRef.current;
      setIsSftpLoading(true);
      setSftpError(null);
      try {
        const res = await invoke<SftpBrowseResponse>("sftp_browse", {
          deviceId: device.deviceId,
          path: targetPath ?? null,
        });
        if (sftpReqSeqRef.current !== reqId) return;
        setSftpData(res);
        requestAnimationFrame(() => {
          if (breadcrumbScrollRef.current) {
            breadcrumbScrollRef.current.scrollLeft =
              breadcrumbScrollRef.current.scrollWidth;
          }
        });
      } catch (err) {
        if (sftpReqSeqRef.current !== reqId) return;
        setSftpError(typeof err === "string" ? err : String(err));
      } finally {
        if (sftpReqSeqRef.current === reqId) {
          setIsSftpLoading(false);
        }
      }
    },
    [device.deviceId, device.reachable, device.pairState],
  );

  const sftpBreadcrumbs = useMemo(() => {
    if (!sftpData) return [];
    const current = sftpData.currentPath || "/";
    const roots = sftpData.roots || [];

    if (current === "/" && roots.length > 1) {
      return [{ label: "Storage locations", path: "/" }];
    }

    // Find the deepest matching configured storage root
    const matchedRoot = [...roots]
      .sort((a, b) => b.path.length - a.path.length)
      .find(
        (r) =>
          current === r.path ||
          (r.path !== "/" && current.startsWith(`${r.path}/`)),
      );

    const crumbs: Array<{ label: string; path: string }> = [];
    if (roots.length > 1) {
      crumbs.push({ label: "Storage", path: "/" });
    }

    if (matchedRoot) {
      crumbs.push({ label: matchedRoot.name, path: matchedRoot.path });
      const rel =
        matchedRoot.path === "/"
          ? current.slice(1)
          : current.slice(matchedRoot.path.length);
      const parts = rel.split("/").filter(Boolean);
      let accum = matchedRoot.path === "/" ? "" : matchedRoot.path;
      for (const seg of parts) {
        accum = `${accum}/${seg}`;
        crumbs.push({ label: seg, path: accum });
      }
      return crumbs;
    }

    // Fallback path segmentation from "/"
    crumbs.push({
      label: roots[0]?.name || "Internal storage",
      path: "/",
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
        deviceId: device.deviceId,
        remotePath: entry.path,
        fileName: entry.name,
      });
      setDownloadedPaths((prev) => {
        const next = new Map(prev);
        next.set(entry.path, savedLocalPath);
        return next;
      });
    } catch (err) {
      setDownloadErrors((prev) => {
        const next = new Map(prev);
        next.set(entry.path, typeof err === "string" ? err : String(err));
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
        deviceId: device.deviceId,
        remotePath: entry.path,
        isDir: entry.isDir,
      });
      setSftpData((prev) => {
        if (!prev) return prev;
        return {
          ...prev,
          entries: prev.entries.filter((item) => item.path !== entry.path),
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
        next.set(entry.path, typeof err === "string" ? err : String(err));
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

  const renderFileEntryIcon = (entry: SftpFileEntry) => {
    if (entry.isDir) {
      return (
        <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-blue-500/12 text-blue-600 dark:bg-blue-500/15 dark:text-blue-400">
          <Folder size={17} weight="fill" />
        </div>
      );
    }
    const ext = entry.name.includes(".")
      ? (entry.name.split(".").pop()?.toLowerCase() ?? "")
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
          <FileImage size={17} weight="duotone" />
        </div>
      );
    }
    if (
      ["mp4", "mov", "mkv", "webm", "avi", "3gp", "m4v", "ts"].includes(ext)
    ) {
      return (
        <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-amber-500/12 text-amber-600 dark:bg-amber-500/15 dark:text-amber-400">
          <FileVideo size={17} weight="duotone" />
        </div>
      );
    }
    if (
      ["mp3", "flac", "wav", "m4a", "aac", "ogg", "opus", "wma"].includes(ext)
    ) {
      return (
        <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-rose-500/12 text-rose-600 dark:bg-rose-500/15 dark:text-rose-400">
          <FileAudio size={17} weight="duotone" />
        </div>
      );
    }
    if (ext === "pdf") {
      return (
        <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-red-500/12 text-red-600 dark:bg-red-500/15 dark:text-red-400">
          <FilePdf size={17} weight="duotone" />
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
          <FileArchive size={17} weight="duotone" />
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
          <FileText size={17} weight="duotone" />
        </div>
      );
    }
    return (
      <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-black/[0.05] text-zinc-600 dark:bg-white/[0.07] dark:text-zinc-300">
        <File size={17} weight="duotone" />
      </div>
    );
  };

  useEffect(() => {
    if (
      !isRemoteInputOpen ||
      !device.reachable ||
      device.pairState !== "Paired"
    ) {
      return;
    }

    const handleWindowKeyDown = (e: globalThis.KeyboardEvent) => {
      // Allow Cmd+V / Ctrl+V to trigger the native paste event
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "v") {
        return;
      }

      // Prevent Enter, Space, Escape, Tab, Arrow keys, etc. from triggering buttons or closing the dialog
      e.preventDefault();
      e.stopPropagation();

      if (
        e.key === "Shift" ||
        e.key === "Control" ||
        e.key === "Alt" ||
        e.key === "Meta" ||
        e.key === "CapsLock"
      ) {
        return;
      }

      const specialCode = SPECIAL_KEY_MAP[e.key] ?? 0;
      if (specialCode > 0) {
        invoke("send_remote_key", {
          deviceId: device.deviceId,
          key: "",
          specialKey: specialCode,
          shift: e.shiftKey,
          ctrl: e.ctrlKey,
          alt: e.altKey,
          superKey: e.metaKey,
        }).catch(() => {});
        return;
      }

      if (e.key.length === 1) {
        invoke("send_remote_key", {
          deviceId: device.deviceId,
          key: e.key,
          specialKey: 0,
          shift: e.shiftKey && (e.ctrlKey || e.altKey || e.metaKey),
          ctrl: e.ctrlKey,
          alt: e.altKey,
          superKey: e.metaKey,
        }).catch(() => {});
      }
    };

    const handleWindowPaste = (e: globalThis.ClipboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const pasted = e.clipboardData?.getData("text");
      if (!pasted) return;
      invoke("send_remote_key", {
        deviceId: device.deviceId,
        key: pasted,
        specialKey: 0,
        shift: false,
        ctrl: false,
        alt: false,
        superKey: false,
      }).catch(() => {});
    };

    window.addEventListener("keydown", handleWindowKeyDown, { capture: true });
    window.addEventListener("paste", handleWindowPaste, { capture: true });
    return () => {
      window.removeEventListener("keydown", handleWindowKeyDown, {
        capture: true,
      });
      window.removeEventListener("paste", handleWindowPaste, {
        capture: true,
      });
    };
  }, [isRemoteInputOpen, device.reachable, device.pairState, device.deviceId]);

  useEffect(() => {
    if (
      !highlightedNotification ||
      highlightedNotification.deviceId !== device.deviceId ||
      !highlightedNotification.notificationId
    ) {
      return;
    }

    const targetId = highlightedNotification.notificationId;
    setActiveHighlightNotifId(targetId);

    const scrollTimer = setTimeout(() => {
      const el = notifRefs.current.get(targetId);
      if (el) {
        el.scrollIntoView({ behavior: "smooth", block: "center" });
      }
    }, 120);

    const clearTimer = setTimeout(() => {
      setActiveHighlightNotifId((prev) => (prev === targetId ? null : prev));
    }, 3200);

    return () => {
      clearTimeout(scrollTimer);
      clearTimeout(clearTimer);
    };
  }, [highlightedNotification, device.deviceId]);

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

      if (opt && now - opt.atMs < 1500) {
        const elapsed = nowPlaying.isPlaying ? Math.max(0, now - opt.atMs) : 0;
        return Math.min(
          nowPlaying.lengthMs,
          Math.max(0, opt.targetMs + elapsed),
        );
      }

      if (!nowPlaying.isPlaying) {
        return Math.min(
          nowPlaying.lengthMs,
          Math.max(0, nowPlaying.positionMs),
        );
      }
      const baseTime = nowPlaying.positionUpdatedAtMs || now;
      const elapsed = Math.max(0, now - baseTime);
      return Math.min(
        nowPlaying.lengthMs,
        Math.max(0, nowPlaying.positionMs + elapsed),
      );
    };

    const syncNow = () => {
      const posMs = computePosMs();
      const pct = Math.min(
        100,
        Math.max(0, (posMs / nowPlaying.lengthMs) * 100),
      );
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
  }, [nowPlaying, progressPctMv]);

  const handlePickFilesToSend = async () => {
    if (!device.reachable || device.pairState !== "Paired") {
      return;
    }
    const selected = await open({
      multiple: true,
      directory: false,
      title: `Send file(s) to ${device.deviceName}`,
    });
    if (selected) {
      const paths = Array.isArray(selected) ? selected : [selected];
      if (paths.length > 0) {
        onFilesPicked(paths.length);
        try {
          await invoke("send_files", {
            deviceId: device.deviceId,
            filePaths: paths,
          });
        } catch (err) {
          console.error("Failed to send selected files:", err);
        }
      }
    }
  };

  // Refresh timestamp after recent transfers finish so completed bars stay visible for ~2.2s and then fade out
  useEffect(() => {
    setNowTickMs(Date.now());
    const hasRecentCompleted = transfers.some(
      (t) =>
        t.deviceId === device.deviceId &&
        t.kind === "File" &&
        t.status === "Completed" &&
        Date.now() - t.timestampMs < 2400,
    );
    if (!hasRecentCompleted) return;
    const timer = setTimeout(() => setNowTickMs(Date.now()), 2300);
    return () => clearTimeout(timer);
  }, [transfers, device.deviceId]);

  const handleMediaAction = async (action: string) => {
    if (!device.nowPlaying) return;
    try {
      await invoke("send_media_action", {
        deviceId: device.deviceId,
        player: device.nowPlaying.player,
        action,
      });
    } catch (err) {
      console.error("Failed to send media action:", err);
    }
  };

  const handleSeekClick = async (e: MouseEvent<HTMLDivElement>) => {
    if (!device.nowPlaying || device.nowPlaying.lengthMs <= 0) {
      return;
    }
    const bar = seekBarRef.current;
    if (!bar) return;

    const rect = bar.getBoundingClientRect();
    const ratio = Math.min(
      1,
      Math.max(0, (e.clientX - rect.left) / rect.width),
    );
    const targetMs = Math.round(ratio * device.nowPlaying.lengthMs);
    const targetPct = ratio * 100;

    optimisticSeekRef.current = {
      player: device.nowPlaying.player,
      title: device.nowPlaying.title,
      targetMs,
      atMs: Date.now(),
    };
    setDisplaySecs(Math.floor(targetMs / 1000));

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
        deviceId: device.deviceId,
        player: device.nowPlaying.player,
        positionMs: targetMs,
      });
    } catch (err) {
      console.error("Failed to seek media:", err);
    }
  };

  const isShareLinkInvalid =
    (shareLinkInput.trim().length > 0 && !isValidHttpUrl(shareLinkInput)) ||
    shareLinkTouchedInvalid;

  const handleSendShareLink = async () => {
    if (!device.reachable || device.pairState !== "Paired") {
      return;
    }
    const trimmed = shareLinkInput.trim();
    if (!isValidHttpUrl(trimmed)) {
      setShareLinkTouchedInvalid(true);
      return;
    }
    try {
      await invoke("send_url", {
        deviceId: device.deviceId,
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

  return (
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
      {device.pairState !== "Paired" && (
        <div className="mt-6 flex items-center justify-between gap-4 rounded-2xl border border-amber-500/25 bg-amber-500/[0.08] px-4 py-3 text-sm">
          <div className="flex flex-col">
            <span className="font-medium text-amber-600 dark:text-amber-300">
              {device.pairState === "Requested"
                ? "Pairing request sent — confirm on your device"
                : device.pairState === "RequestedByPeer"
                  ? "Incoming pairing request from this device"
                  : "Device is not paired yet"}
            </span>
            {device.verificationKey &&
              (device.pairState === "Requested" ||
                device.pairState === "RequestedByPeer") && (
                <span className="mt-0.5 font-mono text-xs text-amber-700/80 dark:text-amber-200/80">
                  Verification Key: {device.verificationKey}
                </span>
              )}
          </div>

          <div className="flex shrink-0 items-center gap-2">
            {device.pairState === "NotPaired" && device.reachable && (
              <motion.button
                type="button"
                whileHover={{ scale: 1.03 }}
                whileTap={{ scale: 0.97 }}
                onClick={() =>
                  invoke("request_pairing", {
                    deviceId: device.deviceId,
                  })
                }
                className="cursor-pointer rounded-xl bg-blue-600 px-3.5 py-1.5 text-xs font-medium text-white transition-colors hover:bg-blue-500"
              >
                Pair Device
              </motion.button>
            )}
            {device.pairState === "Requested" && (
              <motion.button
                type="button"
                whileHover={{ scale: 1.03 }}
                whileTap={{ scale: 0.97 }}
                onClick={() =>
                  invoke("reject_pairing", {
                    deviceId: device.deviceId,
                  })
                }
                className="cursor-pointer rounded-xl bg-red-500/20 px-3.5 py-1.5 text-xs font-medium text-red-600 dark:text-red-300"
              >
                Cancel
              </motion.button>
            )}
            {device.pairState === "RequestedByPeer" && (
              <>
                <motion.button
                  type="button"
                  whileHover={{ scale: 1.03 }}
                  whileTap={{ scale: 0.97 }}
                  onClick={() =>
                    invoke("accept_pairing", {
                      deviceId: device.deviceId,
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
                      deviceId: device.deviceId,
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
        {device.reachable && device.nowPlaying && device.nowPlaying.title && (
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
                        alt={device.nowPlaying.title}
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
                        {device.nowPlaying.title}
                      </span>
                      {device.nowPlaying.player && (
                        <span className="shrink-0 rounded-md bg-black/[0.07] px-1.5 py-0.5 text-[10.5px] font-medium text-zinc-700 backdrop-blur-md dark:bg-white/[0.10] dark:text-zinc-300">
                          {device.nowPlaying.player}
                        </span>
                      )}
                    </div>

                    <span className="mt-0.5 truncate text-[12.5px] text-zinc-600 dark:text-zinc-400">
                      {[device.nowPlaying.artist, device.nowPlaying.album]
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
                    disabled={!device.nowPlaying.canGoPrevious}
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
                    title={device.nowPlaying.isPlaying ? "Pause" : "Play"}
                  >
                    {device.nowPlaying.isPlaying ? (
                      <Pause size={16} weight="fill" />
                    ) : (
                      <Play size={16} weight="fill" />
                    )}
                  </motion.button>

                  <motion.button
                    type="button"
                    whileHover={{ scale: 1.08 }}
                    whileTap={{ scale: 0.92 }}
                    disabled={!device.nowPlaying.canGoNext}
                    onClick={() => handleMediaAction("Next")}
                    className="flex h-8 w-8 cursor-pointer items-center justify-center rounded-lg text-zinc-700 transition-colors hover:bg-black/[0.08] hover:text-zinc-950 disabled:opacity-40 dark:text-zinc-200 dark:hover:bg-white/[0.10] dark:hover:text-white"
                    title="Next Track"
                  >
                    <SkipForward size={16} weight="fill" />
                  </motion.button>
                </div>
              </div>

              {/* Interactive Seek Progress Bar */}
              {device.nowPlaying.lengthMs > 0 && (
                <div className="relative z-10 flex items-center gap-2.5 pt-0.5 text-[11px] font-medium tabular-nums text-zinc-500 dark:text-zinc-400">
                  <span className="w-8 text-left">
                    {formatDurationMs(displaySecs * 1000)}
                  </span>

                  <div
                    ref={seekBarRef}
                    onClick={handleSeekClick}
                    className="group relative flex flex-1 cursor-pointer items-center py-2"
                  >
                    <div className="relative h-1 w-full overflow-hidden rounded-full bg-black/[0.10] dark:bg-white/[0.12]">
                      <motion.div
                        style={{ width: progressPctStyle }}
                        className="h-full rounded-full bg-zinc-800 dark:bg-zinc-200"
                      />
                    </div>

                    <motion.div
                      style={{ left: progressPctStyle }}
                      className="pointer-events-none absolute top-1/2 h-2.5 w-2.5 -translate-x-1/2 -translate-y-1/2 rounded-full bg-zinc-800 opacity-0 transition-opacity duration-150 group-hover:opacity-100 dark:bg-zinc-200"
                    />
                  </div>

                  <span className="w-8 text-right">
                    {formatDurationMs(device.nowPlaying.lengthMs)}
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
        onClick={device.reachable ? handlePickFilesToSend : undefined}
        animate={{
          scale: device.reachable && isDraggingOver ? 1.015 : 1,
        }}
        whileHover={device.reachable ? { scale: 1.005 } : undefined}
        whileTap={device.reachable ? { scale: 0.99 } : undefined}
        transition={{
          type: "spring",
          stiffness: 420,
          damping: 30,
        }}
        className={`group relative mt-6 flex flex-col items-center justify-center overflow-hidden rounded-2xl border-2 border-dotted px-6 py-8 text-center backdrop-blur-md transition-all duration-200 ${
          !device.reachable
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
                {device.deviceName}...
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
                  ? `Drop to send to ${device.deviceName}`
                  : `Drop files to send to ${device.deviceName}`}
              </span>
              <span className="mt-1 text-[12.5px] text-zinc-500 dark:text-zinc-400">
                or click to browse from your Mac
              </span>
            </motion.div>
          )}
        </AnimatePresence>

        {/* Per-file transfer progress bars inside the drop card */}
        {(() => {
          const deviceFileTransfers = transfers.filter(
            (t) => t.deviceId === device.deviceId && t.kind === "File",
          );
          const anyInProgress = deviceFileTransfers.some(
            (t) => t.status === "InProgress",
          );
          const visibleTransfers = deviceFileTransfers
            .filter(
              (t) =>
                t.status === "InProgress" ||
                (t.status === "Completed" &&
                  (anyInProgress
                    ? nowTickMs - t.timestampMs < 15000
                    : nowTickMs - t.timestampMs < 2200)),
            )
            .slice(0, 6);

          if (visibleTransfers.length === 0) return null;

          return (
            <div className="mt-5 flex w-full max-w-md flex-col gap-2.5">
              <AnimatePresence initial={false}>
                {visibleTransfers.map((item) => {
                  const pct =
                    item.status === "Completed"
                      ? 100
                      : item.totalBytes > 0
                        ? Math.min(
                            100,
                            Math.round(
                              (item.transferredBytes / item.totalBytes) * 100,
                            ),
                          )
                        : 0;
                  const isDone = item.status === "Completed";

                  return (
                    <motion.div
                      key={item.id}
                      layout
                      initial={{ opacity: 0, y: 6 }}
                      animate={{ opacity: 1, y: 0 }}
                      exit={{ opacity: 0, height: 0 }}
                      transition={{
                        type: "spring",
                        stiffness: 460,
                        damping: 34,
                      }}
                      className="flex w-full flex-col gap-1"
                    >
                      <div className="flex items-center justify-between gap-2 text-[11.5px] font-medium text-zinc-600 dark:text-zinc-300">
                        <span className="truncate">
                          {isDone
                            ? "Sent"
                            : item.direction === "Outgoing"
                              ? "Sending"
                              : "Receiving"}{" "}
                          {item.filename}
                        </span>
                        <span className="shrink-0 font-mono text-[11px] tabular-nums">
                          {pct}%
                        </span>
                      </div>
                      <div className="h-1.5 w-full overflow-hidden rounded-full bg-black/10 dark:bg-white/10">
                        <motion.div
                          className={`h-full rounded-full transition-colors duration-200 ${
                            isDone ? "bg-emerald-500" : "bg-blue-500"
                          }`}
                          initial={{ width: "0%" }}
                          animate={{ width: `${pct}%` }}
                          transition={{ ease: "easeOut", duration: 0.18 }}
                        />
                      </div>
                    </motion.div>
                  );
                })}
              </AnimatePresence>
            </div>
          );
        })()}
      </motion.div>

      {/* Quick actions */}
      <div
        className={`mt-6 flex flex-col gap-2.5 transition-opacity duration-200 ${
          !device.reachable ? "pointer-events-none opacity-45 grayscale" : ""
        }`}
      >
        <span className="text-[12.5px] font-medium text-zinc-500 dark:text-zinc-400">
          Quick actions
        </span>

        <div className="grid grid-cols-3 gap-3">
          <Popover.Root
            open={device.reachable && isShareLinkOpen}
            onOpenChange={(openState) => {
              setIsShareLinkOpen(openState);
              if (!openState) {
                setShareLinkTouchedInvalid(false);
              }
            }}
          >
            <Popover.Trigger asChild>
              <motion.button
                type="button"
                whileHover={device.reachable ? { scale: 1.015 } : undefined}
                whileTap={device.reachable ? { scale: 0.985 } : undefined}
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
              {device.reachable && isShareLinkOpen && (
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

          <motion.button
            type="button"
            disabled={device.isRinging}
            onClick={() => {
              if (
                !device.reachable ||
                device.pairState !== "Paired" ||
                device.isRinging
              ) {
                return;
              }
              invoke("ring_device", { deviceId: device.deviceId }).catch(
                (err) => {
                  console.error("Failed to ring device:", err);
                },
              );
            }}
            animate={
              device.isRinging
                ? {
                    x: [0, -2.5, 2.5, -2, 2, 0],
                  }
                : { x: 0 }
            }
            whileHover={
              device.reachable && !device.isRinging
                ? { scale: 1.015 }
                : undefined
            }
            whileTap={
              device.reachable && !device.isRinging
                ? { scale: 0.985 }
                : undefined
            }
            transition={
              device.isRinging
                ? {
                    duration: 0.48,
                    repeat: Infinity,
                    repeatDelay: 0.35,
                    ease: "easeInOut",
                  }
                : {
                    type: "spring",
                    stiffness: 450,
                    damping: 28,
                  }
            }
            className={`flex items-center justify-center gap-2 rounded-xl border px-4 py-2.5 text-[13.5px] font-medium transition-colors ${
              device.isRinging
                ? "cursor-default border-blue-500/35 bg-blue-500/[0.08] text-blue-600 dark:border-blue-400/35 dark:bg-blue-500/[0.12] dark:text-blue-400"
                : "cursor-pointer border-black/12 bg-black/[0.02] text-zinc-800 hover:border-black/20 hover:bg-black/[0.05] dark:border-white/12 dark:bg-white/[0.02] dark:text-zinc-100 dark:hover:border-white/20 dark:hover:bg-white/[0.05]"
            }`}
          >
            <motion.span
              animate={
                device.isRinging
                  ? { rotate: [0, -18, 18, -14, 14, -8, 8, 0] }
                  : { rotate: 0 }
              }
              transition={
                device.isRinging
                  ? {
                      duration: 0.48,
                      repeat: Infinity,
                      repeatDelay: 0.35,
                      ease: "easeInOut",
                    }
                  : { duration: 0.15 }
              }
              className="flex items-center"
            >
              <BellRinging
                size={16}
                weight={device.isRinging ? "fill" : "regular"}
              />
            </motion.span>
            <span>{device.isRinging ? "Ringing" : "Ring phone"}</span>
          </motion.button>

          {/* 3rd Quick Action: Remote input Radix Dialog */}
          <Dialog.Root open={device.reachable && isRemoteInputOpen}>
            <Dialog.Trigger asChild>
              <motion.button
                type="button"
                onClick={() => {
                  if (device.reachable && device.pairState === "Paired") {
                    setIsRemoteInputOpen(true);
                  }
                }}
                whileHover={device.reachable ? { scale: 1.015 } : undefined}
                whileTap={device.reachable ? { scale: 0.985 } : undefined}
                transition={{
                  type: "spring",
                  stiffness: 450,
                  damping: 28,
                }}
                className="flex cursor-pointer items-center justify-center gap-2 rounded-xl border border-black/12 bg-black/[0.02] px-4 py-2.5 text-[13.5px] font-medium text-zinc-800 transition-colors hover:border-black/20 hover:bg-black/[0.05] data-[state=open]:border-black/25 data-[state=open]:bg-black/[0.06] dark:border-white/12 dark:bg-white/[0.02] dark:text-zinc-100 dark:hover:border-white/20 dark:hover:bg-white/[0.05] dark:data-[state=open]:border-white/25 dark:data-[state=open]:bg-white/[0.07]"
              >
                <Keyboard size={16} weight="regular" />
                <span>Remote input</span>
              </motion.button>
            </Dialog.Trigger>

            <AnimatePresence>
              {device.reachable && isRemoteInputOpen && (
                <Dialog.Portal forceMount container={rightPaneContainer}>
                  <Dialog.Overlay asChild forceMount>
                    <motion.div
                      initial={{ opacity: 0 }}
                      animate={{ opacity: 1 }}
                      exit={{ opacity: 0 }}
                      transition={{ duration: 0.2, ease: "easeOut" }}
                      className="absolute inset-0 z-40 bg-black/35 backdrop-blur-xs dark:bg-black/55"
                    />
                  </Dialog.Overlay>

                  <Dialog.Content
                    asChild
                    forceMount
                    onOpenAutoFocus={(e) => {
                      e.preventDefault();
                      remoteInputZoneRef.current?.focus();
                    }}
                    onEscapeKeyDown={(e) => {
                      e.preventDefault();
                    }}
                    onPointerDownOutside={(e) => {
                      e.preventDefault();
                    }}
                    onInteractOutside={(e) => {
                      e.preventDefault();
                    }}
                  >
                    <motion.div
                      initial={{ opacity: 0, scale: 0.95, y: 10 }}
                      animate={{ opacity: 1, scale: 1, y: 0 }}
                      exit={{ opacity: 0, scale: 0.96, y: 8 }}
                      transition={{
                        type: "spring",
                        stiffness: 480,
                        damping: 34,
                      }}
                      className="absolute top-1/2 left-1/2 z-50 flex w-[88%] max-w-md -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-2xl border border-black/12 bg-white p-5 shadow-2xl outline-none dark:border-white/15 dark:bg-zinc-900"
                    >
                      {/* Modal Header */}
                      <div className="flex items-center justify-between gap-3">
                        <Dialog.Title className="text-[15.5px] font-semibold text-zinc-900 dark:text-zinc-100">
                          Remote input
                        </Dialog.Title>

                        <motion.button
                          type="button"
                          tabIndex={-1}
                          whileHover={{ scale: 1.06 }}
                          whileTap={{ scale: 0.94 }}
                          onClick={() => setIsRemoteInputOpen(false)}
                          className="flex h-7 w-7 cursor-pointer items-center justify-center rounded-lg text-zinc-400 transition-colors hover:bg-black/[0.06] hover:text-zinc-700 dark:hover:bg-white/[0.08] dark:hover:text-zinc-200"
                          aria-label="Close"
                        >
                          <X size={15} weight="bold" />
                        </motion.button>
                      </div>

                      <Dialog.Description className="sr-only">
                        Capturing keyboard input for {device.deviceName}
                      </Dialog.Description>

                      {/* Dotted Keystroke Capture Zone matching the File Drop Zone */}
                      <div
                        ref={remoteInputZoneRef}
                        tabIndex={0}
                        className="mt-4 flex flex-col items-center justify-center rounded-2xl border-2 border-dotted border-black/20 bg-black/[0.02] px-6 py-9 text-center outline-none dark:border-white/20 dark:bg-white/[0.03]"
                      >
                        <div className="flex h-12 w-12 items-center justify-center rounded-2xl bg-black/[0.05] text-zinc-700 dark:bg-white/[0.08] dark:text-zinc-200">
                          <Keyboard size={22} weight="regular" />
                        </div>
                        <p className="mt-3 text-[13.5px] font-medium text-zinc-700 dark:text-zinc-200">
                          Capturing keystrokes for {device.deviceName}
                        </p>
                        <p className="mt-1 text-xs text-zinc-400 dark:text-zinc-500">
                          Switch to KDE Connect Remote Keyboard on your phone
                          and start typing
                        </p>
                      </div>
                    </motion.div>
                  </Dialog.Content>
                </Dialog.Portal>
              )}
            </AnimatePresence>
          </Dialog.Root>
        </div>

        {/* 4th Quick Action (full-width below grid-cols-3): Browse files SFTP Radix Dialog */}
        <Dialog.Root
          open={device.reachable && isFileBrowserOpen}
          onOpenChange={(nextOpen) => {
            setIsFileBrowserOpen(nextOpen);
            if (nextOpen && !sftpData && !isSftpLoading) {
              loadSftpDirectory();
            }
          }}
        >
          <Dialog.Trigger asChild>
            <motion.button
              type="button"
              onClick={() => {
                if (device.reachable && device.pairState === "Paired") {
                  setIsFileBrowserOpen(true);
                  if (!sftpData && !isSftpLoading) {
                    loadSftpDirectory();
                  }
                }
              }}
              whileHover={device.reachable ? { scale: 1.008 } : undefined}
              whileTap={device.reachable ? { scale: 0.992 } : undefined}
              transition={{
                type: "spring",
                stiffness: 450,
                damping: 28,
              }}
              className="flex w-full cursor-pointer items-center justify-center gap-2 rounded-xl border border-black/12 bg-black/[0.02] px-4 py-2.5 text-[13.5px] font-medium text-zinc-800 transition-colors hover:border-black/20 hover:bg-black/[0.05] data-[state=open]:border-black/25 data-[state=open]:bg-black/[0.06] dark:border-white/12 dark:bg-white/[0.02] dark:text-zinc-100 dark:hover:border-white/20 dark:hover:bg-white/[0.05] dark:data-[state=open]:border-white/25 dark:data-[state=open]:bg-white/[0.07]"
            >
              <FolderOpen size={16} weight="regular" />
              <span>Browse files</span>
            </motion.button>
          </Dialog.Trigger>

          <AnimatePresence>
            {device.reachable && isFileBrowserOpen && (
              <Dialog.Portal forceMount container={rightPaneContainer}>
                <Dialog.Overlay asChild forceMount>
                  <motion.div
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    exit={{ opacity: 0 }}
                    transition={{ duration: 0.2, ease: "easeOut" }}
                    className="absolute inset-0 z-40 bg-black/35 backdrop-blur-xs dark:bg-black/55"
                  />
                </Dialog.Overlay>

                <Dialog.Content asChild forceMount>
                  <motion.div
                    initial={{ opacity: 0, scale: 0.95, y: 10 }}
                    animate={{ opacity: 1, scale: 1, y: 0 }}
                    exit={{ opacity: 0, scale: 0.96, y: 8 }}
                    transition={{
                      type: "spring",
                      stiffness: 480,
                      damping: 34,
                    }}
                    className="absolute top-1/2 left-1/2 z-50 flex h-[440px] w-[90%] max-w-lg -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-2xl border border-black/12 bg-white shadow-2xl outline-none dark:border-white/15 dark:bg-zinc-900"
                  >
                    {/* Dialog Header: Back + Breadcrumbs + Refresh + Close */}
                    <div className="flex items-center justify-between gap-2 border-b border-black/8 px-4 py-3 dark:border-white/10">
                      <div className="flex min-w-0 flex-1 items-center gap-1.5">
                        <motion.button
                          type="button"
                          disabled={!canSftpGoBack || isSftpLoading}
                          whileHover={
                            canSftpGoBack && !isSftpLoading
                              ? { scale: 1.06 }
                              : undefined
                          }
                          whileTap={
                            canSftpGoBack && !isSftpLoading
                              ? { scale: 0.94 }
                              : undefined
                          }
                          onClick={handleSftpGoBack}
                          className={`flex h-7 w-7 shrink-0 items-center justify-center rounded-lg transition-colors ${
                            canSftpGoBack && !isSftpLoading
                              ? "cursor-pointer text-zinc-700 hover:bg-black/[0.06] dark:text-zinc-200 dark:hover:bg-white/[0.08]"
                              : "cursor-default text-zinc-300 dark:text-zinc-600"
                          }`}
                          title="Back"
                        >
                          <ArrowLeft size={15} weight="bold" />
                        </motion.button>

                        <Dialog.Title className="sr-only">
                          Browse files on {device.deviceName}
                        </Dialog.Title>
                        <Dialog.Description className="sr-only">
                          Browse folders and download files from{" "}
                          {device.deviceName}
                        </Dialog.Description>

                        {/* Scrollable Breadcrumb Bar */}
                        <div
                          ref={breadcrumbScrollRef}
                          className="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto whitespace-nowrap [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
                        >
                          {sftpBreadcrumbs.length > 0 ? (
                            sftpBreadcrumbs.map((crumb, idx) => {
                              const isLast = idx === sftpBreadcrumbs.length - 1;
                              return (
                                <div
                                  key={`${crumb.path}-${idx}`}
                                  className="flex shrink-0 items-center gap-1"
                                >
                                  {idx > 0 && (
                                    <CaretRight
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
                              {device.deviceName} files
                            </span>
                          )}
                        </div>
                      </div>

                      <div className="flex shrink-0 items-center gap-1">
                        <motion.button
                          type="button"
                          disabled={isSftpLoading}
                          whileHover={
                            !isSftpLoading ? { scale: 1.06 } : undefined
                          }
                          whileTap={
                            !isSftpLoading ? { scale: 0.94 } : undefined
                          }
                          onClick={() =>
                            loadSftpDirectory(sftpData?.currentPath)
                          }
                          className={`flex h-7 w-7 items-center justify-center rounded-lg text-zinc-500 transition-colors hover:bg-black/[0.06] hover:text-zinc-800 dark:text-zinc-400 dark:hover:bg-white/[0.08] dark:hover:text-zinc-200 ${
                            isSftpLoading
                              ? "cursor-default opacity-60"
                              : "cursor-pointer"
                          }`}
                          title="Refresh folder"
                        >
                          <ArrowsClockwise
                            size={15}
                            weight="bold"
                            className={isSftpLoading ? "animate-spin" : ""}
                          />
                        </motion.button>

                        <Dialog.Close asChild>
                          <motion.button
                            type="button"
                            whileHover={{ scale: 1.06 }}
                            whileTap={{ scale: 0.94 }}
                            className="flex h-7 w-7 cursor-pointer items-center justify-center rounded-lg text-zinc-400 transition-colors hover:bg-black/[0.06] hover:text-zinc-700 dark:hover:bg-white/[0.08] dark:hover:text-zinc-200"
                            aria-label="Close"
                          >
                            <X size={15} weight="bold" />
                          </motion.button>
                        </Dialog.Close>
                      </div>
                    </div>

                    {/* Dialog Body */}
                    <div className="flex-1 overflow-y-auto p-2.5">
                      {isSftpLoading && !sftpData ? (
                        <div className="flex h-full flex-col items-center justify-center gap-2.5 px-6 text-center">
                          <CircleNotch
                            size={24}
                            weight="bold"
                            className="animate-spin text-blue-600 dark:text-blue-400"
                          />
                          <p className="text-[13px] font-medium text-zinc-700 dark:text-zinc-200">
                            Connecting to {device.deviceName} storage...
                          </p>
                        </div>
                      ) : sftpData?.unconfiguredMessage ? (
                        <div className="flex h-full flex-col items-center justify-center px-6 text-center">
                          <div className="flex h-12 w-12 items-center justify-center rounded-2xl bg-amber-500/12 text-amber-600 dark:bg-amber-500/15 dark:text-amber-400">
                            <FolderLock size={24} weight="duotone" />
                          </div>
                          <p className="mt-3 text-[13.5px] font-semibold text-zinc-800 dark:text-zinc-100">
                            Storage access not configured
                          </p>
                          <p className="mt-1.5 max-w-xs text-xs leading-relaxed text-zinc-500 dark:text-zinc-400">
                            Open{" "}
                            <span className="font-medium text-zinc-700 dark:text-zinc-200">
                              KDE Connect
                            </span>{" "}
                            on {device.deviceName} →{" "}
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
                            <WarningCircle size={24} weight="duotone" />
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
                            onClick={() =>
                              loadSftpDirectory(sftpData?.currentPath)
                            }
                            className="mt-4 cursor-pointer rounded-xl border border-black/12 bg-black/[0.03] px-4 py-1.5 text-xs font-medium text-zinc-800 transition-colors hover:bg-black/[0.06] dark:border-white/12 dark:bg-white/[0.05] dark:text-zinc-100 dark:hover:bg-white/[0.09]"
                          >
                            Retry
                          </motion.button>
                        </div>
                      ) : sftpData && sftpData.entries.length === 0 ? (
                        <div className="flex h-full flex-col items-center justify-center px-6 text-center">
                          <div className="flex h-11 w-11 items-center justify-center rounded-2xl bg-black/[0.04] text-zinc-400 dark:bg-white/[0.05] dark:text-zinc-500">
                            <FolderOpen size={22} weight="regular" />
                          </div>
                          <p className="mt-2.5 text-[13px] font-medium text-zinc-500 dark:text-zinc-400">
                            This folder is empty
                          </p>
                        </div>
                      ) : (
                        <div
                          className={`flex flex-col gap-0.5 transition-opacity duration-150 ${
                            isSftpLoading
                              ? "pointer-events-none opacity-60"
                              : ""
                          }`}
                        >
                          {sftpData?.entries.map((entry) => {
                            const isDownloading = downloadingPaths.has(
                              entry.path,
                            );
                            const isDeleting = deletingPaths.has(entry.path);
                            const savedLocalPath = downloadedPaths.get(
                              entry.path,
                            );
                            const dlError = downloadErrors.get(entry.path);
                            const formattedDate = formatShortDate(
                              entry.modifiedSecs,
                            );
                            const isRootStorageEntry =
                              sftpData.currentPath === "/" &&
                              sftpData.roots.some((r) => r.path === entry.path);

                            if (entry.isDir) {
                              return (
                                <div
                                  key={entry.path}
                                  className="group flex w-full items-center justify-between gap-2 rounded-xl px-2.5 py-2 transition-colors hover:bg-black/[0.04] dark:hover:bg-white/[0.05]"
                                >
                                  <button
                                    type="button"
                                    disabled={isDeleting}
                                    onClick={() =>
                                      loadSftpDirectory(entry.path)
                                    }
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
                                          <CircleNotch
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
                                          onClick={() =>
                                            handleSftpDelete(entry)
                                          }
                                          className="flex h-7 w-7 cursor-pointer items-center justify-center rounded-lg border border-black/10 bg-black/[0.02] text-zinc-500 transition-colors hover:border-red-500/30 hover:bg-red-500/10 hover:text-red-600 dark:border-white/10 dark:bg-white/[0.03] dark:text-zinc-400 dark:hover:border-red-400/30 dark:hover:bg-red-500/15 dark:hover:text-red-400"
                                          title="Delete folder"
                                        >
                                          <Trash size={14} weight="regular" />
                                        </motion.button>
                                      ))}

                                    <button
                                      type="button"
                                      disabled={isDeleting}
                                      onClick={() =>
                                        loadSftpDirectory(entry.path)
                                      }
                                      className="flex h-7 w-5 cursor-pointer items-center justify-center"
                                      aria-label={`Open ${entry.name}`}
                                    >
                                      <CaretRight
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
                                            formattedDate
                                              ? ` · ${formattedDate}`
                                              : ""
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
                                      <CircleNotch
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
                                        revealItemInDir(savedLocalPath).catch(
                                          () => {},
                                        );
                                      }}
                                      className="flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded-lg bg-emerald-500/12 text-emerald-600 transition-colors hover:bg-emerald-500/20 dark:bg-emerald-500/15 dark:text-emerald-400 dark:hover:bg-emerald-500/25"
                                      title="Saved — click to reveal in Finder"
                                    >
                                      <Check size={15} weight="bold" />
                                    </motion.button>
                                  ) : (
                                    <motion.button
                                      type="button"
                                      disabled={isDeleting}
                                      whileHover={
                                        !isDeleting
                                          ? { scale: 1.06 }
                                          : undefined
                                      }
                                      whileTap={
                                        !isDeleting
                                          ? { scale: 0.94 }
                                          : undefined
                                      }
                                      onClick={() => handleSftpDownload(entry)}
                                      className="flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded-lg border border-black/10 bg-black/[0.02] text-zinc-600 transition-colors hover:border-black/20 hover:bg-black/[0.06] hover:text-zinc-900 dark:border-white/10 dark:bg-white/[0.03] dark:text-zinc-300 dark:hover:border-white/20 dark:hover:bg-white/[0.08] dark:hover:text-zinc-100"
                                      title="Download file"
                                    >
                                      <DownloadSimple
                                        size={15}
                                        weight="regular"
                                      />
                                    </motion.button>
                                  )}

                                  {isDeleting ? (
                                    <div
                                      className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-red-500/10 text-red-600 dark:bg-red-500/15 dark:text-red-400"
                                      title="Deleting file..."
                                    >
                                      <CircleNotch
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
                                        !isDownloading
                                          ? { scale: 1.06 }
                                          : undefined
                                      }
                                      whileTap={
                                        !isDownloading
                                          ? { scale: 0.94 }
                                          : undefined
                                      }
                                      onClick={() => handleSftpDelete(entry)}
                                      className="flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded-lg border border-black/10 bg-black/[0.02] text-zinc-500 transition-colors hover:border-red-500/30 hover:bg-red-500/10 hover:text-red-600 dark:border-white/10 dark:bg-white/[0.03] dark:text-zinc-400 dark:hover:border-red-400/30 dark:hover:bg-red-500/15 dark:hover:text-red-400"
                                      title="Delete file"
                                    >
                                      <Trash size={14} weight="regular" />
                                    </motion.button>
                                  )}
                                </div>
                              </div>
                            );
                          })}
                        </div>
                      )}
                    </div>

                    {/* Dialog Footer: Item count + Save folder */}
                    <div className="flex items-center justify-between gap-2 border-t border-black/8 bg-black/[0.015] px-4 py-2 text-[11.5px] text-zinc-400 dark:border-white/10 dark:bg-white/[0.015] dark:text-zinc-500">
                      <span>
                        {sftpData && !sftpData.unconfiguredMessage
                          ? `${sftpData.entries.length} ${
                              sftpData.entries.length === 1 ? "item" : "items"
                            }`
                          : "Remote SFTP"}
                      </span>
                      <span className="truncate">
                        Saves to{" "}
                        <span className="font-medium text-zinc-600 dark:text-zinc-300">
                          {device.saveLocation
                            ?.split("/")
                            .filter(Boolean)
                            .pop() || "Downloads"}
                        </span>
                      </span>
                    </div>
                  </motion.div>
                </Dialog.Content>
              </Dialog.Portal>
            )}
          </AnimatePresence>
        </Dialog.Root>
      </div>

      {/* Notifications Section */}
      {device.syncNotifications && (
        <div
          className={`mt-6 flex flex-col gap-2.5 transition-opacity duration-200 ${
            !device.reachable ? "pointer-events-none opacity-45 grayscale" : ""
          }`}
        >
          <span className="text-[12.5px] font-medium text-zinc-500 dark:text-zinc-400">
            Notifications
          </span>

          {device.notifications && device.notifications.length > 0 ? (
            <div className="flex flex-col gap-2">
              <AnimatePresence initial={false}>
                {device.notifications.map((notif) => {
                  const displayBody =
                    notif.text ||
                    (notif.ticker !== notif.title ? notif.ticker : "");
                  const isSensitiveHidden = displayBody
                    .toLowerCase()
                    .includes("sensitive notification content hidden");
                  const isCopied = copiedNotifId === notif.id;
                  const isHighlighted = activeHighlightNotifId === notif.id;

                  return (
                    <motion.div
                      key={notif.id}
                      ref={(el) => {
                        if (el) {
                          notifRefs.current.set(notif.id, el);
                        } else {
                          notifRefs.current.delete(notif.id);
                        }
                      }}
                      layout
                      initial={{ opacity: 0, y: 6, scale: 0.98 }}
                      animate={{ opacity: 1, y: 0, scale: 1 }}
                      exit={{ opacity: 0, height: 0, scale: 0.97 }}
                      transition={{
                        type: "spring",
                        stiffness: 460,
                        damping: 34,
                      }}
                      className={`flex items-start justify-between gap-3 rounded-xl border px-3.5 py-2.5 transition-all duration-300 ${
                        isHighlighted
                          ? "border-blue-500/60 bg-blue-500/[0.08] ring-2 ring-blue-500/25 dark:border-blue-400/60 dark:bg-blue-500/[0.12] dark:ring-blue-400/25"
                          : "border-black/10 bg-black/[0.02] dark:border-white/10 dark:bg-white/[0.02]"
                      }`}
                    >
                      <div className="flex min-w-0 flex-1 flex-col">
                        <div className="flex items-center gap-2">
                          <span className="rounded-md bg-black/[0.06] px-1.5 py-0.5 text-[10.5px] font-medium text-zinc-600 dark:bg-white/[0.08] dark:text-zinc-300">
                            {notif.appName}
                          </span>
                          {notif.title && notif.title !== notif.appName && (
                            <span className="truncate text-[13px] font-semibold text-zinc-900 dark:text-zinc-100">
                              {notif.title}
                            </span>
                          )}
                        </div>

                        {displayBody && (
                          <p className="mt-1 line-clamp-2 text-[12.5px] text-zinc-600 dark:text-zinc-400">
                            {displayBody}
                          </p>
                        )}

                        {isSensitiveHidden && (
                          <button
                            type="button"
                            onClick={() => {
                              openUrl(
                                "https://github.com/vsnthdev/Velquo/blob/main/docs/sensitive-notifications.md",
                              ).catch((err) => {
                                console.error("Failed to open docs URL:", err);
                              });
                            }}
                            className="mt-1 inline-flex w-fit cursor-pointer items-center gap-1 text-[11.5px] font-medium text-blue-600 underline decoration-blue-500/40 underline-offset-2 transition-colors hover:text-blue-500 dark:text-blue-400 dark:hover:text-blue-300"
                          >
                            <span>See how to remove this</span>
                            <ArrowSquareOut size={12} weight="regular" />
                          </button>
                        )}
                      </div>

                      <div className="flex shrink-0 items-center gap-1.5">
                        {notif.otpCode && (
                          <motion.button
                            type="button"
                            whileHover={{ scale: 1.03 }}
                            whileTap={{ scale: 0.96 }}
                            onClick={() => {
                              if (!notif.otpCode) return;
                              invoke("copy_text_to_clipboard", {
                                text: notif.otpCode,
                              }).catch(() => {});
                              setCopiedNotifId(notif.id);
                              setTimeout(() => {
                                setCopiedNotifId((prev) =>
                                  prev === notif.id ? null : prev,
                                );
                              }, 1800);
                            }}
                            className={`flex cursor-pointer items-center gap-1.5 rounded-lg border px-2.5 py-1 text-[11.5px] font-medium transition-colors ${
                              isCopied
                                ? "border-emerald-500/30 bg-emerald-500/15 text-emerald-700 dark:text-emerald-300"
                                : "border-blue-500/25 bg-blue-500/10 text-blue-600 hover:bg-blue-500/18 dark:text-blue-400"
                            }`}
                            title="Copy verification code"
                          >
                            {isCopied ? (
                              <>
                                <Check size={12.5} weight="bold" />
                                <span>Copied</span>
                              </>
                            ) : (
                              <>
                                <Copy size={12.5} weight="regular" />
                                <span className="font-mono">
                                  {notif.otpCode}
                                </span>
                              </>
                            )}
                          </motion.button>
                        )}

                        {notif.isClearable && (
                          <motion.button
                            type="button"
                            whileHover={{ scale: 1.08 }}
                            whileTap={{ scale: 0.92 }}
                            onClick={() =>
                              invoke("dismiss_notification", {
                                deviceId: device.deviceId,
                                notificationId: notif.id,
                              }).catch(() => {})
                            }
                            className="flex h-6 w-6 cursor-pointer items-center justify-center rounded-lg text-zinc-400 transition-colors hover:bg-black/[0.06] hover:text-zinc-700 dark:hover:bg-white/[0.08] dark:hover:text-zinc-200"
                            title="Dismiss notification"
                          >
                            <X size={13} weight="bold" />
                          </motion.button>
                        )}
                      </div>
                    </motion.div>
                  );
                })}
              </AnimatePresence>
            </div>
          ) : (
            <div className="flex items-center justify-center rounded-xl border border-black/8 bg-black/[0.015] px-4 py-4 text-[12.5px] text-zinc-400 dark:border-white/8 dark:bg-white/[0.015] dark:text-zinc-500">
              No active notifications on {device.deviceName}
            </div>
          )}
        </div>
      )}
    </motion.div>
  );
}
