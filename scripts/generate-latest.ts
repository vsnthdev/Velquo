#!/usr/bin/env bun
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { createInterface } from "node:readline";

const rootDir = resolve(import.meta.dir, "..");
const pkgPath = join(rootDir, "package.json");
const latestJsonPath = join(rootDir, "latest.json");
const sigPath = join(
  rootDir,
  "src-tauri",
  "target",
  "release",
  "bundle",
  "macos",
  "Velquo.app.tar.gz.sig",
);

const pkg = JSON.parse(readFileSync(pkgPath, "utf-8")) as {
  version: string;
  homepage?: string;
};
const version = pkg.version.trim().replace(/^v/, "");
const repoBaseUrl = (
  pkg.homepage || "https://github.com/vsnthdev/Velquo"
).replace(/\/+$/, "");

let existingSignature = "";
let existingNotes = "";
if (existsSync(latestJsonPath)) {
  try {
    const currentLatest = JSON.parse(readFileSync(latestJsonPath, "utf-8"));
    existingSignature =
      currentLatest?.platforms?.["darwin-aarch64"]?.signature ?? "";
    existingNotes = currentLatest?.notes ?? "";
  } catch {
    // Ignore malformed existing latest.json
  }
}

let signature = existingSignature;
if (existsSync(sigPath)) {
  const builtSig = readFileSync(sigPath, "utf-8").trim();
  if (builtSig) {
    signature = builtSig;
    console.log(
      `==> Loaded fresh signature from src-tauri/target/release/bundle/macos/Velquo.app.tar.gz.sig`,
    );
  }
} else {
  console.warn(
    `==> Note: ${sigPath} not found; keeping existing signature from latest.json`,
  );
}

console.log(`==> Preparing latest.json for Velquo v${version}`);
console.log(
  `==> Paste or type release notes below (press Enter on an empty line or Ctrl+D to finish):`,
);

const rl = createInterface({
  input: process.stdin,
  output: process.stdout,
  terminal: true,
});

const lines: string[] = [];
for await (const line of rl) {
  if (line.trim() === "" && lines.length > 0) {
    break;
  }
  lines.push(line);
}
rl.close();

const notes = lines.join("\n").trim() || existingNotes || `Velquo v${version}`;
const pubDate = new Date().toISOString().replace(/\.\d{3}Z$/, "Z");
const downloadUrl = `${repoBaseUrl}/releases/download/v${version}/Velquo.app.tar.gz`;

const latestPayload = {
  version,
  notes,
  pub_date: pubDate,
  platforms: {
    "darwin-aarch64": {
      signature,
      url: downloadUrl,
    },
  },
};

writeFileSync(
  latestJsonPath,
  `${JSON.stringify(latestPayload, null, 4)}\n`,
  "utf-8",
);

console.log(`\n==> Updated latest.json successfully:`);
console.log(`    version  : ${version}`);
console.log(`    pub_date : ${pubDate}`);
console.log(`    url      : ${downloadUrl}`);
console.log(`    notes    : ${notes}`);
