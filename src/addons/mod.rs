//! Optional addons: separate programs that read the story and propose edits.
//!
//! An addon lives in its own folder under [`default_directory`], described by an
//! `addon.json` [`Manifest`]. The app starts enabled addons, talks to each over JSON-RPC on
//! stdin/stdout (see [`host`]), and stops them on exit. Addons never touch story data:
//! they send requests through [`crate::wire`], and edits become proposals the user applies
//! or cancels. With no addons installed nothing here runs.

pub mod host;
pub mod install;
pub mod manifest;
pub mod process;

pub use install::{Installed, Registry};
pub use manifest::{Manifest, Permission};

use std::path::PathBuf;

/// The per-user addons folder, beside the app's recovery and settings folders.
/// Nothing is created or read here.
pub fn default_directory() -> PathBuf {
    if let Some(path) = std::env::var_os("LOCALAPPDATA").filter(|s| !s.is_empty()) {
        return PathBuf::from(path).join("Scroll Studio").join("addons");
    }
    if let Some(path) = std::env::var_os("XDG_DATA_HOME").filter(|s| !s.is_empty()) {
        return PathBuf::from(path).join("scroll-studio").join("addons");
    }
    if let Some(path) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        return PathBuf::from(path)
            .join(".local")
            .join("share")
            .join("scroll-studio")
            .join("addons");
    }
    std::env::temp_dir().join("Scroll Studio").join("addons")
}

#[cfg(test)]
pub(crate) mod tests;
