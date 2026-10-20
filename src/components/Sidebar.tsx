import { GearSix, PlusCircle } from "@phosphor-icons/react";
import { AnimatePresence, motion } from "motion/react";
import { useState } from "react";
import type { ActiveView, DeviceView } from "../types";
import { DeviceIcon } from "./DeviceIcon";

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
  onSelectAppSettings,
  onSelectPairDevice,
}: SidebarProps) {
  const [hoveredItemId, setHoveredItemId] = useState<string | null>(null);

  const isAppSettingsSelected = activeView.kind === "app-settings";
  const isPairDeviceSelected = activeView.kind === "pair-device";

  return (
    <aside
      className="flex h-full w-60 shrink-0 select-none flex-col border-r border-black/10 bg-transparent px-2 pt-12 pb-4 dark:border-white/10"
      onMouseLeave={() => setHoveredItemId(null)}
    >
      <div className="mb-2 select-none px-3.5 text-xs font-medium text-zinc-500 dark:text-zinc-400">
        Devices
      </div>

      <div className="flex flex-1 flex-col gap-1 overflow-y-auto px-1.5 py-1">
        <AnimatePresence initial={false}>
          {devices.map((device, index) => {
            const isSelected =
              activeView.kind === "device" &&
              activeView.deviceId === device.deviceId;
            const isHovered = hoveredItemId === device.deviceId;

            return (
              <motion.div
                key={device.deviceId}
                layout
                initial={{ opacity: 0, y: 6 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, scale: 0.96 }}
                transition={{
                  type: "spring",
                  stiffness: 520,
                  damping: 34,
                  delay: index * 0.03,
                }}
                whileHover={{ x: 2 }}
                whileTap={{ scale: 0.975 }}
                onMouseEnter={() => setHoveredItemId(device.deviceId)}
                onClick={() => onSelectDevice(device.deviceId)}
                className={`relative flex cursor-pointer select-none items-center justify-between rounded-xl px-3 py-2 transition-colors duration-150 ${
                  isSelected
                    ? "text-zinc-900 dark:text-zinc-100"
                    : "text-zinc-600 hover:text-zinc-900 dark:text-zinc-400 dark:hover:text-zinc-200"
                }`}
              >
                {/* Animated active selection pill */}
                {isSelected && (
                  <motion.div
                    layoutId="active-sidebar-pill"
                    transition={{
                      type: "spring",
                      stiffness: 500,
                      damping: 35,
                    }}
                    className="absolute inset-0 rounded-xl bg-black/[0.08] dark:bg-white/[0.11]"
                  />
                )}

                {/* Subtle animated hover pill when not selected */}
                {!isSelected && isHovered && (
                  <motion.div
                    layoutId="hover-sidebar-pill"
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    exit={{ opacity: 0 }}
                    transition={{
                      type: "spring",
                      stiffness: 500,
                      damping: 38,
                    }}
                    className="absolute inset-0 rounded-xl bg-black/[0.04] dark:bg-white/[0.05]"
                  />
                )}

                <div className="relative z-10 flex min-w-0 select-none items-center gap-2.5">
                  <motion.span
                    animate={{
                      scale: isSelected || isHovered ? 1.06 : 1,
                    }}
                    transition={{ type: "spring", stiffness: 450, damping: 25 }}
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
                  transition={{ type: "spring", stiffness: 500, damping: 28 }}
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
      <div className="mt-2 flex flex-col gap-1 border-t border-black/8 px-1.5 pt-2.5 dark:border-white/8">
        <motion.button
          type="button"
          whileHover={{ x: 2 }}
          whileTap={{ scale: 0.975 }}
          transition={{ type: "spring", stiffness: 520, damping: 34 }}
          onMouseEnter={() => setHoveredItemId("__app_settings__")}
          onClick={onSelectAppSettings}
          className={`relative flex w-full cursor-pointer select-none items-center gap-2.5 rounded-xl px-3 py-2 text-[13.5px] font-medium transition-colors duration-150 ${
            isAppSettingsSelected
              ? "text-zinc-900 dark:text-zinc-100"
              : "text-zinc-600 hover:text-zinc-900 dark:text-zinc-400 dark:hover:text-zinc-200"
          }`}
        >
          {isAppSettingsSelected && (
            <motion.div
              layoutId="active-sidebar-pill"
              transition={{
                type: "spring",
                stiffness: 500,
                damping: 35,
              }}
              className="absolute inset-0 rounded-xl bg-black/[0.08] dark:bg-white/[0.11]"
            />
          )}

          {!isAppSettingsSelected && hoveredItemId === "__app_settings__" && (
            <motion.div
              layoutId="hover-sidebar-pill"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{
                type: "spring",
                stiffness: 500,
                damping: 38,
              }}
              className="absolute inset-0 rounded-xl bg-black/[0.04] dark:bg-white/[0.05]"
            />
          )}

          <span className="relative z-10 flex items-center gap-2.5">
            <GearSix size={18} weight="regular" className="shrink-0" />
            <span className="truncate">Settings</span>
          </span>
        </motion.button>

        <motion.button
          type="button"
          whileHover={{ x: 2 }}
          whileTap={{ scale: 0.975 }}
          transition={{ type: "spring", stiffness: 520, damping: 34 }}
          onMouseEnter={() => setHoveredItemId("__pair_device__")}
          onClick={onSelectPairDevice}
          className={`relative flex w-full cursor-pointer select-none items-center gap-2.5 rounded-xl px-3 py-2 text-[13.5px] font-medium transition-colors duration-150 ${
            isPairDeviceSelected
              ? "text-zinc-900 dark:text-zinc-100"
              : "text-zinc-600 hover:text-zinc-900 dark:text-zinc-400 dark:hover:text-zinc-200"
          }`}
        >
          {isPairDeviceSelected && (
            <motion.div
              layoutId="active-sidebar-pill"
              transition={{
                type: "spring",
                stiffness: 500,
                damping: 35,
              }}
              className="absolute inset-0 rounded-xl bg-black/[0.08] dark:bg-white/[0.11]"
            />
          )}

          {!isPairDeviceSelected && hoveredItemId === "__pair_device__" && (
            <motion.div
              layoutId="hover-sidebar-pill"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{
                type: "spring",
                stiffness: 500,
                damping: 38,
              }}
              className="absolute inset-0 rounded-xl bg-black/[0.04] dark:bg-white/[0.05]"
            />
          )}

          <span className="relative z-10 flex items-center gap-2.5">
            <PlusCircle size={18} weight="regular" className="shrink-0" />
            <span className="truncate">Pair new device</span>
          </span>
        </motion.button>
      </div>
    </aside>
  );
}
