import { BellRinging } from "@phosphor-icons/react";
import { AnimatePresence, motion } from "motion/react";
import type { PingToastData } from "../types";

export function PingToast({
  pingToast,
  onDismiss,
}: {
  pingToast: PingToastData | null;
  onDismiss: () => void;
}) {
  return (
    <AnimatePresence>
      {pingToast && (
        <motion.div
          key="ping-toast"
          initial={{ opacity: 0, y: -16, scale: 0.94 }}
          animate={{ opacity: 1, y: 0, scale: 1 }}
          exit={{ opacity: 0, y: -12, scale: 0.95 }}
          transition={{ type: "spring", stiffness: 520, damping: 32 }}
          onClick={onDismiss}
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
  );
}
