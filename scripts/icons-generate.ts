#!/usr/bin/env bun
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { $ } from "bun";

const rootDir = resolve(import.meta.dir, "..");
const iconSvgPath = join(rootDir, "icon.svg");
const symbolSvgPath = join(rootDir, "symbol.svg");
const trayPngOut = join(rootDir, "src-tauri", "icons", "tray.png");

console.log("==> Generating Tauri app & Dock icons from icon.svg...");
await $`bun run tauri icon ${iconSvgPath}`.cwd(rootDir);

console.log("==> Generating macOS Menu Bar template icon (tray.png) from symbol.svg...");
const tmpDir = mkdtempSync(join(tmpdir(), "velquo-icons-"));
try {
  await $`bun run tauri icon ${symbolSvgPath} -o ${tmpDir}`.cwd(rootDir);
  const rendered128 = join(tmpDir, "128x128.png");
  await $`/usr/bin/sips -z 44 44 ${rendered128} --out ${trayPngOut}`.quiet();
} finally {
  rmSync(tmpDir, { recursive: true, force: true });
}

console.log("==> All icons generated in src-tauri/icons/");
