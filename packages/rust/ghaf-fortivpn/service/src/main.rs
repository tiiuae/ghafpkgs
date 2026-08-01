/*
 * SPDX-FileCopyrightText: 2026 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

use openssl::asn1::Asn1Time;
use openssl::pkcs12::Pkcs12;
use openssl::pkey::{PKey, Private};
use openssl::x509::X509;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::net::{IpAddr, Ipv6Addr};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use tempfile::Builder as TempBuilder;
use uuid::Uuid;
use zbus::fdo;
use zbus::interface;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Str};
use zeroize::Zeroizing;

const BUS_NAME: &str = "org.ghaf.FortiVpn";
const OBJECT_PATH: &str = "/org/ghaf/FortiVpn";
const NM_BUS_NAME: &str = "org.freedesktop.NetworkManager";
const NM_SETTINGS_PATH: &str = "/org/freedesktop/NetworkManager/Settings";
const NM_SETTINGS_INTERFACE: &str = "org.freedesktop.NetworkManager.Settings";
const FORTISSLVPN_SERVICE_TYPE: &str = "org.freedesktop.NetworkManager.fortisslvpn";
const DEFAULT_STATE_ROOT: &str = "/var/lib/ghaf/fortivpn";
const MAX_BUNDLE_SIZE: usize = 10 * 1024 * 1024;
const MAX_CERTIFICATE_SIZE: usize = 2 * 1024 * 1024;
const MAX_PRIVATE_KEY_SIZE: usize = 2 * 1024 * 1024;
const MAX_PASSWORD_SIZE: usize = 4096;

type Setting = HashMap<String, OwnedValue>;
type Settings = HashMap<String, Setting>;

#[derive(Debug)]
struct ProfileError(&'static str);

type ProfileResult<T> = Result<T, ProfileError>;

impl From<std::io::Error> for ProfileError {
    fn from(_: std::io::Error) -> Self {
        Self("Certificate storage failed")
    }
}

struct Materials {
    certificate: Option<Vec<u8>>,
    private_key: Option<Zeroizing<Vec<u8>>>,
    ca_certificate: Option<Vec<u8>>,
}

struct StoredMaterials {
    directory: Option<PathBuf>,
    certificate: Option<String>,
    private_key: Option<String>,
    ca_certificate: Option<String>,
}

struct ProfileParameters<'a> {
    name: &'a str,
    gateway: &'a str,
    port: u16,
    realm: &'a str,
    username: &'a str,
    password: &'a str,
    trusted_certificate: &'a str,
}

struct FortiVpnService {
    state_root: PathBuf,
}

#[interface(name = "org.ghaf.FortiVpn1")]
impl FortiVpnService {
    #[allow(clippy::too_many_arguments)]
    async fn create_profile(
        &self,
        name: String,
        gateway: String,
        port: u16,
        realm: String,
        username: String,
        password: String,
        trusted_certificate: String,
        pkcs12: Vec<u8>,
        pkcs12_password: String,
        client_certificate: Vec<u8>,
        private_key: Vec<u8>,
        private_key_password: String,
        ca_certificate: Vec<u8>,
    ) -> fdo::Result<String> {
        let password = Zeroizing::new(password);
        let pkcs12 = Zeroizing::new(pkcs12);
        let pkcs12_password = Zeroizing::new(pkcs12_password);
        let private_key = Zeroizing::new(private_key);
        let private_key_password = Zeroizing::new(private_key_password);
        let parameters = ProfileParameters {
            name: &name,
            gateway: &gateway,
            port,
            realm: &realm,
            username: &username,
            password: &password,
            trusted_certificate: &trusted_certificate,
        };

        create_profile(
            &self.state_root,
            &parameters,
            &pkcs12,
            &pkcs12_password,
            &client_certificate,
            &private_key,
            &private_key_password,
            &ca_certificate,
        )
        .await
        .map_err(|error| fdo::Error::Failed(error.0.into()))
    }
}

fn check_size(data: &[u8], maximum: usize, label: &'static str) -> ProfileResult<()> {
    if data.len() > maximum {
        return Err(ProfileError(label));
    }
    Ok(())
}

fn validate_request(
    pkcs12: &[u8],
    pkcs12_password: &str,
    client_certificate: &[u8],
    private_key: &[u8],
    private_key_password: &str,
    ca_certificate: &[u8],
) -> ProfileResult<()> {
    check_size(
        pkcs12,
        MAX_BUNDLE_SIZE,
        "The PKCS#12 file is larger than 10 MiB",
    )?;
    check_size(
        client_certificate,
        MAX_CERTIFICATE_SIZE,
        "The client certificate is larger than 2 MiB",
    )?;
    check_size(
        private_key,
        MAX_PRIVATE_KEY_SIZE,
        "The private key is larger than 2 MiB",
    )?;
    check_size(
        ca_certificate,
        MAX_CERTIFICATE_SIZE,
        "The CA certificate is larger than 2 MiB",
    )?;

    if pkcs12_password.len() > MAX_PASSWORD_SIZE || private_key_password.len() > MAX_PASSWORD_SIZE {
        return Err(ProfileError("A certificate password is too long"));
    }
    if !pkcs12.is_empty() && (!client_certificate.is_empty() || !private_key.is_empty()) {
        return Err(ProfileError(
            "Choose either a PKCS#12 bundle or a client certificate and private key",
        ));
    }
    if client_certificate.is_empty() != private_key.is_empty() {
        return Err(ProfileError(
            "A client certificate and its private key must be supplied together",
        ));
    }
    if pkcs12.is_empty() && !pkcs12_password.is_empty() {
        return Err(ProfileError(
            "A PKCS#12 password was supplied without a bundle",
        ));
    }
    if private_key.is_empty() && !private_key_password.is_empty() {
        return Err(ProfileError(
            "A key password was supplied without a private key",
        ));
    }
    Ok(())
}

fn parse_x509_certificates(data: &[u8]) -> ProfileResult<Vec<X509>> {
    let certificates = if data.starts_with(b"-----BEGIN") {
        X509::stack_from_pem(data)
    } else {
        X509::from_der(data).map(|certificate| vec![certificate])
    }
    .map_err(|_| ProfileError("A certificate is not valid PEM or DER X.509 data"))?;

    if certificates.is_empty() {
        return Err(ProfileError("The certificate file is empty"));
    }
    Ok(certificates)
}

fn validate_certificate(certificate: &X509) -> ProfileResult<()> {
    let now = Asn1Time::days_from_now(0)
        .map_err(|_| ProfileError("Certificate validity could not be checked"))?;
    if certificate
        .not_before()
        .compare(&now)
        .map_err(|_| ProfileError("Certificate validity could not be checked"))?
        == Ordering::Greater
    {
        return Err(ProfileError("A certificate is not valid yet"));
    }
    if certificate
        .not_after()
        .compare(&now)
        .map_err(|_| ProfileError("Certificate validity could not be checked"))?
        == Ordering::Less
    {
        return Err(ProfileError("A certificate has expired"));
    }
    Ok(())
}

fn parse_private_key(data: &[u8], password: &str) -> ProfileResult<PKey<Private>> {
    let parsed = if data.starts_with(b"-----BEGIN") {
        if password.is_empty() {
            PKey::private_key_from_pem(data)
        } else {
            PKey::private_key_from_pem_passphrase(data, password.as_bytes())
        }
    } else if password.is_empty() {
        PKey::private_key_from_der(data).or_else(|_| PKey::private_key_from_pkcs8(data))
    } else {
        PKey::private_key_from_pkcs8_passphrase(data, password.as_bytes())
    };

    parsed.map_err(|_| ProfileError("The private key or its password is invalid"))
}

fn encode_certificate_chain(certificates: &[X509]) -> ProfileResult<Vec<u8>> {
    let mut output = Vec::new();
    for certificate in certificates {
        validate_certificate(certificate)?;
        output.extend_from_slice(
            &certificate
                .to_pem()
                .map_err(|_| ProfileError("Certificate conversion failed"))?,
        );
    }
    Ok(output)
}

fn parse_materials(
    pkcs12: &[u8],
    pkcs12_password: &str,
    client_certificate: &[u8],
    private_key: &[u8],
    private_key_password: &str,
    ca_certificate: &[u8],
) -> ProfileResult<Materials> {
    validate_request(
        pkcs12,
        pkcs12_password,
        client_certificate,
        private_key,
        private_key_password,
        ca_certificate,
    )?;

    let (certificate, key) = if !pkcs12.is_empty() {
        let parsed = Pkcs12::from_der(pkcs12)
            .and_then(|bundle| bundle.parse2(pkcs12_password))
            .map_err(|_| ProfileError("The PKCS#12 bundle or its password is invalid"))?;
        let certificate = parsed
            .cert
            .ok_or(ProfileError("The PKCS#12 bundle has no client certificate"))?;
        let key = parsed
            .pkey
            .ok_or(ProfileError("The PKCS#12 bundle has no private key"))?;
        validate_certificate(&certificate)?;
        if !certificate
            .public_key()
            .map_err(|_| ProfileError("The client certificate is invalid"))?
            .public_eq(&key)
        {
            return Err(ProfileError(
                "The client certificate and private key do not match",
            ));
        }

        let mut certificates = vec![certificate];
        if let Some(chain) = parsed.ca {
            certificates.extend(chain);
        }
        (
            Some(encode_certificate_chain(&certificates)?),
            Some(Zeroizing::new(key.private_key_to_pem_pkcs8().map_err(
                |_| ProfileError("Private-key conversion failed"),
            )?)),
        )
    } else if !client_certificate.is_empty() {
        let certificates = parse_x509_certificates(client_certificate)?;
        let key = parse_private_key(private_key, private_key_password)?;
        if !certificates[0]
            .public_key()
            .map_err(|_| ProfileError("The client certificate is invalid"))?
            .public_eq(&key)
        {
            return Err(ProfileError(
                "The client certificate and private key do not match",
            ));
        }
        (
            Some(encode_certificate_chain(&certificates)?),
            Some(Zeroizing::new(key.private_key_to_pem_pkcs8().map_err(
                |_| ProfileError("Private-key conversion failed"),
            )?)),
        )
    } else {
        (None, None)
    };

    let ca_certificate = if ca_certificate.is_empty() {
        None
    } else {
        Some(encode_certificate_chain(&parse_x509_certificates(
            ca_certificate,
        )?)?)
    };

    Ok(Materials {
        certificate,
        private_key: key,
        ca_certificate,
    })
}

fn ensure_private_directory(path: &Path) -> ProfileResult<()> {
    if path.exists() {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
            return Err(ProfileError("Certificate storage is not a safe directory"));
        }
    } else {
        fs::create_dir(path)?;
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

fn write_file(path: &Path, contents: &[u8], mode: u32) -> ProfileResult<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(path)?;
    file.write_all(contents)?;
    file.sync_all()?;
    Ok(())
}

fn store_materials(
    state_root: &Path,
    profile_uuid: &str,
    materials: &Materials,
) -> ProfileResult<StoredMaterials> {
    if materials.certificate.is_none() && materials.ca_certificate.is_none() {
        return Ok(StoredMaterials {
            directory: None,
            certificate: None,
            private_key: None,
            ca_certificate: None,
        });
    }

    ensure_private_directory(state_root)?;
    let directory = state_root.join(profile_uuid);
    let temporary = TempBuilder::new()
        .prefix(".profile-")
        .tempdir_in(state_root)?;

    let certificate = materials
        .certificate
        .as_ref()
        .map(|contents| {
            write_file(&temporary.path().join("client-cert.pem"), contents, 0o644).map(|()| {
                directory
                    .join("client-cert.pem")
                    .to_string_lossy()
                    .into_owned()
            })
        })
        .transpose()?;
    let private_key = materials
        .private_key
        .as_ref()
        .map(|contents| {
            write_file(&temporary.path().join("client-key.pem"), contents, 0o600).map(|()| {
                directory
                    .join("client-key.pem")
                    .to_string_lossy()
                    .into_owned()
            })
        })
        .transpose()?;
    let ca_certificate = materials
        .ca_certificate
        .as_ref()
        .map(|contents| {
            write_file(&temporary.path().join("gateway-ca.pem"), contents, 0o644).map(|()| {
                directory
                    .join("gateway-ca.pem")
                    .to_string_lossy()
                    .into_owned()
            })
        })
        .transpose()?;
    fs::rename(temporary.path(), &directory)?;

    Ok(StoredMaterials {
        directory: Some(directory),
        certificate,
        private_key,
        ca_certificate,
    })
}

fn validate_plain_text(
    value: &str,
    maximum: usize,
    empty_message: &'static str,
    invalid_message: &'static str,
) -> ProfileResult<()> {
    if value.is_empty() {
        return Err(ProfileError(empty_message));
    }
    if value.len() > maximum || value.chars().any(char::is_control) {
        return Err(ProfileError(invalid_message));
    }
    Ok(())
}

fn validate_optional_text(
    value: &str,
    maximum: usize,
    invalid_message: &'static str,
) -> ProfileResult<()> {
    if value.len() > maximum || value.chars().any(char::is_control) {
        return Err(ProfileError(invalid_message));
    }
    Ok(())
}

fn validate_hostname(host: &str) -> ProfileResult<()> {
    if host.parse::<IpAddr>().is_ok() {
        return Ok(());
    }
    if host.len() > 253
        || host.is_empty()
        || host.split('.').any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '-')
        })
    {
        return Err(ProfileError(
            "Enter a valid gateway host without a URL scheme",
        ));
    }
    Ok(())
}

fn normalize_fingerprint(value: &str) -> ProfileResult<Option<String>> {
    if value.is_empty() {
        return Ok(None);
    }
    let normalized: String = value
        .chars()
        .filter(|character| *character != ':')
        .collect();
    if normalized.len() != 64
        || !normalized
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(ProfileError(
            "The trusted certificate must be a SHA-256 fingerprint",
        ));
    }
    Ok(Some(normalized.to_ascii_lowercase()))
}

fn string_value(value: impl Into<String>) -> OwnedValue {
    OwnedValue::from(Str::from(value.into()))
}

fn build_settings(
    parameters: &ProfileParameters<'_>,
    profile_uuid: &str,
    stored: &StoredMaterials,
) -> ProfileResult<Settings> {
    let name = parameters.name.trim();
    let gateway = parameters.gateway.trim();
    let realm = parameters.realm.trim();
    let username = parameters.username.trim();
    let trusted_certificate = parameters.trusted_certificate.trim();

    validate_plain_text(
        name,
        128,
        "Enter a connection name",
        "The connection name is invalid",
    )?;
    validate_hostname(gateway)?;
    validate_optional_text(realm, 256, "The realm is invalid")?;
    validate_plain_text(username, 256, "Enter a username", "The username is invalid")?;
    if parameters.port == 0 {
        return Err(ProfileError("Enter a valid port from 1 to 65535"));
    }
    if parameters.password.len() > MAX_PASSWORD_SIZE {
        return Err(ProfileError("The VPN password is too long"));
    }
    if parameters.password.is_empty() && stored.certificate.is_none() {
        return Err(ProfileError(
            "Enter a VPN password or provide a client certificate",
        ));
    }
    let trusted_certificate = normalize_fingerprint(trusted_certificate)?;

    let mut connection = Setting::new();
    connection.insert("id".into(), string_value(name));
    connection.insert("uuid".into(), string_value(profile_uuid));
    connection.insert("type".into(), string_value("vpn"));
    connection.insert("autoconnect".into(), OwnedValue::from(false));

    let gateway = if gateway.parse::<Ipv6Addr>().is_ok() {
        format!("[{gateway}]:{}", parameters.port)
    } else {
        format!("{gateway}:{}", parameters.port)
    };
    let mut data = HashMap::<String, String>::new();
    data.insert("gateway".into(), gateway);
    data.insert("user".into(), username.into());
    if !realm.is_empty() {
        data.insert("realm".into(), realm.into());
    }
    if let Some(fingerprint) = trusted_certificate {
        data.insert("trusted-cert".into(), fingerprint);
    }
    if let Some(path) = &stored.certificate {
        data.insert("cert".into(), path.clone());
    }
    if let Some(path) = &stored.private_key {
        data.insert("key".into(), path.clone());
    }
    if let Some(path) = &stored.ca_certificate {
        data.insert("ca".into(), path.clone());
    }

    let mut secrets = HashMap::<String, String>::new();
    if !parameters.password.is_empty() {
        data.insert("password-flags".into(), "0".into());
        secrets.insert("password".into(), parameters.password.into());
    }

    let mut vpn = Setting::new();
    vpn.insert(
        "service-type".into(),
        string_value(FORTISSLVPN_SERVICE_TYPE),
    );
    vpn.insert("data".into(), OwnedValue::from(data));
    vpn.insert("secrets".into(), OwnedValue::from(secrets));

    let mut ipv4 = Setting::new();
    ipv4.insert("method".into(), string_value("auto"));
    let mut ipv6 = Setting::new();
    ipv6.insert("method".into(), string_value("auto"));

    Ok(HashMap::from([
        ("connection".into(), connection),
        ("vpn".into(), vpn),
        ("ipv4".into(), ipv4),
        ("ipv6".into(), ipv6),
    ]))
}

#[allow(clippy::too_many_arguments)]
async fn create_profile(
    state_root: &Path,
    parameters: &ProfileParameters<'_>,
    pkcs12: &[u8],
    pkcs12_password: &str,
    client_certificate: &[u8],
    private_key: &[u8],
    private_key_password: &str,
    ca_certificate: &[u8],
) -> ProfileResult<String> {
    let materials = parse_materials(
        pkcs12,
        pkcs12_password,
        client_certificate,
        private_key,
        private_key_password,
        ca_certificate,
    )?;
    let profile_uuid = Uuid::new_v4().to_string();
    let stored = store_materials(state_root, &profile_uuid, &materials)?;
    let settings = match build_settings(parameters, &profile_uuid, &stored) {
        Ok(settings) => settings,
        Err(error) => {
            if let Some(directory) = &stored.directory {
                let _ = fs::remove_dir_all(directory);
            }
            return Err(error);
        }
    };

    let result = async {
        let connection = zbus::Connection::system()
            .await
            .map_err(|_| ProfileError("NetworkManager is unavailable"))?;
        let settings_proxy = zbus::Proxy::new(
            &connection,
            NM_BUS_NAME,
            NM_SETTINGS_PATH,
            NM_SETTINGS_INTERFACE,
        )
        .await
        .map_err(|_| ProfileError("NetworkManager is unavailable"))?;
        let _: OwnedObjectPath = settings_proxy
            .call("AddConnection", &(settings))
            .await
            .map_err(|_| ProfileError("NetworkManager rejected the VPN profile"))?;
        Ok(())
    }
    .await;

    if result.is_err() {
        if let Some(directory) = &stored.directory {
            let _ = fs::remove_dir_all(directory);
        }
        return result.map(|()| profile_uuid);
    }
    Ok(profile_uuid)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let state_root = std::env::var_os("GHAF_FORTIVPN_STATE_DIRECTORY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_STATE_ROOT));
    let service = FortiVpnService { state_root };
    let _connection = zbus::connection::Builder::system()?
        .name(BUS_NAME)?
        .serve_at(OBJECT_PATH, service)?
        .build()
        .await?;
    std::future::pending::<()>().await;
    Ok(())
}
