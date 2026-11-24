//! Widevine CDM, Google's closed-source DRM module that streaming sites ask for.
//!
//! It is never shipped with Nebula. The bundled Chromium is built with Widevine support (`enable_widevine`), and this
//! module copies the CDM once from the user's own Microsoft Edge or Google Chrome (both keep it next to the
//! application, in `<version>\WidevineCdm`) into the app's data folder, then gives every session a link to it in its
//! own user-data folder, where Chromium's component updater looks for `WidevineCdm/<version>/`.

use serde_json::Value;
use std::path::{Path, PathBuf};

/// A Widevine CDM folder found in an installed browser.
#[derive(Clone, Debug, PartialEq)]
pub struct Cdm {
    pub dir: PathBuf,
    /// From the CDM's manifest.json, e.g. "4.10.3112.1".
    pub version: String,
    /// "Microsoft Edge" or "Google Chrome".
    pub from: &'static str,
}

const DLL: &str = r"_platform_specific\win_x64\widevinecdm.dll";

fn version_key(v: &str) -> Vec<u64> {
    v.split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

/// The highest-version CDM among `<root>/<browser version>/WidevineCdm` folders that have a manifest and the library.
pub fn find_in(roots: &[(PathBuf, &'static str)]) -> Option<Cdm> {
    let mut best: Option<Cdm> = None;
    for (root, from) in roots {
        let Ok(entries) = std::fs::read_dir(root) else { continue };
        for e in entries.flatten() {
            let dir = e.path().join("WidevineCdm");
            if !dir.join(DLL).is_file() {
                continue;
            }
            let Some(version) = std::fs::read_to_string(dir.join("manifest.json"))
                .ok()
                .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                .and_then(|m| m["version"].as_str().map(String::from))
            else {
                continue;
            };
            if best.as_ref().is_none_or(|b| version_key(&version) > version_key(&b.version)) {
                best = Some(Cdm { dir, version, from });
            }
        }
    }
    best
}

/// The CDM of an installed Chrome or Edge on this computer, if any.
pub fn find_source() -> Option<Cdm> {
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    let mut roots = vec![];
    for var in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
        if let Some(b) = env(var) {
            roots.push((b.join(r"Google\Chrome\Application"), "Google Chrome"));
            roots.push((b.join(r"Microsoft\Edge\Application"), "Microsoft Edge"));
        }
    }
    find_in(&roots)
}

fn copy_tree(from: &Path, to: &Path, link: bool) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let target = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_tree(&e.path(), &target, link)?;
        } else if !target.is_file() {
            // A hard link costs no disk space (same volume, folders we own); a copy is the fallback.
            if !(link && std::fs::hard_link(e.path(), &target).is_ok()) {
                std::fs::copy(e.path(), &target)?;
            }
        }
    }
    Ok(())
}

fn installed(dir: &Path) -> bool {
    dir.join("manifest.json").is_file() && dir.join(DLL).is_file()
}

/// Makes the CDM available to a session whose user-data folder is `data_dir`. Returns the version when it is there,
/// `None` when no Edge or Chrome with a CDM is installed. `store` is the app's own copy (made on first use).
pub fn prepare(store: &Path, data_dir: &Path) -> std::io::Result<Option<String>> {
    prepare_from(find_source(), store, data_dir)
}

pub fn prepare_from(source: Option<Cdm>, store: &Path, data_dir: &Path) -> std::io::Result<Option<String>> {
    let Some(cdm) = source else {
        // No browser to copy from; a CDM copied earlier is still good.
        return match newest(store) {
            Some((version, dir)) => link_into(&dir, &version, data_dir).map(|_| Some(version)),
            None => Ok(None),
        };
    };
    let kept = store.join(&cdm.version);
    if !installed(&kept) {
        // Into a temporary folder first, so a half-copied CDM is never picked up.
        let tmp = store.join(format!("{}.tmp", cdm.version));
        let _ = std::fs::remove_dir_all(&tmp);
        copy_tree(&cdm.dir, &tmp, false)?;
        let _ = std::fs::remove_dir_all(&kept);
        std::fs::rename(&tmp, &kept)?;
    }
    link_into(&kept, &cdm.version, data_dir)?;
    Ok(Some(cdm.version))
}

fn newest(store: &Path) -> Option<(String, PathBuf)> {
    let mut v: Vec<(String, PathBuf)> = std::fs::read_dir(store)
        .ok()?
        .flatten()
        .filter(|e| installed(&e.path()))
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
        .collect();
    v.sort_by_key(|(n, _)| version_key(n));
    v.pop()
}

fn link_into(kept: &Path, version: &str, data_dir: &Path) -> std::io::Result<()> {
    let target = data_dir.join("WidevineCdm").join(version);
    if installed(&target) {
        return Ok(());
    }
    copy_tree(kept, &target, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_browser(root: &Path, browser_version: &str, cdm_version: &str) {
        let d = root.join(browser_version).join("WidevineCdm");
        std::fs::create_dir_all(d.join(r"_platform_specific\win_x64")).unwrap();
        std::fs::write(d.join(DLL), b"dll").unwrap();
        std::fs::write(d.join("manifest.json"), format!(r#"{{"version":"{cdm_version}"}}"#)).unwrap();
    }

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("nebula-wv-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn picks_the_highest_cdm_version() {
        let (edge, chrome) = (tmp("e"), tmp("c"));
        fake_browser(&edge, "158.0.1", "4.10.2710.0");
        fake_browser(&chrome, "157.0.2", "4.10.3112.1");
        fake_browser(&chrome, "158.0.3", "4.10.9.1"); // 9 < 3112: compared as numbers, not text
        let c = find_in(&[(edge.clone(), "Microsoft Edge"), (chrome.clone(), "Google Chrome")]).unwrap();
        assert_eq!((c.version.as_str(), c.from), ("4.10.3112.1", "Google Chrome"));
        assert!(find_in(&[(tmp("none"), "Microsoft Edge")]).is_none());
    }

    #[test]
    fn copies_once_and_links_into_each_session() {
        let (edge, store, s1, s2) = (tmp("e2"), tmp("store"), tmp("s1"), tmp("s2"));
        fake_browser(&edge, "158.0.1", "4.10.3112.1");
        let src = find_in(&[(edge, "Microsoft Edge")]);
        assert_eq!(prepare_from(src.clone(), &store, &s1).unwrap().as_deref(), Some("4.10.3112.1"));
        assert!(installed(&s1.join("WidevineCdm").join("4.10.3112.1")));
        assert_eq!(prepare_from(src, &store, &s2).unwrap().as_deref(), Some("4.10.3112.1"));
        assert!(installed(&s2.join("WidevineCdm").join("4.10.3112.1")));
        // The browser is gone: the stored copy still serves new sessions.
        let s3 = tmp("s3");
        assert_eq!(prepare_from(None, &store, &s3).unwrap().as_deref(), Some("4.10.3112.1"));
        assert_eq!(prepare_from(None, &tmp("empty-store"), &tmp("s4")).unwrap(), None);
    }
}
