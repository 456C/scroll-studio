//! Small, local application preferences. They are separate from story content.

use anyhow::{bail, ensure, Context, Result};
use scroll_studio::snapping::SnapSettings;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const MAX_PREFERENCES_BYTES: u64 = 16 * 1024;
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Motion {
    Off,
    Calm,
    #[default]
    #[serde(other)]
    Lively,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Preferences {
    pub glass_enabled: bool,
    pub glass_strength: f32,
    pub motion: Motion,
    pub snap_enabled: bool,
    pub snap_canvas: bool,
    pub snap_objects: bool,
    pub snap_spacing: bool,
    pub snap_threshold_px: f32,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            glass_enabled: true,
            glass_strength: 0.65,
            motion: Motion::Lively,
            snap_enabled: true,
            snap_canvas: true,
            snap_objects: true,
            snap_spacing: true,
            snap_threshold_px: 8.0,
        }
    }
}

impl Preferences {
    pub fn sanitize(&mut self) {
        self.glass_strength = finite_clamp(self.glass_strength, 0.0, 1.0, 0.65);
        self.snap_threshold_px = finite_clamp(self.snap_threshold_px, 3.0, 16.0, 8.0);
    }

    pub fn snap_settings(&self) -> SnapSettings {
        SnapSettings {
            enabled: self.snap_enabled,
            canvas: self.snap_canvas,
            objects: self.snap_objects,
            spacing: self.snap_spacing,
            threshold_px: finite_clamp(self.snap_threshold_px, 3.0, 16.0, 8.0),
        }
    }

    pub fn set_snap_settings(&mut self, settings: &SnapSettings) {
        self.snap_enabled = settings.enabled;
        self.snap_canvas = settings.canvas;
        self.snap_objects = settings.objects;
        self.snap_spacing = settings.spacing;
        self.snap_threshold_px = finite_clamp(settings.threshold_px, 3.0, 16.0, 8.0);
    }
}

fn finite_clamp(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}

/// Resolve a per-user path without making a directory or reading a file.
/// No configuration root means this launch simply uses session preferences.
pub(super) fn default_path() -> Option<PathBuf> {
    let populated = |name| std::env::var_os(name).filter(|value| !value.is_empty());
    if cfg!(target_os = "windows") {
        return populated("LOCALAPPDATA")
            .or_else(|| populated("APPDATA"))
            .map(|path| {
                PathBuf::from(path)
                    .join("Scroll Studio")
                    .join("settings.json")
            });
    }
    if cfg!(target_os = "macos") {
        return populated("HOME").map(|path| {
            PathBuf::from(path)
                .join("Library")
                .join("Application Support")
                .join("Scroll Studio")
                .join("settings.json")
        });
    }
    populated("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| populated("HOME").map(|path| PathBuf::from(path).join(".config")))
        .map(|path| path.join("scroll-studio").join("settings.json"))
}

pub(super) fn load(path: &Path) -> Result<Preferences> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Preferences::default());
        }
        Err(error) => return Err(error).context("Could not read app settings"),
    };
    ensure!(
        file.metadata()?.len() <= MAX_PREFERENCES_BYTES,
        "App settings file is too large"
    );
    let mut bytes = Vec::new();
    file.take(MAX_PREFERENCES_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_PREFERENCES_BYTES,
        "App settings file is too large"
    );
    let mut preferences: Preferences =
        serde_json::from_slice(&bytes).context("App settings file is invalid")?;
    preferences.sanitize();
    Ok(preferences)
}

pub(super) fn save(path: &Path, preferences: &Preferences) -> Result<()> {
    let mut preferences = preferences.clone();
    preferences.sanitize();
    let bytes = serde_json::to_vec_pretty(&preferences).context("Could not encode app settings")?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).context("Could not create app settings folder")?;
    let (temporary, mut file) = create_temporary(path, parent)?;
    let result = (|| -> Result<()> {
        file.write_all(&bytes)
            .context("Could not write app settings")?;
        file.sync_all().context("Could not flush app settings")?;
        drop(file);
        // Same-volume std::fs::rename replaces the destination on Windows too.
        // A failed replacement leaves the previous preferences intact.
        fs::rename(&temporary, path).context("Could not replace app settings")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn create_temporary(path: &Path, parent: &Path) -> Result<(PathBuf, File)> {
    let name = path
        .file_name()
        .context("App settings path has no filename")?;
    for _ in 0..32 {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temporary = parent.join(format!(
            ".{}.{}.{}.tmp",
            name.to_string_lossy(),
            std::process::id(),
            counter
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error).context("Could not create temporary app settings"),
        }
    }
    bail!("Could not allocate temporary app settings")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_directory(label: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "scroll-studio-settings-{label}-{}-{}",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn missing_partial_and_unknown_fields_keep_usable_preferences() {
        let directory = test_directory("defaults");
        let path = directory.join("settings.json");
        assert_eq!(load(&path).unwrap(), Preferences::default());
        fs::write(&path, br#"{"motion":"off","future_setting":true}"#).unwrap();
        let preferences = load(&path).unwrap();
        assert_eq!(preferences.motion, Motion::Off);
        assert!(preferences.glass_enabled && preferences.snap_enabled);
        fs::write(&path, br#"{"motion":"future_mode"}"#).unwrap();
        assert_eq!(load(&path).unwrap().motion, Motion::Lively);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn round_trip_replaces_existing_file_without_temp_files() {
        let directory = test_directory("round-trip");
        let path = directory.join("settings.json");
        save(&path, &Preferences::default()).unwrap();
        let preferences = Preferences {
            glass_enabled: false,
            glass_strength: 0.25,
            motion: Motion::Calm,
            snap_enabled: false,
            snap_canvas: false,
            snap_objects: false,
            snap_spacing: false,
            snap_threshold_px: 12.0,
        };
        save(&path, &preferences).unwrap();
        assert_eq!(load(&path).unwrap(), preferences);
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn corrupt_and_oversized_preferences_are_reported_and_ranges_are_bounded() {
        let directory = test_directory("invalid");
        let path = directory.join("settings.json");
        fs::write(&path, b"not json").unwrap();
        assert!(load(&path).is_err());
        fs::write(&path, vec![b' '; MAX_PREFERENCES_BYTES as usize + 1]).unwrap();
        assert!(load(&path).is_err());
        fs::write(&path, br#"{"glass_strength":-10,"snap_threshold_px":900}"#).unwrap();
        let mut preferences = load(&path).unwrap();
        assert_eq!(preferences.glass_strength, 0.0);
        assert_eq!(preferences.snap_threshold_px, 16.0);
        preferences.glass_strength = f32::NAN;
        preferences.snap_threshold_px = f32::INFINITY;
        save(&path, &preferences).unwrap();
        let preferences = load(&path).unwrap();
        assert_eq!(preferences.glass_strength, 0.65);
        assert_eq!(preferences.snap_threshold_px, 8.0);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn failed_replacement_does_not_remove_destination_or_leave_temporary_file() {
        let directory = test_directory("failed-save");
        let path = directory.join("settings.json");
        fs::create_dir(&path).unwrap();
        fs::write(path.join("keep"), b"unchanged").unwrap();
        assert!(save(&path, &Preferences::default()).is_err());
        assert_eq!(fs::read(path.join("keep")).unwrap(), b"unchanged");
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_dir_all(directory).unwrap();
    }
}
