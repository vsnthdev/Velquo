import type { RefObject } from "react";
import { formatLastSeen } from "../../utils/format";
import { motion, AnimatePresence } from "motion/react";
import { DeviceControlView } from "./DeviceControlView";
import { DeviceIcon } from "../../components/DeviceIcon";
import { DeviceSettingsView } from "./DeviceSettingsView";
import type { DeviceView, TransferItem } from "../../types";
import { ArrowLeftIcon, SlidersHorizontalIcon } from "@phosphor-icons/react";

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
    allowPresenter: boolean;
    onTogglePresenter: () => void;
    syncNotifications: boolean;
    onToggleNotifications: () => void;
    autoCopyOtp: boolean;
    onToggleAutoCopyOtp: () => void;
    onAddRunCommand: (name: string, command: string) => Promise<void>;
    onRemoveRunCommand: (key: string) => Promise<void>;
    rightPaneContainer: HTMLElement | null;
    openAddCommandSignal: number;
    highlightedNotification: {
        deviceId: string;
        notificationId: string;
        seq: number;
    } | null;
    onUnpair: () => void;
}

export function DevicePage({
    device,
    subview,
    onUnpair,
    transfers,
    artworkSrc,
    dropZoneRef,
    autoCopyOtp,
    onFilesPicked,
    syncClipboard,
    isDraggingOver,
    allowPresenter,
    onToggleSubview,
    onAddRunCommand,
    justDroppedCount,
    fileSaveLocation,
    onTogglePresenter,
    syncNotifications,
    hasArtworkBackdrop,
    onRemoveRunCommand,
    rightPaneContainer,
    onToggleAutoCopyOtp,
    openAddCommandSignal,
    onToggleSyncClipboard,
    onToggleNotifications,
    highlightedNotification,
    onChangeFileSaveLocation,
}: DevicePageProps) {
    const isSettingsOpen = subview == "settings";

    return (
        <motion.div
            key={`device-page-${device.deviceId}`}
            initial={{ y: 8, opacity: 0 }}
            animate={{ y: 0, opacity: 1 }}
            exit={{ y: -6, opacity: 0 }}
            transition={{ damping: 34, type: "spring", stiffness: 460 }}
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
                                animate={{ scale: 1, rotate: 0, opacity: 1 }}
                                exit={{ opacity: 0, scale: 0.7, rotate: -45 }}
                                transition={{
                                    damping: 30,
                                    type: "spring",
                                    stiffness: 520,
                                }}
                                className="flex items-center justify-center"
                            >
                                <ArrowLeftIcon size={18} weight="regular" />
                            </motion.span>
                        ) : (
                            <motion.span
                                key="settings-sliders-icon"
                                initial={{ opacity: 0, scale: 0.7 }}
                                animate={{ scale: 1, opacity: 1 }}
                                exit={{ opacity: 0, scale: 0.7 }}
                                transition={{
                                    damping: 30,
                                    type: "spring",
                                    stiffness: 520,
                                }}
                                className="flex items-center justify-center"
                            >
                                <SlidersHorizontalIcon size={18} weight="regular" />
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
                        allowPresenter={allowPresenter}
                        onTogglePresenter={onTogglePresenter}
                        syncNotifications={syncNotifications}
                        onToggleNotifications={onToggleNotifications}
                        autoCopyOtp={autoCopyOtp}
                        onToggleAutoCopyOtp={onToggleAutoCopyOtp}
                        runCommands={device.runCommands || []}
                        onAddRunCommand={onAddRunCommand}
                        onRemoveRunCommand={onRemoveRunCommand}
                        rightPaneContainer={rightPaneContainer}
                        openAddCommandSignal={openAddCommandSignal}
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
                        highlightedNotification={highlightedNotification}
                        rightPaneContainer={rightPaneContainer}
                    />
                )}
            </AnimatePresence>
        </motion.div>
    );
}
