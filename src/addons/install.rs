//! Installed addons: listing, installing from a `.zip`, removing, enabling.

use super::manifest::{self, Manifest};
use anyhow::{bail, ensure, Context, Result};
use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};

const ENABLED_FILE: &str = "enabled.json";
const MAX_PACKAGE_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 20_000;

/// One addon folder, readable or not.
#[derive(Clone, Debug, PartialEq)]
pub struct Installed {
    /// The folder name, which is the addon's ID for a valid addon.
    pub id: String,
    pub folder: PathBuf,
    /// `Err` when the folder's manifest cannot be used; the message says why.
    pub manifest: Result<Manifest, String>,
    pub size_bytes: u64,
    pub enabled: bool,
}

/// The addons folder.
#[derive(Clone, Debug)]
pub struct Registry {
    pub directory: PathBuf,
}

impl Registry {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    /// Every addon folder, sorted by ID. A missing addons folder means none.
    pub fn list(&self) -> Vec<Installed> {
        let Ok(entries) = std::fs::read_dir(&self.directory) else {
            return Vec::new();
        };
        let enabled = self.enabled_ids();
        let mut installed: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter_map(|e| {
                let id = e.file_name().into_string().ok()?;
                // Staging folders from an install in progress are not addons.
                if id.starts_with('.') {
                    return None;
                }
                let folder = e.path();
                let manifest = Manifest::read(&folder)
                    .and_then(|m| {
                        ensure!(m.id == id, "The folder name does not match the addon ID");
                        Ok(m)
                    })
                    .map_err(|e| format!("{e:#}"));
                Some(Installed {
                    enabled: enabled.contains(&id) && manifest.is_ok(),
                    size_bytes: folder_size(&folder),
                    id,
                    folder,
                    manifest,
                })
            })
            .collect();
        installed.sort_by(|a, b| a.id.cmp(&b.id));
        installed
    }

    /// Install (or replace) an addon from a `.zip` whose root holds `addon.json`. The
    /// package is unpacked into a staging folder and only then moved into place, so a
    /// failed install leaves nothing behind. New addons start disabled.
    pub fn install_zip(&self, package: &Path) -> Result<Manifest> {
        let file = std::fs::File::open(package).context("Could not open the addon file")?;
        let mut zip = zip::ZipArchive::new(file).context("This is not an addon package (.zip)")?;
        ensure!(
            zip.len() <= MAX_ENTRIES,
            "The addon package has too many files"
        );
        let manifest = {
            let mut entry = zip
                .by_name(manifest::FILE_NAME)
                .context("The package has no addon.json at its top level")?;
            let mut bytes = Vec::new();
            (&mut entry).take(64 * 1024 + 1).read_to_end(&mut bytes)?;
            Manifest::parse(&bytes)?
        };
        std::fs::create_dir_all(&self.directory).context("Could not create the addons folder")?;
        let staging = self.directory.join(format!(".installing-{}", manifest.id));
        if staging.exists() {
            std::fs::remove_dir_all(&staging)?;
        }
        let result = (|| -> Result<()> {
            let mut total = 0u64;
            for i in 0..zip.len() {
                let mut entry = zip.by_index(i)?;
                // `enclosed_name` refuses absolute paths and `..` components.
                let relative = entry
                    .enclosed_name()
                    .with_context(|| format!("Unsafe path in the package: {}", entry.name()))?;
                let target = staging.join(relative);
                if entry.is_dir() {
                    std::fs::create_dir_all(&target)?;
                    continue;
                }
                let declared = entry.size();
                let budget = MAX_PACKAGE_BYTES - total;
                total = total.saturating_add(declared);
                ensure!(total <= MAX_PACKAGE_BYTES, "The addon package is too large");
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let mut out = std::fs::File::create(&target)?;
                // Never trust the declared size: copy at most what is left of the budget.
                let copied = std::io::copy(&mut (&mut entry).take(budget + 1), &mut out)?;
                ensure!(copied <= declared, "The addon package is corrupt");
                #[cfg(unix)]
                if let Some(mode) = entry.unix_mode() {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(
                        &target,
                        std::fs::Permissions::from_mode(mode & 0o755),
                    )?;
                }
            }
            let entry = manifest.entry_in(&staging)?;
            ensure!(
                entry.is_file(),
                "The addon's program `{}` is missing",
                manifest.entry
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = std::fs::metadata(&entry)?.permissions().mode();
                std::fs::set_permissions(&entry, std::fs::Permissions::from_mode(mode | 0o700))?;
            }
            let destination = self.directory.join(&manifest.id);
            if destination.exists() {
                std::fs::remove_dir_all(&destination)
                    .context("Could not replace the installed version; close it and try again")?;
            }
            std::fs::rename(&staging, &destination)?;
            Ok(())
        })();
        if result.is_err() && staging.exists() {
            let _ = std::fs::remove_dir_all(&staging);
        }
        result?;
        Ok(manifest)
    }

    /// Delete an addon's folder and forget whether it was enabled.
    pub fn remove(&self, id: &str) -> Result<()> {
        let folder = self.folder(id)?;
        if folder.exists() {
            std::fs::remove_dir_all(&folder)
                .context("Could not remove the addon; is it still running?")?;
        }
        self.set_enabled(id, false)
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        self.folder(id)?;
        let mut ids = self.enabled_ids();
        if enabled {
            ids.insert(id.to_owned());
        } else {
            ids.remove(id);
        }
        std::fs::create_dir_all(&self.directory)?;
        let bytes = serde_json::to_vec_pretty(&ids)?;
        let path = self.directory.join(ENABLED_FILE);
        let temp = self.directory.join(format!(".{ENABLED_FILE}.tmp"));
        std::fs::write(&temp, bytes)?;
        std::fs::rename(&temp, &path)?;
        Ok(())
    }

    fn enabled_ids(&self) -> BTreeSet<String> {
        std::fs::read(self.directory.join(ENABLED_FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// The folder for `id`, refusing anything that is not a plain addon ID.
    fn folder(&self, id: &str) -> Result<PathBuf> {
        if id.is_empty() || id.starts_with('.') || id.contains(['/', '\\']) || id.contains("..") {
            bail!("Invalid addon ID");
        }
        Ok(self.directory.join(id))
    }
}

fn folder_size(folder: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return 0;
    };
    entries
        .filter_map(|e| e.ok())
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => folder_size(&e.path()),
            Ok(_) => e.metadata().map(|m| m.len()).unwrap_or(0),
            Err(_) => 0,
        })
        .sum()
}
