import { ArrowLeft, SlidersHorizontal } from "@phosphor-icons/react";
import { AnimatePresence, motion } from "motion/react";
import type { RefObject } from "react";
import { DeviceIcon } from "../../components/DeviceIcon";
import type { DeviceView, TransferItem } from "../../types";
import { formatLastSeen } from "../../utils/format";
import { DeviceControlView } from "./DeviceControlView";
import { DeviceSettingsView } from "./DeviceSettingsView";

interface DevicePageProps {
  device: DeviceView;
  transfers: TransferItem[];
  subview: "control" | "settings";
  onToggleSubview: () => void;
  hasArtworkBackdrop: boolean;
  artworkSrc: string | null;
  dropZoneRef: RefObject<HTMLDivElement | null>;
  isDraggingOver: boolean;
  justDroppedCount: number;
  onFilesPicked: (count: number) => void;
  fileSaveLocation: string;
  onChangeFileSaveLocation: (nextPath: string) => void;
  syncClipboard: boolean;
  onToggleSyncClipboard: () => void;
  onUnpair: () => void;
}

export function DevicePage({
  device,
  transfers,
  subview,
  onToggleSubview,
  hasArtworkBackdrop,
  artworkSrc,
  dropZoneRef,
  isDraggingOver,
  justDroppedCount,
  onFilesPicked,
  fileSaveLocation,
  onChangeFileSaveLocation,
  syncClipboard,
  onToggleSyncClipboard,
  onUnpair,
}: DevicePageProps) {
  const isSettingsOpen = subview === "settings";

  return (
    <motion.div
      key={`device-page-${device.deviceId}`}
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -6 }}
      transition={{ type: "spring", stiffness: 460, damping: 34 }}
      className="relative z-10 flex flex-col"
    >
      {/* Top Device Header */}
      <div className="flex items-center justify-between gap-4">
        <div className="flex items-center gap-4">
          <div
            className={`flex h-12 w-12 shrink-0 items-center justify-center rounded-2xl transition-colors duration-300 ${
              hasArtworkBackdrop
                ? "bg-white/15 text-white ring-1 ring-white/20 backdrop-blur-md"
                : "bg-blue-500/12 text-blue-600 dark:bg-[#102648] dark:text-[#60a5fa]"
            }`}
          >
            <DeviceIcon type={device.deviceType} size={24} />
          </div>

          <div className="flex flex-col">
            <h1
              className={`text-[18px] font-semibold tracking-tight transition-colors duration-300 ${
                hasArtworkBackdrop
                  ? "text-white drop-shadow-xs"
                  : "text-zinc-900 dark:text-zinc-100"
              }`}
            >
              {device.deviceName}
            </h1>
            <p
              className={`mt-0.5 text-[13.5px] transition-colors duration-300 ${
                hasArtworkBackdrop
                  ? "text-white/85 drop-shadow-xs"
                  : "text-zinc-500 dark:text-zinc-400"
              }`}
            >
              {formatLastSeen(device.reachable, device.lastSeenSecs)}
            </p>
          </div>
        </div>

        <motion.button
          type="button"
          whileHover={{ scale: 1.04 }}
          whileTap={{ scale: 0.96 }}
          onClick={onToggleSubview}
          className={`relative flex h-9 w-11 cursor-pointer items-center justify-center overflow-hidden rounded-xl border transition-colors duration-300 ${
            hasArtworkBackdrop
              ? "border-white/25 bg-white/10 text-white backdrop-blur-md hover:bg-white/20"
              : "border-black/10 bg-black/[0.02] text-zinc-600 hover:bg-black/[0.05] dark:border-white/10 dark:bg-white/[0.02] dark:text-zinc-300 dark:hover:bg-white/[0.06]"
          }`}
          title={isSettingsOpen ? "Back to Device" : "Device Settings"}
        >
          <AnimatePresence mode="wait" initial={false}>
            {isSettingsOpen ? (
              <motion.span
                key="back-arrow-icon"
                initial={{ opacity: 0, rotate: 45, scale: 0.7 }}
                animate={{ opacity: 1, rotate: 0, scale: 1 }}
                exit={{ opacity: 0, rotate: -45, scale: 0.7 }}
                transition={{
                  type: "spring",
                  stiffness: 520,
                  damping: 30,
                }}
                className="flex items-center justify-center"
              >
                <ArrowLeft size={18} weight="regular" />
              </motion.span>
            ) : (
              <motion.span
                key="settings-sliders-icon"
                initial={{ opacity: 0, scale: 0.7 }}
                animate={{ opacity: 1, scale: 1 }}
                exit={{ opacity: 0, scale: 0.7 }}
                transition={{
                  type: "spring",
                  stiffness: 520,
                  damping: 30,
                }}
                className="flex items-center justify-center"
              >
                <SlidersHorizontal size={18} weight="regular" />
              </motion.span>
            )}
          </AnimatePresence>
        </motion.button>
      </div>

      {/* Animated view swap between Device Control Page and Device Settings Page */}
      <AnimatePresence mode="wait" initial={false}>
        {isSettingsOpen ? (
          <DeviceSettingsView
            key="device-settings-view"
            fileSaveLocation={fileSaveLocation}
            onChangeFileSaveLocation={onChangeFileSaveLocation}
            syncClipboard={syncClipboard}
            onToggleSyncClipboard={onToggleSyncClipboard}
            onUnpair={onUnpair}
          />
        ) : (
          <DeviceControlView
            key="device-control-view"
            device={device}
            transfers={transfers}
            artworkSrc={artworkSrc}
            dropZoneRef={dropZoneRef}
            isDraggingOver={isDraggingOver}
            justDroppedCount={justDroppedCount}
            onFilesPicked={onFilesPicked}
          />
        )}
      </AnimatePresence>
    </motion.div>
  );
}
