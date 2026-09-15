//! Independent-verification tests for US-0095 (russh 0.63 / russh-sftp 3.0).
//!
//! Written by the verifier, not the implementer. They cover the three things
//! the bump changed that the shipped suite does not reach:
//!
//! 1. the new `check_server_key(&PublicKeyOrCertificate)` certificate arm,
//!    end to end against a loopback server that really presents a host
//!    certificate (the shipped tests only exercise the `PublicKey` arm);
//! 2. private-key file parsing cases the new `keyfile_tests.rs` omits
//!    (`aes256-ctr` + bcrypt RSA, PKCS#8 PEM, CRLF, truncated input);
//! 3. how russh-sftp 3.0's new read pipelining behaves under the
//!    seek-per-chunk access pattern OneTerm's striped download used.
//!
//! Point 3 is a dated record of what `US-0095` measured. `IN-0037` has since
//! retired the striping, so nothing in OneTerm reads this way any more; the
//! live guard against re-introducing it is
//! `sftp_task::transfer::pipeline::pipeline_budget_tests`.

use std::borrow::Cow;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use russh::client;
use russh::keys::{Algorithm, PrivateKey};

use crate::handler::{SshClientHandler, SshHandlerError};
use crate::session::load_private_key;
use crate::test_support::TempKnownHosts;

// ---------------------------------------------------------------------------
// Loopback server that can present a host certificate
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct BareServer;

impl russh::server::Server for BareServer {
    type Handler = Self;
    fn new_client(&mut self, _peer: Option<SocketAddr>) -> Self {
        self.clone()
    }
}

impl russh::server::Handler for BareServer {
    type Error = russh::Error;

    async fn auth_password(
        &mut self,
        _user: &str,
        _password: &str,
    ) -> Result<russh::server::Auth, Self::Error> {
        Ok(russh::server::Auth::Accept)
    }
}

/// A self-signed OpenSSH *host* certificate over `subject`'s public key.
fn host_certificate(subject: &PrivateKey, ca: &PrivateKey) -> russh::keys::ssh_key::Certificate {
    let mut builder = russh::keys::ssh_key::certificate::Builder::new_with_random_nonce(
        &mut rand::rng(),
        subject.public_key(),
        0,
        u64::MAX,
    )
    .expect("certificate builder");
    builder.key_id("oneterm-verify").expect("key id");
    builder
        .cert_type(russh::keys::ssh_key::certificate::CertType::Host)
        .expect("cert type");
    builder.valid_principal("127.0.0.1").expect("principal");
    builder.sign(ca).expect("sign the certificate")
}

/// Run a loopback server with `host_key`, optionally advertising `certificate`.
async fn spawn_server_with(
    host_key: PrivateKey,
    certificate: Option<russh::keys::ssh_key::Certificate>,
) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let config = Arc::new(russh::server::Config {
        keys: vec![host_key],
        certificates: certificate.into_iter().collect(),
        auth_rejection_time: Duration::ZERO,
        auth_rejection_time_initial: Some(Duration::ZERO),
        ..Default::default()
    });
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("loopback listener");
    let address = listener.local_addr().expect("listener address");
    let task = tokio::spawn(async move {
        use russh::server::Server as _;
        if let Err(error) = BareServer.run_on_socket(config, &listener).await {
            eprintln!("verify server stopped: {error}");
        }
    });
    (address, task)
}

fn temp_known_hosts(tag: &str) -> TempKnownHosts {
    TempKnownHosts(std::env::temp_dir().join(format!(
        "oneterm-verify-known-hosts-{}-{tag}",
        std::process::id()
    )))
}

/// Connect with the strict policy, optionally advertising certificate host-key
/// algorithms so the server is asked for a certificate instead of a bare key.
async fn connect_strict(
    address: SocketAddr,
    known_hosts: &TempKnownHosts,
    want_certificates: bool,
) -> Result<client::Handle<SshClientHandler>, SshHandlerError> {
    let mut config = client::Config::default();
    if want_certificates {
        config.preferred.host_key_certificates = Cow::Owned(vec![Algorithm::Ed25519]);
    }
    client::connect(
        Arc::new(config),
        address,
        SshClientHandler::new(
            address.ip().to_string(),
            address.port(),
            oneterm_core::HostKeyPolicy::Strict,
        )
        .with_known_hosts_path(known_hosts.0.clone()),
    )
    .await
}

/// A host key already recorded in known_hosts is accepted through a real
/// handshake. The control case for the certificate test below: it proves the
/// fixture really does trust this server's bare key.
#[tokio::test]
async fn a_recorded_host_key_is_accepted_through_the_handshake() {
    let host_key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).expect("host key");
    let (address, server) = spawn_server_with(host_key.clone(), None).await;
    let known_hosts = temp_known_hosts("known-ok");
    russh::keys::known_hosts::learn_known_hosts_path(
        &address.ip().to_string(),
        address.port(),
        host_key.public_key(),
        &known_hosts.0,
    )
    .expect("record the host key");

    let handle = connect_strict(address, &known_hosts, false).await;
    assert!(handle.is_ok(), "a recorded key must connect");
    server.abort();
}

/// An unknown host key is refused with `UnknownHostKey` under the strict policy.
#[tokio::test]
async fn an_unrecorded_host_key_is_unknown_through_the_handshake() {
    let host_key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).expect("host key");
    let (address, server) = spawn_server_with(host_key, None).await;
    let known_hosts = temp_known_hosts("unknown");

    match connect_strict(address, &known_hosts, false).await {
        Err(SshHandlerError::UnknownHostKey { fingerprint, .. }) => {
            assert!(
                fingerprint.starts_with("SHA256:"),
                "a bare key must report a plain fingerprint, got {fingerprint}"
            );
        }
        other => panic!("expected UnknownHostKey, got {:?}", other.err()),
    }
    server.abort();
}

/// A *different* key for a host that is already recorded is a mismatch, not an
/// "unknown host": the user must never get the friendly first-use prompt.
#[tokio::test]
async fn a_changed_host_key_is_refused_as_changed_not_unknown() {
    let recorded = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).expect("recorded key");
    let impostor = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).expect("impostor key");
    let (address, server) = spawn_server_with(impostor, None).await;
    let known_hosts = temp_known_hosts("changed");
    russh::keys::known_hosts::learn_known_hosts_path(
        &address.ip().to_string(),
        address.port(),
        recorded.public_key(),
        &known_hosts.0,
    )
    .expect("record the original key");

    match connect_strict(address, &known_hosts, false).await {
        Err(SshHandlerError::ChangedHostKey { .. }) => {}
        other => panic!("expected ChangedHostKey, got {:?}", other.err()),
    }
    server.abort();
}

/// The certificate arm must refuse even when the certificate's *inner* key is
/// the one recorded in known_hosts. A fall-through to the inner key would make
/// this connect succeed, which is exactly the mistake the arm exists to prevent.
#[tokio::test]
async fn a_host_certificate_is_refused_even_when_its_inner_key_is_trusted() {
    let host_key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).expect("host key");
    let ca = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).expect("ca key");
    let certificate = host_certificate(&host_key, &ca);
    let (address, server) = spawn_server_with(host_key.clone(), Some(certificate)).await;

    let known_hosts = temp_known_hosts("cert");
    russh::keys::known_hosts::learn_known_hosts_path(
        &address.ip().to_string(),
        address.port(),
        host_key.public_key(),
        &known_hosts.0,
    )
    .expect("record the inner key");

    // The variant is `HostCertificate`, not `UnknownHostKey`: see the assertion
    // on `to_app_error` below. The original verification asserted
    // `UnknownHostKey` because that is what a33a994 shipped; defect D8 of the
    // verification is precisely that the unknown-key path shows a first-use
    // approval dialog whose Accept can never succeed. The substance of the
    // check — refused, and reported as a certificate — is unchanged.
    let error = match connect_strict(address, &known_hosts, true).await {
        Err(error) => error,
        Ok(_) => panic!("a host certificate must never be accepted"),
    };
    match &error {
        SshHandlerError::HostCertificate {
            fingerprint,
            algorithm,
            ..
        } => {
            assert!(
                fingerprint.starts_with("cert:"),
                "a certificate must be reported as such, got {fingerprint}"
            );
            assert!(
                algorithm.contains("ed25519"),
                "unexpected algorithm {algorithm}"
            );
        }
        other => panic!("expected HostCertificate, got {other:?}"),
    }

    // D8: it must reach the UI as a plain connect error. `AppError::HostKeyUnknown`
    // is what opens `open_host_key_confirmation`, and approving a certificate
    // retries with `AcceptNewFingerprint("cert:…")`, which the arm above rejects
    // again before any policy check — an Accept button that can only fail.
    match error.to_app_error() {
        oneterm_core::AppError::Connect { message, .. } => {
            assert!(
                message.contains("host certificate"),
                "the message must name host certificates: {message}"
            );
        }
        other => panic!("a certificate must not open the host-key dialog, got {other:?}"),
    }
    server.abort();
}

// ---------------------------------------------------------------------------
// Private-key file parsing
// ---------------------------------------------------------------------------

struct TempFile(std::path::PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn temp_file(name: &str, contents: &str) -> TempFile {
    let path =
        std::env::temp_dir().join(format!("oneterm-verify-key-{}-{name}", std::process::id()));
    std::fs::write(&path, contents).expect("write temp key");
    TempFile(path)
}

/// An OpenSSH key encrypted with `aes256-ctr` + bcrypt (what `ssh-keygen -N`
/// writes) loads with its passphrase. `PrivateKey::encrypt` uses that same
/// cipher/KDF pair, so this exercises the bcrypt-pbkdf and AES-CTR decrypt
/// path that moved with the bump.
#[test]
fn an_aes256_ctr_bcrypt_rsa_key_loads_with_its_passphrase() {
    // A 2048-bit RSA key generated once by `ssh-keygen -t rsa -b 2048`, then
    // re-encrypted here so the fixture stays small. RSA generation is far too
    // slow for a debug-build test, hence the fixture.
    let plain = russh::keys::PrivateKey::from_openssh(RSA_2048_OPENSSH).expect("decode fixture");
    assert!(plain.algorithm().is_rsa());
    let encrypted = plain
        .encrypt(&mut rand::rng(), "hunter2")
        .expect("encrypt with aes256-ctr + bcrypt");
    let encoded = encrypted
        .to_openssh(russh::keys::ssh_key::LineEnding::LF)
        .expect("encode");
    let file = temp_file("rsa-enc", &encoded);

    let loaded = load_private_key(&file.0, Some("hunter2")).expect("load the encrypted RSA key");
    assert_eq!(loaded.public_key(), plain.public_key());
}

/// A PKCS#8 PEM ed25519 key (`-----BEGIN PRIVATE KEY-----`) loads.
///
/// The fixture is the Ed25519 private key from RFC 8410 section 10.3 — a
/// published test vector, not a secret.
#[test]
fn a_pkcs8_pem_ed25519_key_loads() {
    const PKCS8_ED25519: &str = "\
-----BEGIN PRIVATE KEY-----
MC4CAQAwBQYDK2VwBCIEINTuctv5E1hK1bbY8fdp+K06/nwoy/HU++CXqI9EdVhC
-----END PRIVATE KEY-----
";
    let file = temp_file("pkcs8", PKCS8_ED25519);
    match load_private_key(&file.0, None) {
        Ok(loaded) => assert!(
            matches!(loaded.algorithm(), Algorithm::Ed25519),
            "unexpected algorithm {}",
            loaded.algorithm()
        ),
        Err(error) => panic!("PKCS#8 PEM ed25519 key rejected: {error}"),
    }
}

/// A key file written with CRLF line endings (a Windows editor, or a key pasted
/// through a Windows tool). Records what OneTerm actually does: russh's
/// `decode_secret_key` compares whole lines without trimming `\r`, and that
/// file is byte-identical between 0.61.2 and 0.63.3, so whatever this shows is
/// not a regression of the bump.
#[test]
fn a_key_file_with_crlf_line_endings_records_its_outcome() {
    let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).expect("ed25519 key");
    let crlf = key
        .to_openssh(russh::keys::ssh_key::LineEnding::CRLF)
        .expect("encode with CRLF");
    assert!(crlf.contains("\r\n"), "the fixture must really be CRLF");
    let file = temp_file("crlf", &crlf);

    match load_private_key(&file.0, None) {
        Ok(loaded) => {
            assert_eq!(loaded.public_key(), key.public_key());
            eprintln!("US-0095 verify: a CRLF OpenSSH key file LOADS");
        }
        Err(error) => eprintln!("US-0095 verify: a CRLF OpenSSH key file is REFUSED: {error}"),
    }
}

/// An empty passphrase on an encrypted key is an error, not a load of the
/// still-encrypted material. `Some("")` is what the connect dialog sends when
/// the user leaves the passphrase box blank.
#[test]
fn an_empty_passphrase_on_an_encrypted_key_is_refused() {
    let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).expect("ed25519 key");
    let encrypted = key
        .encrypt(&mut rand::rng(), "correct horse")
        .expect("encrypt");
    let encoded = encrypted
        .to_openssh(russh::keys::ssh_key::LineEnding::LF)
        .expect("encode");
    let file = temp_file("empty-pass", &encoded);

    let error = load_private_key(&file.0, Some(""))
        .expect_err("an empty passphrase must not decrypt the key");
    assert!(
        error.to_string().contains("failed to load key"),
        "unexpected error text: {error}"
    );
}

/// A truncated key file is an error, never a panic.
#[test]
fn a_truncated_key_file_errors_rather_than_panics() {
    let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).expect("ed25519 key");
    let encoded = key
        .to_openssh(russh::keys::ssh_key::LineEnding::LF)
        .expect("encode");
    let truncated = &encoded[..encoded.len() / 2];
    let file = temp_file("truncated", truncated);

    let error = load_private_key(&file.0, None).expect_err("a truncated key must not load");
    assert!(
        error.to_string().contains("failed to load key"),
        "unexpected error text: {error}"
    );
}

/// The fixture key, reused from `keyfile_tests.rs`.
const RSA_2048_OPENSSH: &str = "\
-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAABFwAAAAdzc2gtcn
NhAAAAAwEAAQAAAQEA0KjRE4L615cP1aCVYQ0XQ0JnZoGSFHEt1K1qRnwZJ2Y9HRwpLpJd
2FDpe4gLODbDMC6EFuUvMRqz/n0Bb6JMCPTGnDPQrlefAh3smPtdbOICSMIdSyVu4qfoRr
a+6J/FpzPHlQlVRFd+WmczrPXMrK1nJorAxxCf2CAVkEmsSeHhR+L6cwoG4beDKcrCEdgJ
6uZAdJUkoX6kLmqLbEd5041ZcgaDjpQJF2K+39kRscgLYKgT3UdQ3yAK7ByYfW88NFzzIL
9WQ4nQ4Pp8RsPiI3oWaMPoqhs2insurX5G+vzuWhqr8pnPjRlhz3LFbyOT6qDDVyU8mzQp
28iiBOBjqwAAA8hrmam6a5mpugAAAAdzc2gtcnNhAAABAQDQqNETgvrXlw/VoJVhDRdDQm
dmgZIUcS3UrWpGfBknZj0dHCkukl3YUOl7iAs4NsMwLoQW5S8xGrP+fQFvokwI9MacM9Cu
V58CHeyY+11s4gJIwh1LJW7ip+hGtr7on8WnM8eVCVVEV35aZzOs9cysrWcmisDHEJ/YIB
WQSaxJ4eFH4vpzCgbht4MpysIR2Anq5kB0lSShfqQuaotsR3nTjVlyBoOOlAkXYr7f2RGx
yAtgqBPdR1DfIArsHJh9bzw0XPMgv1ZDidDg+nxGw+IjehZow+iqGzaKey6tfkb6/O5aGq
vymc+NGWHPcsVvI5PqoMNXJTybNCnbyKIE4GOrAAAAAwEAAQAAAQAGQzrbOhArTlZkVAiH
vCvZkfGmivcGdAsrGfVZnjnnC9ODvyehRTVZ27vWQFQN4N7k4FCIm2JaN/H1Dm1vm1Bq6G
XZpFh8ExcrqhhC0zCPpwzogCL+8WWtmdqH3M5IDxuQlCZGW9xaS8H4FqbfZxU4jY/OAVYd
42rYwsXC6eMo6HfJQi7+JPhwEKpaGJ4K6i94yfKm7H2SczUDugXtJy7FNdeljGxsJF8eq8
l5B7nkUP/hK9BEWwJKtQKgm2FK+oC+MsownsAAbnb5b9QKbRMKzSPhszECRkRg3rPinuwY
vSNPOQ/SRI0jAUjezKVxYVssL6QyHGRJPpy85QLbY7yRAAAAgFNxXdtbKeA8KRJ4bQHAoR
2xTJclMDJt4Bppf44th6+nqObaT2o0yAD6SayX/mCMOreO8r3FfsxR6cU4Z+lasscfusR7
4IXaDh9rr+srNSrbfEm9MVsCzMtx+8mO79b+IlANWUBSiDsbkhX8WjV4U9OyBBe40785hy
czctvcXnd6AAAAgQDrRifWtkF5FBGwR+P8aPKPyAWh+yCxHEZJzatFBO/D9kXKFz7M+DbV
ze6ZbrVeFmvIHmIpXW8qYJtIRM/cc5bn7y4UlYKKBF+C+8/TZb3BJYp0MClbX11VVh39Kn
DeoqX1RLU+iTU148OVMv9MJeAfEokt3jglxv/x4I5i8kdMOQAAAIEA4wp0b9S8Pon1KM+N
/lWz1zHrfnmRBOcep029oNVNkaHaQTccUBZ0uRKsezufe2CtlFGKUQiX50eOm6i6+4D03i
Kf6rlymRM+r0IeGnbwVVPPRKp5JFUeGpTsQd5PjyxRwsPgJpGPXbijQNJlIEMK5oW7pmx8
VG+c/VlbVg58dwMAAAAMb25ldGVybS10ZXN0AQIDBAUGBw==
-----END OPENSSH PRIVATE KEY-----
";

// ---------------------------------------------------------------------------
// russh-sftp 3.0 read pipelining under OneTerm's seek-per-chunk pattern
// ---------------------------------------------------------------------------

/// Bytes OneTerm asks for per SFTP request (`transfer::pipeline::CHUNK_LEN`).
const ONETERM_CHUNK_LEN: usize = 255 * 1024;

#[derive(Default)]
struct SftpCounters {
    reads: AtomicUsize,
    bytes_read: AtomicUsize,
    writes: AtomicUsize,
    bytes_written: AtomicUsize,
    max_write_packet: AtomicUsize,
}

struct CountingSftpServer {
    file: std::fs::File,
    len: u64,
    reads: Arc<AtomicUsize>,
    bytes_served: Arc<AtomicUsize>,
}

impl russh_sftp::server::Handler for CountingSftpServer {
    type Error = russh_sftp::protocol::StatusCode;

    fn unimplemented(&self) -> Self::Error {
        russh_sftp::protocol::StatusCode::OpUnsupported
    }

    async fn init(
        &mut self,
        _version: u32,
        _extensions: HashMap<String, String>,
    ) -> Result<russh_sftp::protocol::Version, Self::Error> {
        Ok(russh_sftp::protocol::Version::new())
    }

    async fn open(
        &mut self,
        id: u32,
        _filename: String,
        _pflags: russh_sftp::protocol::OpenFlags,
        _attrs: russh_sftp::protocol::FileAttributes,
    ) -> Result<russh_sftp::protocol::Handle, Self::Error> {
        Ok(russh_sftp::protocol::Handle {
            id,
            handle: "h".to_owned(),
        })
    }

    async fn close(
        &mut self,
        id: u32,
        _handle: String,
    ) -> Result<russh_sftp::protocol::Status, Self::Error> {
        Ok(russh_sftp::protocol::Status {
            id,
            status_code: russh_sftp::protocol::StatusCode::Ok,
            error_message: "Ok".to_owned(),
            language_tag: "en-US".to_owned(),
        })
    }

    async fn read(
        &mut self,
        id: u32,
        _handle: String,
        offset: u64,
        len: u32,
    ) -> Result<russh_sftp::protocol::Data, Self::Error> {
        if offset >= self.len {
            return Err(russh_sftp::protocol::StatusCode::Eof);
        }
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(|_| russh_sftp::protocol::StatusCode::Failure)?;
        let want = (len as u64).min(self.len - offset) as usize;
        let mut data = vec![0u8; want];
        self.file
            .read_exact(&mut data)
            .map_err(|_| russh_sftp::protocol::StatusCode::Failure)?;
        self.bytes_served.fetch_add(data.len(), Ordering::SeqCst);
        Ok(russh_sftp::protocol::Data { id, data })
    }
}

/// OneTerm's striped download (retired by `IN-0037`) seeked to a stripe offset,
/// read one chunk, then
/// seeks elsewhere. russh-sftp 3.0 answers each read by queueing
/// `max_concurrent_reads` (16 by default) read requests ahead of the current
/// offset; a seek clears the queue *locally*, but the requests are already on
/// the wire, so the server serves — and the link carries — read-ahead the
/// client throws away.
///
/// The test asserts data integrity (which holds) and prints the measured
/// amplification for the evidence record.
#[tokio::test]
async fn seek_per_chunk_reads_measure_the_new_pipeline_read_ahead() {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};

    const CHUNKS: usize = 8;
    let total = ONETERM_CHUNK_LEN * CHUNKS;

    let path = std::env::temp_dir().join(format!("oneterm-verify-sftp-{}", std::process::id()));
    let payload: Vec<u8> = (0..total).map(|i| (i % 251) as u8).collect();
    std::fs::write(&path, &payload).expect("write payload");
    let guard = TempFile(path.clone());

    let reads = Arc::new(AtomicUsize::new(0));
    let bytes_served = Arc::new(AtomicUsize::new(0));
    let (client_side, server_side) = tokio::io::duplex(1024 * 1024);
    russh_sftp::server::run(
        server_side,
        CountingSftpServer {
            file: std::fs::File::open(&path).expect("open payload"),
            len: total as u64,
            reads: reads.clone(),
            bytes_served: bytes_served.clone(),
        },
    )
    .await;

    let sftp = russh_sftp::client::SftpSession::new(client_side)
        .await
        .expect("sftp handshake");
    let mut file = sftp.open("payload").await.expect("open remote");

    // The access pattern the retired `copy_striped` used on one handle: seek to
    // the chunk, read exactly one chunk.
    let mut got = Vec::with_capacity(total);
    let mut buffer = vec![0u8; ONETERM_CHUNK_LEN];
    for index in 0..CHUNKS {
        file.seek(SeekFrom::Start((index * ONETERM_CHUNK_LEN) as u64))
            .await
            .expect("seek to the stripe");
        file.read_exact(&mut buffer).await.expect("read one chunk");
        got.extend_from_slice(&buffer);
    }
    assert_eq!(got, payload, "striped reads must reassemble the file");

    let served = bytes_served.load(Ordering::SeqCst);
    let requests = reads.load(Ordering::SeqCst);
    eprintln!(
        "US-0095 verify: {CHUNKS} useful chunks ({total} bytes) cost {requests} server READ \
         requests and {served} bytes served — amplification {:.1}x",
        served as f64 / total as f64
    );
    // Only a sanity floor: the useful bytes must at least have been served.
    assert!(served >= total);
    drop(guard);
}

/// An SFTP server over one real file on disk, counting every READ and WRITE.
struct RoundTripSftpServer {
    path: std::path::PathBuf,
    file: Option<std::fs::File>,
    counters: Arc<SftpCounters>,
}

fn sftp_ok(id: u32) -> russh_sftp::protocol::Status {
    russh_sftp::protocol::Status {
        id,
        status_code: russh_sftp::protocol::StatusCode::Ok,
        error_message: "Ok".to_owned(),
        language_tag: "en-US".to_owned(),
    }
}

impl russh_sftp::server::Handler for RoundTripSftpServer {
    type Error = russh_sftp::protocol::StatusCode;

    fn unimplemented(&self) -> Self::Error {
        russh_sftp::protocol::StatusCode::OpUnsupported
    }

    async fn init(
        &mut self,
        _version: u32,
        _extensions: HashMap<String, String>,
    ) -> Result<russh_sftp::protocol::Version, Self::Error> {
        Ok(russh_sftp::protocol::Version::new())
    }

    async fn open(
        &mut self,
        id: u32,
        _filename: String,
        pflags: russh_sftp::protocol::OpenFlags,
        _attrs: russh_sftp::protocol::FileAttributes,
    ) -> Result<russh_sftp::protocol::Handle, Self::Error> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(pflags.contains(russh_sftp::protocol::OpenFlags::WRITE))
            .create(pflags.contains(russh_sftp::protocol::OpenFlags::CREATE))
            .truncate(pflags.contains(russh_sftp::protocol::OpenFlags::TRUNCATE))
            .open(&self.path)
            .map_err(|_| russh_sftp::protocol::StatusCode::Failure)?;
        self.file = Some(file);
        Ok(russh_sftp::protocol::Handle {
            id,
            handle: "h".to_owned(),
        })
    }

    async fn close(
        &mut self,
        id: u32,
        _handle: String,
    ) -> Result<russh_sftp::protocol::Status, Self::Error> {
        self.file = None;
        Ok(sftp_ok(id))
    }

    async fn read(
        &mut self,
        id: u32,
        _handle: String,
        offset: u64,
        len: u32,
    ) -> Result<russh_sftp::protocol::Data, Self::Error> {
        let file = self
            .file
            .as_mut()
            .ok_or(russh_sftp::protocol::StatusCode::BadMessage)?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|_| russh_sftp::protocol::StatusCode::Failure)?;
        let mut data = vec![0u8; len as usize];
        let mut filled = 0;
        while filled < data.len() {
            let read = file
                .read(&mut data[filled..])
                .map_err(|_| russh_sftp::protocol::StatusCode::Failure)?;
            if read == 0 {
                break;
            }
            filled += read;
        }
        if filled == 0 {
            return Err(russh_sftp::protocol::StatusCode::Eof);
        }
        data.truncate(filled);
        self.counters.reads.fetch_add(1, Ordering::SeqCst);
        self.counters
            .bytes_read
            .fetch_add(data.len(), Ordering::SeqCst);
        Ok(russh_sftp::protocol::Data { id, data })
    }

    async fn write(
        &mut self,
        id: u32,
        _handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<russh_sftp::protocol::Status, Self::Error> {
        use std::io::Write as _;
        let file = self
            .file
            .as_mut()
            .ok_or(russh_sftp::protocol::StatusCode::BadMessage)?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|_| russh_sftp::protocol::StatusCode::Failure)?;
        file.write_all(&data)
            .map_err(|_| russh_sftp::protocol::StatusCode::Failure)?;
        self.counters.writes.fetch_add(1, Ordering::SeqCst);
        self.counters
            .bytes_written
            .fetch_add(data.len(), Ordering::SeqCst);
        self.counters
            .max_write_packet
            .fetch_max(data.len(), Ordering::SeqCst);
        Ok(sftp_ok(id))
    }

    async fn fstat(
        &mut self,
        id: u32,
        _handle: String,
    ) -> Result<russh_sftp::protocol::Attrs, Self::Error> {
        let meta =
            std::fs::metadata(&self.path).map_err(|_| russh_sftp::protocol::StatusCode::Failure)?;
        Ok(russh_sftp::protocol::Attrs {
            id,
            attrs: russh_sftp::protocol::FileAttributes::from(&meta),
        })
    }
}

/// A 5 MiB upload and download through russh-sftp 3.0, byte-compared, with the
/// per-packet write size recorded.
///
/// 3.0 caps every `SSH_FXP_WRITE` at `Config::max_write_packet_len` (32 KiB)
/// minus the packet overhead; 2.3.0 had no such cap and sent one packet per
/// `write_all` chunk (OneTerm's `CHUNK_LEN` is 255 KiB). The in-flight write
/// budget is `max_concurrent_writes x packet size`, so it went from
/// 8 x 261 120 B down to 16 x ~32 735 B — a 4x reduction, not the doubling the
/// packet describes.
#[tokio::test]
async fn a_five_mib_round_trip_matches_and_records_the_write_packet_budget() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    const SIZE: usize = 5 * 1024 * 1024;
    let payload: Vec<u8> = (0..SIZE).map(|i| (i % 253) as u8).collect();
    let path =
        std::env::temp_dir().join(format!("oneterm-verify-roundtrip-{}", std::process::id()));
    std::fs::write(&path, b"").expect("create remote file");
    let guard = TempFile(path.clone());

    let counters = Arc::new(SftpCounters::default());
    let (client_side, server_side) = tokio::io::duplex(1024 * 1024);
    russh_sftp::server::run(
        server_side,
        RoundTripSftpServer {
            path: path.clone(),
            file: None,
            counters: counters.clone(),
        },
    )
    .await;

    let sftp = russh_sftp::client::SftpSession::new(client_side)
        .await
        .expect("sftp handshake");

    // Upload: the write path OneTerm's `copy_sequential` drives, same chunk size.
    let mut remote = sftp
        .open_with_flags(
            "payload",
            russh_sftp::protocol::OpenFlags::CREATE
                | russh_sftp::protocol::OpenFlags::TRUNCATE
                | russh_sftp::protocol::OpenFlags::WRITE,
        )
        .await
        .expect("open for write");
    for chunk in payload.chunks(ONETERM_CHUNK_LEN) {
        remote.write_all(chunk).await.expect("upload chunk");
    }
    remote.shutdown().await.expect("flush and close");

    let uploaded = std::fs::read(&path).expect("read what the server stored");
    assert_eq!(uploaded.len(), SIZE, "the upload must be complete");
    assert_eq!(uploaded, payload, "the upload must be byte-identical");

    // Download: straight sequential read back, no seeks.
    let mut remote = sftp.open("payload").await.expect("open for read");
    let mut got = Vec::with_capacity(SIZE);
    remote
        .read_to_end(&mut got)
        .await
        .expect("download the file");
    assert_eq!(got, payload, "the download must be byte-identical");

    eprintln!(
        "US-0095 verify: 5 MiB round trip OK. uploaded in {} WRITE packets \
         (largest {} bytes, {} bytes total); downloaded in {} READ packets ({} bytes).",
        counters.writes.load(Ordering::SeqCst),
        counters.max_write_packet.load(Ordering::SeqCst),
        counters.bytes_written.load(Ordering::SeqCst),
        counters.reads.load(Ordering::SeqCst),
        counters.bytes_read.load(Ordering::SeqCst),
    );
    drop(guard);
}
