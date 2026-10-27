#!/usr/bin/env bun
import { $ } from "bun";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { rmSync, mkdtempSync, readFileSync } from "node:fs";

const rootDir = resolve(import.meta.dir, "..");
const mediaDir = join(rootDir, "media");
const iconSvgPath = join(mediaDir, "icon.svg");
const symbolSvgPath = join(mediaDir, "symbol.svg");
const githubCoverPngPath = join(mediaDir, "github-cover.png");
const trayPngOut = join(rootDir, "src-tauri", "icons", "tray.png");

console.log("==> Generating Tauri app & Dock icons from media/icon.svg...");
await $`bun run tauri icon ${iconSvgPath}`.cwd(rootDir);

const tmpDir = mkdtempSync(join(tmpdir(), "velquo-icons-"));
try {
    console.log(
        "==> Generating macOS Menu Bar template icon (tray.png) from media/symbol.svg...",
    );
    await $`bun run tauri icon ${symbolSvgPath} -o ${tmpDir}`.cwd(rootDir);
    const rendered128 = join(tmpDir, "128x128.png");
    await $`/usr/bin/sips -z 44 44 ${rendered128} --out ${trayPngOut}`.quiet();

    console.log(
        "==> Rendering media/github-cover.png from media/original.png...",
    );
    await $`bun run ${join(import.meta.dir, "generate-cover.ts")}`
        .cwd(rootDir)
        .quiet();
} finally {
    rmSync(tmpDir, { force: true, recursive: true });
}

const cover = readFileSync(githubCoverPngPath);
if (cover.length == 0) {
    throw new Error("media/github-cover.png was not generated");
}

console.log("==> All icons & media/github-cover.png generated successfully!");

