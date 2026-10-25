import { useState } from "react";
import { DeviceIcon } from "./DeviceIcon";
import { motion, AnimatePresence } from "motion/react";
import type { ActiveView, DeviceView } from "../types";
import { GearSixIcon, PlusCircleIcon } from "@phosphor-icons/react";

interface SidebarProps {
    devices: DeviceView[];
    activeView: ActiveView;
    onSelectDevice: (deviceId: string) => void;
    onSelectAppSettings: () => void;
    onSelectPairDevice: () => void;
}

export function Sidebar({
    devices,
    activeView,
    onSelectDevice,
    onSelectPairDevice,
    onSelectAppSettings,
}: SidebarProps) {
    const [hoveredDeviceId, setHoveredDeviceId] = useState<string | null>(null);
    const [hoveredBottomId, setHoveredBottomId] = useState<string | null>(null);

    const isAppSettingsSelected = activeView.kind == "app-settings";
    const isPairDeviceSelected = activeView.kind == "pair-device";

    return (
        <aside className="flex h-full w-60 shrink-0 select-none flex-col border-r border-black/10 bg-transparent px-2 pt-12 pb-4 dark:border-white/10">
            <div className="mb-2 select-none px-3.5 text-xs font-medium text-zinc-500 dark:text-zinc-400">
                Devices
            </div>

            <div
                className="flex flex-1 flex-col gap-1 overflow-y-auto px-1.5 py-1"
                onMouseLeave={() => setHoveredDeviceId(null)}
            >
                <AnimatePresence initial={false}>
                    {devices.map((device, index) => {
                        const isSelected =
                            activeView.kind == "device" &&
              activeView.deviceId == device.deviceId;
                        const isHovered = hoveredDeviceId == device.deviceId;

                        return (
                            <motion.div
                                key={device.deviceId}
                                layout
                                initial={{ y: 6, opacity: 0 }}
                                animate={{ y: 0, opacity: 1 }}
                                exit={{ opacity: 0, scale: 0.96 }}
                                transition={{
                                    damping: 34,
                                    type: "spring",
                                    stiffness: 520,
                                    delay: index * 0.03,
                                }}
                                whileTap={{ scale: 0.975 }}
                                onMouseEnter={() => setHoveredDeviceId(device.deviceId)}
                                onClick={() => onSelectDevice(device.deviceId)}
                                className={`relative flex cursor-pointer select-none items-center justify-between rounded-xl px-3 py-2 transition-colors duration-150 ${
                                    isSelected
                                        ? "text-zinc-900 dark:text-zinc-100"
                                        : "text-zinc-600 hover:text-zinc-900 dark:text-zinc-400 dark:hover:text-zinc-200"
                                }`}
                            >
                                {/* Animated active selection pill (top devices group only) */}
                                {isSelected && (
                                    <motion.div
                                        layoutId="active-device-pill"
                                        transition={{
                                            damping: 35,
                                            type: "spring",
                                            stiffness: 500,
                                        }}
                                        className="absolute inset-0 rounded-xl bg-black/[0.08] dark:bg-white/[0.11]"
                                    />
                                )}

                                {/* Subtle animated hover pill (top devices group only) */}
                                {!isSelected && isHovered && (
                                    <motion.div
                                        layoutId="hover-device-pill"
                                        initial={{ opacity: 0 }}
                                        animate={{ opacity: 1 }}
                                        exit={{ opacity: 0 }}
                                        transition={{
                                            damping: 38,
                                            type: "spring",
                                            stiffness: 500,
                                        }}
                                        className="absolute inset-0 rounded-xl bg-black/[0.04] dark:bg-white/[0.05]"
                                    />
                                )}

                                <div className="relative z-10 flex min-w-0 select-none items-center gap-2.5">
                                    <motion.span
                                        animate={{
                                            scale: isSelected || isHovered ? 1.06 : 1,
                                        }}
                                        transition={{ damping: 25, type: "spring", stiffness: 450 }}
                                        className="flex items-center"
                                    >
                                        <DeviceIcon type={device.deviceType} />
                                    </motion.span>
                                    <span className="truncate select-none text-[13.5px] font-medium">
                                        {device.deviceName}
                                    </span>
                                </div>

                                <motion.span
                                    layout
                                    initial={{ scale: 0.8 }}
                                    animate={{
                                        scale: device.reachable ? 1 : 0.9,
                                    }}
                                    transition={{ damping: 28, type: "spring", stiffness: 500 }}
                                    className={`relative z-10 ml-2 h-2 w-2 shrink-0 select-none rounded-full ${
                                        device.reachable
                                            ? "bg-[#30d158]"
                                            : "bg-zinc-400 dark:bg-zinc-600"
                                    }`}
                                />
                            </motion.div>
                        );
                    })}
                </AnimatePresence>
            </div>

            {/* Bottom sidebar action buttons */}
            <div
                className="mt-2 flex flex-col gap-1 border-t border-black/8 px-1.5 pt-2.5 dark:border-white/8"
                onMouseLeave={() => setHoveredBottomId(null)}
            >
                <motion.button
                    type="button"
                    whileTap={{ scale: 0.975 }}
                    transition={{ damping: 34, type: "spring", stiffness: 520 }}
                    onMouseEnter={() => setHoveredBottomId("app-settings")}
                    onClick={onSelectAppSettings}
                    className={`relative flex w-full cursor-pointer select-none items-center gap-2.5 rounded-xl px-3 py-2 text-[13.5px] font-medium transition-colors duration-150 ${
                        isAppSettingsSelected
                            ? "text-zinc-900 dark:text-zinc-100"
                            : "text-zinc-600 hover:text-zinc-900 dark:text-zinc-400 dark:hover:text-zinc-200"
                    }`}
                >
                    {isAppSettingsSelected && (
                        <motion.div
                            layoutId="active-bottom-pill"
                            transition={{
                                damping: 35,
                                type: "spring",
                                stiffness: 500,
                            }}
                            className="absolute inset-0 rounded-xl bg-black/[0.08] dark:bg-white/[0.11]"
                        />
                    )}

                    {!isAppSettingsSelected && hoveredBottomId == "app-settings" && (
                        <motion.div
                            layoutId="hover-bottom-pill"
                            initial={{ opacity: 0 }}
                            animate={{ opacity: 1 }}
                            exit={{ opacity: 0 }}
                            transition={{
                                damping: 38,
                                type: "spring",
                                stiffness: 500,
                            }}
                            className="absolute inset-0 rounded-xl bg-black/[0.04] dark:bg-white/[0.05]"
                        />
                    )}

                    <span className="relative z-10 flex items-center gap-2.5">
                        <GearSixIcon size={18} weight="regular" className="shrink-0" />
                        <span className="truncate">Settings</span>
                    </span>
                </motion.button>

                <motion.button
                    type="button"
                    whileTap={{ scale: 0.975 }}
                    transition={{ damping: 34, type: "spring", stiffness: 520 }}
                    onMouseEnter={() => setHoveredBottomId("pair-device")}
                    onClick={onSelectPairDevice}
                    className={`relative flex w-full cursor-pointer select-none items-center gap-2.5 rounded-xl px-3 py-2 text-[13.5px] font-medium transition-colors duration-150 ${
                        isPairDeviceSelected
                            ? "text-zinc-900 dark:text-zinc-100"
                            : "text-zinc-600 hover:text-zinc-900 dark:text-zinc-400 dark:hover:text-zinc-200"
                    }`}
                >
                    {isPairDeviceSelected && (
                        <motion.div
                            layoutId="active-bottom-pill"
                            transition={{
                                damping: 35,
                                type: "spring",
                                stiffness: 500,
                            }}
                            className="absolute inset-0 rounded-xl bg-black/[0.08] dark:bg-white/[0.11]"
                        />
                    )}

                    {!isPairDeviceSelected && hoveredBottomId == "pair-device" && (
                        <motion.div
                            layoutId="hover-bottom-pill"
                            initial={{ opacity: 0 }}
                            animate={{ opacity: 1 }}
                            exit={{ opacity: 0 }}
                            transition={{
                                damping: 38,
                                type: "spring",
                                stiffness: 500,
                            }}
                            className="absolute inset-0 rounded-xl bg-black/[0.04] dark:bg-white/[0.05]"
                        />
                    )}

                    <span className="relative z-10 flex items-center gap-2.5">
                        <PlusCircleIcon size={18} weight="regular" className="shrink-0" />
                        <span className="truncate">Pair new device</span>
                    </span>
                </motion.button>
            </div>
        </aside>
    );
}
