#!/usr/bin/env bun
import { $ } from "bun";
import { tmpdir } from "node:os";
import { Resvg } from "@resvg/resvg-js";
import { join, resolve } from "node:path";
import { rmSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";

const rootDir = resolve(import.meta.dir, "..");
const mediaDir = join(rootDir, "media");
const iconSvgPath = join(mediaDir, "icon.svg");
const symbolSvgPath = join(mediaDir, "symbol.svg");
const githubCoverSvgPath = join(mediaDir, "github-cover.svg");
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
        "==> Rendering media/github-cover.svg -> media/github-cover.png...",
    );
    // Fetch static Urbanist 500 & 700 TTF font files from Google Fonts API so resvg renders exact Urbanist weights
    const fontFiles: string[] = [];
    try {
        const cssResp = await fetch(
            "https://fonts.googleapis.com/css2?family=Urbanist:wght@500;700",
            {
                headers: {
                    // Non-modern UA instructs Google Fonts to return static .ttf URLs instead of .woff2
                    "User-Agent":
            "Mozilla/5.0 (Macintosh; U; Intel Mac OS X 10_6_8; en-us)",
                },
            },
        );
        if (cssResp.ok) {
            const cssText = await cssResp.text();
            const ttfUrls = [
                ...cssText.matchAll(/url\((https:\/\/[^)]+\.ttf)\)/g),
            ].map((m) => m[1]);
            for (let i = 0; i < ttfUrls.length; i++) {
                const ttfResp = await fetch(ttfUrls[i]);
                if (ttfResp.ok) {
                    const fontPath = join(tmpDir, `Urbanist-${i}.ttf`);
                    writeFileSync(fontPath, Buffer.from(await ttfResp.arrayBuffer()));
                    fontFiles.push(fontPath);
                }
            }
        }
    } catch {
    // Fallback to system fonts if offline
    }

    const coverSvg = readFileSync(githubCoverSvgPath, "utf-8");
    const resvg = new Resvg(coverSvg, {
        fitTo: { value: 1280, mode: "width" },
        font: {
            fontFiles,
            loadSystemFonts: true,
            defaultFontFamily: "Urbanist",
        },
    });
    const pngData = resvg.render();
    writeFileSync(githubCoverPngPath, pngData.asPng());
} finally {
    rmSync(tmpDir, { force: true, recursive: true });
}

console.log("==> All icons & media/github-cover.png generated successfully!");
