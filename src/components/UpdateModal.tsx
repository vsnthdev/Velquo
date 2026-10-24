import {
  ArrowClockwise,
  CloudArrowDown,
  Sparkle,
  X,
} from "@phosphor-icons/react";
import * as Dialog from "@radix-ui/react-dialog";
import { invoke } from "@tauri-apps/api/core";
import type { Update } from "@tauri-apps/plugin-updater";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useState } from "react";

interface UpdateModalProps {
  update: Update | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSkipVersion: (version: string) => void;
  rightPaneContainer: HTMLElement | null;
}

type UpdateStep = "prompt" | "downloading" | "ready" | "error";

export function UpdateModal({
  update,
  open,
  onOpenChange,
  onSkipVersion,
  rightPaneContainer,
}: UpdateModalProps) {
  const [step, setStep] = useState<UpdateStep>("prompt");
  const [progressPct, setProgressPct] = useState<number>(0);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  useEffect(() => {
    if (open) {
      setStep("prompt");
      setProgressPct(0);
      setErrorMessage(null);
    }
  }, [open, update?.version]);

  if (!update) return null;

  const handleUpdateNow = async () => {
    if (step === "downloading") return;
    setStep("downloading");
    setProgressPct(0);
    setErrorMessage(null);

    let downloadedBytes = 0;
    let totalBytes = 0;

    try {
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          totalBytes = event.data.contentLength ?? 0;
          setProgressPct(2);
        } else if (event.event === "Progress") {
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
        } else if (event.event === "Finished") {
          setProgressPct(100);
        }
      });

      setProgressPct(100);
      setStep("ready");
    } catch (err) {
      console.error("Failed to download and install update:", err);
      setErrorMessage(
        err instanceof Error
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
    <Dialog.Root
      open={open}
      onOpenChange={(nextOpen) => {
        if (step === "downloading") return;
        onOpenChange(nextOpen);
      }}
    >
      <AnimatePresence>
        {open && (
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
                className="absolute top-1/2 left-1/2 z-50 flex w-[88%] max-w-md -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-2xl border border-black/12 bg-white p-5 shadow-2xl outline-none dark:border-white/15 dark:bg-zinc-900"
              >
                {/* Modal Header */}
                <div className="flex items-start justify-between gap-3">
                  <div className="flex items-center gap-3">
                    <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-blue-500/12 text-blue-600 dark:bg-blue-500/15 dark:text-blue-400">
                      <Sparkle size={20} weight="duotone" />
                    </div>
                    <div className="flex flex-col">
                      <Dialog.Title className="text-[15.5px] font-semibold text-zinc-900 dark:text-zinc-100">
                        Update available
                      </Dialog.Title>
                      <Dialog.Description className="text-[12.5px] text-zinc-500 dark:text-zinc-400">
                        Velquo{" "}
                        <span className="font-mono font-medium text-zinc-700 dark:text-zinc-200">
                          v{update.version}
                        </span>{" "}
                        is ready to install (current:{" "}
                        <span className="font-mono">
                          v{update.currentVersion}
                        </span>
                        )
                      </Dialog.Description>
                    </div>
                  </div>

                  {step !== "downloading" && (
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
                  )}
                </div>

                {/* Release Notes (if provided in latest.json) */}
                {update.body && (
                  <div className="mt-4 max-h-36 overflow-y-auto rounded-xl border border-black/10 bg-black/[0.02] px-3.5 py-2.5 text-[12.5px] leading-relaxed whitespace-pre-wrap text-zinc-600 dark:border-white/10 dark:bg-white/[0.02] dark:text-zinc-300">
                    {update.body}
                  </div>
                )}

                {errorMessage && (
                  <div className="mt-3 rounded-xl border border-red-500/30 bg-red-500/10 px-3 py-2 text-[12px] text-red-600 dark:text-red-300">
                    {errorMessage}
                  </div>
                )}

                {/* Action Buttons / Progress Bar / Red Restart Button */}
                <div className="mt-5 flex items-center gap-2.5">
                  <AnimatePresence mode="wait" initial={false}>
                    {(step === "prompt" || step === "error") && (
                      <motion.div
                        key="prompt-buttons"
                        initial={{ opacity: 0, y: 4 }}
                        animate={{ opacity: 1, y: 0 }}
                        exit={{ opacity: 0, y: -4 }}
                        transition={{ duration: 0.16 }}
                        className="flex w-full items-center gap-2.5"
                      >
                        <motion.button
                          type="button"
                          whileHover={{ scale: 1.015 }}
                          whileTap={{ scale: 0.985 }}
                          onClick={() => onSkipVersion(update.version)}
                          className="flex flex-1 cursor-pointer items-center justify-center rounded-xl border border-black/12 bg-black/[0.02] px-4 py-2.5 text-[13px] font-medium text-zinc-700 transition-colors hover:border-black/20 hover:bg-black/[0.05] dark:border-white/12 dark:bg-white/[0.03] dark:text-zinc-300 dark:hover:border-white/20 dark:hover:bg-white/[0.06]"
                        >
                          Skip this version
                        </motion.button>

                        <motion.button
                          type="button"
                          whileHover={{ scale: 1.015 }}
                          whileTap={{ scale: 0.985 }}
                          onClick={handleUpdateNow}
                          className="flex flex-1 cursor-pointer items-center justify-center gap-1.5 rounded-xl bg-blue-600 px-4 py-2.5 text-[13px] font-medium text-white shadow-xs transition-colors hover:bg-blue-500"
                        >
                          <CloudArrowDown size={16} weight="bold" />
                          <span>
                            {step === "error" ? "Retry update" : "Update now"}
                          </span>
                        </motion.button>
                      </motion.div>
                    )}

                    {step === "downloading" && (
                      <motion.div
                        key="downloading-progress-button"
                        initial={{ opacity: 0, scale: 0.98 }}
                        animate={{ opacity: 1, scale: 1 }}
                        exit={{ opacity: 0, scale: 0.98 }}
                        transition={{ duration: 0.18 }}
                        className="relative flex h-10 w-full items-center justify-center overflow-hidden rounded-xl border border-blue-500/30 bg-blue-500/12 text-[13px] font-medium text-blue-700 dark:border-blue-400/30 dark:bg-blue-500/15 dark:text-blue-200"
                      >
                        <motion.div
                          className="absolute inset-y-0 left-0 bg-blue-600/25 dark:bg-blue-500/35"
                          initial={{ width: "0%" }}
                          animate={{ width: `${progressPct}%` }}
                          transition={{ ease: "easeOut", duration: 0.2 }}
                        />
                        <span className="relative z-10 flex items-center gap-2">
                          <span>Downloading & installing...</span>
                          <span className="font-mono text-[12px] tabular-nums">
                            {progressPct}%
                          </span>
                        </span>
                      </motion.div>
                    )}

                    {step === "ready" && (
                      <motion.button
                        key="restart-button"
                        type="button"
                        initial={{ opacity: 0, scale: 0.96 }}
                        animate={{ opacity: 1, scale: 1 }}
                        exit={{ opacity: 0, scale: 0.96 }}
                        whileHover={{ scale: 1.012 }}
                        whileTap={{ scale: 0.988 }}
                        transition={{
                          type: "spring",
                          stiffness: 480,
                          damping: 30,
                        }}
                        onClick={handleRestartApp}
                        className="flex w-full cursor-pointer items-center justify-center gap-2 rounded-xl bg-red-600 px-4 py-2.5 text-[13.5px] font-semibold text-white shadow-sm transition-colors hover:bg-red-500"
                      >
                        <ArrowClockwise size={16} weight="bold" />
                        <span>Restart</span>
                      </motion.button>
                    )}
                  </AnimatePresence>
                </div>
              </motion.div>
            </Dialog.Content>
          </Dialog.Portal>
        )}
      </AnimatePresence>
    </Dialog.Root>
  );
}
