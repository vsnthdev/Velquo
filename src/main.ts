import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir, openUrl } from "@tauri-apps/plugin-opener";

interface HostInfo {
  deviceId: string;
  deviceName: string;
  deviceType: string;
  protocolVersion: number;
  tcpPort: number;
  udpPort: number;
  certFingerprint: string;
  downloadsDir: string;
  logFilePath: string;
  localIps: string[];
}

interface DeviceView {
  deviceId: string;
  deviceName: string;
  deviceType: string;
  protocolVersion: number;
  address: string;
  tcpPort: number;
  reachable: boolean;
  pairState: "NotPaired" | "Requested" | "RequestedByPeer" | "Paired";
  verificationKey: string | null;
  certFingerprint: string | null;
  incomingCapabilities: string[];
  outgoingCapabilities: string[];
  lastSeenSecs: number;
}

interface TransferItem {
  id: string;
  deviceId: string;
  deviceName: string;
  direction: "Incoming" | "Outgoing";
  kind: "File" | "Text" | "Url";
  filename: string;
  totalBytes: number;
  transferredBytes: number;
  status: "InProgress" | "Completed" | "Failed";
  savedPath: string | null;
  textContent: string | null;
  urlContent: string | null;
  error: string | null;
  timestampMs: number;
}

interface LogEntry {
  timestamp: string;
  level: string;
  target: string;
  message: string;
}

interface AppSnapshot {
  host: HostInfo;
  devices: DeviceView[];
  transfers: TransferItem[];
  logs: LogEntry[];
}

let state: AppSnapshot = {
  host: {
    deviceId: "...",
    deviceName: "Mac",
    deviceType: "laptop",
    protocolVersion: 8,
    tcpPort: 1716,
    udpPort: 1716,
    certFingerprint: "",
    downloadsDir: "",
    logFilePath: "",
    localIps: [],
  },
  devices: [],
  transfers: [],
  logs: [],
};

let manualIpInput = "";
let isDraggingOver = false;
let selectedDropDeviceId: string | null = null;
let autoScrollLogs = true;

async function refreshState() {
  try {
    const snap = await invoke<AppSnapshot>("get_app_snapshot");
    state = snap;
    ensureSelectedDevice();
    render();
  } catch (err) {
    console.error("Failed to fetch snapshot:", err);
  }
}

function ensureSelectedDevice() {
  const paired = state.devices.filter((d) => d.reachable && d.pairState === "Paired");
  if (paired.length > 0) {
    if (!selectedDropDeviceId || !paired.some((d) => d.deviceId === selectedDropDeviceId)) {
      selectedDropDeviceId = paired[0].deviceId;
    }
  } else {
    selectedDropDeviceId = null;
  }
}

function formatBytes(bytes: number): string {
  if (bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  const idx = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / Math.pow(1024, idx)).toFixed(idx === 0 ? 0 : 1)} ${units[idx]}`;
}

function escapeHtml(str: string): string {
  return str
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function render() {
  const app = document.getElementById("app");
  if (!app) return;

  const logBodyEl = document.getElementById("log-body");
  const prevScrollTop = logBodyEl ? logBodyEl.scrollTop : 0;

  const pairedReachable = state.devices.filter((d) => d.reachable && d.pairState === "Paired");

  app.innerHTML = `
    <div id="titlebar-header" class="header" data-tauri-drag-region>
      <div class="header-left" data-tauri-drag-region>
        <div class="status-dot" title="Velquo daemon running on UDP ${state.host.udpPort} / TCP ${state.host.tcpPort}"></div>
        <div data-tauri-drag-region>
          <div class="app-title" data-tauri-drag-region>${escapeHtml(state.host.deviceName)} <span style="font-weight:400;font-size:11px;color:var(--text-secondary)">(v${state.host.protocolVersion})</span></div>
          <div class="host-meta" data-tauri-drag-region>IPs: ${state.host.localIps.join(", ") || "Detecting..."} • TCP :${state.host.tcpPort}</div>
        </div>
      </div>
      <div class="header-actions">
        <button id="btn-broadcast" class="btn btn-primary btn-sm" title="Send UDP Broadcast + mDNS announcement">
          📡 Broadcast
        </button>
      </div>
    </div>

    <div class="main-content">
      <div class="card">
        <div class="section-title">
          <span>Direct Discovery (Optional IP)</span>
          <span style="font-size:10px;text-transform:none;color:var(--text-muted)">ID: ${state.host.deviceId.slice(0, 10)}...</span>
        </div>
        <div class="connect-bar">
          <input id="input-ip" class="input" type="text" placeholder="Phone IP (e.g. 192.168.1.50)" value="${escapeHtml(manualIpInput)}" />
          <button id="btn-connect-ip" class="btn btn-sm">Probe IP</button>
        </div>
      </div>

      <div>
        <div class="section-title">
          <span>Devices (${state.devices.length})</span>
          <span style="font-size:10px;text-transform:none;">Listening UDP :1716 & mDNS</span>
        </div>
        ${
          state.devices.length === 0
            ? `<div class="card empty-state">
                No KDE Connect devices discovered yet.<br/>
                Open the <b>KDE Connect</b> app on your phone (on the same Wi-Fi) and click <b>Broadcast</b> or enter its IP above.
              </div>`
            : `<div class="device-list">
                ${state.devices
                  .map((dev) => {
                    const badgeClass =
                      dev.pairState === "Paired"
                        ? "badge-paired"
                        : dev.pairState === "NotPaired"
                        ? "badge-unpaired"
                        : "badge-pending";
                    return `
                      <div class="device-card ${dev.pairState === "Paired" ? "paired" : ""}">
                        <div class="device-top">
                          <div class="device-name">
                            <span>${dev.deviceType === "phone" || dev.deviceType === "smartphone" ? "📱" : "💻"}</span>
                            <span>${escapeHtml(dev.deviceName)}</span>
                            <span class="badge ${badgeClass}">${dev.pairState}</span>
                            ${!dev.reachable ? `<span class="badge badge-unpaired">Offline</span>` : ""}
                          </div>
                          <div style="font-size:10.5px;color:var(--text-secondary)">
                            v${dev.protocolVersion} • ${escapeHtml(dev.address)}
                          </div>
                        </div>

                        <div class="device-details">
                          ID: <code>${escapeHtml(dev.deviceId)}</code>
                          ${dev.certFingerprint ? `<br/>Cert SHA256: <code>${escapeHtml(dev.certFingerprint.slice(0, 29))}...</code>` : ""}
                        </div>

                        ${
                          dev.verificationKey && (dev.pairState === "Requested" || dev.pairState === "RequestedByPeer")
                            ? `<div class="verification-box">
                                <span>Verification Key:</span>
                                <span class="verification-key">${escapeHtml(dev.verificationKey)}</span>
                              </div>`
                            : ""
                        }

                        <div class="device-actions">
                          ${
                            dev.pairState === "NotPaired" && dev.reachable
                              ? `<button class="btn btn-primary btn-sm" data-action="pair" data-id="${escapeHtml(dev.deviceId)}">🔗 Pair Device</button>`
                              : ""
                          }
                          ${
                            dev.pairState === "Requested"
                              ? `<button class="btn btn-danger btn-sm" data-action="reject-pair" data-id="${escapeHtml(dev.deviceId)}">Cancel Pairing</button>`
                              : ""
                          }
                          ${
                            dev.pairState === "RequestedByPeer"
                              ? `
                                <button class="btn btn-success btn-sm" data-action="accept-pair" data-id="${escapeHtml(dev.deviceId)}">✓ Accept Pairing</button>
                                <button class="btn btn-danger btn-sm" data-action="reject-pair" data-id="${escapeHtml(dev.deviceId)}">✕ Reject</button>
                              `
                              : ""
                          }
                          ${
                            dev.pairState === "Paired" && dev.reachable
                              ? `
                                <button class="btn btn-primary btn-sm" data-action="send-file" data-id="${escapeHtml(dev.deviceId)}">📤 Send Files...</button>
                                <button class="btn btn-sm" data-action="send-ping" data-id="${escapeHtml(dev.deviceId)}">🔔 Ping</button>
                              `
                              : ""
                          }
                          ${
                            dev.pairState === "Paired"
                              ? `<button class="btn btn-danger btn-sm" data-action="unpair" data-id="${escapeHtml(dev.deviceId)}">Unpair</button>`
                              : ""
                          }
                        </div>
                      </div>
                    `;
                  })
                  .join("")}
              </div>`
        }
      </div>

      ${
        pairedReachable.length > 0
          ? `<div class="dropzone ${isDraggingOver ? "dragging" : ""}">
              <div><b>Drag & Drop files anywhere in this window</b> to send to <b>${escapeHtml(
                pairedReachable.find((d) => d.deviceId === selectedDropDeviceId)?.deviceName || pairedReachable[0].deviceName
              )}</b></div>
            </div>`
          : ""
      }

      <div>
        <div class="section-title">
          <span>Transfers (${state.transfers.length})</span>
          <button id="btn-open-downloads" class="btn btn-sm" style="padding:1px 6px;font-size:10px;">📂 Downloads</button>
        </div>
        ${
          state.transfers.length === 0
            ? `<div class="card empty-state" style="padding:10px;">No file transfers yet. Share a file from your phone or click "Send Files...".</div>`
            : `<div class="device-list">
                ${state.transfers
                  .slice(0, 8)
                  .map((t) => {
                    const pct =
                      t.totalBytes > 0
                        ? Math.min(100, Math.round((t.transferredBytes / t.totalBytes) * 100))
                        : t.status === "Completed"
                        ? 100
                        : 50;
                    const fillClass =
                      t.status === "Completed" ? "completed" : t.status === "Failed" ? "failed" : "";
                    return `
                      <div class="transfer-item">
                        <div class="transfer-header">
                          <span>${t.direction === "Incoming" ? "⬇️" : "⬆️"} ${escapeHtml(t.filename)} <span style="color:var(--text-secondary);font-weight:400;">(${escapeHtml(t.deviceName)})</span></span>
                          <span>${
                            t.kind === "File"
                              ? `${formatBytes(t.transferredBytes)} / ${formatBytes(t.totalBytes)} (${pct}%)`
                              : t.kind
                          }</span>
                        </div>
                        ${
                          t.kind === "File"
                            ? `<div class="progress-bar-bg"><div class="progress-bar-fill ${fillClass}" style="width:${pct}%"></div></div>`
                            : ""
                        }
                        <div style="display:flex;justify-content:space-between;align-items:center;font-size:10.5px;color:var(--text-secondary);">
                          <span>${escapeHtml(t.status)}${t.error ? `: ${escapeHtml(t.error)}` : ""}</span>
                          ${
                            t.savedPath
                              ? `<button class="btn btn-sm" data-action="reveal-file" data-path="${escapeHtml(t.savedPath)}" style="padding:1px 6px;font-size:10px;">Show in Finder</button>`
                              : ""
                          }
                          ${
                            t.urlContent
                              ? `<button class="btn btn-sm" data-action="open-url" data-url="${escapeHtml(t.urlContent)}" style="padding:1px 6px;font-size:10px;">Open URL</button>`
                              : ""
                          }
                        </div>
                      </div>
                    `;
                  })
                  .join("")}
              </div>`
        }
      </div>
    </div>

    <div class="log-panel">
      <div class="log-header">
        <span><b>Live Protocol & Network Logs</b> (${state.logs.length})</span>
        <div style="display:flex;gap:4px;">
          <button id="btn-reveal-log" class="btn btn-sm" style="padding:1px 6px;font-size:10px;" title="${escapeHtml(state.host.logFilePath)}">📄 Log File</button>
          <button id="btn-clear-logs" class="btn btn-sm" style="padding:1px 6px;font-size:10px;">Clear</button>
        </div>
      </div>
      <div id="log-body" class="log-body">
        ${state.logs
          .map(
            (l) =>
              `<div class="log-line log-${escapeHtml(l.level)}"><span style="color:var(--text-muted)">[${escapeHtml(
                l.timestamp
              )}]</span> <b>${escapeHtml(l.level)}</b> <span style="color:var(--text-secondary)">[${escapeHtml(
                l.target
              )}]</span> ${escapeHtml(l.message)}</div>`
          )
          .join("")}
      </div>
    </div>

    <div class="footer-bar">
      <span>Host Cert: <code>${escapeHtml(state.host.certFingerprint.slice(0, 17))}...</code></span>
      <button id="btn-quit" class="btn btn-danger btn-sm" style="padding:2px 8px;font-size:10.5px;">Quit App</button>
    </div>
  `;

  bindEvents();

  const newLogBody = document.getElementById("log-body");
  if (newLogBody) {
    if (autoScrollLogs) {
      newLogBody.scrollTop = newLogBody.scrollHeight;
    } else {
      newLogBody.scrollTop = prevScrollTop;
    }
    newLogBody.addEventListener("scroll", () => {
      const nearBottom =
        newLogBody.scrollHeight - newLogBody.scrollTop - newLogBody.clientHeight < 25;
      autoScrollLogs = nearBottom;
    });
  }
}

function bindEvents() {
  document.getElementById("titlebar-header")?.addEventListener("mousedown", (e) => {
    const target = e.target as HTMLElement;
    if (target.closest("button") || target.closest("input")) {
      return;
    }
    if (e.buttons === 1) {
      getCurrentWindow().startDragging();
    }
  });

  document.getElementById("btn-broadcast")?.addEventListener("click", async () => {
    await invoke("broadcast_discovery");
  });

  const ipInput = document.getElementById("input-ip") as HTMLInputElement | null;
  ipInput?.addEventListener("input", (e) => {
    manualIpInput = (e.target as HTMLInputElement).value;
  });
  ipInput?.addEventListener("keydown", async (e) => {
    if (e.key === "Enter" && manualIpInput.trim()) {
      await invoke("probe_device_ip", { ip: manualIpInput.trim() });
    }
  });

  document.getElementById("btn-connect-ip")?.addEventListener("click", async () => {
    if (manualIpInput.trim()) {
      await invoke("probe_device_ip", { ip: manualIpInput.trim() });
    }
  });

  document.getElementById("btn-open-downloads")?.addEventListener("click", async () => {
    if (state.host.downloadsDir) {
      await revealItemInDir(state.host.downloadsDir);
    }
  });

  document.getElementById("btn-reveal-log")?.addEventListener("click", async () => {
    if (state.host.logFilePath) {
      await revealItemInDir(state.host.logFilePath);
    }
  });

  document.getElementById("btn-clear-logs")?.addEventListener("click", async () => {
    await invoke("clear_logs");
    state.logs = [];
    render();
  });

  document.getElementById("btn-quit")?.addEventListener("click", async () => {
    await invoke("quit_app");
  });

  document.querySelectorAll("[data-action]").forEach((el) => {
    el.addEventListener("click", async (e) => {
      const target = e.currentTarget as HTMLElement;
      const action = target.getAttribute("data-action");
      const deviceId = target.getAttribute("data-id");

      if (action === "pair" && deviceId) {
        await invoke("request_pairing", { deviceId });
      } else if (action === "accept-pair" && deviceId) {
        await invoke("accept_pairing", { deviceId });
      } else if (action === "reject-pair" && deviceId) {
        await invoke("reject_pairing", { deviceId });
      } else if (action === "unpair" && deviceId) {
        await invoke("unpair_device", { deviceId });
      } else if (action === "send-ping" && deviceId) {
        await invoke("send_ping", { deviceId, message: "Hello from Velquo (macOS Apple Silicon)!" });
      } else if (action === "send-file" && deviceId) {
        const selected = await open({
          multiple: true,
          directory: false,
          title: "Select file(s) to share via Velquo",
        });
        if (selected) {
          const paths = Array.isArray(selected) ? selected : [selected];
          for (const filePath of paths) {
            await invoke("send_file", { deviceId, filePath });
          }
        }
      } else if (action === "reveal-file") {
        const path = target.getAttribute("data-path");
        if (path) {
          await revealItemInDir(path);
        }
      } else if (action === "open-url") {
        const url = target.getAttribute("data-url");
        if (url) {
          await openUrl(url);
        }
      }
    });
  });
}

async function init() {
  await refreshState();

  await listen("velquo://state-updated", () => {
    refreshState();
  });

  await listen<LogEntry>("velquo://log", (event) => {
    state.logs.push(event.payload);
    if (state.logs.length > 300) {
      state.logs.shift();
    }
    const logBody = document.getElementById("log-body");
    if (logBody) {
      const div = document.createElement("div");
      div.className = `log-line log-${event.payload.level}`;
      div.innerHTML = `<span style="color:var(--text-muted)">[${escapeHtml(
        event.payload.timestamp
      )}]</span> <b>${escapeHtml(event.payload.level)}</b> <span style="color:var(--text-secondary)">[${escapeHtml(
        event.payload.target
      )}]</span> ${escapeHtml(event.payload.message)}`;
      logBody.appendChild(div);
      if (autoScrollLogs) {
        logBody.scrollTop = logBody.scrollHeight;
      }
    }
  });

  // Native file drag-and-drop onto Tauri window
  const webview = getCurrentWebview();
  await webview.onDragDropEvent(async (event) => {
    if (event.payload.type === "over" || event.payload.type === "enter") {
      if (!isDraggingOver) {
        isDraggingOver = true;
        render();
      }
    } else if (event.payload.type === "leave") {
      if (isDraggingOver) {
        isDraggingOver = false;
        render();
      }
    } else if (event.payload.type === "drop") {
      isDraggingOver = false;
      render();
      ensureSelectedDevice();
      if (selectedDropDeviceId && event.payload.paths.length > 0) {
        for (const filePath of event.payload.paths) {
          await invoke("send_file", {
            deviceId: selectedDropDeviceId,
            filePath,
          });
        }
      }
    }
  });
}

init();
