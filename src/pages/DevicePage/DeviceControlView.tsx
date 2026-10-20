import {
  CheckCircle,
  Link,
  MusicNotes,
  PaperPlaneRight,
  Pause,
  Play,
  SkipBack,
  SkipForward,
  UploadSimple,
} from "@phosphor-icons/react";
import * as Popover from "@radix-ui/react-popover";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  animate,
  AnimatePresence,
  motion,
  useMotionValue,
  useTransform,
} from "motion/react";
import { type MouseEvent, type RefObject, useEffect, useRef, useState } from "react";
import type { DeviceView, TransferItem } from "../../types";
import { formatDurationMs, isValidHttpUrl } from "../../utils/format";

interface DeviceControlViewProps {
  device: DeviceView;
  transfers: TransferItem[];
  artworkSrc: string | null;
  dropZoneRef: RefObject<HTMLDivElement | null>;
  isDraggingOver: boolean;
  justDroppedCount: number;
  onFilesPicked: (count: number) => void;
}

export function DeviceControlView({
  device,
  transfers,
  artworkSrc,
  dropZoneRef,
  isDraggingOver,
  justDroppedCount,
  onFilesPicked,
}: DeviceControlViewProps) {
  const [displaySecs, setDisplaySecs] = useState<number>(0);
  const [isShareLinkOpen, setIsShareLinkOpen] = useState<boolean>(false);
  const [shareLinkInput, setShareLinkInput] = useState<string>("");
  const [shareLinkTouchedInvalid, setShareLinkTouchedInvalid] =
    useState<boolean>(false);
  const [nowTickMs, setNowTickMs] = useState<number>(() => Date.now());

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
  }, [device.deviceId]);

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
      const pct = Math.min(
        100,
        Math.max(0, (posMs / nowPlaying.lengthMs) * 100)
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
  }, [
    nowPlaying?.player,
    nowPlaying?.title,
    nowPlaying?.positionMs,
    nowPlaying?.positionUpdatedAtMs,
    nowPlaying?.isPlaying,
    nowPlaying?.lengthMs,
    progressPctMv,
  ]);

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
        Date.now() - t.timestampMs < 2400
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
      Math.max(0, (e.clientX - rect.left) / rect.width)
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
        {device.reachable &&
          device.nowPlaying &&
          device.nowPlaying.title && (
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
            (t) => t.deviceId === device.deviceId && t.kind === "File"
          );
          const anyInProgress = deviceFileTransfers.some(
            (t) => t.status === "InProgress"
          );
          const visibleTransfers = deviceFileTransfers
            .filter(
              (t) =>
                t.status === "InProgress" ||
                (t.status === "Completed" &&
                  (anyInProgress
                    ? nowTickMs - t.timestampMs < 15000
                    : nowTickMs - t.timestampMs < 2200))
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
                              (item.transferredBytes / item.totalBytes) * 100
                            )
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
        </div>
      </div>
    </motion.div>
  );
}
