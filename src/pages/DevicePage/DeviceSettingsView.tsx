import { Trash } from "@phosphor-icons/react";
import { open } from "@tauri-apps/plugin-dialog";
import { motion } from "motion/react";

interface DeviceSettingsViewProps {
  fileSaveLocation: string;
  onChangeFileSaveLocation: (nextPath: string) => void;
  syncClipboard: boolean;
  onToggleSyncClipboard: () => void;
  onUnpair: () => void;
}

export function DeviceSettingsView({
  fileSaveLocation,
  onChangeFileSaveLocation,
  syncClipboard,
  onToggleSyncClipboard,
  onUnpair,
}: DeviceSettingsViewProps) {
  const handleChooseSaveLocation = async () => {
    const selected = await open({
      directory: true,
      multiple: false,
      defaultPath: fileSaveLocation || undefined,
      title: "Select File Save Location",
    });
    if (typeof selected === "string" && selected.trim()) {
      onChangeFileSaveLocation(selected);
    }
  };

  return (
    <motion.div
      key="device-settings-view"
      initial={{ opacity: 0, y: 10, filter: "blur(4px)" }}
      animate={{ opacity: 1, y: 0, filter: "blur(0px)" }}
      exit={{ opacity: 0, y: -8, filter: "blur(4px)" }}
      transition={{
        type: "spring",
        stiffness: 460,
        damping: 34,
      }}
      className="mt-6 flex flex-col gap-5"
    >
      {/* File save location setting */}
      <div className="flex flex-col gap-2">
        <label className="text-[12.5px] font-medium text-zinc-500 dark:text-zinc-400">
          File save location
        </label>

        <div className="flex items-center overflow-hidden rounded-xl border border-black/12 bg-black/[0.02] focus-within:border-black/25 dark:border-white/12 dark:bg-white/[0.02] dark:focus-within:border-white/25">
          <input
            type="text"
            value={fileSaveLocation}
            onChange={(e) => onChangeFileSaveLocation(e.target.value)}
            readOnly
            className="min-w-0 flex-1 select-text bg-transparent px-3.5 py-2.5 font-mono text-[12.5px] text-zinc-800 focus:outline-none dark:text-zinc-200"
          />

          <motion.button
            type="button"
            onClick={handleChooseSaveLocation}
            whileHover={{ scale: 1.01 }}
            whileTap={{ scale: 0.98 }}
            className="shrink-0 cursor-pointer border-l border-black/12 bg-black/[0.04] px-4 py-2.5 text-[13px] font-medium text-zinc-800 transition-colors hover:bg-black/[0.08] dark:border-white/12 dark:bg-white/[0.05] dark:text-zinc-100 dark:hover:bg-white/[0.10]"
          >
            Change
          </motion.button>
        </div>
      </div>

      {/* Sync clipboard toggle */}
      <div
        onClick={onToggleSyncClipboard}
        className="flex cursor-pointer items-center justify-between rounded-xl border border-black/12 bg-black/[0.02] px-3.5 py-2.5 transition-colors hover:border-black/20 dark:border-white/12 dark:bg-white/[0.02] dark:hover:border-white/20"
      >
        <span className="text-[13.5px] font-medium text-zinc-800 dark:text-zinc-100">
          Sync clipboard
        </span>

        <button
          type="button"
          role="switch"
          aria-checked={syncClipboard}
          onClick={(e) => {
            e.stopPropagation();
            onToggleSyncClipboard();
          }}
          className={`relative flex h-6 w-11 shrink-0 cursor-pointer items-center rounded-full p-0.5 transition-colors duration-200 ${
            syncClipboard
              ? "bg-blue-600 dark:bg-blue-500"
              : "bg-zinc-300 dark:bg-zinc-700"
          }`}
        >
          <motion.span
            layout
            transition={{
              type: "spring",
              stiffness: 560,
              damping: 34,
            }}
            animate={{
              x: syncClipboard ? 20 : 0,
            }}
            className="h-5 w-5 rounded-full bg-white shadow-sm"
          />
        </button>
      </div>

      <motion.button
        type="button"
        onClick={onUnpair}
        whileHover={{ scale: 1.008 }}
        whileTap={{ scale: 0.99 }}
        transition={{
          type: "spring",
          stiffness: 450,
          damping: 28,
        }}
        className="flex w-full cursor-pointer items-center justify-center gap-2 rounded-xl border border-red-500/25 bg-red-500/[0.08] px-4 py-2.5 text-[13.5px] font-medium text-red-600 transition-colors hover:border-red-500/40 hover:bg-red-500/[0.14] dark:border-red-500/25 dark:bg-red-500/[0.10] dark:text-red-400 dark:hover:border-red-400/40 dark:hover:bg-red-500/[0.16]"
      >
        <Trash size={16} weight="regular" />
        <span>Unpair</span>
      </motion.button>
    </motion.div>
  );
}
