//! Real archive fixtures for the extraction guards (TEST-15).

use std::io::Write as _;
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::write::GzEncoder;
use tar::{EntryType, Header};
use zip::write::{SimpleFileOptions, ZipWriter};

use super::*;

const WINDOWS_TARGET: &str = "x86_64-pc-windows-msvc";
const LINUX_TARGET: &str = "x86_64-unknown-linux-gnu";

/// Temporary root holding `archive` (built by the test) and `out` (extraction destination).
struct Fixture {
    root: PathBuf,
    out: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let root = test_dir(name);
        let out = root.join("out");
        std::fs::create_dir_all(&out).unwrap();
        Self { root, out }
    }

    fn zip(&self, build: impl FnOnce(&mut ZipWriter<File>)) -> PathBuf {
        let path = self.root.join("update.zip");
        let mut writer = ZipWriter::new(File::create(&path).unwrap());
        build(&mut writer);
        writer.finish().unwrap();
        path
    }

    fn tar_gz(&self, build: impl FnOnce(&mut tar::Builder<GzEncoder<File>>)) -> PathBuf {
        let path = self.root.join("update.tar.gz");
        let encoder = GzEncoder::new(File::create(&path).unwrap(), Compression::fast());
        let mut builder = tar::Builder::new(encoder);
        build(&mut builder);
        builder.into_inner().unwrap().finish().unwrap();
        path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn zip_options() -> SimpleFileOptions {
    SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored)
}

fn zip_file(writer: &mut ZipWriter<File>, name: &str, bytes: &[u8]) {
    writer.start_file(name, zip_options()).unwrap();
    writer.write_all(bytes).unwrap();
}

fn tar_file(builder: &mut tar::Builder<GzEncoder<File>>, path: &str, bytes: &[u8]) {
    let mut header = Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o755);
    header.set_cksum();
    builder.append_data(&mut header, path, bytes).unwrap();
}

/// `tar::Header::set_path` refuses `..`, so a hostile entry is written straight
/// into the raw 100-byte name field the way a malicious producer would.
fn tar_raw_name_file(builder: &mut tar::Builder<GzEncoder<File>>, raw_name: &[u8], bytes: &[u8]) {
    let mut header = Header::new_gnu();
    header.as_old_mut().name[..raw_name.len()].copy_from_slice(raw_name);
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_entry_type(EntryType::Regular);
    header.set_cksum();
    builder.append(&header, bytes).unwrap();
}

fn assert_extraction_fails(archive: &Path, destination: &Path, needle: &str) {
    let error = extract_archive(archive, destination)
        .expect_err("hostile archive must be rejected")
        .to_string();
    assert!(
        error.contains(needle),
        "expected error mentioning {needle:?}, got {error:?}"
    );
}

#[test]
fn rejects_parent_paths() {
    assert!(reject_unsafe_path(Path::new("../outside")).is_err());
}

#[test]
fn accepts_relative_paths() {
    reject_unsafe_path(Path::new("oneterm-x86_64-unknown-linux-gnu/oneterm")).unwrap();
}

#[test]
fn accepts_versioned_windows_package_dir() {
    let dir = test_dir("versioned-windows");
    let package_dir = dir.join("oneterm-0.3.1-x86_64-pc-windows-msvc");
    std::fs::create_dir_all(&package_dir).unwrap();
    std::fs::write(package_dir.join("oneterm.exe"), b"binary").unwrap();
    let validated = validate_staged_package(&dir, WINDOWS_TARGET).unwrap();
    assert_eq!(validated, package_dir);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn accepts_versioned_windows_package_dir_inside_dist() {
    let dir = test_dir("versioned-windows-dist");
    let package_dir = dir
        .join("dist")
        .join("oneterm-0.3.1-x86_64-pc-windows-msvc");
    std::fs::create_dir_all(&package_dir).unwrap();
    std::fs::write(package_dir.join("oneterm.exe"), b"binary").unwrap();
    let validated = validate_staged_package(&dir, WINDOWS_TARGET).unwrap();
    assert_eq!(validated, package_dir);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn zip_with_nested_dist_layout_extracts_and_validates() {
    let fixture = Fixture::new("zip-nested");
    let package = "dist/oneterm-0.3.1-x86_64-pc-windows-msvc";
    let archive = fixture.zip(|writer| {
        writer
            .add_directory(format!("{package}/x64"), zip_options())
            .unwrap();
        zip_file(writer, &format!("{package}/oneterm.exe"), b"exe");
        zip_file(writer, &format!("{package}/conpty.dll"), b"dll");
        zip_file(
            writer,
            &format!("{package}/x64/OpenConsole.exe"),
            b"console",
        );
    });

    extract_archive(&archive, &fixture.out).unwrap();
    let package_dir = validate_staged_package(&fixture.out, WINDOWS_TARGET).unwrap();

    assert_eq!(package_dir, fixture.out.join(package));
    assert_eq!(
        std::fs::read(package_dir.join("x64").join("OpenConsole.exe")).unwrap(),
        b"console"
    );
}

#[test]
fn zip_slip_entry_is_rejected_before_anything_escapes() {
    let fixture = Fixture::new("zip-slip");
    let archive = fixture.zip(|writer| {
        zip_file(writer, "../evil.txt", b"owned");
    });

    assert_extraction_fails(&archive, &fixture.out, "unsafe path");
    assert!(!fixture.root.join("evil.txt").exists());
}

#[test]
fn zip_absolute_entry_is_rejected() {
    let fixture = Fixture::new("zip-absolute");
    let archive = fixture.zip(|writer| {
        zip_file(writer, "/etc/evil.txt", b"owned");
    });

    assert_extraction_fails(&archive, &fixture.out, "unsafe path");
}

#[test]
fn zip_symlink_entry_is_rejected() {
    let fixture = Fixture::new("zip-symlink");
    let archive = fixture.zip(|writer| {
        zip_file(writer, "oneterm.exe", b"exe");
        writer
            .add_symlink("conpty.dll", "../../outside.dll", zip_options())
            .unwrap();
    });

    assert_extraction_fails(&archive, &fixture.out, "symlink");
    assert!(!fixture.out.join("conpty.dll").exists());
}

#[test]
fn zip_expansion_beyond_budget_is_rejected() {
    let fixture = Fixture::new("zip-budget");
    let archive = fixture.zip(|writer| {
        zip_file(writer, "oneterm.exe", &[0_u8; 4096]);
    });

    let error = extract_archive_with_limit(&archive, &fixture.out, 1024)
        .expect_err("oversized archive must be rejected")
        .to_string();
    assert!(error.contains("beyond"), "{error}");
    extract_archive_with_limit(&archive, &fixture.out, 4096).unwrap();
}

#[test]
fn tar_with_nested_layout_extracts_and_validates() {
    let fixture = Fixture::new("tar-nested");
    let package = "oneterm-0.3.1-x86_64-unknown-linux-gnu";
    let archive = fixture.tar_gz(|builder| {
        tar_file(builder, &format!("{package}/oneterm"), b"elf");
        tar_file(builder, &format!("{package}/README.md"), b"docs");
    });

    extract_archive(&archive, &fixture.out).unwrap();
    let package_dir = validate_staged_package(&fixture.out, LINUX_TARGET).unwrap();

    assert_eq!(package_dir, fixture.out.join(package));
    assert_eq!(std::fs::read(package_dir.join("oneterm")).unwrap(), b"elf");
}

#[test]
fn tar_parent_directory_entry_is_rejected() {
    let fixture = Fixture::new("tar-parent");
    let archive = fixture.tar_gz(|builder| {
        tar_file(builder, "oneterm", b"elf");
        tar_raw_name_file(builder, b"../evil", b"owned");
    });

    assert_extraction_fails(&archive, &fixture.out, "unsafe path");
    assert!(!fixture.root.join("evil").exists());
}

#[test]
fn tar_symlink_entry_is_rejected() {
    let fixture = Fixture::new("tar-symlink");
    let archive = fixture.tar_gz(|builder| {
        let mut header = Header::new_gnu();
        header.set_entry_type(EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o777);
        header.set_link_name("oneterm").unwrap();
        header.set_cksum();
        builder.append_data(&mut header, "link", &[][..]).unwrap();
    });

    assert_extraction_fails(&archive, &fixture.out, "links");
}

#[test]
fn tar_expansion_beyond_budget_is_rejected() {
    let fixture = Fixture::new("tar-budget");
    let archive = fixture.tar_gz(|builder| {
        tar_file(builder, "oneterm", &[0_u8; 4096]);
    });

    let error = extract_archive_with_limit(&archive, &fixture.out, 1024)
        .expect_err("oversized archive must be rejected")
        .to_string();
    assert!(error.contains("beyond"), "{error}");
    extract_archive_with_limit(&archive, &fixture.out, 4096).unwrap();
}

#[test]
fn unsupported_archive_extension_is_rejected() {
    let fixture = Fixture::new("unsupported");
    let archive = fixture.root.join("update.7z");
    std::fs::write(&archive, b"7z").unwrap();

    assert_extraction_fails(&archive, &fixture.out, "unsupported");
}

fn test_dir(name: &str) -> PathBuf {
    // A process-wide sequence keeps directories distinct even when parallel
    // tests read the same coarse timestamp (as on macOS).
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "oneterm-archive-{name}-{}-{nonce}-{sequence}",
        std::process::id()
    ))
}
