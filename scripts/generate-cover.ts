#!/usr/bin/env bun
import sharp from "sharp";
import { Resvg } from "@resvg/resvg-js";

console.log("[1] Starting generate-cover.ts");

// Locate installed Urbanist font files on the system first, before loading images.
const fontSearchDirs = [`${Bun.env.HOME}/Library/Fonts`, "/Library/Fonts"];
console.log("[2] fontSearchDirs:", fontSearchDirs);

const urbanistGlob = new Bun.Glob("*[Uu]rbanist*.{ttf,otf,ttc}");
console.log("[3] Created Bun.Glob");

const fontFiles: string[] = [];

for (const dir of fontSearchDirs) {
    console.log("[4] Scanning directory:", dir);
    try {
        for (const fontPath of urbanistGlob.scanSync({
            cwd: dir,
            absolute: true,
        })) {
            fontFiles.push(fontPath);
        }
    } catch {
        // Ignore non-existent font directories
    }
}
console.log("[5] Finished scanning font directories");

if (fontFiles.length == 0) {
    throw new Error(
        `Urbanist font is not installed on this system (searched ${fontSearchDirs.join(", ")}). Please install Urbanist before running generate-cover.`,
    );
}

console.log(
    `==> Found ${fontFiles.length} Urbanist font file(s):\n${fontFiles.map((f) => `    - ${f}`).join("\n")}`,
);

const mediaUrl = new URL("../media/", import.meta.url);
console.log("[6] mediaUrl:", mediaUrl.href);

const originalPngFile = Bun.file(new URL("original.png", mediaUrl));
const iconSvgFile = Bun.file(new URL("icon.svg", mediaUrl));
const githubCoverPngFile = Bun.file(new URL("github-cover.png", mediaUrl));
console.log("[7] Created Bun.file handles");

// GitHub's recommended social preview ratio, sized for a Retina (2x) export
const OUT_W = 2560;
const OUT_H = 1280;

console.log("[8] Reading original.png bytes...");
const originalBytes = await originalPngFile.bytes();
console.log("[9] Read original.png bytes:", originalBytes.byteLength);

console.log("[10] Reading icon.svg text...");
const iconSource = await iconSvgFile.text();
console.log("[11] Read icon.svg text, length:", iconSource.length);

console.log("[12] Calling sharp(originalBytes).metadata()...");
const original = await sharp(originalBytes).metadata();
console.log("[13] Got metadata:", original.width, "x", original.height);

const originalW = original.width;
const originalH = original.height;
if (!originalW || !originalH) {
    throw new Error("Could not read the dimensions of media/original.png");
}

// Keep the full screenshot width and crop vertically so the wordmark band sits
// on top of the wallpaper while the app window stays fully visible below it.
const cropH = Math.min(originalH, Math.round((originalW * OUT_H) / OUT_W));
if (cropH < originalH) {
    console.log(
        `==> Cropping media/original.png ${originalW}x${originalH} -> ${originalW}x${cropH} (16:9)`,
    );
}
console.log("[14] Calculated cropH:", cropH);

// Sizes live in cropped-screenshot pixel space so the wordmark is rendered
// natively at high resolution instead of being upscaled.
const ICON_SIZE = Math.round(originalW * 0.066);
const ICON_GAP = Math.round(originalW * 0.006);
const TITLE_SIZE = Math.round(originalW * 0.048);
const TITLE_BASELINE = Math.round(originalW * 0.081);
const TAGLINE_SIZE = Math.round(originalW * 0.0185);
const TAGLINE_BASELINE = Math.round(originalW * 0.112);
const TITLE_MID_Y = TITLE_BASELINE - Math.round(TITLE_SIZE * 0.36);
const ICON_TOP = TITLE_MID_Y - Math.round(ICON_SIZE / 2);
const CORNER_RADIUS = Math.round(originalW * 0.014);
console.log("[15] Calculated layout constants");

// Reuse the real app icon artwork so the cover always matches the Dock icon.
console.log("[16] Matching iconInnerMatch regex...");
const iconInnerMatch = iconSource.match(/[\s\S]*?<svg[^>]*>([\s\S]*)<\/svg>/);
console.log("[17] Matched iconInnerMatch:", Boolean(iconInnerMatch));
if (!iconInnerMatch) {
    throw new Error("Could not parse media/icon.svg");
}

console.log("[18] Matching iconDefsMatch regex...");
const iconDefsMatch = iconInnerMatch[1].match(/<defs>([\s\S]*?)<\/defs>/);
console.log("[19] Matched iconDefsMatch:", Boolean(iconDefsMatch));

// Namespace the app icon ids so its gradients never clash with the cover's.
function scopeIconIds(svg: string): string {
    return svg
        .replace(/id="([^"]+)"/g, (_match, id: string) => `id="app-icon-${id}"`)
        .replace(
            /url\(#([^)]+)\)/g,
            (_match, id: string) => `url(#app-icon-${id})`,
        );
}

console.log("[20] Scoping iconDefs...");
const iconDefs = scopeIconIds(iconDefsMatch?.[1] ?? "");
console.log("[21] Scoping iconBody...");
const iconBody = scopeIconIds(
    iconInnerMatch[1].replace(/<defs>[\s\S]*?<\/defs>/, ""),
);
console.log("[22] Scoped iconDefs and iconBody");

function buildOverlaySvg(unitLeft: number): string {
    return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${originalW} ${cropH}" width="${originalW}" height="${cropH}">
  <defs>
${iconDefs}

    <!-- Deep obsidian scrim fading the wallpaper out behind the wordmark -->
    <linearGradient id="scrim" x1="50%" y1="0%" x2="50%" y2="100%">
      <stop offset="0%" stop-color="#080a11" stop-opacity="0.92" />
      <stop offset="55%" stop-color="#080a11" stop-opacity="0.72" />
      <stop offset="100%" stop-color="#080a11" stop-opacity="0" />
    </linearGradient>

    <!-- Ambient cyan/indigo bloom behind the title -->
    <radialGradient id="bloom" cx="50%" cy="26%" r="42%">
      <stop offset="0%" stop-color="#38bdf8" stop-opacity="0.16" />
      <stop offset="55%" stop-color="#6366f1" stop-opacity="0.07" />
      <stop offset="100%" stop-color="#090a0f" stop-opacity="0" />
    </radialGradient>
  </defs>

  <rect x="0" y="0" width="${originalW}" height="${Math.round(originalW * 0.2)}" fill="url(#scrim)" />
  <rect x="0" y="0" width="${originalW}" height="${Math.round(originalW * 0.24)}" fill="url(#bloom)" />

  <!-- App icon + Velquo wordmark, centred as a single unit -->
  <g transform="translate(${unitLeft}, ${ICON_TOP}) scale(${(ICON_SIZE / 1024).toFixed(6)})">
    ${iconBody}
  </g>

  <text
    x="${unitLeft + ICON_SIZE + ICON_GAP}"
    y="${TITLE_BASELINE}"
    text-anchor="start"
    font-family="Urbanist"
    font-weight="700"
    font-size="${TITLE_SIZE}"
    letter-spacing="-2"
    fill="#ffffff"
  >Velquo</text>

  <!-- Tagline -->
  <text
    x="${originalW / 2}"
    y="${TAGLINE_BASELINE}"
    text-anchor="middle"
    font-family="Urbanist"
    font-weight="500"
    font-size="${TAGLINE_SIZE}"
    fill="#c3cddb"
  >KDE Connect, reborn natively for Apple Silicon Macs.</text>
</svg>
`;
}

// Measure the wordmark so the icon + text pair can be centred on the canvas.
const MEASURE_SIZE = 512;
console.log("[23] Constructing Resvg for measurement...");
const measureResvg = new Resvg(
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 4096 ${MEASURE_SIZE * 2}" width="4096" height="${MEASURE_SIZE * 2}">
      <text x="0" y="${MEASURE_SIZE * 1.5}" font-family="Urbanist" font-weight="700" font-size="${MEASURE_SIZE}" fill="#000000">Velquo</text>
    </svg>`,
    {
        font: {
            fontFiles,
            loadSystemFonts: false,
            defaultFontFamily: "Urbanist",
        },
    },
);
console.log("[24] Calling measureResvg.render()...");
const measured = measureResvg.render();
console.log("[25] Rendered measured image:", measured.width, "x", measured.height);

let titleWidth = Math.round(TITLE_SIZE * 2.8);
let measuredMinX = measured.width;
let measuredMaxX = 0;
console.log("[26] Scanning measured pixels...");
const measuredPixels = measured.pixels;
const measuredWidth = measured.width;
const measuredHeight = measured.height;
for (let y = 0; y < measuredHeight; y++) {
    for (let x = 0; x < measuredWidth; x++) {
        if (measuredPixels[(y * measuredWidth + x) * 4 + 3] > 8) {
            if (x < measuredMinX) measuredMinX = x;
            if (x > measuredMaxX) measuredMaxX = x;
        }
    }
}
console.log("[27] Finished scanning measured pixels:", measuredMinX, "to", measuredMaxX);

if (measuredMaxX > measuredMinX) {
    titleWidth = Math.round(
        ((measuredMaxX - measuredMinX + 1) * TITLE_SIZE) / MEASURE_SIZE,
    );
}
console.log("[28] Calculated titleWidth:", titleWidth);

const unitWidth = ICON_SIZE + ICON_GAP + titleWidth;
const unitLeft = Math.round(originalW / 2 - unitWidth / 2);
console.log("[29] Calculated unitWidth:", unitWidth, "unitLeft:", unitLeft);

console.log(
    `==> Rendering icon + wordmark overlay -> ${originalW}x${cropH} PNG...`,
);
console.log("[30] Building overlay SVG string...");
const overlaySvgString = buildOverlaySvg(unitLeft);
console.log("[31] Built overlay SVG string, length:", overlaySvgString.length);

console.log("[32] Constructing Resvg for overlay...");
const overlayResvg = new Resvg(overlaySvgString, {
    fitTo: { mode: "width", value: originalW },
    font: {
        fontFiles,
        loadSystemFonts: false,
        defaultFontFamily: "Urbanist",
    },
});
console.log("[33] Calling overlayResvg.render()...");
const overlayRendered = overlayResvg.render();
console.log("[34] Calling overlayRendered.asPng()...");
const overlayPng = overlayRendered.asPng();
console.log("[35] Got overlayPng, byteLength:", overlayPng.byteLength);

// Rounded-corner mask: white inside the arc, transparent outside, so the
// `dest-in` blend keeps only the rounded region of the composed image.
console.log("[36] Encoding cornerMask...");
const cornerMask = Buffer.from(
    `<svg xmlns="http://www.w3.org/2000/svg" width="${originalW}" height="${cropH}">
      <rect x="0" y="0" width="${originalW}" height="${cropH}" rx="${CORNER_RADIUS}" ry="${CORNER_RADIUS}" fill="#ffffff" />
    </svg>`,
);
console.log("[37] Encoded cornerMask, byteLength:", cornerMask.byteLength);

console.log(
    "==> Compositing overlay on top of media/original.png with rounded corners...",
);
console.log("[38] Running sharp composite pipeline...");
const compositedBytes = await sharp(originalBytes)
    .extract({ left: 0, top: 0, width: originalW, height: cropH })
    .ensureAlpha()
    .composite([
        { blend: "over", input: overlayPng, left: 0, top: 0 },
        { blend: "dest-in", input: cornerMask, left: 0, top: 0 },
    ])
    .png()
    .toBuffer();

console.log("[39] Resizing composited image to 2560x1280...");
const coverBytes = await sharp(compositedBytes)
    .resize(OUT_W, OUT_H, { fit: "fill", kernel: "lanczos3" })
    .png()
    .toBuffer();
console.log("[40] Finished sharp pipeline, coverBytes:", coverBytes.byteLength);

console.log("[41] Checking cover metadata...");
const cover = await sharp(coverBytes).metadata();
console.log("[42] Cover metadata:", cover.width, "x", cover.height);
if (cover.width != OUT_W || cover.height != OUT_H) {
    throw new Error(
        `media/github-cover.png has an unexpected size ${cover.width}x${cover.height}`,
    );
}
if (coverBytes.byteLength == 0) {
    throw new Error("media/github-cover.png is empty");
}

console.log("[42] Writing media/github-cover.png via Bun.write...");
await Bun.write(githubCoverPngFile, coverBytes);
console.log("[43] Finished Bun.write");

console.log("==> media/github-cover.png generated successfully!");
