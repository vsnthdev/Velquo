#!/usr/bin/env bun
import prompts from "prompts";
import readFileInput from "read-file-input";

const rootUrl = new URL("../", import.meta.url);
const pkgFile = Bun.file(new URL("package.json", rootUrl));
const cargoTomlFile = Bun.file(new URL("src-tauri/Cargo.toml", rootUrl));
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
  [key: string]: unknown;
};
const currentVersion = pkg.version.trim().replace(/^v/, "");
const [rawMajor = 1, rawMinor = 0, rawPatch = 0] = currentVersion
  .split(".")
  .map((part) => Number.parseInt(part, 10));

const major = Number.isNaN(rawMajor) ? 1 : rawMajor;
const minor = Number.isNaN(rawMinor) ? 0 : rawMinor;
const patch = Number.isNaN(rawPatch) ? 0 : rawPatch;

const majorNext = `${major + 1}.0.0`;
const minorNext = `${major}.${minor + 1}.0`;
const patchNext = `${major}.${minor}.${patch + 1}`;

const { nextVersion } = await prompts(
  {
    type: "select",
    name: "nextVersion",
    message: `Select version bump from v${currentVersion}:`,
    choices: [
      { title: `Major (${majorNext})`, value: majorNext },
      { title: `Minor (${minorNext})`, value: minorNext },
      { title: `Patch (${patchNext})`, value: patchNext },
    ],
    initial: 2,
  },
  {
    onCancel: () => {
      console.error("==> Aborted: Version selection cancelled.");
      process.exit(1);
    },
  },
);

if (!nextVersion || typeof nextVersion !== "string") {
  console.error("==> Aborted: No version selected.");
  process.exit(1);
}

console.log(`==> Opening editor to enter release notes for v${nextVersion}...`);
const rawNotes = await readFileInput();
const notes = rawNotes.trim();

if (!notes) {
  console.error("==> Aborted: Release notes cannot be empty.");
  process.exit(1);
}

const repoBaseUrl = (
  pkg.homepage || "https://github.com/vsnthdev/Velquo"
).replace(/\/+$/, "");

// 1. Update package.json
pkg.version = nextVersion;
await Bun.write(pkgFile, `${JSON.stringify(pkg, null, 2)}\n`);

// 2. Update src-tauri/Cargo.toml [package] version
const cargoTomlText = await cargoTomlFile.text();
const updatedCargoToml = cargoTomlText.replace(
  /(\[package\][\s\S]*?\nversion\s*=\s*")[^"]+(")/,
  `$1${nextVersion}$2`,
);
await Bun.write(cargoTomlFile, updatedCargoToml);

// 3. Build and sign the release bundle so Velquo.app.tar.gz.sig is generated for v${nextVersion}
const keyFile = Bun.file(`${Bun.env.HOME}/.tauri/velquo.key`);
if (!(await keyFile.exists())) {
  console.error("==> Error: Signing key not found at ~/.tauri/velquo.key");
  process.exit(1);
}
const signingPrivateKey = (await keyFile.text()).trim();
Bun.env.TAURI_SIGNING_PRIVATE_KEY = signingPrivateKey;
Bun.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "";

console.log(`\n==> Building and signing release bundle for v${nextVersion}...`);
const bundleProc = Bun.spawn(["bun", "run", "bundle"], {
  cwd: new URL(".", rootUrl).pathname,
  stdio: ["inherit", "inherit", "inherit"],
});

const exitCode = await bundleProc.exited;
if (exitCode !== 0) {
  console.error(`==> Error: bun run bundle failed with exit code ${exitCode}`);
  process.exit(exitCode);
}

// 4. Read the freshly generated v${nextVersion} signature and write latest.json
let signature = "";
if (await sigFile.exists()) {
  signature = (await sigFile.text()).trim();
  console.log(
    "==> Loaded fresh signature from src-tauri/target/release/bundle/macos/Velquo.app.tar.gz.sig",
  );
} else if (await latestJsonFile.exists()) {
  try {
    const currentLatest = await latestJsonFile.json();
    signature = currentLatest?.platforms?.["darwin-aarch64"]?.signature ?? "";
  } catch {
    // Ignore malformed existing latest.json
  }
}

const pubDate = new Date().toISOString().replace(/\.\d{3}Z$/, "Z");
const downloadUrl = `${repoBaseUrl}/releases/download/v${nextVersion}/Velquo.app.tar.gz`;

const latestPayload = {
  version: nextVersion,
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

console.log(
  `\n==> Bumped version to v${nextVersion}, built signed bundle, and updated latest.json:`,
);
console.log(`    version  : ${nextVersion}`);
console.log(`    pub_date : ${pubDate}`);
console.log(`    url      : ${downloadUrl}`);
console.log(`    notes    : ${notes}`);
