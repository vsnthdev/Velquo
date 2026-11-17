import { motion } from "motion/react";
import { invoke } from "@tauri-apps/api/core";
import { useRef, useState, useEffect } from "react";
import { CopyIcon, CheckIcon } from "@phosphor-icons/react";

const HOMEBREW_UPGRADE_COMMAND = "brew upgrade --cask vsnthdev/tap/velquo";

interface HomebrewUpgradeBoxProps {
    title?: string;
    className?: string;
}

export function HomebrewUpgradeBox({
    title,
    className = "",
}: HomebrewUpgradeBoxProps) {
    const [copied, setCopied] = useState<boolean>(false);
    const resetTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

    useEffect(() => {
        return () => {
            if (resetTimerRef.current) {
                clearTimeout(resetTimerRef.current);
            }
        };
    }, []);

    const handleCopy = async () => {
        try {
            await invoke("copy_text_to_clipboard", {
                text: HOMEBREW_UPGRADE_COMMAND,
            });
        } catch {
            try {
                await navigator.clipboard.writeText(HOMEBREW_UPGRADE_COMMAND);
            } catch {
                return;
            }
        }

        setCopied(true);
        if (resetTimerRef.current) {
            clearTimeout(resetTimerRef.current);
        }
        resetTimerRef.current = setTimeout(() => {
            setCopied(false);
        }, 1800);
    };

    return (
        <div
            className={`flex flex-col gap-2 rounded-xl border border-amber-500/25 bg-amber-500/[0.06] px-3 py-2.5 dark:border-amber-400/20 dark:bg-amber-400/[0.06] ${className}`.trim()}
        >
            {title && (
                <p className="text-[12.5px] font-semibold text-zinc-900 dark:text-zinc-100">
                    {title}
                </p>
            )}
            <p className="text-[12px] leading-relaxed text-zinc-700 dark:text-zinc-300">
                Velquo was installed by Homebrew so run the below command to get the
                latest version.
            </p>
            <div className="flex items-center gap-2">
                <input
                    type="text"
                    readOnly
                    value={HOMEBREW_UPGRADE_COMMAND}
                    aria-label="Homebrew upgrade command"
                    onClick={(e) => e.currentTarget.select()}
                    onFocus={(e) => e.currentTarget.select()}
                    className="min-w-0 flex-1 rounded-lg border border-black/12 bg-white/85 px-2.5 py-1.5 font-mono text-sm text-zinc-900 select-text focus:border-blue-500/50 focus:outline-none dark:border-white/12 dark:bg-zinc-950/60 dark:text-zinc-100"
                />
                <motion.button
                    type="button"
                    whileHover={{ scale: 1.02 }}
                    whileTap={{ scale: 0.97 }}
                    onClick={() => void handleCopy()}
                    title="Copy Homebrew upgrade command"
                    className={`flex shrink-0 cursor-pointer items-center gap-1.5 rounded-lg border px-3 py-1.5 text-[12px] font-medium transition-colors ${
                        copied
                            ? "border-emerald-500/30 bg-emerald-500/15 text-emerald-700 dark:text-emerald-300"
                            : "border-black/12 bg-white/85 text-zinc-800 hover:border-black/20 hover:bg-white dark:border-white/12 dark:bg-white/[0.08] dark:text-zinc-100 dark:hover:border-white/20 dark:hover:bg-white/[0.12]"
                    }`}
                >
                    {copied ? (
                        <>
                            <CheckIcon size={13} weight="bold" />
                            <span>Copied</span>
                        </>
                    ) : (
                        <>
                            <CopyIcon size={13} weight="regular" />
                            <span>Copy</span>
                        </>
                    )}
                </motion.button>
            </div>
        </div>
    );
}
