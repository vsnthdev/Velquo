use anyhow::{Context, Result};
use rcgen::{CertificateParams, DistinguishedName, DnType, IsCa, KeyPair, PKCS_ECDSA_P256_SHA256};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use uuid::Uuid;
use x509_parser::prelude::*;

#[derive(Clone)]
pub struct HostIdentity {
    pub device_id: String,
    pub device_name: String,
    pub device_type: String,
    pub cert_der: Vec<u8>,
    pub key_der: Vec<u8>,
    pub cert_fingerprint: String,
}

impl HostIdentity {
    pub fn load_or_create(config_dir: &Path) -> Result<Self> {
        fs::create_dir_all(config_dir)?;

        let id_path = config_dir.join("device_id.txt");
        let cert_der_path = config_dir.join("cert.der");
        let key_der_path = config_dir.join("key.der");
        let cert_pem_path = config_dir.join("cert.pem");
        let key_pem_path = config_dir.join("key.pem");

        let device_id = if id_path.exists() {
            let raw = fs::read_to_string(&id_path)?.trim().to_string();
            if raw.len() >= 32 {
                raw
            } else {
                let new_id = Uuid::new_v4().simple().to_string();
                fs::write(&id_path, &new_id)?;
                new_id
            }
        } else {
            let new_id = Uuid::new_v4().simple().to_string();
            fs::write(&id_path, &new_id)?;
            new_id
        };

        let raw_hostname = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "MacBook".to_string());
        let device_name = sanitize_device_name(&raw_hostname);

        let (cert_der, key_der) = if cert_der_path.exists() && key_der_path.exists() {
            log::info!(
                "[cert] Loading existing host X.509 certificate and private key from {:?}",
                config_dir
            );
            (fs::read(&cert_der_path)?, fs::read(&key_der_path)?)
        } else {
            log::info!(
                "[cert] Generating new ECDSA P-256 self-signed X.509 certificate for device_id={}",
                device_id
            );
            let mut params = CertificateParams::default();
            params.is_ca = IsCa::ExplicitNoCa;
            let mut dn = DistinguishedName::new();
            dn.push(DnType::CommonName, device_id.clone());
            dn.push(DnType::OrganizationName, "KDE");
            dn.push(DnType::OrganizationalUnitName, "KDE Connect");
            params.distinguished_name = dn;

            let key_pair = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256)
                .context("Failed to generate ECDSA P-256 keypair")?;
            let cert = params
                .self_signed(&key_pair)
                .context("Failed to self-sign certificate")?;

            let cert_der = cert.der().to_vec();
            let key_der = key_pair.serialize_der();

            fs::write(&cert_der_path, &cert_der)?;
            fs::write(&key_der_path, &key_der)?;
            fs::write(&cert_pem_path, cert.pem())?;
            fs::write(&key_pem_path, key_pair.serialize_pem())?;

            (cert_der, key_der)
        };

        let cert_fingerprint = sha256_fingerprint(&cert_der);
        log::info!(
            "[cert] Host identity ready: name='{}' id='{}' sha256={}",
            device_name,
            device_id,
            cert_fingerprint
        );

        Ok(Self {
            device_id,
            device_name,
            device_type: "laptop".to_string(),
            cert_der,
            key_der,
            cert_fingerprint,
        })
    }
}

/// Filter characters disallowed by KDE Connect's DeviceInfo::filterName
pub fn sanitize_device_name(input: &str) -> String {
    let cleaned: String = input
        .trim_end_matches(".local")
        .chars()
        .filter(|c| !"\"',;:.!?()[]<>".contains(*c))
        .take(32)
        .collect();
    if cleaned.trim().is_empty() {
        "MacBook".to_string()
    } else {
        cleaned
    }
}

pub fn sha256_fingerprint(cert_der: &[u8]) -> String {
    let digest = Sha256::digest(cert_der);
    digest
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(":")
}

/// Extracts the Common Name (CN) from a DER-encoded X.509 certificate
pub fn extract_cert_common_name(cert_der: &[u8]) -> Option<String> {
    let (_, cert) = X509Certificate::from_der(cert_der).ok()?;
    for cn in cert.subject().iter_common_name() {
        if let Ok(s) = cn.as_str() {
            return Some(s.to_string());
        }
    }
    None
}

/// Extracts SubjectPublicKeyInfo raw DER bytes from a DER-encoded X.509 certificate
/// Matches Qt's `certificate.publicKey().toDer()` used in `PairingHandler::verificationKey()`
pub fn extract_public_key_spki_der(cert_der: &[u8]) -> Option<Vec<u8>> {
    let (_, cert) = X509Certificate::from_der(cert_der).ok()?;
    Some(cert.public_key().raw.to_vec())
}

/// Computes the 8-character uppercase hex verification key per `kdeconnect-kde/core/backends/pairinghandler.cpp`:
/// 1. Extract publicKey().toDer() (SPKI DER) for both certificates
/// 2. Sort the two byte slices so `a >= b` (note in C++: `if (a < b) std::swap(a, b);` -> larger first!)
/// 3. SHA256(a || b || ascii_decimal(timestamp))
/// 4. Left 8 hex chars in uppercase
pub fn compute_verification_key(
    local_cert_der: &[u8],
    peer_cert_der: &[u8],
    timestamp_secs: i64,
) -> Option<String> {
    let mut a = extract_public_key_spki_der(local_cert_der)?;
    let mut b = extract_public_key_spki_der(peer_cert_der)?;

    // In kdeconnect-kde/core/backends/pairinghandler.cpp:
    // if (a < b) { std::swap(a, b); }
    if a < b {
        std::mem::swap(&mut a, &mut b);
    }

    let mut hasher = Sha256::new();
    hasher.update(&a);
    hasher.update(&b);
    hasher.update(timestamp_secs.to_string().as_bytes());
    let digest = hasher.finalize();
    let hex_str: String = digest.iter().map(|byte| format!("{:02X}", byte)).collect();
    Some(hex_str.chars().take(8).collect())
}
