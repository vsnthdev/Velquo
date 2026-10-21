export interface NowPlayingInfo {
  player: string;
  title: string;
  artist: string;
  album: string;
  isPlaying: boolean;
  positionMs: number;
  positionUpdatedAtMs: number;
  lengthMs: number;
  canPlay: boolean;
  canPause: boolean;
  canGoNext: boolean;
  canGoPrevious: boolean;
  canSeek: boolean;
  albumArtUrl: string | null;
  albumArtData: string | null;
}

export interface SimSignalInfo {
  slotId: string;
  networkType: string;
  signalStrength: number;
}

export interface DeviceView {
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
  batteryCharge: number | null;
  batteryCharging: boolean | null;
  cellularNetworkType: string | null;
  cellularSignalStrength: number | null;
  simSignals: SimSignalInfo[];
  storageFreeGb: number | null;
  nowPlaying: NowPlayingInfo | null;
  syncClipboard: boolean;
  saveLocation: string;
}

export interface TransferItem {
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
  error: string | null;
  timestampMs: number;
}

export interface HostInfo {
  deviceId?: string;
  deviceName?: string;
  deviceType?: string;
  protocolVersion?: number;
  tcpPort?: number;
  udpPort?: number;
  certFingerprint?: string;
  downloadsDir?: string;
  logFilePath?: string;
  localIps?: string[];
}

export interface AppSnapshot {
  host?: HostInfo;
  devices: DeviceView[];
  transfers: TransferItem[];
}

export interface PingToastData {
  deviceName: string;
  message: string;
}

export type ActiveView =
  | { kind: "device"; deviceId: string; subview: "control" | "settings" }
  | { kind: "app-settings" }
  | { kind: "pair-device" };
