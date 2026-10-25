import pkg from "../../package.json";
import { motion } from "motion/react";
import { getVersion } from "@tauri-apps/api/app";
import { useRef, useState, useEffect } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { enable, disable, isEnabled } from "@tauri-apps/plugin-autostart";
import { CheckIcon, GithubLogoIcon, ArrowsClockwiseIcon } from "@phosphor-icons/react";

interface AppSettingsPageProps {
    ignoreWhatsappConversations: boolean;
    onToggleIgnoreWhatsappConversations: () => void;
    onCheckForUpdates: () => Promise<"update-found" | "up-to-date" | "error">;
    scrollToAboutSignal?: number;
}

export function AppSettingsPage({
    onCheckForUpdates,
    scrollToAboutSignal,
    ignoreWhatsappConversations,
    onToggleIgnoreWhatsappConversations,
}: AppSettingsPageProps) {
    const [launchOnStartup, setLaunchOnStartup] = useState<boolean>(false);
    const [appVersion, setAppVersion] = useState<string>(pkg.version);
    const [checkStatus, setCheckStatus] = useState<
    "idle" | "checking" | "up-to-date" | "error"
    >("idle");
    const aboutCardRef = useRef<HTMLDivElement | null>(null);
    const repoUrl = pkg.homepage;
    const repoDisplay = repoUrl.replace(/^https?:\/\//, "");

    useEffect(() => {
        if (!scrollToAboutSignal) return;
        const timer = setTimeout(() => {
            aboutCardRef.current?.scrollIntoView({
                block: "end",
                behavior: "smooth",
            });
        }, 60);
        return () => clearTimeout(timer);
    }, [scrollToAboutSignal]);

    useEffect(() => {
        getVersion()
            .then((v) => {
                if (v) setAppVersion(v);
            })
            .catch(() => {});

        isEnabled()
            .then((enabled) => {
                setLaunchOnStartup(enabled == true);
            })
            .catch(() => {
                setLaunchOnStartup(false);
            });
    }, []);

    const handleToggleLaunchOnStartup = async () => {
        const prev = launchOnStartup;
        const next = !prev;
        setLaunchOnStartup(next);
        try {
            if (next) {
                await enable();
            } else {
                await disable();
            }
            const actual = await isEnabled();
            setLaunchOnStartup(actual == true);
        } catch (err) {
            console.error("Failed to update autostart state:", err);
            setLaunchOnStartup(prev);
        }
    };

    const handleOpenGitHub = async () => {
        try {
            await openUrl(repoUrl);
        } catch (err) {
            console.error("Failed to open GitHub URL:", err);
        }
    };

    const handleManualUpdateCheck = async () => {
        if (checkStatus == "checking") return;
        setCheckStatus("checking");
        try {
            const res = await onCheckForUpdates();
            if (res == "up-to-date") {
                setCheckStatus("up-to-date");
                setTimeout(() => {
                    setCheckStatus((prev) => (prev == "up-to-date" ? "idle" : prev));
                }, 2500);
            } else if (res == "error") {
                setCheckStatus("error");
                setTimeout(() => {
                    setCheckStatus((prev) => (prev == "error" ? "idle" : prev));
                }, 2500);
            } else {
                setCheckStatus("idle");
            }
        } catch {
            setCheckStatus("error");
            setTimeout(() => {
                setCheckStatus((prev) => (prev == "error" ? "idle" : prev));
            }, 2500);
        }
    };

    return (
        <motion.div
            key="app-settings-page"
            initial={{ y: 8, opacity: 0 }}
            animate={{ y: 0, opacity: 1 }}
            exit={{ y: -6, opacity: 0 }}
            transition={{ damping: 34, type: "spring", stiffness: 460 }}
            className="relative z-10 flex flex-col"
        >
            {/* Settings section header */}
            <h2 className="text-[12.5px] font-medium tracking-wide text-zinc-500 uppercase dark:text-zinc-400">
                Settings
            </h2>

            {/* Launch on startup toggle */}
            <div
                onClick={handleToggleLaunchOnStartup}
                className="mt-3 flex cursor-pointer items-center justify-between rounded-xl border border-black/12 bg-black/[0.02] px-3.5 py-2.5 transition-colors hover:border-black/20 dark:border-white/12 dark:bg-white/[0.02] dark:hover:border-white/20"
            >
                <span className="text-[13.5px] font-medium text-zinc-800 dark:text-zinc-100">
                    Launch on startup
                </span>

                <button
                    type="button"
                    role="switch"
                    aria-checked={launchOnStartup}
                    onClick={(e) => {
                        e.stopPropagation();
                        handleToggleLaunchOnStartup();
                    }}
                    className={`relative flex h-6 w-11 shrink-0 cursor-pointer items-center rounded-full p-0.5 transition-colors duration-200 ${
                        launchOnStartup
                            ? "bg-blue-600 dark:bg-blue-500"
                            : "bg-zinc-300 dark:bg-zinc-700"
                    }`}
                >
                    <motion.span
                        layout
                        transition={{
                            damping: 34,
                            type: "spring",
                            stiffness: 560,
                        }}
                        animate={{
                            x: launchOnStartup ? 20 : 0,
                        }}
                        className="h-5 w-5 rounded-full bg-white shadow-sm"
                    />
                </button>
            </div>

            {/* Check for updates row */}
            <div className="mt-3 flex items-center justify-between rounded-xl border border-black/12 bg-black/[0.02] px-3.5 py-2 transition-colors hover:border-black/20 dark:border-white/12 dark:bg-white/[0.02] dark:hover:border-white/20">
                <span className="text-[13.5px] font-medium text-zinc-800 dark:text-zinc-100">
                    Check for updates
                </span>

                <motion.button
                    type="button"
                    disabled={checkStatus == "checking"}
                    whileHover={checkStatus == "checking" ? undefined : { scale: 1.02 }}
                    whileTap={checkStatus == "checking" ? undefined : { scale: 0.97 }}
                    onClick={handleManualUpdateCheck}
                    className={`flex cursor-pointer items-center gap-1.5 rounded-lg border px-3 py-1 text-[12px] font-medium transition-colors ${
                        checkStatus == "up-to-date"
                            ? "border-emerald-500/30 bg-emerald-500/12 text-emerald-700 dark:text-emerald-300"
                            : checkStatus == "error"
                                ? "border-amber-500/30 bg-amber-500/12 text-amber-700 dark:text-amber-300"
                                : "border-black/12 bg-black/[0.04] text-zinc-800 hover:border-black/20 hover:bg-black/[0.07] dark:border-white/12 dark:bg-white/[0.05] dark:text-zinc-100 dark:hover:border-white/20 dark:hover:bg-white/[0.09]"
                    }`}
                >
                    {checkStatus == "up-to-date" ? (
                        <>
                            <CheckIcon size={13} weight="bold" />
                            <span>Up to date</span>
                        </>
                    ) : (
                        <>
                            <ArrowsClockwiseIcon
                                size={13}
                                weight="bold"
                                className={checkStatus == "checking" ? "animate-spin" : ""}
                            />
                            <span>
                                {checkStatus == "checking"
                                    ? "Checking..."
                                    : checkStatus == "error"
                                        ? "Up to date"
                                        : "Check now"}
                            </span>
                        </>
                    )}
                </motion.button>
            </div>

            {/* Ignore WhatsApp conversations toggle */}
            <div
                onClick={onToggleIgnoreWhatsappConversations}
                className="mt-3 flex cursor-pointer items-center justify-between rounded-xl border border-black/12 bg-black/[0.02] px-3.5 py-2.5 transition-colors hover:border-black/20 dark:border-white/12 dark:bg-white/[0.02] dark:hover:border-white/20"
            >
                <span className="text-[13.5px] font-medium text-zinc-800 dark:text-zinc-100">
                    Ignore WhatsApp conversations
                </span>

                <button
                    type="button"
                    role="switch"
                    aria-checked={ignoreWhatsappConversations}
                    onClick={(e) => {
                        e.stopPropagation();
                        onToggleIgnoreWhatsappConversations();
                    }}
                    className={`relative flex h-6 w-11 shrink-0 cursor-pointer items-center rounded-full p-0.5 transition-colors duration-200 ${
                        ignoreWhatsappConversations
                            ? "bg-blue-600 dark:bg-blue-500"
                            : "bg-zinc-300 dark:bg-zinc-700"
                    }`}
                >
                    <motion.span
                        layout
                        transition={{
                            damping: 34,
                            type: "spring",
                            stiffness: 560,
                        }}
                        animate={{
                            x: ignoreWhatsappConversations ? 20 : 0,
                        }}
                        className="h-5 w-5 rounded-full bg-white shadow-sm"
                    />
                </button>
            </div>

            {/* About section header */}
            <h2 className="mt-8 text-[12.5px] font-medium tracking-wide text-zinc-500 uppercase dark:text-zinc-400">
                About
            </h2>

            {/* About Card matching media/github-cover.svg */}
            <div
                ref={aboutCardRef}
                className="mt-3 flex flex-col items-center justify-center rounded-2xl border border-black/10 bg-black/[0.02] px-6 py-7 text-center dark:border-white/10 dark:bg-white/[0.02]"
            >
                {/* Interlocking Velquo Symbol from media/github-cover.svg */}
                <svg
                    xmlns="http://www.w3.org/2000/svg"
                    viewBox="230 230 564 564"
                    className="h-18 w-18 drop-shadow-md"
                >
                    <defs>
                        <linearGradient
                            id="about-mac-ring-grad"
                            x1="0%"
                            y1="0%"
                            x2="100%"
                            y2="100%"
                        >
                            <stop offset="0%" stopColor="#38bdf8" />
                            <stop offset="50%" stopColor="#6366f1" />
                            <stop offset="100%" stopColor="#a855f7" />
                        </linearGradient>

                        <linearGradient
                            id="about-phone-ring-grad"
                            x1="0%"
                            y1="0%"
                            x2="100%"
                            y2="100%"
                        >
                            <stop offset="0%" stopColor="#ffffff" />
                            <stop offset="70%" stopColor="#f1f5f9" />
                            <stop offset="100%" stopColor="#bae6fd" />
                        </linearGradient>

                        <mask
                            id="about-mac-under-phone-mask"
                            maskUnits="userSpaceOnUse"
                            x="0"
                            y="0"
                            width="1024"
                            height="1024"
                        >
                            <rect x="0" y="0" width="1024" height="1024" fill="white" />
                            <line
                                x1="330"
                                y1="380"
                                x2="450"
                                y2="380"
                                stroke="black"
                                strokeWidth="96"
                            />
                        </mask>

                        <mask
                            id="about-phone-under-mac-mask"
                            maskUnits="userSpaceOnUse"
                            x="0"
                            y="0"
                            width="1024"
                            height="1024"
                        >
                            <rect x="0" y="0" width="1024" height="1024" fill="white" />
                            <line
                                x1="450"
                                y1="600"
                                x2="570"
                                y2="600"
                                stroke="black"
                                strokeWidth="96"
                            />
                        </mask>
                    </defs>

                    <g mask="url(#about-mac-under-phone-mask)">
                        <rect
                            x="390"
                            y="276"
                            width="360"
                            height="324"
                            rx="74"
                            ry="74"
                            fill="none"
                            stroke="url(#about-mac-ring-grad)"
                            strokeWidth="60"
                        />
                    </g>

                    <g mask="url(#about-phone-under-mac-mask)">
                        <rect
                            x="274"
                            y="380"
                            width="236"
                            height="368"
                            rx="74"
                            ry="74"
                            fill="none"
                            stroke="url(#about-phone-ring-grad)"
                            strokeWidth="60"
                            className="stroke-zinc-800 dark:stroke-[url(#about-phone-ring-grad)]"
                        />
                    </g>
                </svg>

                {/* App Name in Urbanist 700 (centered directly under the symbol) */}
                <span
                    style={{ fontFamily: "'Urbanist', -apple-system, sans-serif" }}
                    className="mt-3 text-[30px] leading-none font-bold tracking-tight text-zinc-900 dark:text-white"
                >
                    Velquo
                </span>

                {/* Tagline */}
                <p className="mt-2 text-[13.5px] font-medium text-zinc-500 dark:text-slate-400">
                    KDE Connect, reborn natively for Apple Silicon Macs.
                </p>

                {/* Version Pill + GitHub Repository Link */}
                <div className="mt-4 flex items-center gap-2.5">
                    <span className="inline-flex items-center rounded-xl border border-black/10 bg-black/[0.03] px-3 py-2 text-[12.5px] font-medium text-zinc-600 dark:border-white/10 dark:bg-white/[0.03] dark:text-zinc-400">
                        Version {appVersion}
                    </span>

                    <motion.button
                        type="button"
                        onClick={handleOpenGitHub}
                        whileHover={{ scale: 1.02 }}
                        whileTap={{ scale: 0.98 }}
                        transition={{ damping: 28, type: "spring", stiffness: 450 }}
                        className="flex cursor-pointer items-center gap-2 rounded-xl border border-black/12 bg-black/[0.03] px-3.5 py-2 text-[12.5px] font-medium text-zinc-700 transition-colors hover:border-black/20 hover:bg-black/[0.06] hover:text-zinc-950 dark:border-white/12 dark:bg-white/[0.04] dark:text-zinc-200 dark:hover:border-white/20 dark:hover:bg-white/[0.08] dark:hover:text-white"
                    >
                        <GithubLogoIcon size={16} weight="regular" />
                        <span>{repoDisplay}</span>
                    </motion.button>
                </div>
            </div>
        </motion.div>
    );
}
