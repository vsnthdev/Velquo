import { useState, useEffect } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { open } from "@tauri-apps/plugin-dialog";
import type { RunCommandItem } from "../../types";
import { motion, AnimatePresence } from "motion/react";
import {
    XIcon,
    PlusIcon,
    TrashIcon,
    ArrowLeftIcon,
    TerminalWindowIcon,
} from "@phosphor-icons/react";

interface DeviceSettingsViewProps {
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
    runCommands: RunCommandItem[];
    onAddRunCommand: (name: string, command: string) => Promise<void>;
    onRemoveRunCommand: (key: string) => Promise<void>;
    rightPaneContainer: HTMLElement | null;
    openAddCommandSignal: number;
    onUnpair: () => void;
}

const SAMPLE_MAC_COMMANDS = [
    { name: "Lock Screen", command: "pmset displaysleepnow" },
    { name: "Sleep Mac", command: "pmset sleepnow" },
    {
        name: "Mute Volume",
        command: 'osascript -e "set volume output muted true"',
    },
];

export function DeviceSettingsView({
    onUnpair,
    autoCopyOtp,
    runCommands,
    syncClipboard,
    allowPresenter,
    onAddRunCommand,
    fileSaveLocation,
    onTogglePresenter,
    syncNotifications,
    onRemoveRunCommand,
    rightPaneContainer,
    onToggleAutoCopyOtp,
    openAddCommandSignal,
    onToggleSyncClipboard,
    onToggleNotifications,
    onChangeFileSaveLocation,
}: DeviceSettingsViewProps) {
    const [isCommandsModalOpen, setIsCommandsModalOpen] =
        useState<boolean>(false);
    const [modalView, setModalView] = useState<"list" | "add">("list");
    const [newCmdName, setNewCmdName] = useState<string>("");
    const [newCmdString, setNewCmdString] = useState<string>("");
    const [formError, setFormError] = useState<boolean>(false);

    useEffect(() => {
        if (openAddCommandSignal > 0) {
            setFormError(false);
            setModalView("add");
            setIsCommandsModalOpen(true);
        }
    }, [openAddCommandSignal]);

    const handleChooseSaveLocation = async () => {
        const selected = await open({
            directory: true,
            multiple: false,
            title: "Select File Save Location",
            defaultPath: fileSaveLocation || undefined,
        });
        if (typeof selected === "string" && selected.trim()) {
            onChangeFileSaveLocation(selected);
        }
    };

    const handleCreateCommand = async () => {
        const trimmedName = newCmdName.trim();
        const trimmedCmd = newCmdString.trim();
        if (!trimmedName || !trimmedCmd) {
            setFormError(true);
            return;
        }
        try {
            await onAddRunCommand(trimmedName, trimmedCmd);
            setNewCmdName("");
            setNewCmdString("");
            setFormError(false);
            setModalView("list");
        } catch (err) {
            console.error("Failed to add run command:", err);
            setFormError(true);
        }
    };

    return (
        <motion.div
            key="device-settings-view"
            initial={{ y: 10, opacity: 0, filter: "blur(4px)" }}
            animate={{ y: 0, opacity: 1, filter: "blur(0px)" }}
            exit={{ y: -8, opacity: 0, filter: "blur(4px)" }}
            transition={{
                damping: 34,
                type: "spring",
                stiffness: 460,
            }}
            className="mt-6 flex flex-col gap-3.5"
        >
            {/* File save location setting */}
            <div className="mb-1 flex flex-col gap-2">
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
                            damping: 34,
                            type: "spring",
                            stiffness: 560,
                        }}
                        animate={{
                            x: syncClipboard ? 20 : 0,
                        }}
                        className="h-5 w-5 rounded-full bg-white shadow-sm"
                    />
                </button>
            </div>

            {/* Presentation remote toggle */}
            <div
                onClick={onTogglePresenter}
                className="flex cursor-pointer items-center justify-between rounded-xl border border-black/12 bg-black/[0.02] px-3.5 py-2.5 transition-colors hover:border-black/20 dark:border-white/12 dark:bg-white/[0.02] dark:hover:border-white/20"
            >
                <span className="text-[13.5px] font-medium text-zinc-800 dark:text-zinc-100">
                    Presentation remote
                </span>

                <button
                    type="button"
                    role="switch"
                    aria-checked={allowPresenter}
                    onClick={(e) => {
                        e.stopPropagation();
                        onTogglePresenter();
                    }}
                    className={`relative flex h-6 w-11 shrink-0 cursor-pointer items-center rounded-full p-0.5 transition-colors duration-200 ${
                        allowPresenter
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
                            x: allowPresenter ? 20 : 0,
                        }}
                        className="h-5 w-5 rounded-full bg-white shadow-sm"
                    />
                </button>
            </div>

            {/* Sync notifications toggle */}
            <div
                onClick={onToggleNotifications}
                className="flex cursor-pointer items-center justify-between rounded-xl border border-black/12 bg-black/[0.02] px-3.5 py-2.5 transition-colors hover:border-black/20 dark:border-white/12 dark:bg-white/[0.02] dark:hover:border-white/20"
            >
                <span className="text-[13.5px] font-medium text-zinc-800 dark:text-zinc-100">
                    Sync notifications
                </span>

                <button
                    type="button"
                    role="switch"
                    aria-checked={syncNotifications}
                    onClick={(e) => {
                        e.stopPropagation();
                        onToggleNotifications();
                    }}
                    className={`relative flex h-6 w-11 shrink-0 cursor-pointer items-center rounded-full p-0.5 transition-colors duration-200 ${
                        syncNotifications
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
                            x: syncNotifications ? 20 : 0,
                        }}
                        className="h-5 w-5 rounded-full bg-white shadow-sm"
                    />
                </button>
            </div>

            {/* Auto-copy OTPs toggle */}
            <div
                onClick={onToggleAutoCopyOtp}
                className="flex cursor-pointer items-center justify-between rounded-xl border border-black/12 bg-black/[0.02] px-3.5 py-2.5 transition-colors hover:border-black/20 dark:border-white/12 dark:bg-white/[0.02] dark:hover:border-white/20"
            >
                <span className="text-[13.5px] font-medium text-zinc-800 dark:text-zinc-100">
                    Auto-copy OTPs
                </span>

                <button
                    type="button"
                    role="switch"
                    aria-checked={autoCopyOtp}
                    onClick={(e) => {
                        e.stopPropagation();
                        onToggleAutoCopyOtp();
                    }}
                    className={`relative flex h-6 w-11 shrink-0 cursor-pointer items-center rounded-full p-0.5 transition-colors duration-200 ${
                        autoCopyOtp
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
                            x: autoCopyOtp ? 20 : 0,
                        }}
                        className="h-5 w-5 rounded-full bg-white shadow-sm"
                    />
                </button>
            </div>

            {/* Configure commands button + Radix Dialog scoped to the right pane */}
            <Dialog.Root
                open={isCommandsModalOpen}
                onOpenChange={(openState) => {
                    setIsCommandsModalOpen(openState);
                    if (!openState) {
                        setModalView("list");
                        setFormError(false);
                    }
                }}
            >
                <Dialog.Trigger asChild>
                    <motion.button
                        type="button"
                        whileHover={{ scale: 1.008 }}
                        whileTap={{ scale: 0.99 }}
                        transition={{
                            damping: 28,
                            type: "spring",
                            stiffness: 450,
                        }}
                        className="mt-1 flex w-full cursor-pointer items-center justify-center gap-2 rounded-xl border border-black/12 bg-black/[0.02] px-4 py-2.5 text-[13.5px] font-medium text-zinc-800 transition-colors hover:border-black/20 hover:bg-black/[0.05] dark:border-white/12 dark:bg-white/[0.02] dark:text-zinc-100 dark:hover:border-white/20 dark:hover:bg-white/[0.05]"
                    >
                        <TerminalWindowIcon size={16} weight="regular" />
                        <span>Configure commands</span>
                        {runCommands.length > 0 && (
                            <span className="ml-0.5 rounded-md bg-black/[0.07] px-1.5 py-0.2 text-[11px] font-medium text-zinc-600 dark:bg-white/[0.10] dark:text-zinc-300">
                                {runCommands.length}
                            </span>
                        )}
                    </motion.button>
                </Dialog.Trigger>

                <AnimatePresence>
                    {isCommandsModalOpen && (
                        <Dialog.Portal forceMount container={rightPaneContainer}>
                            <Dialog.Overlay asChild forceMount>
                                <motion.div
                                    initial={{ opacity: 0 }}
                                    animate={{ opacity: 1 }}
                                    exit={{ opacity: 0 }}
                                    transition={{ duration: 0.2, ease: "easeOut" }}
                                    className="absolute inset-0 z-40 bg-black/35 backdrop-blur-xs dark:bg-black/55"
                                />
                            </Dialog.Overlay>

                            <Dialog.Content asChild forceMount>
                                <motion.div
                                    initial={{ y: 10, opacity: 0, scale: 0.95 }}
                                    animate={{ y: 0, scale: 1, opacity: 1 }}
                                    exit={{ y: 8, opacity: 0, scale: 0.96 }}
                                    transition={{
                                        damping: 34,
                                        type: "spring",
                                        stiffness: 480,
                                    }}
                                    className="absolute top-1/2 left-1/2 z-50 flex max-h-[82%] w-[88%] max-w-md -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-2xl border border-black/12 bg-white p-5 shadow-2xl outline-none dark:border-white/15 dark:bg-zinc-900"
                                >
                                    {/* Modal Header */}
                                    <div className="flex items-center justify-between gap-3">
                                        <div className="flex items-center gap-2.5">
                                            {modalView === "add" && (
                                                <motion.button
                                                    type="button"
                                                    whileHover={{ scale: 1.06 }}
                                                    whileTap={{ scale: 0.94 }}
                                                    onClick={() => {
                                                        setFormError(false);
                                                        setModalView("list");
                                                    }}
                                                    className="flex h-7 w-7 cursor-pointer items-center justify-center rounded-lg border border-black/10 bg-black/[0.02] text-zinc-600 transition-colors hover:bg-black/[0.06] dark:border-white/10 dark:bg-white/[0.03] dark:text-zinc-300 dark:hover:bg-white/[0.08]"
                                                    title="Back to commands list"
                                                >
                                                    <ArrowLeftIcon size={14} weight="bold" />
                                                </motion.button>
                                            )}

                                            <Dialog.Title className="text-[15.5px] font-semibold text-zinc-900 dark:text-zinc-100">
                                                {modalView === "list"
                                                    ? "Configure commands"
                                                    : "Add command"}
                                            </Dialog.Title>
                                        </div>

                                        <div className="flex items-center gap-2">
                                            {modalView === "list" && (
                                                <motion.button
                                                    type="button"
                                                    whileHover={{ scale: 1.03 }}
                                                    whileTap={{ scale: 0.97 }}
                                                    onClick={() => {
                                                        setFormError(false);
                                                        setModalView("add");
                                                    }}
                                                    className="flex cursor-pointer items-center gap-1 rounded-lg bg-blue-600 px-2.5 py-1 text-[12px] font-medium text-white shadow-xs transition-colors hover:bg-blue-500"
                                                >
                                                    <PlusIcon size={13} weight="bold" />
                                                    <span>Add</span>
                                                </motion.button>
                                            )}

                                            <Dialog.Close asChild>
                                                <motion.button
                                                    type="button"
                                                    whileHover={{ scale: 1.06 }}
                                                    whileTap={{ scale: 0.94 }}
                                                    className="flex h-7 w-7 cursor-pointer items-center justify-center rounded-lg text-zinc-400 transition-colors hover:bg-black/[0.06] hover:text-zinc-700 dark:hover:bg-white/[0.08] dark:hover:text-zinc-200"
                                                    aria-label="Close"
                                                >
                                                    <XIcon size={15} weight="bold" />
                                                </motion.button>
                                            </Dialog.Close>
                                        </div>
                                    </div>

                                    {/* Swappable Modal Inner Content: List View <-> Add New Command View */}
                                    <div className="mt-4 flex-1 overflow-y-auto">
                                        <AnimatePresence mode="wait" initial={false}>
                                            {modalView === "list" ? (
                                                <motion.div
                                                    key="commands-list-view"
                                                    initial={{ x: -10, opacity: 0 }}
                                                    animate={{ x: 0, opacity: 1 }}
                                                    exit={{ x: -10, opacity: 0 }}
                                                    transition={{
                                                        damping: 35,
                                                        type: "spring",
                                                        stiffness: 500,
                                                    }}
                                                    className="flex flex-col gap-2"
                                                >
                                                    {runCommands.length > 0 ? (
                                                        runCommands.map((item) => (
                                                            <div
                                                                key={item.key}
                                                                className="flex items-center justify-between gap-3 rounded-xl border border-black/10 bg-black/[0.02] px-3.5 py-2.5 dark:border-white/10 dark:bg-white/[0.03]"
                                                            >
                                                                <div className="flex min-w-0 flex-1 flex-col">
                                                                    <span className="truncate text-[13.5px] font-medium text-zinc-900 dark:text-zinc-100">
                                                                        {item.name}
                                                                    </span>
                                                                    <span className="mt-0.5 truncate font-mono text-[11.5px] text-zinc-500 dark:text-zinc-400">
                                                                        {item.command}
                                                                    </span>
                                                                </div>

                                                                <motion.button
                                                                    type="button"
                                                                    whileHover={{ scale: 1.08 }}
                                                                    whileTap={{ scale: 0.92 }}
                                                                    onClick={() => onRemoveRunCommand(item.key)}
                                                                    className="flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded-lg text-zinc-400 transition-colors hover:bg-red-500/12 hover:text-red-500 dark:hover:bg-red-500/20 dark:hover:text-red-400"
                                                                    title="Delete command"
                                                                >
                                                                    <TrashIcon size={14} weight="regular" />
                                                                </motion.button>
                                                            </div>
                                                        ))
                                                    ) : (
                                                        <div className="flex flex-col items-center justify-center rounded-xl border border-black/8 bg-black/[0.015] px-4 py-8 text-center dark:border-white/8 dark:bg-white/[0.015]">
                                                            <TerminalWindowIcon
                                                                size={24}
                                                                weight="regular"
                                                                className="text-zinc-400 dark:text-zinc-500"
                                                            />
                                                            <span className="mt-2 text-[13.5px] font-medium text-zinc-700 dark:text-zinc-300">
                                                                No commands configured yet
                                                            </span>
                                                            <span className="mt-0.5 max-w-xs text-[12px] text-zinc-500 dark:text-zinc-400">
                                                                Add shell commands to trigger them remotely from
                                                                the KDE Connect app on your phone.
                                                            </span>
                                                        </div>
                                                    )}
                                                </motion.div>
                                            ) : (
                                                <motion.div
                                                    key="commands-add-view"
                                                    initial={{ x: 10, opacity: 0 }}
                                                    animate={{ x: 0, opacity: 1 }}
                                                    exit={{ x: 10, opacity: 0 }}
                                                    transition={{
                                                        damping: 35,
                                                        type: "spring",
                                                        stiffness: 500,
                                                    }}
                                                    className="flex flex-col gap-3.5"
                                                >
                                                    <div className="flex flex-col gap-1.5">
                                                        <label className="text-[12px] font-medium text-zinc-500 dark:text-zinc-400">
                                                            Command name
                                                        </label>
                                                        <input
                                                            type="text"
                                                            value={newCmdName}
                                                            onChange={(e) => {
                                                                setNewCmdName(e.target.value);
                                                                if (formError) setFormError(false);
                                                            }}
                                                            placeholder="Lock Screen"
                                                            autoFocus
                                                            className={`select-text rounded-xl border bg-black/[0.02] px-3 py-2 text-[13px] text-zinc-900 placeholder:text-zinc-400 focus:outline-none dark:bg-white/[0.03] dark:text-zinc-100 dark:placeholder:text-zinc-500 ${
                                                                formError && !newCmdName.trim()
                                                                    ? "border-red-500/80 dark:border-red-400/80"
                                                                    : "border-black/12 focus:border-blue-500/60 dark:border-white/12 dark:focus:border-blue-400/60"
                                                            }`}
                                                        />
                                                    </div>

                                                    <div className="flex flex-col gap-1.5">
                                                        <label className="text-[12px] font-medium text-zinc-500 dark:text-zinc-400">
                                                            Shell command
                                                        </label>
                                                        <input
                                                            type="text"
                                                            value={newCmdString}
                                                            onChange={(e) => {
                                                                setNewCmdString(e.target.value);
                                                                if (formError) setFormError(false);
                                                            }}
                                                            onKeyDown={(e) => {
                                                                if (e.key === "Enter") {
                                                                    e.preventDefault();
                                                                    handleCreateCommand();
                                                                }
                                                            }}
                                                            placeholder="pmset displaysleepnow"
                                                            className={`select-text rounded-xl border bg-black/[0.02] px-3 py-2 font-mono text-[12.5px] text-zinc-900 placeholder:text-zinc-400 focus:outline-none dark:bg-white/[0.03] dark:text-zinc-100 dark:placeholder:text-zinc-500 ${
                                                                formError && !newCmdString.trim()
                                                                    ? "border-red-500/80 dark:border-red-400/80"
                                                                    : "border-black/12 focus:border-blue-500/60 dark:border-white/12 dark:focus:border-blue-400/60"
                                                            }`}
                                                        />
                                                    </div>

                                                    {/* Sample quick presets */}
                                                    <div className="flex flex-col gap-1.5">
                                                        <span className="text-[11.5px] font-medium text-zinc-400 dark:text-zinc-500">
                                                            Presets
                                                        </span>
                                                        <div className="flex flex-wrap gap-1.5">
                                                            {SAMPLE_MAC_COMMANDS.map((preset) => (
                                                                <button
                                                                    key={preset.name}
                                                                    type="button"
                                                                    onClick={() => {
                                                                        setNewCmdName(preset.name);
                                                                        setNewCmdString(preset.command);
                                                                        setFormError(false);
                                                                    }}
                                                                    className="cursor-pointer rounded-lg border border-black/10 bg-black/[0.02] px-2.5 py-1 text-[11.5px] font-medium text-zinc-600 transition-colors hover:border-black/20 hover:bg-black/[0.05] dark:border-white/10 dark:bg-white/[0.03] dark:text-zinc-300 dark:hover:border-white/20 dark:hover:bg-white/[0.06]"
                                                                >
                                                                    {preset.name}
                                                                </button>
                                                            ))}
                                                        </div>
                                                    </div>

                                                    <motion.button
                                                        type="button"
                                                        whileHover={{ scale: 1.01 }}
                                                        whileTap={{ scale: 0.985 }}
                                                        onClick={handleCreateCommand}
                                                        className="mt-1 flex w-full cursor-pointer items-center justify-center rounded-xl bg-blue-600 px-4 py-2 text-[13px] font-medium text-white shadow-xs transition-colors hover:bg-blue-500"
                                                    >
                                                        Save command
                                                    </motion.button>
                                                </motion.div>
                                            )}
                                        </AnimatePresence>
                                    </div>
                                </motion.div>
                            </Dialog.Content>
                        </Dialog.Portal>
                    )}
                </AnimatePresence>
            </Dialog.Root>

            <motion.button
                type="button"
                onClick={onUnpair}
                whileHover={{ scale: 1.008 }}
                whileTap={{ scale: 0.99 }}
                transition={{
                    damping: 28,
                    type: "spring",
                    stiffness: 450,
                }}
                className="flex w-full cursor-pointer items-center justify-center gap-2 rounded-xl border border-red-500/25 bg-red-500/[0.08] px-4 py-2.5 text-[13.5px] font-medium text-red-600 transition-colors hover:border-red-500/40 hover:bg-red-500/[0.14] dark:border-red-500/25 dark:bg-red-500/[0.10] dark:text-red-400 dark:hover:border-red-400/40 dark:hover:bg-red-500/[0.16]"
            >
                <TrashIcon size={16} weight="regular" />
                <span>Unpair</span>
            </motion.button>
        </motion.div>
    );
}
