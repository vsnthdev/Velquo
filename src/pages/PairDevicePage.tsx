import { ArrowsClockwise, Check, WifiHigh, X } from "@phosphor-icons/react";
import { invoke } from "@tauri-apps/api/core";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useState } from "react";
import { DeviceIcon } from "../components/DeviceIcon";
import type { DeviceView } from "../types";

interface PairDevicePageProps {
  unpairedDevices: DeviceView[];
}

export function PairDevicePage({ unpairedDevices }: PairDevicePageProps) {
  const [isScanning, setIsScanning] = useState<boolean>(false);
  const [manualIp, setManualIp] = useState<string>("");
  const [ipError, setIpError] = useState<boolean>(false);

  const triggerScan = async () => {
    setIsScanning(true);
    try {
      await invoke("broadcast_discovery");
    } catch (err) {
      console.error("Failed to broadcast discovery:", err);
    } finally {
      setTimeout(() => setIsScanning(false), 750);
    }
  };

  // Broadcast once when opening the Pair new device page (user can click Refresh to scan again)
  useEffect(() => {
    triggerScan();
  }, []);

  const handleProbeIp = async () => {
    const trimmed = manualIp.trim();
    const ipv4Regex =
      /^(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}$/;
    if (!ipv4Regex.test(trimmed)) {
      setIpError(true);
      return;
    }
    try {
      await invoke("probe_device_ip", { ip: trimmed });
      setManualIp("");
      setIpError(false);
    } catch (err) {
      console.error("Failed to probe IP:", err);
      setIpError(true);
    }
  };

  return (
    <motion.div
      key="pair-device-page"
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -6 }}
      transition={{ type: "spring", stiffness: 460, damping: 34 }}
      className="relative z-10 flex flex-col"
    >
      {/* Available Devices section header + Refresh button */}
      <div className="flex items-center justify-between">
        <h2 className="text-[12.5px] font-medium tracking-wide text-zinc-500 uppercase dark:text-zinc-400">
          Available Devices
        </h2>

        <motion.button
          type="button"
          onClick={triggerScan}
          whileHover={{ scale: 1.04 }}
          whileTap={{ scale: 0.95 }}
          className="flex cursor-pointer items-center gap-1.5 rounded-lg border border-black/10 bg-black/[0.02] px-2.5 py-1 text-[12px] font-medium text-zinc-600 transition-colors hover:border-black/20 hover:bg-black/[0.05] hover:text-zinc-900 dark:border-white/10 dark:bg-white/[0.02] dark:text-zinc-300 dark:hover:border-white/20 dark:hover:bg-white/[0.06] dark:hover:text-white"
        >
          <motion.span
            animate={{ rotate: isScanning ? 360 : 0 }}
            transition={
              isScanning
                ? { duration: 0.65, ease: "easeInOut" }
                : { duration: 0 }
            }
            className="flex items-center"
          >
            <ArrowsClockwise size={13.5} weight="bold" />
          </motion.span>
          <span>Refresh</span>
        </motion.button>
      </div>

      {/* Discovered Unpaired Devices List */}
      <div className="mt-3 flex flex-col gap-2.5">
        <AnimatePresence mode="popLayout" initial={false}>
          {unpairedDevices.length > 0 ? (
            unpairedDevices.map((device) => (
              <motion.div
                key={device.deviceId}
                layout
                initial={{ opacity: 0, y: 8, scale: 0.98 }}
                animate={{ opacity: 1, y: 0, scale: 1 }}
                exit={{ opacity: 0, scale: 0.96 }}
                transition={{ type: "spring", stiffness: 480, damping: 34 }}
                className="flex items-center justify-between gap-4 rounded-2xl border border-black/10 bg-black/[0.02] px-4 py-3.5 dark:border-white/10 dark:bg-white/[0.02]"
              >
                <div className="flex min-w-0 items-center gap-3.5">
                  <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-blue-500/12 text-blue-600 dark:bg-[#102648] dark:text-[#60a5fa]">
                    <DeviceIcon type={device.deviceType} size={20} />
                  </div>

                  <div className="flex min-w-0 flex-col">
                    <span className="truncate text-[14.5px] font-semibold text-zinc-900 dark:text-zinc-100">
                      {device.deviceName}
                    </span>

                    {device.pairState === "Requested" ||
                    device.pairState === "RequestedByPeer" ? (
                      <div className="mt-0.5 flex flex-wrap items-center gap-1.5 text-[12px] text-amber-600 dark:text-amber-300">
                        <span>
                          {device.pairState === "Requested"
                            ? "Confirm on device"
                            : "Incoming pairing request"}
                        </span>
                        {device.verificationKey && (
                          <span className="rounded bg-amber-500/15 px-1.5 py-0.5 font-mono text-[11px] font-medium text-amber-700 dark:text-amber-200">
                            {device.verificationKey}
                          </span>
                        )}
                      </div>
                    ) : (
                      <span className="mt-0.5 font-mono text-[12px] text-zinc-500 dark:text-zinc-400">
                        {device.address}
                      </span>
                    )}
                  </div>
                </div>

                {/* Action buttons based on pairing state */}
                <div className="flex shrink-0 items-center gap-2">
                  {device.pairState === "NotPaired" && (
                    <motion.button
                      type="button"
                      whileHover={{ scale: 1.03 }}
                      whileTap={{ scale: 0.97 }}
                      onClick={() =>
                        invoke("request_pairing", {
                          deviceId: device.deviceId,
                        })
                      }
                      className="cursor-pointer rounded-xl bg-blue-600 px-4 py-1.5 text-[13px] font-medium text-white shadow-xs transition-colors hover:bg-blue-500"
                    >
                      Pair
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
                      className="cursor-pointer rounded-xl border border-red-500/25 bg-red-500/[0.08] px-3.5 py-1.5 text-[12.5px] font-medium text-red-600 transition-colors hover:bg-red-500/[0.15] dark:text-red-400"
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
                        className="flex cursor-pointer items-center gap-1.5 rounded-xl bg-emerald-600 px-3.5 py-1.5 text-[12.5px] font-medium text-white shadow-xs transition-colors hover:bg-emerald-500"
                      >
                        <Check size={14} weight="bold" />
                        <span>Accept</span>
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
                        className="flex cursor-pointer items-center gap-1 rounded-xl border border-red-500/25 bg-red-500/[0.08] px-3 py-1.5 text-[12.5px] font-medium text-red-600 transition-colors hover:bg-red-500/[0.15] dark:text-red-400"
                      >
                        <X size={14} weight="bold" />
                        <span>Reject</span>
                      </motion.button>
                    </>
                  )}
                </div>
              </motion.div>
            ))
          ) : (
            <motion.div
              key="empty-scanning-card"
              initial={{ opacity: 0, y: 6 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, scale: 0.98 }}
              className="flex flex-col items-center justify-center rounded-2xl border border-black/10 bg-black/[0.02] px-6 py-10 text-center dark:border-white/10 dark:bg-white/[0.02]"
            >
              <div className="relative flex h-11 w-11 items-center justify-center rounded-2xl bg-blue-500/12 text-blue-600 dark:bg-[#102648] dark:text-[#60a5fa]">
                <WifiHigh size={22} weight="regular" />
              </div>

              <span className="mt-3 text-[14px] font-semibold text-zinc-900 dark:text-zinc-100">
                Searching for devices on your Wi-Fi...
              </span>
              <span className="mt-1 max-w-sm text-[12.5px] text-zinc-500 dark:text-zinc-400">
                Keep the KDE Connect app open on your phone and connected to the
                same Wi-Fi network.
              </span>
            </motion.div>
          )}
        </AnimatePresence>
      </div>

      {/* Connect by IP fallback section */}
      <h2 className="mt-8 text-[12.5px] font-medium tracking-wide text-zinc-500 uppercase dark:text-zinc-400">
        Connect by IP
      </h2>

      <div
        className={`mt-3 flex items-center overflow-hidden rounded-xl border bg-black/[0.02] transition-colors dark:bg-white/[0.02] ${
          ipError
            ? "border-red-500/80 focus-within:border-red-500 dark:border-red-400/80 dark:focus-within:border-red-400"
            : "border-black/12 focus-within:border-black/25 dark:border-white/12 dark:focus-within:border-white/25"
        }`}
      >
        <input
          type="text"
          value={manualIp}
          onChange={(e) => {
            setManualIp(e.target.value);
            if (ipError) setIpError(false);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              handleProbeIp();
            }
          }}
          placeholder="192.168.1.50"
          className="min-w-0 flex-1 select-text bg-transparent px-3.5 py-2.5 font-mono text-[12.5px] text-zinc-800 placeholder:text-zinc-400 focus:outline-none dark:text-zinc-200 dark:placeholder:text-zinc-500"
        />

        <motion.button
          type="button"
          onClick={handleProbeIp}
          whileHover={{ scale: 1.01 }}
          whileTap={{ scale: 0.98 }}
          className="shrink-0 cursor-pointer border-l border-black/12 bg-black/[0.04] px-4 py-2.5 text-[13px] font-medium text-zinc-800 transition-colors hover:bg-black/[0.08] dark:border-white/12 dark:bg-white/[0.05] dark:text-zinc-100 dark:hover:bg-white/[0.10]"
        >
          Connect
        </motion.button>
      </div>
    </motion.div>
  );
}
