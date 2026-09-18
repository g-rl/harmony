use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};

use crate::hm::game::{Fingerprint, Mount, Progress, Support, Title, TitleId};
use crate::hm::pack::store::Store;
use crate::hm::pack::{PackageSet, kapi, oodle::Oodle};
use crate::hm::zone::{ZoneSet, jup as pools};

pub struct Jup;

const ROOTS: &[&str] = &["zone", "cod23"];
const CONTAINERS: &[&str] = &["xsub", "xpak"];

pub fn packages_in(root: &Path, roots: &[&str], containers: &[&str]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for folder in roots {
        let Ok(entries) = std::fs::read_dir(root.join(folder)) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let matches = path
                .extension()
                .map(|e| containers.iter().any(|c| e == *c))
                .unwrap_or(false);
            if matches {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

pub fn fastfiles_in(root: &Path, roots: &[&str]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for folder in roots {
        let Ok(entries) = std::fs::read_dir(root.join(folder)) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "ff").unwrap_or(false) {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

pub fn build_from_bootstrap(root: &Path) -> Option<String> {
    let raw = std::fs::read(root.join("bootstrap.data.bin")).ok()?;
    let mut version: Option<String> = None;
    let mut branch: Option<String> = None;
    let mut run = Vec::new();
    for byte in raw.iter().copied().chain(std::iter::once(0)) {
        if byte.is_ascii_graphic() || byte == b' ' {
            run.push(byte);
            continue;
        }
        if run.len() >= 7 {
            let text = String::from_utf8_lossy(&run).to_string();
            let dots = text.chars().filter(|c| *c == '.').count();
            let digits = text.chars().filter(|c| c.is_ascii_digit()).count();
            if dots >= 2 && digits >= 3 && text.len() <= 24 && version.is_none() {
                version = Some(text.clone());
            }
            // Failing a version number, the branch the build came off, such as
            // "zprx-randgrid-jup-rel", still tells one install from another.
            if branch.is_none()
                && text.len() <= 48
                && text.matches('-').count() >= 2
                && !text.contains('.')
            {
                branch = Some(text);
            }
        }
        run.clear();
    }
    version.or(branch)
}

pub fn kapi_survey(root: &Path, roots: &[&str], containers: &[&str]) -> (usize, Option<u16>) {
    let paths = packages_in(root, roots, containers);
    let mut version = None;
    let mut count = 0usize;
    for path in paths.iter().take(64) {
        if let Some(header) = kapi::peek(path) {
            count += 1;
            version.get_or_insert(header.version);
        }
    }
    (paths.len().max(count), version)
}

impl Title for Jup {
    fn id(&self) -> TitleId {
        TitleId::Jup
    }

    fn label(&self) -> &'static str {
        "modern warfare iii"
    }

    fn status(&self) -> Support {
        Support::Verified
    }

    fn sound_roots(&self) -> &'static [&'static str] {
        ROOTS
    }

    fn containers(&self) -> &'static [&'static str] {
        CONTAINERS
    }

    fn fingerprint(&self, root: &Path) -> Option<Fingerprint> {
        if !root.join("cod23").is_dir() {
            return None;
        }
        let (count, version) = kapi_survey(root, ROOTS, CONTAINERS);
        if count == 0 {
            return None;
        }
        let score = match version {
            Some(23) => 100,
            Some(_) => 40,
            None => 20,
        };
        Some(Fingerprint {
            title: TitleId::Jup,
            score,
            roots: ROOTS.iter().map(|r| root.join(r)).collect(),
            reason: format!(
                "cod23\\ and {count} kapi packages, header version {}",
                version.map(|v| v.to_string()).unwrap_or("?".into())
            ),
            build: build_from_bootstrap(root),
        })
    }

    fn mount(&self, root: &Path, report: &mut dyn Progress) -> Result<Mount> {
        let oodle = Oodle::find(root).ok();
        let paths = packages_in(root, ROOTS, CONTAINERS);
        if paths.is_empty() {
            return Err(anyhow!("no packages under zone\\ or cod23\\"));
        }
        let mut packages = PackageSet::default();
        let total = paths.len();
        for (i, path) in paths.iter().enumerate() {
            report.step(i, total, "mounting");
            let _ = packages.mount(path);
        }
        report.step(total, total, "mounted");
        Ok(Mount {
            title: TitleId::Jup,
            root: root.to_path_buf(),
            store: Store::Kapi(packages),
            zones: ZoneSet::default(),
            oodle,
            build: build_from_bootstrap(root),
        })
    }

    fn sound_pool(&self) -> u64 {
        pools::ASSET_SNDASSET
    }
}
