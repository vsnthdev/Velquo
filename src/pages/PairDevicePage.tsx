import { PlusCircle } from "@phosphor-icons/react";
import { motion } from "motion/react";

export function PairDevicePage() {
  return (
    <motion.div
      key="pair-device-page"
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -6 }}
      transition={{ type: "spring", stiffness: 460, damping: 34 }}
      className="relative z-10 flex flex-col"
    >
      <div className="flex items-center gap-4">
        <div className="flex h-12 w-12 shrink-0 items-center justify-center rounded-2xl bg-blue-500/12 text-blue-600 dark:bg-[#102648] dark:text-[#60a5fa]">
          <PlusCircle size={24} weight="regular" />
        </div>

        <div className="flex flex-col">
          <h1 className="text-[18px] font-semibold tracking-tight text-zinc-900 dark:text-zinc-100">
            Pair new device
          </h1>
          <p className="mt-0.5 text-[13.5px] text-zinc-500 dark:text-zinc-400">
            Discover and pair KDE Connect devices on your Wi-Fi network
          </p>
        </div>
      </div>
    </motion.div>
  );
}
