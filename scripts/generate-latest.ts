#!/usr/bin/env bun
import readFileInput from "read-file-input";

const rootUrl = new URL("../", import.meta.url);
const pkgFile = Bun.file(new URL("package.json", rootUrl));
const latestJsonFile = Bun.file(new URL("latest.json", rootUrl));
const sigFile = Bun.file(
  new URL(
    "src-tauri/target/release/bundle/macos/Velquo.app.tar.gz.sig",
    rootUrl,
  ),
);

const pkg = (await pkgFile.json()) as {
  version: string;
  homepage?: string;
};
const version = pkg.version.trim().replace(/^v/, "");
const repoBaseUrl = (
  pkg.homepage || "https://github.com/vsnthdev/Velquo"
).replace(/\/+$/, "");

let existingSignature = "";
if (await latestJsonFile.exists()) {
  try {
    const currentLatest = await latestJsonFile.json();
    existingSignature =
      currentLatest?.platforms?.["darwin-aarch64"]?.signature ?? "";
  } catch {
    // Ignore malformed existing latest.json
  }
}

let signature = existingSignature;
if (await sigFile.exists()) {
  const builtSig = (await sigFile.text()).trim();
  if (builtSig) {
    signature = builtSig;
    console.log(
      "==> Loaded fresh signature from src-tauri/target/release/bundle/macos/Velquo.app.tar.gz.sig",
    );
  }
} else {
  console.warn(
    "==> Note: Velquo.app.tar.gz.sig not found; keeping existing signature from latest.json",
  );
}

console.log(`==> Preparing latest.json for Velquo v${version}`);
console.log("==> Opening editor to enter release notes...");

const rawNotes = await readFileInput();
const notes = rawNotes.trim();

if (!notes) {
  console.error("==> Aborted: Release notes cannot be empty.");
  process.exit(1);
}

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

await Bun.write(latestJsonFile, `${JSON.stringify(latestPayload, null, 4)}\n`);

console.log("\n==> Updated latest.json successfully:");
console.log(`    version  : ${version}`);
console.log(`    pub_date : ${pubDate}`);
console.log(`    url      : ${downloadUrl}`);
console.log(`    notes    : ${notes}`);
