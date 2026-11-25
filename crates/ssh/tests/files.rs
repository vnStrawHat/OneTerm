//! The engine's files over SFTP (specs/oneterm, "M9 in detail") against the test server, whose
//! files are a folder of `.tmp`: home, listing in order, download and upload through a part
//! file, a cancelled transfer leaving nothing, a folder made and removed, a path's properties.

use std::sync::atomic::AtomicBool;

use oneterm_ssh::testing::{connect, folder, key, options, serve_files};
use oneterm_ssh::{Auth, Entry, Files};
use russh::keys::known_hosts::learn_known_hosts_path;

/// A connection with its shell open, and its files; the server serves `served`.
fn files(name: &str) -> (oneterm_ssh::testing::Client, Files, std::path::PathBuf) {
    let dir = folder(name);
    let served = dir.join("served");
    std::fs::create_dir_all(served.join("home/src")).unwrap();
    std::fs::write(served.join("home/Makefile"), b"all:\n").unwrap();
    std::fs::write(served.join("home/config.yaml"), vec![b'x'; 200_000]).unwrap();
    let (port, _) = serve_files(7, served.clone());
    learn_known_hosts_path("127.0.0.1", port, key(7).public_key(), dir.join("known_hosts")).unwrap();
    let c = connect(options(port, &dir, Auth::Password));
    c.ask();
    c.answer(Some("secret"));
    c.wait_for("welcome");
    let files = c.ssh.files().expect("SFTP opens");
    (c, files, served)
}

#[test]
fn a_folder_lists_folders_first_and_files_go_both_ways() {
    let (_c, files, served) = files("sftp");
    assert_eq!(files.home().unwrap(), "/home");
    let names: Vec<(String, bool)> =
        files.list("/home").unwrap().into_iter().map(|e| (e.name, e.dir)).collect();
    assert_eq!(names, [("src".into(), true), ("config.yaml".into(), false), ("Makefile".into(), false)]);

    let local = folder("sftp-local").join("config.yaml");
    let seen = std::sync::Mutex::new(Vec::new());
    files
        .download(
            "/home/config.yaml",
            &local,
            |done, whole| seen.lock().unwrap().push((done, whole)),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(std::fs::read(&local).unwrap().len(), 200_000);
    assert_eq!(seen.lock().unwrap().last(), Some(&(200_000, 200_000)));
    assert!(!local.with_file_name("config.yaml.part").exists());

    files.upload(&local, "/home/src/copy.yaml", |_, _| {}, &AtomicBool::new(false)).unwrap();
    assert_eq!(std::fs::read(served.join("home/src/copy.yaml")).unwrap().len(), 200_000);
    // Again, over the one there.
    files.upload(&local, "/home/src/copy.yaml", |_, _| {}, &AtomicBool::new(false)).unwrap();
    let listed = files.list("/home/src").unwrap();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let modified = listed[0].modified;
    assert!(now.abs_diff(modified) < 60, "the mtime is told: {modified}");
    assert_eq!(
        listed,
        [Entry { name: "copy.yaml".into(), dir: false, size: 200_000, modified, mode: 0o100644 }]
    );

    files.mkdir("/home/old").unwrap();
    files.rename("/home/old", "/home/new").unwrap();
    assert!(served.join("home/new").is_dir() && !served.join("home/old").exists());
    files.remove("/home/new", true).unwrap();
    files.remove("/home/src/copy.yaml", false).unwrap();
    assert!(!served.join("home/new").exists() && !served.join("home/src/copy.yaml").exists());
}

#[test]
fn a_cancelled_transfer_leaves_no_part() {
    let (_c, files, served) = files("sftp-cancel");
    let local = folder("sftp-cancel-local").join("config.yaml");
    let cancel = AtomicBool::new(true);
    assert_eq!(files.download("/home/config.yaml", &local, |_, _| {}, &cancel), Err("cancelled".into()));
    assert!(!local.exists() && !local.with_file_name("config.yaml.part").exists());
    std::fs::write(&local, b"x").unwrap();
    assert_eq!(files.upload(&local, "/home/up.txt", |_, _| {}, &cancel), Err("cancelled".into()));
    assert!(!served.join("home/up.txt").exists() && !served.join("home/up.txt.part").exists());
    assert!(files.list("/nowhere").is_err());
}

#[test]
fn a_path_s_properties_name_its_owner_where_the_server_tells() {
    let (_c, files, served) = files("sftp-stat");
    std::fs::create_dir_all(served.join("etc")).unwrap();
    std::fs::write(served.join("etc/passwd"), "root:x:0:0::/root:/bin/sh\ntester:x:1000:1000::/home:\n")
        .unwrap();
    let stat = files.stat("/home/config.yaml").unwrap();
    assert!(!stat.dir && !stat.link && stat.size == 200_000 && stat.mode == 0o100644, "{stat:?}");
    assert_eq!((stat.uid, stat.gid), (Some(1000), Some(1000)));
    // No group table: the id alone.
    assert_eq!((stat.owner.as_deref(), stat.group.as_deref()), (Some("tester"), None));
    assert!(stat.modified > 0 && stat.accessed > 0, "{stat:?}");
    let dir = files.stat("/home/src").unwrap();
    assert!(dir.dir && dir.mode == 0o040755, "{dir:?}");
    assert!(files.stat("/home/nowhere").is_err());
}

#[test]
fn a_folder_s_tree_goes_both_ways_with_the_progress_over_all_of_it() {
    let (_c, files, served) = files("sftp-tree");
    std::fs::create_dir_all(served.join("home/src/deep")).unwrap();
    std::fs::write(served.join("home/src/a.rs"), b"fn main() {}\n").unwrap();
    std::fs::write(served.join("home/src/deep/b.txt"), vec![b'b'; 70_000]).unwrap();
    let local = folder("sftp-tree-local").join("src");
    let last = std::sync::Mutex::new((0, 0));
    let tell = |d, w| *last.lock().unwrap() = (d, w);
    files.download_folder("/home/src", &local, tell, &AtomicBool::new(false)).unwrap();
    assert_eq!(std::fs::read(local.join("a.rs")).unwrap(), b"fn main() {}\n");
    assert_eq!(std::fs::read(local.join("deep/b.txt")).unwrap().len(), 70_000);
    assert_eq!(*last.lock().unwrap(), (70_013, 70_013));

    files.upload_folder(&local, "/home/copy", |_, _| {}, &AtomicBool::new(false)).unwrap();
    assert_eq!(std::fs::read(served.join("home/copy/deep/b.txt")).unwrap().len(), 70_000);
    // Again, into the folder there now.
    files.upload_folder(&local, "/home/copy", |_, _| {}, &AtomicBool::new(false)).unwrap();
    assert_eq!(std::fs::read(served.join("home/copy/a.rs")).unwrap(), b"fn main() {}\n");
}
