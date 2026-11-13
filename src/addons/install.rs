//! Installed addons: listing, installing from a `.zip`, removing, enabling.

use super::manifest::{self, Manifest};
use anyhow::{bail, ensure, Context, Result};
use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};

const ENABLED_FILE: &str = "enabled.json";
/// The folder inside an addon that is kept when the addon is updated.
pub const DATA_FOLDER: &str = "data";
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
        self.recover_interrupted_updates();
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
        // On Windows and macOS, `A.txt` and `a.txt` are the same file: a package relying on
        // the difference could install files other than the ones that were checked.
        let mut names = std::collections::HashSet::new();
        for name in zip.file_names() {
            ensure!(
                names.insert(name.trim_end_matches('/').to_lowercase()),
                "The addon package has files whose names differ only in letter case"
            );
        }
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
            // Move the old version aside first, so a failed swap can put it back.
            let destination = self.directory.join(&manifest.id);
            let previous = self.directory.join(format!(".previous-{}", manifest.id));
            self.recover_interrupted_updates();
            if previous.exists() {
                std::fs::remove_dir_all(&previous)?;
            }
            let replacing = destination.exists();
            if replacing {
                std::fs::rename(&destination, &previous).context(
                    "Could not replace the installed version; stop the addon and try again",
                )?;
            }
            // The addon's own `data` folder (its settings, downloads) carries over to the
            // new version unless the package brings one.
            let old_data = previous.join(DATA_FOLDER);
            let new_data = staging.join(DATA_FOLDER);
            let carried = replacing && old_data.is_dir() && !new_data.exists();
            if carried {
                if let Err(error) = std::fs::rename(&old_data, &new_data) {
                    let _ = std::fs::rename(&previous, &destination);
                    return Err(error).context("Could not keep the addon's data");
                }
            }
            if let Err(error) = std::fs::rename(&staging, &destination) {
                if carried {
                    let _ = std::fs::rename(&new_data, &old_data);
                }
                if replacing {
                    let _ = std::fs::rename(&previous, &destination);
                }
                return Err(error).context("Could not move the new version into place");
            }
            if replacing {
                // The new version is in place; leftovers here are cleaned up next time.
                let _ = std::fs::remove_dir_all(&previous);
            }
            Ok(())
        })();
        if result.is_err() && staging.exists() {
            let _ = std::fs::remove_dir_all(&staging);
        }
        result?;
        Ok(manifest)
    }

    /// An update stopped between its two renames leaves only `.previous-<id>`: put that
    /// version back, so the addon is never lost.
    fn recover_interrupted_updates(&self) {
        let Ok(entries) = std::fs::read_dir(&self.directory) else {
            return;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            let Some(id) = name.strip_prefix(".previous-") else {
                continue;
            };
            if manifest::valid_id(id) && !self.directory.join(id).exists() {
                let _ = std::fs::rename(entry.path(), self.directory.join(id));
            }
        }
    }

    /// Delete an addon's folder and forget whether it was enabled.
    pub fn remove(&self, id: &str) -> Result<()> {
        // A listed folder whose name is not a valid ID can still be removed, as long as the
        // name is one plain folder inside the addons folder.
        let plain = !id.is_empty()
            && !id.starts_with('.')
            && Path::new(id)
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
            && Path::new(id).components().count() == 1;
        let folder = if manifest::valid_id(id) {
            self.folder(id)?
        } else if plain {
            self.directory.join(id)
        } else {
            bail!("Invalid addon ID");
        };
        if folder.is_dir() {
            std::fs::remove_dir_all(&folder)
                .context("Could not remove the addon; is it still running?")?;
        }
        if manifest::valid_id(id) {
            self.set_enabled(id, false)?;
        }
        Ok(())
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
        if !manifest::valid_id(id) {
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
