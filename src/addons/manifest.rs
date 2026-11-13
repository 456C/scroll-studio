//! `addon.json`: what an addon is, what it runs and what it may do.

use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

/// The manifest file at the root of every addon folder.
pub const FILE_NAME: &str = "addon.json";
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;

/// What an addon may ask for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    /// Use the read API.
    #[serde(rename = "story.read")]
    StoryRead,
    /// Send edits, which the user sees and applies or cancels.
    #[serde(rename = "story.propose")]
    StoryPropose,
    /// Apply edits without asking. No addon is granted this in 0.14.0.
    #[serde(rename = "story.apply")]
    StoryApply,
}

impl Permission {
    pub fn label(self) -> &'static str {
        match self {
            Permission::StoryRead => "Read your story",
            Permission::StoryPropose => "Suggest changes for you to approve",
            Permission::StoryApply => "Change your story without asking",
        }
    }
}

/// `addon.json`. Unknown keys are ignored so newer addons can add fields.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    /// The oldest Scroll Studio version this addon supports.
    pub min_app: String,
    /// IDs of other addons that must be installed.
    #[serde(default)]
    pub requires: Vec<String>,
    /// The program to start, relative to the addon folder.
    pub entry: String,
    #[serde(default)]
    pub permissions: Vec<Permission>,
    #[serde(default)]
    pub download_mb: f64,
    #[serde(default)]
    pub hardware: String,
}

impl Manifest {
    /// Parse and check a manifest's own fields (not whether its files exist).
    pub fn parse(bytes: &[u8]) -> Result<Manifest> {
        ensure!(
            bytes.len() as u64 <= MAX_MANIFEST_BYTES,
            "addon.json is too large"
        );
        let manifest: Manifest =
            serde_json::from_slice(bytes).context("addon.json is not a valid addon manifest")?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn read(folder: &Path) -> Result<Manifest> {
        let path = folder.join(FILE_NAME);
        let meta = std::fs::metadata(&path).with_context(|| format!("No {FILE_NAME} found"))?;
        ensure!(meta.len() <= MAX_MANIFEST_BYTES, "addon.json is too large");
        Manifest::parse(&std::fs::read(&path)?)
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            valid_id(&self.id),
            "Addon IDs use 1–64 lowercase letters, digits and hyphens"
        );
        ensure!(
            !self.name.trim().is_empty() && self.name.len() <= 64,
            "Addon names use 1–64 characters"
        );
        ensure!(
            parse_version(&self.version).is_some(),
            "Addon version must look like 1.2.3"
        );
        let min_app = parse_version(&self.min_app)
            .context("min_app must be a Scroll Studio version like 0.14.0")?;
        ensure!(
            min_app <= app_version(),
            "This addon needs Scroll Studio {} or newer",
            self.min_app
        );
        for id in &self.requires {
            ensure!(valid_id(id), "Invalid required addon `{id}`");
        }
        entry_path(&self.entry)?;
        if self.permissions.contains(&Permission::StoryApply) {
            bail!("This addon asks to change stories without asking, which Scroll Studio does not allow");
        }
        ensure!(
            self.download_mb.is_finite() && self.download_mb >= 0.0,
            "Invalid download size"
        );
        ensure!(
            self.hardware.len() <= 256,
            "Hardware description is too long"
        );
        Ok(())
    }

    pub fn has(&self, permission: Permission) -> bool {
        self.permissions.contains(&permission)
    }

    /// The program to run, inside `folder`.
    pub fn entry_in(&self, folder: &Path) -> Result<PathBuf> {
        Ok(folder.join(entry_path(&self.entry)?))
    }
}

fn valid_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !id.starts_with('-')
}

/// A relative path that stays inside the addon folder.
fn entry_path(entry: &str) -> Result<PathBuf> {
    let path = Path::new(entry);
    ensure!(
        !entry.is_empty()
            && entry.len() <= 256
            && path.components().all(|c| matches!(c, Component::Normal(_))),
        "The addon's program must be a file inside its folder"
    );
    Ok(path.to_path_buf())
}

/// `major.minor.patch`, ignoring any `-pre` or `+build` suffix.
pub fn parse_version(version: &str) -> Option<(u64, u64, u64)> {
    let core = version.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let version = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(version)
}

pub fn app_version() -> (u64, u64, u64) {
    parse_version(env!("CARGO_PKG_VERSION")).expect("the app version is semantic")
}
