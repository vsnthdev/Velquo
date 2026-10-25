import type { PingToastData } from "../types";
import { motion, AnimatePresence } from "motion/react";
import { BellRingingIcon } from "@phosphor-icons/react";

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
                    initial={{ y: -16, opacity: 0, scale: 0.94 }}
                    animate={{ y: 0, scale: 1, opacity: 1 }}
                    exit={{ y: -12, opacity: 0, scale: 0.95 }}
                    transition={{ damping: 32, type: "spring", stiffness: 520 }}
                    onClick={onDismiss}
                    className="fixed top-4 right-5 z-50 flex cursor-pointer items-center gap-3 rounded-2xl border border-black/10 bg-white/90 px-4 py-2.5 shadow-lg backdrop-blur-xl dark:border-white/15 dark:bg-zinc-800/90"
                >
                    <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-xl bg-blue-500/15 text-blue-600 dark:bg-blue-500/20 dark:text-blue-400">
                        <BellRingingIcon size={18} weight="fill" />
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
