use crate::kdeconnect::cert::HostIdentity;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::time::{SystemTime, UNIX_EPOCH};

pub const PROTOCOL_VERSION: i32 = 8;

pub const PACKET_TYPE_IDENTITY: &str = "kdeconnect.identity";
pub const PACKET_TYPE_PAIR: &str = "kdeconnect.pair";
pub const PACKET_TYPE_PING: &str = "kdeconnect.ping";
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
    ]
}

pub fn supported_outgoing_capabilities() -> Vec<String> {
    vec![
        PACKET_TYPE_SHARE_REQUEST.to_string(),
        PACKET_TYPE_SHARE_REQUEST_UPDATE.to_string(),
        PACKET_TYPE_PING.to_string(),
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
        self.body.get(key).and_then(|v| v.as_bool())
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

    pub fn share_file_header(
        filename: &str,
        file_size: u64,
        creation_time_ms: i64,
        last_modified_ms: i64,
        payload_port: u16,
    ) -> Self {
        let mut body = Map::new();
        body.insert("filename".into(), json!(filename));
        body.insert("creationTime".into(), json!(creation_time_ms));
        body.insert("lastModified".into(), json!(last_modified_ms));
        body.insert("open".into(), json!(false));
        body.insert("numberOfFiles".into(), json!(1));
        body.insert("totalPayloadSize".into(), json!(file_size));

        let mut transfer_info = Map::new();
        transfer_info.insert("port".into(), json!(payload_port));

        let mut pkt = Self::new(PACKET_TYPE_SHARE_REQUEST, body);
        pkt.payload_size = Some(file_size as i64);
        pkt.payload_transfer_info = Some(transfer_info);
        pkt
    }
}
