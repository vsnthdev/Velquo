use crate::kdeconnect::cert::HostIdentity;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::time::{SystemTime, UNIX_EPOCH};

pub const PROTOCOL_VERSION: i32 = 8;

pub const PACKET_TYPE_IDENTITY: &str = "kdeconnect.identity";
pub const PACKET_TYPE_PAIR: &str = "kdeconnect.pair";
pub const PACKET_TYPE_PING: &str = "kdeconnect.ping";
pub const PACKET_TYPE_BATTERY: &str = "kdeconnect.battery";
pub const PACKET_TYPE_BATTERY_REQUEST: &str = "kdeconnect.battery.request";
pub const PACKET_TYPE_CONNECTIVITY_REPORT: &str = "kdeconnect.connectivity_report";
pub const PACKET_TYPE_CONNECTIVITY_REPORT_REQUEST: &str = "kdeconnect.connectivity_report.request";
pub const PACKET_TYPE_MPRIS: &str = "kdeconnect.mpris";
pub const PACKET_TYPE_MPRIS_REQUEST: &str = "kdeconnect.mpris.request";
pub const PACKET_TYPE_TELEPHONY: &str = "kdeconnect.telephony";
pub const PACKET_TYPE_CLIPBOARD: &str = "kdeconnect.clipboard";
pub const PACKET_TYPE_CLIPBOARD_CONNECT: &str = "kdeconnect.clipboard.connect";
pub const PACKET_TYPE_PRESENTER: &str = "kdeconnect.presenter";
pub const PACKET_TYPE_MOUSEPAD_REQUEST: &str = "kdeconnect.mousepad.request";
pub const PACKET_TYPE_MOUSEPAD_KEYBOARDSTATE: &str = "kdeconnect.mousepad.keyboardstate";
pub const PACKET_TYPE_FINDMYPHONE_REQUEST: &str = "kdeconnect.findmyphone.request";
pub const PACKET_TYPE_NOTIFICATION: &str = "kdeconnect.notification";
pub const PACKET_TYPE_NOTIFICATION_REQUEST: &str = "kdeconnect.notification.request";
pub const PACKET_TYPE_RUNCOMMAND: &str = "kdeconnect.runcommand";
pub const PACKET_TYPE_RUNCOMMAND_OUTPUT: &str = "kdeconnect.runcommand.output";
pub const PACKET_TYPE_RUNCOMMAND_REQUEST: &str = "kdeconnect.runcommand.request";
pub const PACKET_TYPE_SHARE_REQUEST: &str = "kdeconnect.share.request";
pub const PACKET_TYPE_SHARE_REQUEST_UPDATE: &str = "kdeconnect.share.request.update";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPacket {
    #[serde(default = "current_millis_value")]
    pub id: Value,
    #[serde(rename = "type")]
    pub packet_type: String,
    #[serde(default)]
    pub body: Map<String, Value>,
    #[serde(rename = "payloadSize", skip_serializing_if = "Option::is_none")]
    pub payload_size: Option<i64>,
    #[serde(rename = "payloadTransferInfo", skip_serializing_if = "Option::is_none")]
    pub payload_transfer_info: Option<Map<String, Value>>,
}

fn current_millis_value() -> Value {
    Value::Number(serde_json::Number::from(current_millis()))
}

pub fn current_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn current_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn supported_incoming_capabilities() -> Vec<String> {
    vec![
        PACKET_TYPE_SHARE_REQUEST.to_string(),
        PACKET_TYPE_SHARE_REQUEST_UPDATE.to_string(),
        PACKET_TYPE_PING.to_string(),
        PACKET_TYPE_BATTERY.to_string(),
        PACKET_TYPE_BATTERY_REQUEST.to_string(),
        PACKET_TYPE_CONNECTIVITY_REPORT.to_string(),
        PACKET_TYPE_MPRIS.to_string(),
        PACKET_TYPE_MPRIS_REQUEST.to_string(),
        PACKET_TYPE_TELEPHONY.to_string(),
        PACKET_TYPE_CLIPBOARD.to_string(),
        PACKET_TYPE_CLIPBOARD_CONNECT.to_string(),
        PACKET_TYPE_PRESENTER.to_string(),
        PACKET_TYPE_MOUSEPAD_REQUEST.to_string(),
        PACKET_TYPE_FINDMYPHONE_REQUEST.to_string(),
        PACKET_TYPE_NOTIFICATION.to_string(),
        PACKET_TYPE_RUNCOMMAND_REQUEST.to_string(),
    ]
}

pub fn supported_outgoing_capabilities() -> Vec<String> {
    vec![
        PACKET_TYPE_SHARE_REQUEST.to_string(),
        PACKET_TYPE_SHARE_REQUEST_UPDATE.to_string(),
        PACKET_TYPE_PING.to_string(),
        PACKET_TYPE_BATTERY.to_string(),
        PACKET_TYPE_BATTERY_REQUEST.to_string(),
        PACKET_TYPE_CONNECTIVITY_REPORT.to_string(),
        PACKET_TYPE_CONNECTIVITY_REPORT_REQUEST.to_string(),
        PACKET_TYPE_MPRIS.to_string(),
        PACKET_TYPE_MPRIS_REQUEST.to_string(),
        PACKET_TYPE_CLIPBOARD.to_string(),
        PACKET_TYPE_CLIPBOARD_CONNECT.to_string(),
        PACKET_TYPE_MOUSEPAD_KEYBOARDSTATE.to_string(),
        PACKET_TYPE_FINDMYPHONE_REQUEST.to_string(),
        PACKET_TYPE_NOTIFICATION_REQUEST.to_string(),
        PACKET_TYPE_RUNCOMMAND.to_string(),
        PACKET_TYPE_RUNCOMMAND_OUTPUT.to_string(),
    ]
}

impl NetworkPacket {
    pub fn new(packet_type: &str, body: Map<String, Value>) -> Self {
        Self {
            id: Value::Number(serde_json::Number::from(current_millis())),
            packet_type: packet_type.to_string(),
            body,
            payload_size: None,
            payload_transfer_info: None,
        }
    }

    pub fn serialize_line(&self) -> Result<Vec<u8>> {
        let mut bytes = serde_json::to_vec(self).context("Failed to serialize NetworkPacket")?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    pub fn parse_line(raw: &[u8]) -> Result<Self> {
        let trimmed = raw
            .strip_suffix(b"\r\n")
            .or_else(|| raw.strip_suffix(b"\n"))
            .unwrap_or(raw);
        let packet: Self =
            serde_json::from_slice(trimmed).context("Failed to deserialize NetworkPacket JSON")?;
        Ok(packet)
    }

    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.body.get(key).and_then(|v| v.as_str())
    }

    pub fn get_i64(&self, key: &str) -> Option<i64> {
        self.body.get(key).and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_u64().map(|u| u as i64))
                .or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()))
        })
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.body.get(key).and_then(|v| {
            v.as_bool()
                .or_else(|| v.as_i64().map(|n| n != 0))
                .or_else(|| {
                    v.as_str().and_then(|s| match s.trim().to_ascii_lowercase().as_str() {
                        "true" | "1" | "yes" => Some(true),
                        "false" | "0" | "no" => Some(false),
                        _ => None,
                    })
                })
        })
    }

    pub fn get_string_list(&self, key: &str) -> Vec<String> {
        self.body
            .get(key)
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| item.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn payload_port(&self) -> Option<u16> {
        self.payload_transfer_info
            .as_ref()?
            .get("port")
            .and_then(|v| v.as_u64())
            .and_then(|p| u16::try_from(p).ok())
    }

    /// UDP Discovery Broadcast Packet (`DeviceInfo::toUdpDiscoveryPacket`)
    /// Includes full capabilities as well so both v7 and v8 peers can inspect it.
    pub fn udp_discovery(host: &HostIdentity, tcp_port: u16) -> Self {
        let mut body = Map::new();
        body.insert("deviceId".into(), json!(host.device_id));
        body.insert("deviceName".into(), json!(host.device_name));
        body.insert("deviceType".into(), json!(host.device_type));
        body.insert("protocolVersion".into(), json!(PROTOCOL_VERSION));
        body.insert("tcpPort".into(), json!(tcp_port));
        body.insert(
            "incomingCapabilities".into(),
            json!(supported_incoming_capabilities()),
        );
        body.insert(
            "outgoingCapabilities".into(),
            json!(supported_outgoing_capabilities()),
        );
        Self::new(PACKET_TYPE_IDENTITY, body)
    }

    /// Plaintext TCP Connection Packet (`DeviceInfo::toConnectionPacket`)
    pub fn tcp_connection_packet(
        host: &HostIdentity,
        target_device_id: &str,
        target_protocol_version: i32,
    ) -> Self {
        let mut body = Map::new();
        body.insert("deviceId".into(), json!(host.device_id));
        body.insert("deviceName".into(), json!(host.device_name));
        body.insert("deviceType".into(), json!(host.device_type));
        body.insert("protocolVersion".into(), json!(PROTOCOL_VERSION));
        body.insert("targetDeviceId".into(), json!(target_device_id));
        body.insert("targetProtocolVersion".into(), json!(target_protocol_version));
        body.insert(
            "incomingCapabilities".into(),
            json!(supported_incoming_capabilities()),
        );
        body.insert(
            "outgoingCapabilities".into(),
            json!(supported_outgoing_capabilities()),
        );
        Self::new(PACKET_TYPE_IDENTITY, body)
    }

    /// Full Encrypted Identity Packet (`DeviceInfo::toIdentityPacket`)
    pub fn full_identity(host: &HostIdentity) -> Self {
        let mut body = Map::new();
        body.insert("deviceId".into(), json!(host.device_id));
        body.insert("deviceName".into(), json!(host.device_name));
        body.insert("deviceType".into(), json!(host.device_type));
        body.insert("protocolVersion".into(), json!(PROTOCOL_VERSION));
        body.insert(
            "incomingCapabilities".into(),
            json!(supported_incoming_capabilities()),
        );
        body.insert(
            "outgoingCapabilities".into(),
            json!(supported_outgoing_capabilities()),
        );
        Self::new(PACKET_TYPE_IDENTITY, body)
    }

    pub fn pair_request(timestamp_secs: i64) -> Self {
        let mut body = Map::new();
        body.insert("pair".into(), json!(true));
        body.insert("timestamp".into(), json!(timestamp_secs));
        Self::new(PACKET_TYPE_PAIR, body)
    }

    pub fn pair_accept() -> Self {
        let mut body = Map::new();
        body.insert("pair".into(), json!(true));
        Self::new(PACKET_TYPE_PAIR, body)
    }

    pub fn unpair() -> Self {
        let mut body = Map::new();
        body.insert("pair".into(), json!(false));
        Self::new(PACKET_TYPE_PAIR, body)
    }

    pub fn ping(message: Option<&str>) -> Self {
        let mut body = Map::new();
        if let Some(msg) = message {
            body.insert("message".into(), json!(msg));
        }
        Self::new(PACKET_TYPE_PING, body)
    }

    pub fn battery_request() -> Self {
        let mut body = Map::new();
        body.insert("request".into(), json!(true));
        Self::new(PACKET_TYPE_BATTERY_REQUEST, body)
    }

    pub fn battery_status(current_charge: i32, is_charging: bool) -> Self {
        let threshold_event = if current_charge <= 15 && !is_charging {
            1
        } else {
            0
        };
        let mut body = Map::new();
        body.insert("currentCharge".into(), json!(current_charge));
        body.insert("isCharging".into(), json!(is_charging));
        body.insert("thresholdEvent".into(), json!(threshold_event));
        Self::new(PACKET_TYPE_BATTERY, body)
    }

    pub fn connectivity_report_request() -> Self {
        Self::new(PACKET_TYPE_CONNECTIVITY_REPORT_REQUEST, Map::new())
    }

    pub fn keepalive() -> Self {
        Self::new("velquo.keepalive", Map::new())
    }

    pub fn mpris_request_player_list() -> Self {
        let mut body = Map::new();
        body.insert("requestPlayerList".into(), json!(true));
        Self::new(PACKET_TYPE_MPRIS_REQUEST, body)
    }

    pub fn mpris_request_player_status(player: &str) -> Self {
        let mut body = Map::new();
        body.insert("player".into(), json!(player));
        body.insert("requestNowPlaying".into(), json!(true));
        body.insert("requestVolume".into(), json!(true));
        Self::new(PACKET_TYPE_MPRIS_REQUEST, body)
    }

    pub fn mpris_action(player: &str, action: &str) -> Self {
        let mut body = Map::new();
        body.insert("player".into(), json!(player));
        body.insert("action".into(), json!(action));
        Self::new(PACKET_TYPE_MPRIS_REQUEST, body)
    }

    pub fn mpris_set_position(player: &str, position_ms: i64) -> Self {
        let mut body = Map::new();
        body.insert("player".into(), json!(player));
        body.insert("SetPosition".into(), json!(position_ms));
        Self::new(PACKET_TYPE_MPRIS_REQUEST, body)
    }

    pub fn mpris_request_album_art(player: &str, album_art_url: &str) -> Self {
        let mut body = Map::new();
        body.insert("player".into(), json!(player));
        body.insert("albumArtUrl".into(), json!(album_art_url));
        Self::new(PACKET_TYPE_MPRIS_REQUEST, body)
    }

    pub fn share_file_header(
        filename: &str,
        file_size: u64,
        creation_time_ms: i64,
        last_modified_ms: i64,
        payload_port: u16,
        number_of_files: usize,
        total_payload_size: u64,
    ) -> Self {
        let mut body = Map::new();
        body.insert("filename".into(), json!(filename));
        body.insert("creationTime".into(), json!(creation_time_ms));
        body.insert("lastModified".into(), json!(last_modified_ms));
        body.insert("open".into(), json!(false));
        body.insert("numberOfFiles".into(), json!(number_of_files.max(1)));
        body.insert("totalPayloadSize".into(), json!(total_payload_size));

        let mut transfer_info = Map::new();
        transfer_info.insert("port".into(), json!(payload_port));

        let mut pkt = Self::new(PACKET_TYPE_SHARE_REQUEST, body);
        pkt.payload_size = Some(file_size as i64);
        pkt.payload_transfer_info = Some(transfer_info);
        pkt
    }

    pub fn share_request_update(number_of_files: usize, total_payload_size: u64) -> Self {
        let mut body = Map::new();
        body.insert("numberOfFiles".into(), json!(number_of_files.max(1)));
        body.insert("totalPayloadSize".into(), json!(total_payload_size));
        Self::new(PACKET_TYPE_SHARE_REQUEST_UPDATE, body)
    }

    pub fn share_url(url: &str) -> Self {
        let mut body = Map::new();
        body.insert("url".into(), json!(url));
        Self::new(PACKET_TYPE_SHARE_REQUEST, body)
    }

    pub fn share_text(text: &str) -> Self {
        let mut body = Map::new();
        body.insert("text".into(), json!(text));
        Self::new(PACKET_TYPE_SHARE_REQUEST, body)
    }

    pub fn clipboard(content: &str) -> Self {
        let mut body = Map::new();
        body.insert("content".into(), json!(content));
        Self::new(PACKET_TYPE_CLIPBOARD, body)
    }

    pub fn clipboard_connect(content: &str, timestamp_ms: i64) -> Self {
        let mut body = Map::new();
        body.insert("content".into(), json!(content));
        body.insert("timestamp".into(), json!(timestamp_ms));
        Self::new(PACKET_TYPE_CLIPBOARD_CONNECT, body)
    }

    pub fn mousepad_keyboard_state(state: bool) -> Self {
        let mut body = Map::new();
        body.insert("state".into(), json!(state));
        Self::new(PACKET_TYPE_MOUSEPAD_KEYBOARDSTATE, body)
    }

    pub fn find_my_phone() -> Self {
        Self::new(PACKET_TYPE_FINDMYPHONE_REQUEST, Map::new())
    }

    pub fn notification_request() -> Self {
        let mut body = Map::new();
        body.insert("request".into(), json!(true));
        Self::new(PACKET_TYPE_NOTIFICATION_REQUEST, body)
    }

    pub fn notification_cancel(notification_id: &str) -> Self {
        let mut body = Map::new();
        body.insert("cancel".into(), json!(notification_id));
        Self::new(PACKET_TYPE_NOTIFICATION_REQUEST, body)
    }

    pub fn runcommand_list(commands_json: &str) -> Self {
        let mut body = Map::new();
        body.insert("commandList".into(), json!(commands_json));
        body.insert("canAddCommand".into(), json!(true));
        Self::new(PACKET_TYPE_RUNCOMMAND, body)
    }
}
