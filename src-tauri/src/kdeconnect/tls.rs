use crate::kdeconnect::cert::HostIdentity;
use anyhow::{Context, Result};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{
    ClientConfig, DigitallySignedStruct, DistinguishedName, Error, ServerConfig, SignatureScheme,
};
use std::sync::Arc;
use tokio_rustls::{TlsAcceptor, TlsConnector};

/// Custom verifier for KDE Connect's self-signed peer-to-peer mutual TLS certificates.
/// - Cryptographically verifies the TLS 1.2 / 1.3 handshake signature against the peer's presented X.509 certificate
/// - Allows self-signed certificates so unpaired devices can complete the TLS handshake (TOFU)
///   and paired devices can have their pinned certificate verified immediately after handshake.
#[derive(Debug)]
pub struct KdeConnectCertVerifier {
    provider: Arc<CryptoProvider>,
    empty_roots: Vec<DistinguishedName>,
}

impl Default for KdeConnectCertVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl KdeConnectCertVerifier {
    pub fn new() -> Self {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        Self {
            provider,
            empty_roots: Vec::new(),
        }
    }
}

impl ServerCertVerifier for KdeConnectCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, Error> {
        if end_entity.is_empty() {
            return Err(Error::General("Empty server certificate".into()));
        }
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

impl ClientCertVerifier for KdeConnectCertVerifier {
    fn offer_client_auth(&self) -> bool {
        true
    }

    fn client_auth_mandatory(&self) -> bool {
        true
    }

    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &self.empty_roots
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> std::result::Result<ClientCertVerified, Error> {
        if end_entity.is_empty() {
            return Err(Error::General("Empty client certificate".into()));
        }
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

pub fn build_tls_acceptor(identity: &HostIdentity) -> Result<TlsAcceptor> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = Arc::new(KdeConnectCertVerifier::new());

    let cert = CertificateDer::from(identity.cert_der.clone());
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(identity.key_der.clone()));

    let server_config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .context("Failed to configure TLS protocol versions")?
        .with_client_cert_verifier(verifier)
        .with_single_cert(vec![cert], key)
        .context("Failed to configure server certificate")?;

    Ok(TlsAcceptor::from(Arc::new(server_config)))
}

pub fn build_tls_connector(identity: &HostIdentity) -> Result<TlsConnector> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = Arc::new(KdeConnectCertVerifier::new());

    let cert = CertificateDer::from(identity.cert_der.clone());
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(identity.key_der.clone()));

    let client_config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .context("Failed to configure client TLS protocol versions")?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_client_auth_cert(vec![cert], key)
        .context("Failed to configure client auth certificate")?;

    Ok(TlsConnector::from(Arc::new(client_config)))
}
