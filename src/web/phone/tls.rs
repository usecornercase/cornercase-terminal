use std::io;
use std::path::Path;
use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use rcgen::{CertificateParams, DnType, KeyPair};
use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::log;

const VERSION: u32 = 1;
const NAME_CHARS: usize = 64;
const DNS_NAME: &str = "cornercase";

#[derive(Serialize, Deserialize)]
struct Saved {
    version: u32,
    certificate: String,
    key: String,
}

pub(super) struct Identity {
    pub(super) config: Arc<ServerConfig>,
    pub(super) fingerprint: String,
}

impl Identity {
    pub(super) fn load_or_make(path: &Path, name: &dyn Fn() -> Option<String>) -> io::Result<Self> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Self::made(path, name().as_deref()),
            Err(e) => return Err(e),
        };
        if let Some(identity) = loaded(&text) {
            log::info!("web", "phone app certificate loaded", fingerprint = identity.fingerprint);
            Ok(identity)
        } else {
            log::warning!("web", "the phone app's certificate could not be read: making a new one");
            Self::made(path, name().as_deref())
        }
    }

    fn made(path: &Path, name: Option<&str>) -> io::Result<Self> {
        let identity = Self::make(path, name)?;
        log::info!("web", "phone app certificate made", fingerprint = identity.fingerprint);
        Ok(identity)
    }

    pub(super) fn make(path: &Path, name: Option<&str>) -> io::Result<Self> {
        let (certificate, key) = generate(name).map_err(io::Error::other)?;
        let saved = Saved { version: VERSION, certificate: STANDARD.encode(&certificate), key: STANDARD.encode(&key) };
        crate::persist::write(path, &saved, true)?;
        identity(certificate, key)
    }
}

pub(super) fn fingerprint(certificate: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(certificate))
}

fn subject(name: Option<&str>) -> String {
    match name.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => name.chars().take(NAME_CHARS).collect(),
        None => DNS_NAME.to_string(),
    }
}

fn generate(name: Option<&str>) -> Result<(Vec<u8>, Vec<u8>), rcgen::Error> {
    let key = KeyPair::generate()?;
    let mut params = CertificateParams::new(vec![DNS_NAME.to_string()])?;
    params.distinguished_name.push(DnType::CommonName, subject(name));
    let certificate = params.self_signed(&key)?;
    Ok((certificate.der().to_vec(), key.serialize_der()))
}

fn loaded(text: &str) -> Option<Identity> {
    let saved = serde_json::from_str::<Saved>(text).ok().filter(|saved| saved.version == VERSION)?;
    let certificate = STANDARD.decode(saved.certificate).ok()?;
    let key = STANDARD.decode(saved.key).ok()?;
    identity(certificate, key).ok()
}

fn identity(certificate: Vec<u8>, key: Vec<u8>) -> io::Result<Identity> {
    let fingerprint = fingerprint(&certificate);
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key));
    let config = ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(io::Error::other)?
        .with_no_client_auth()
        .with_single_cert(vec![CertificateDer::from(certificate)], key)
        .map_err(io::Error::other)?;
    Ok(Identity { config: Arc::new(config), fingerprint })
}

#[cfg(test)]
mod tests;
