# Releasing a New Version of Velquo

This guide walks through the complete end-to-end process for bumping the version of **Velquo**, building signed macOS release & auto-updater artifacts, generating `latest.json`, and publishing a GitHub Release so existing users receive automatic in-app updates.

---

## Step 1: Bump the Version Number (2 Files Only)

Velquo defines its version in **only two files** (both `src-tauri/tauri.conf.json` and the React **About** section read from these automatically):

1. **`package.json`**
   ```json
   {
     "name": "velquo",
     "version": "0.3.0"
   }
   ```
2. **`src-tauri/Cargo.toml`**
   ```toml
   [package]
   name = "velquo"
   version = "0.3.0"
   ```

_(Do not add a `"version"` field to `src-tauri/tauri.conf.json` — Tauri automatically inherits the version from `src-tauri/Cargo.toml` / `package.json`)._

---

## Step 2: Build the Signed macOS Release & Updater Artifacts

Tauri's built-in updater (`tauri-plugin-updater`) requires `TAURI_SIGNING_PRIVATE_KEY` to be set in your shell environment when running `bun run bundle` so it can sign the `.app.tar.gz` update archive with your private key (`~/.tauri/velquo.key`).

Run the following command from the repository root:

```bash
export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/velquo.key)"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""

bun run bundle
```

### What `bun run bundle` Generates

Inside `src-tauri/target/release/bundle/`, Tauri produces:

1. **DMG Installer (for new users downloading from GitHub Releases):**
   - `src-tauri/target/release/bundle/dmg/Velquo_<VERSION>_aarch64.dmg`
2. **macOS App Bundle & Signed Updater Archive (for in-app auto-updates):**
   - `src-tauri/target/release/bundle/macos/Velquo.app`
   - `src-tauri/target/release/bundle/macos/Velquo.app.tar.gz` _(downloaded by the auto-updater)_
   - `src-tauri/target/release/bundle/macos/Velquo.app.tar.gz.sig` _(cryptographic Minisign signature)_

---

## Step 3: Create `latest.json` for the Auto-Updater

Velquo checks `https://github.com/vsnthdev/Velquo/releases/latest/download/latest.json` on launch, every 8 hours in the background, and whenever a user clicks **Check now** in **Settings → Check for updates**.

Create `latest.json` using the signature generated in `src-tauri/target/release/bundle/macos/Velquo.app.tar.gz.sig`:

```bash
VERSION="0.3.0"
SIG="$(cat src-tauri/target/release/bundle/macos/Velquo.app.tar.gz.sig)"
PUB_DATE="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

cat <<EOF > latest.json
{
  "version": "${VERSION}",
  "notes": "Bug fixes and performance improvements.",
  "pub_date": "${PUB_DATE}",
  "platforms": {
    "darwin-aarch64": {
      "signature": "${SIG}",
      "url": "https://github.com/vsnthdev/Velquo/releases/download/v${VERSION}/Velquo.app.tar.gz"
    }
  }
}
EOF
```

### Important Fields in `latest.json`

- **`"version"`:** Must match the new version in `package.json` and `src-tauri/Cargo.toml` (e.g., `"0.3.0"`).
- **`"notes"`:** Shown inside Velquo's **Update available** dialog box before the user clicks **Update now**.
- **`"platforms.darwin-aarch64.signature"`:** Must be the **exact string content** of `Velquo.app.tar.gz.sig` (not a file path or URL).
- **`"platforms.darwin-aarch64.url"`:** Must point to the `Velquo.app.tar.gz` asset uploaded to that GitHub Release tag (`v0.3.0`).

---

## Step 4: Commit, Tag, and Publish the GitHub Release

1. **Commit the version bump and push the tag:**

   ```bash
   git add package.json src-tauri/Cargo.toml src-tauri/Cargo.lock
   git commit -m "chore: release v0.3.0"
   git tag v0.3.0
   git push origin main --tags
   ```

2. **Create the GitHub Release with all 3 required files:**
   Upload the `.dmg`, `Velquo.app.tar.gz`, and `latest.json` to the `v0.3.0` release (either via the GitHub web UI or using the GitHub CLI `gh`):

   ```bash
   gh release create v0.3.0 \
     "src-tauri/target/release/bundle/dmg/Velquo_0.3.0_aarch64.dmg" \
     "src-tauri/target/release/bundle/macos/Velquo.app.tar.gz" \
     "latest.json" \
     --title "Velquo v0.3.0" \
     --notes "Bug fixes and performance improvements."
   ```

3. **Clean up the local `latest.json` file (optional):**
   ```bash
   rm -f latest.json
   ```

---

## Release Checklist Summary

| #   | Item                                                                                                         | Path / Location                       |
| :-- | :----------------------------------------------------------------------------------------------------------- | :------------------------------------ |
| 1   | Bump `"version"` in `package.json`                                                                           | `package.json`                        |
| 2   | Bump `version` in `Cargo.toml`                                                                               | `src-tauri/Cargo.toml`                |
| 3   | Export `TAURI_SIGNING_PRIVATE_KEY` & run `bun run bundle`                                                    | `~/.tauri/velquo.key`                 |
| 4   | Generate `latest.json` with `Velquo.app.tar.gz.sig` content                                                  | `./latest.json`                       |
| 5   | Upload `Velquo_<VERSION>_aarch64.dmg`, `Velquo.app.tar.gz`, and `latest.json` to GitHub Release `v<VERSION>` | `github.com/vsnthdev/Velquo/releases` |
