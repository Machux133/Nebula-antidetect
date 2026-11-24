//! Local backups of a stopped session's browser data (the original keeps up to 10 per session in its cloud).
//! A backup is a copy of the files that make a profile "logged in": cookies, local storage, IndexedDB, service-worker
//! registrations, history, saved passwords, bookmarks, extension settings and the key that decrypts the cookies. Caches are left out.

use crate::model::{now, profile_dir};
use std::path::{Path, PathBuf};

pub const MAX: usize = 10;

/// Paths inside the user-data folder (`<profile>/data`). `Local State` holds the key the cookie and password
/// databases are encrypted with, so it must travel with them.
const ITEMS: &[&str] = &[
    "Local State",
    "Default/Preferences",
    "Default/Bookmarks",
    "Default/Cookies",
    "Default/Cookies-journal",
    "Default/Network/Cookies",
    "Default/Network/Cookies-journal",
    "Default/Local Storage",
    "Default/Session Storage",
    "Default/IndexedDB",
    "Default/Service Worker/Database",
    "Default/History",
    "Default/History-journal",
    "Default/Login Data",
    "Default/Login Data-journal",
    "Default/Web Data",
    "Default/Local Extension Settings",
    "Default/Sync Extension Settings",
];

pub struct Info {
    pub id: u64,
    pub bytes: u64,
}

fn root(uuid: &str) -> PathBuf {
    profile_dir(uuid).join("backups")
}

fn data(uuid: &str) -> PathBuf {
    profile_dir(uuid).join("data")
}

fn size(p: &Path) -> u64 {
    match std::fs::metadata(p) {
        Ok(m) if m.is_dir() => std::fs::read_dir(p).into_iter().flatten().flatten().map(|e| size(&e.path())).sum(),
        Ok(m) => m.len(),
        Err(_) => 0,
    }
}

fn copy(from: &Path, to: &Path) -> std::io::Result<()> {
    if from.is_dir() {
        std::fs::create_dir_all(to)?;
        for e in std::fs::read_dir(from)? {
            let e = e?;
            copy(&e.path(), &to.join(e.file_name()))?;
        }
    } else {
        if let Some(d) = to.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::copy(from, to)?;
    }
    Ok(())
}

fn remove(p: &Path) {
    if p.is_dir() {
        let _ = std::fs::remove_dir_all(p);
    } else {
        let _ = std::fs::remove_file(p);
    }
}

/// Backups of a session, newest first.
pub fn list(uuid: &str) -> Vec<Info> {
    let mut v: Vec<Info> = std::fs::read_dir(root(uuid))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| Some(Info { id: e.file_name().to_str()?.parse().ok()?, bytes: size(&e.path()) }))
        .collect();
    v.sort_by(|a, b| b.id.cmp(&a.id));
    v
}

/// Copies the session's browser data into a new backup and drops the oldest beyond `MAX`. Returns the backup's id (its time).
pub fn create(uuid: &str) -> Result<u64, String> {
    create_at(uuid, now())
}

fn create_at(uuid: &str, mut id: u64) -> Result<u64, String> {
    let src = data(uuid);
    if !src.is_dir() {
        return Err("This session has no browser data yet: start it first.".into());
    }
    while root(uuid).join(id.to_string()).exists() {
        id += 1;
    }
    let dst = root(uuid).join(id.to_string());
    let res = ITEMS.iter().filter(|rel| src.join(rel).exists()).try_for_each(|rel| copy(&src.join(rel), &dst.join(rel)));
    if let Err(e) = res {
        let _ = std::fs::remove_dir_all(&dst); // no half backup
        return Err(format!("Backup failed: {e}"));
    }
    if !dst.is_dir() {
        return Err("Nothing to back up yet: the browser has not saved any data.".into());
    }
    for old in list(uuid).into_iter().skip(MAX) {
        let _ = std::fs::remove_dir_all(root(uuid).join(old.id.to_string()));
    }
    Ok(id)
}

/// Puts a backup's files back. The new files are copied next to the live ones first, so a failure leaves the session as it was;
/// files the backup does not have are removed (the session returns to that moment).
pub fn restore(uuid: &str, id: u64) -> Result<(), String> {
    let (src, live) = (root(uuid).join(id.to_string()), data(uuid));
    if !src.is_dir() {
        return Err("No such backup".into());
    }
    let stage = live.join(".restore-stage");
    remove(&stage);
    let res = ITEMS.iter().filter(|rel| src.join(rel).exists()).try_for_each(|rel| copy(&src.join(rel), &stage.join(rel)));
    if let Err(e) = res {
        remove(&stage);
        return Err(format!("Restore failed, nothing was changed: {e}"));
    }
    for rel in ITEMS {
        let (target, new) = (live.join(rel), stage.join(rel));
        remove(&target);
        if new.exists() {
            if let Some(d) = target.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            std::fs::rename(&new, &target).map_err(|e| format!("Could not put back {rel}: {e}"))?;
        }
    }
    remove(&stage);
    Ok(())
}

pub fn delete(uuid: &str, id: u64) {
    let _ = std::fs::remove_dir_all(root(uuid).join(id.to_string()));
}

pub fn human(bytes: u64) -> String {
    match bytes {
        0..=1023 => format!("{bytes} B"),
        1024..=1_048_575 => format!("{:.0} KB", bytes as f64 / 1024.0),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(p: PathBuf, t: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, t).unwrap();
    }

    #[test]
    fn backup_restore_roundtrip_and_limit() {
        let uuid = format!("test-backup-{}", uuid::Uuid::new_v4());
        let d = data(&uuid);
        write(d.join("Default/Cookies"), "cookies v1");
        write(d.join("Default/Local Storage/leveldb/000003.log"), "ls v1");
        write(d.join("Default/Cache/junk"), "cache is not backed up");
        write(d.join("Local State"), "key");
        let b1 = create_at(&uuid, 1000).unwrap();
        assert!(!root(&uuid).join(b1.to_string()).join("Default/Cache").exists(), "caches stay out");
        // The profile moves on: a changed file, a deleted one and a new one.
        write(d.join("Default/Cookies"), "cookies v2");
        std::fs::remove_dir_all(d.join("Default/Local Storage")).unwrap();
        write(d.join("Default/History"), "history made after the backup");
        restore(&uuid, b1).unwrap();
        assert_eq!(std::fs::read_to_string(d.join("Default/Cookies")).unwrap(), "cookies v1");
        assert_eq!(std::fs::read_to_string(d.join("Default/Local Storage/leveldb/000003.log")).unwrap(), "ls v1");
        assert!(!d.join("Default/History").exists(), "a file the backup lacks is removed");
        assert!(d.join("Default/Cache/junk").exists(), "caches are not touched by a restore");
        assert!(!d.join(".restore-stage").exists());
        // Same second: ids stay unique, and only the newest MAX are kept.
        for _ in 0..12 {
            create_at(&uuid, 1000).unwrap();
        }
        let l = list(&uuid);
        assert_eq!(l.len(), MAX);
        assert!(l.windows(2).all(|w| w[0].id > w[1].id), "newest first");
        assert!(l.iter().all(|b| b.bytes > 0));
        assert!(restore(&uuid, 999_999).is_err());
        delete(&uuid, l[0].id);
        assert_eq!(list(&uuid).len(), MAX - 1);
        let _ = std::fs::remove_dir_all(profile_dir(&uuid));
    }

    #[test]
    fn nothing_to_back_up() {
        let uuid = format!("test-backup-{}", uuid::Uuid::new_v4());
        assert!(create(&uuid).is_err(), "no data folder");
        write(data(&uuid).join("Default/Cache/junk"), "x");
        assert!(create(&uuid).is_err(), "only caches");
        assert!(list(&uuid).is_empty(), "no empty backup is left behind");
        let _ = std::fs::remove_dir_all(profile_dir(&uuid));
    }
}
