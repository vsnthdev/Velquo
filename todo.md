# Velquo Bug Fixes & Hardening

Source: full codebase review (Nov 2026).  
Static checks pass (`cargo check`, `bun check`, ESLint); items below are runtime, security, logic, and state issues.

## Critical — Security

- [x] **Gate all non-pair packets on pairing** — Implemented with a lock-free `AtomicBool` fast-path plus defense-in-depth checks in share, MPRIS, and SFTP.  
      `src-tauri/src/kdeconnect/daemon.rs`, `handle_incoming_packet` (~1992–2222).  
      Unpaired TLS peers can trigger `share.request` (URL auto-open, file/text write), `ping`, `findmyphone.request`, `mpris`/`mpris.request`, and `sftp`. The TLS verifier accepts any self-signed cert (`tls.rs:39–52`, `102–111`).  
      Fix: reject every packet except `kdeconnect.pair` unless `view.pair_state == PairState::Paired`; add the same check inside `handle_share_request`, `handle_mpris_packet`, `handle_incoming_mpris_request`, and SFTP commands.

- [x] **Pin payload TLS peers to the paired certificate** — Implemented for outgoing files, incoming files, outgoing album art, and incoming album art using peer IP checks plus pinned certificate verification.
      Outgoing file/art listeners bind `0.0.0.0` and accept any TLS client: `daemon.rs:4804–4822`, `4955–4978`, `3741–3793`.  
      Incoming payload clients accept any server cert: `daemon.rs:4641–4647`, `4358–4364`; verifier at `tls.rs:39–52`.  
      Fix: after every payload handshake, compare `peer_certificates()` against `DeviceEntry.peer_cert_der`; bind outgoing listeners to the peer IP instead of `Ipv4Addr::UNSPECIFIED`.

- [x] **Enforce packet line size limits** — Implemented a 1.5 MiB limit for all control-packet reads, with oversized lines rejected before allocation.
      Unbounded `read_line` in `session/connect.rs` and `session/link_loop.rs`.
      Fix: cap TCP packet lines at 1.5 MiB and reject oversized packets before allocation.

- [x] **Cap discovery/connection flood handling** — Implemented a token bucket (burst 8, refill 2/sec), a maximum of 8 concurrent outbound connection attempts, and 60-second pruning of the per-device attempt map.
      `network/udp.rs` now admits discovery work before spawning; `last_discovery_attempt_ms` is pruned on every admission and inbound dedup check.
      Fix: prune the attempt map, rate-limit globally, and cap concurrent outbound connection tasks.

- [x] **Redact sensitive packet data from logs** — Release builds now log only packet type/IDs, sizes, ports, and boolean/count metadata; debug builds retain the full packet body for development. Sensitive targeted logs (OTP, telephony, SFTP, share, MPRIS, run commands) are redacted, and the log rotates at 5 MiB with one backup.
      Generic packet logging now uses `#[cfg(debug_assertions)]`; release builds exclude body serialization entirely.
      OTP codes, contact names, URLs, filenames, SFTP credentials, and media metadata are no longer written to release logs.
      Fix: log packet type/ID only and explicitly redact sensitive fields. Add log rotation for `~/Library/Logs/Velquo/app.log` (`logger.rs:15–25`).

- [x] **Pin SFTP SSH host key** — Implemented TOFU pinning: the first verified connection stores the `SHA256:` fingerprint in `trusted_devices.json`, later connections require an exact match, and mismatches abort before authentication.
      Removed SHA-1 key exchange, MAC, and RSA-signature fallbacks; raised DH-GEX minimum to 2048 bits.
      Fix: pin the phone’s SSH host key or implement TOFU with a visible fingerprint. Remove weak `DH_G1_SHA1` / `HMAC_SHA1` fallbacks if compatible (`sftp.rs:555–604`).

## High — Logic & State

- [x] **Fix call-media pause/resume race** — Removed the Spotify/Music AppleScript fallback so call pause/resume only uses the MediaRemote/Control Center path. The post-call resume delay is now 1 second, giving an in-flight MediaRemote pause time to finish before resume runs.
      Pause task is async (`daemon.rs:2274–2292`); if the call is canceled first, resume drains empty `paused_sources` (`2309–2311`, `2337–2350`), then the pause task inserts sources (`2281–2289`) and media stays paused.  
      Fix: keep the pause/resume path on MediaRemote only and delay the resume check to 1 second, so a rejected call cannot resume before the pause has committed.

- [x] ~~**Resume media when disabling “Pause media on calls”**~~ — **Cancelled**: intentional behavior. Disabling the toggle means the user no longer wants Velquo to control playback, so previously paused media is not auto-resumed.
      `daemon.rs:2868–2881` clears `paused_sources` and disowns without resuming.  
      Fix: resume the drained sources, or explicitly leave them owned and inform the user.

- [x] **Tear down SFTP and file browser on unpair** — Implemented a shared `sftp_window_label` helper and `teardown_sftp`, which clears the cached session and closes the `sftp-*` window from both unpair paths. `open_file_browser_window` now refuses unpaired devices, and the existing pairing guards in `sftp_browse/download/delete` remain in place.
      `unpair_or_reject` (`daemon.rs:4398–4433`) and the `pair:false` branch (`2440–2460`) do not call `sftp_manager.clear_device`; `sftp-*` windows stay open.  
      `sftp_browse/download/delete` (`lib.rs:411–481`, `daemon.rs:5121–5168`) do not check pair state.  
      Fix: clear the SFTP manager, close the matching window, and enforce pairing in SFTP commands.

- [x] **Handle pair requests from already-paired devices correctly** — Implemented a dedicated `PairState::Paired` arm: redundant `pair: true` requests are re-acknowledged, the device stays paired and reachable, no verification prompt is shown, and telemetry is refreshed.
      `handle_pair_packet` treats `PairState::Paired` like `NotPaired` and sets `RequestedByPeer` (`daemon.rs:2419–2431`), removing the device from `pairedDevices` in `App.tsx:273`.  
      Fix: separate the `Paired` arm and re-acknowledge/ignore redundant pair requests without changing state.

- [x] **Make notification IDs consistent across click, highlight, and dismiss** — `NSUserNotification.userInfo` now stores the raw notification ID, while `NSUserNotification.identifier` and internal removal matching normalize it. Click, highlight, and dismiss now all use the same raw `PhoneNotification.id`, and `notification_cancel` sends the raw ID back to the phone.

- [x] **Fix SFTP “unconfigured” response delivery** — `clear_device_and_take_waiters` now removes and returns pending handshake waiters before clearing the cached session, so the `errorMessage` path can deliver `SftpHandshakeOutcome::Unconfigured` and the dedicated UI appears. Added a regression test for waiter delivery.

- [ ] **Fix updater resource leaks**  
      `check_for_update` registers an `Update` on every call (`lib.rs:509–526`, `545–557`); `runUpdateCheck` discards `rid` (`App.tsx:59–79`); `get_pending_update` registers another on every call (`lib.rs:560–570`); `UpdateWindow` replaces updates without `close()` (`UpdateWindow.tsx:57–70`, `145–149`, `183–193`).  
      Fix: stop registering in the main webview; call `update.close()` before replacing/skip/close, or add a release command.

- [ ] **Show update-check failures as errors, not “up to date”**  
      `UpdateWindow.tsx:57–70` catches errors and sets `update = null`; render treats null as latest version (`239–263`).  
      Fix: add `loadError` state and an error/retry UI.

- [ ] **Add `NSAppleEventsUsageDescription`**  
      `Info.plist:4–24` lacks it, but `macos_media.rs` runs `osascript` for Spotify/Music/System Events (`918`, `1066`, `1326`, `1351`, `1370`, `1461`, `1498`, `1649`).  
      Fix: add the key/description and verify the Automation prompt on a clean machine.

## Medium

- [ ] **Prevent run-command data loss**  
      `add_device_run_command` / `remove_device_run_command` can write an empty `updated_list` to the trusted record if no device entry exists or the lock fails (`daemon.rs:2618–2642`, `2652–2676`).  
      Fix: only update `rec.run_commands` after successfully mutating a `DeviceEntry`.

- [ ] **Make the add-command modal one-shot**  
      `openAddCommandSignal` is never reset (`App.tsx:41`, `183–190`); `DeviceSettingsView.tsx:71–77` reopens the modal on every mount.  
      Fix: reset the signal after consuming it, or use a one-shot token.

- [ ] **Fix update error label**  
      `AppSettingsPage.tsx:194–199` renders “Up to date” when `checkStatus == "error"` (amber styling).  
      Fix: use “Check failed” / “Try again”.

- [ ] **Handle text-share write failures**  
      `daemon.rs:4529` ignores `fs::write` errors, then records `Completed` with a path at lines 4545–4560.  
      Fix: return/record `Failed` on write error; don’t set `saved_path`.

- [ ] **Surface failed transfers in the UI**  
      `DeviceControlView.tsx:802–817` filters only `InProgress` and `Completed`; `Failed` transfers disappear immediately. Dropped-file errors only go to `console.error` (`App.tsx:348–355`).  
      Fix: render failed items with their `error` text and optionally a retry action.

- [ ] **Fix album-art request retry blocking**  
      `album_art_in_flight` is inserted before requesting (`daemon.rs:4231–4240`) and only removed on payload success/failure (`4086`, `4113`). A missing response blocks retries forever.  
      Fix: timestamp entries and expire them, or remove on send failure and next metadata update.

- [ ] **Abort release when fresh updater signature is missing**  
      `update-version.ts:123–137` falls back to the previous `latest.json` signature while publishing a new archive, causing update verification failure.  
      Fix: `process.exit(1)` if the fresh `.sig` is absent.

- [ ] **Fix remote-input dialog controlled state**  
      `DeviceControlView.tsx:1020–1029` has no `onOpenChange` and ignores pair state.  
      Fix: add `onOpenChange={setIsRemoteInputOpen}` and require `pairState == "Paired"`.

- [ ] **Remove or wire up the dead in-device pairing banner**  
      `App.tsx:273`, `278–283` only select paired devices, so the banner in `DeviceControlView.tsx:458–540` is unreachable.  
      Fix: either include reachable unpaired devices in selection or remove the banner.

- [ ] **Remove dead notification-dismiss branch or implement actions**  
      `macos_notify.rs:85–100` is unreachable because `setHasActionButton(false)` is set at `240` and no additional actions exist; `_dismissable` is ignored (`183–190`).  
      Fix: add action buttons/additional actions if dismissal from the banner is intended, or remove the branch.

## Low / Cleanup

- [ ] Catch unhandled `invoke()` rejections in pairing buttons, toggles, and run-command removal (`App.tsx:484–616`, `DeviceControlView.tsx:141–201`, `DeviceSettingsView.tsx:490`).
- [ ] Guard `handleSeekClick` against `rect.width == 0` (`DeviceControlView.tsx:380–385`).
- [ ] URL-encode `device_id` in the file-browser query (`lib.rs:663`).
- [ ] Reject/ignore IPv6 in `probe_device_ip` or use a matching socket (`daemon.rs:1195–1209`).
- [ ] Remove unused helpers: `is_macos_audio_output_running` (`macos_media.rs:1585`, `1634`), `share_text` / `share_request_update` (`packet.rs:373–390`), `has_online_paired_device` / `broadcast_to_paired_devices` (`daemon.rs:3659–3678`), `velquo_mr_get` / `velquo_mr_control` (`velquo_mr_bridge.m:224–264`).
- [ ] Remove or handle advertised-but-unhandled `PACKET_TYPE_MOUSEPAD_ECHO` (`packet.rs:23`, `86`).
- [ ] Remove unread fields `CachedAlbumArt.itunes_checked` (`macos_media.rs:651`, `658`, `738`) and `observed_paused` (`175`).
- [ ] Populate or remove `DeviceView.storage_free_gb`, currently always `None` (`daemon.rs:145`, `335`, `1803`).
- [ ] **Set a real CSP** instead of `"csp": null` (`tauri.conf.json:34–36`), allowing Tauri, Google Fonts, iTunes, and GitHub as needed.
- [ ] **Remove the fragile AppKit ivar mirror** in `mark_menu_icons_as_template` (`lib.rs:824–882`) or add version/layout guards; it is undefined behavior if AppKit changes.
- [ ] **Replace deprecated `NSUserNotification`** (`macos_notify.rs:1–12`, `183–261`) with `UserNotifications` when feasible.
- [ ] Retry media bridge startup if dylib setup fails; `MR_STREAM_STARTED` is set before setup (`macos_media.rs:358–370`).
- [ ] Consider removing the project-wide “no strict equality” ESLint rule (`eslint.config.js:20–60`, `165`); it forces coercive `==` and removes a safety net.
- [x] ~~Update README/docs stale versions (`README.md:44` says `0.1.0`; `docs/releasing.md` examples use `0.3.0`).~~ — **Cancelled**

## Verification

- [ ] Reproduce unpaired-peer issue on a real LAN: connect with a self-signed client cert, send `share.request`, `ping`, and `mpris.request`; confirm they are rejected after the fix.
- [ ] Reproduce file/art MITM and unpaired payload injection; verify pinned-cert rejection.
- [ ] Test call ringing → immediate cancel; verify media resumes after the 1-second resume delay.
- [ ] Test unpair while file browser is open; verify window closes and SFTP is inaccessible.
- [ ] Test macOS notification click/highlight/dismiss with an ID containing `org.kde.kdeconnect_tp::`.
- [ ] Test SFTP “not configured” path; verify the dedicated UI appears.
- [ ] Test update check with the network disabled; verify an error state, not “latest version”.
- [ ] Run `cargo clippy -- -D warnings`, `cargo fmt --check`, `bun check`, and ESLint after fixes.
