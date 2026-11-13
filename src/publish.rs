//! One local episode package and the review that must precede any upload.
//!
//! Pin `22f76c7` is review-only and not a frozen contract. This module does
//! not send commands, does not replay an authorization result, and does not
//! treat a confirm acknowledgement as the current publication.

use crate::{
    export::{self, ExportOptions},
    model::{AssetBytes, Chapter},
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

/// Caps copied from the reviewed checkpoint's package limits, not its retry rules.
const MAX_SLICES: usize = 100;
const MAX_SLICE_BYTES: u64 = 2_000_000;
const MAX_EPISODE_BYTES: u64 = 20_000_000;
const MAX_SLICE_EDGE: u32 = 4096;
const MAX_SLICE_PIXELS: u64 = 4_000_000;
const MAX_PROSE_BYTES: usize = 65_536;
const MAX_TITLE_CHARS: usize = 120;
const MAX_SYNOPSIS_CHARS: usize = 2_000;

static PACKAGE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicMetadata {
    series_title: String,
    episode_title: String,
    synopsis: String,
    language: String,
}

impl PublicMetadata {
    pub fn new(
        series_title: impl Into<String>,
        episode_title: impl Into<String>,
        synopsis: impl Into<String>,
        language: impl Into<String>,
    ) -> Result<Self, PackageError> {
        let series_title = series_title.into();
        let episode_title = episode_title.into();
        let synopsis = synopsis.into();
        let language = language.into();
        check_title("Series title", &series_title)?;
        check_title("Episode title", &episode_title)?;
        check_synopsis(&synopsis)?;
        if language.len() != 2 || !language.bytes().all(|byte| byte.is_ascii_lowercase()) {
            return Err(PackageError::Metadata(
                "Language must be two lowercase letters.".into(),
            ));
        }
        Ok(Self {
            series_title,
            episode_title,
            synopsis,
            language,
        })
    }

    pub fn series_title(&self) -> &str {
        &self.series_title
    }
    pub fn episode_title(&self) -> &str {
        &self.episode_title
    }
    pub fn synopsis(&self) -> &str {
        &self.synopsis
    }
    pub fn language(&self) -> &str {
        &self.language
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalDestination {
    account_label: String,
    series_title: String,
    episode_title: String,
}

impl LocalDestination {
    /// Labels the person typed for this review. They are not server ids.
    pub fn new(
        account_label: impl Into<String>,
        series_title: impl Into<String>,
        episode_title: impl Into<String>,
    ) -> Result<Self, PackageError> {
        let account_label = account_label.into();
        check_label("Account", &account_label)?;
        let series_title = series_title.into();
        let episode_title = episode_title.into();
        check_title("Series title", &series_title)?;
        check_title("Episode title", &episode_title)?;
        Ok(Self {
            account_label,
            series_title,
            episode_title,
        })
    }

    pub fn account_label(&self) -> &str {
        &self.account_label
    }
    pub fn series_title(&self) -> &str {
        &self.series_title
    }
    pub fn episode_title(&self) -> &str {
        &self.episode_title
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EpisodeKind {
    Comic,
    Prose,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageMember {
    label: String,
    media_type: &'static str,
    width: Option<u32>,
    height: Option<u32>,
    sha256_hex: String,
    bytes: Vec<u8>,
}

impl PackageMember {
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn media_type(&self) -> &'static str {
        self.media_type
    }
    pub fn width(&self) -> Option<u32> {
        self.width
    }
    pub fn height(&self) -> Option<u32> {
        self.height
    }
    pub fn sha256_hex(&self) -> &str {
        &self.sha256_hex
    }
    pub fn byte_len(&self) -> u64 {
        self.bytes.len() as u64
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpisodePackage {
    kind: EpisodeKind,
    metadata: PublicMetadata,
    members: Vec<PackageMember>,
    content_sha256_hex: String,
    source: PackageSource,
    profile_label: Option<&'static str>,
    format_label: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PackageSource {
    Chapter(u64),
    Prose(String),
}

impl EpisodePackage {
    pub fn kind(&self) -> EpisodeKind {
        self.kind
    }
    pub fn metadata(&self) -> &PublicMetadata {
        &self.metadata
    }
    pub fn members(&self) -> &[PackageMember] {
        &self.members
    }
    pub fn content_sha256_hex(&self) -> &str {
        &self.content_sha256_hex
    }
    pub fn chapter_id(&self) -> Option<u64> {
        match self.source {
            PackageSource::Chapter(id) => Some(id),
            PackageSource::Prose(_) => None,
        }
    }
    pub fn prose_sha256_hex(&self) -> Option<&str> {
        match &self.source {
            PackageSource::Prose(sha) => Some(sha),
            PackageSource::Chapter(_) => None,
        }
    }
    pub fn profile_label(&self) -> Option<&'static str> {
        self.profile_label
    }
    pub fn format_label(&self) -> Option<&'static str> {
        self.format_label
    }

    pub fn contains_bytes(&self, needle: &[u8]) -> bool {
        if needle.is_empty() {
            return false;
        }
        self.members
            .iter()
            .any(|member| contains_slice(&member.bytes, needle))
            || contains_slice(self.metadata.series_title.as_bytes(), needle)
            || contains_slice(self.metadata.episode_title.as_bytes(), needle)
            || contains_slice(self.metadata.synopsis.as_bytes(), needle)
            || contains_slice(self.metadata.language.as_bytes(), needle)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublishSession {
    package: Option<EpisodePackage>,
    destination: Option<LocalDestination>,
    seal: Option<ReviewSeal>,
    credential: Option<crate::publish_wire::Credential>,
    account: Option<crate::publish_wire::AccountSnapshot>,
    owned: Option<crate::publish_wire::OwnedEpisode>,
    acknowledgement: Option<crate::publish_wire::Acknowledgement>,
    live: crate::publish_wire::LiveStatus,
    keeping: Option<crate::publish_wire::CredentialKeeping>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ReviewSeal {
    package_id: [u8; 32],
    destination_id: [u8; 32],
}

impl Default for PublishSession {
    fn default() -> Self {
        Self::new()
    }
}

impl PublishSession {
    pub fn new() -> Self {
        Self {
            package: None,
            destination: None,
            seal: None,
            credential: None,
            account: None,
            owned: None,
            acknowledgement: None,
            live: crate::publish_wire::LiveStatus::NotRead,
            keeping: None,
        }
    }

    pub fn keeping(&self) -> Option<crate::publish_wire::CredentialKeeping> {
        self.keeping
    }

    pub fn account(&self) -> Option<&crate::publish_wire::AccountSnapshot> {
        self.account.as_ref()
    }

    pub fn owned(&self) -> Option<&crate::publish_wire::OwnedEpisode> {
        self.owned.as_ref()
    }

    pub fn acknowledgement(&self) -> Option<&crate::publish_wire::Acknowledgement> {
        self.acknowledgement.as_ref()
    }

    pub fn store_sign_in(
        &mut self,
        credential: crate::publish_wire::Credential,
        account: crate::publish_wire::AccountSnapshot,
    ) {
        self.keeping = Some(crate::publish_wire::keep_credential(&credential));
        self.credential = Some(credential);
        self.account = Some(account);
        self.owned = None;
        self.acknowledgement = None;
        self.live = crate::publish_wire::LiveStatus::NotRead;
        self.seal = None;
    }

    pub fn select_owned(&mut self, owned: crate::publish_wire::OwnedEpisode) {
        let changed = self.owned.as_ref() != Some(&owned);
        self.owned = Some(owned);
        if changed {
            self.seal = None;
            self.acknowledgement = None;
            self.live = crate::publish_wire::LiveStatus::NotRead;
        }
    }

    pub fn note_acknowledgement(&mut self, acknowledgement: crate::publish_wire::Acknowledgement) {
        self.acknowledgement = Some(acknowledgement);
        self.live = crate::publish_wire::LiveStatus::NotRead;
    }

    pub fn note_live(&mut self, live: crate::publish_wire::LiveStatus) {
        self.live = live;
    }

    pub fn credential(&self) -> Option<&crate::publish_wire::Credential> {
        self.credential.as_ref()
    }

    pub fn package(&self) -> Option<&EpisodePackage> {
        self.package.as_ref()
    }

    pub fn destination(&self) -> Option<&LocalDestination> {
        self.destination.as_ref()
    }

    pub fn set_package(&mut self, package: EpisodePackage) {
        self.package = Some(package);
        self.seal = None;
    }

    pub fn set_destination(&mut self, destination: LocalDestination) {
        self.destination = Some(destination);
        self.seal = None;
    }

    pub fn invalidate(&mut self) {
        self.package = None;
        self.destination = None;
        self.seal = None;
        self.owned = None;
        self.acknowledgement = None;
        self.live = crate::publish_wire::LiveStatus::NotRead;
    }

    pub fn is_reviewed(&self) -> bool {
        let Some(seal) = &self.seal else {
            return false;
        };
        match (&self.package, &self.destination) {
            (Some(package), Some(destination)) => {
                seal.package_id == package_id(package)
                    && seal.destination_id == destination_id(destination)
            }
            _ => false,
        }
    }

    pub fn confirm_review(&mut self) -> Result<(), LinkError> {
        let Some(package) = &self.package else {
            return Err(LinkError::ReviewRequired);
        };
        let Some(destination) = &self.destination else {
            return Err(LinkError::ReviewRequired);
        };
        if destination.series_title != package.metadata.series_title
            || destination.episode_title != package.metadata.episode_title
        {
            return Err(LinkError::ReviewRequired);
        }
        self.seal = Some(ReviewSeal {
            package_id: package_id(package),
            destination_id: destination_id(destination),
        });
        Ok(())
    }

    /// Current publication is unknown until a later read. A refused upload
    /// does not invent a revision, a URL, or a cached confirm result.
    pub fn publication_status(&self) -> PublicationStatus {
        match &self.live {
            crate::publish_wire::LiveStatus::NotRead
            | crate::publish_wire::LiveStatus::Discover { .. } => PublicationStatus::NotRead,
            crate::publish_wire::LiveStatus::Read { status, .. } if status == "published" => {
                PublicationStatus::Published
            }
            crate::publish_wire::LiveStatus::Read { .. } => PublicationStatus::Withdrawn,
        }
    }

    pub fn upload_draft(&self) -> Result<std::convert::Infallible, LinkError> {
        if !self.is_reviewed() {
            return Err(LinkError::ReviewRequired);
        }
        Err(runtime_transport_refused(None))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicationStatus {
    NotRead,
    Published,
    Withdrawn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeHook {
    Clock,
    PauseBeforeCommit,
    TestOnlyRedirect,
    DeterministicIdentity,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkError {
    ReviewRequired,
    ServiceNotConnected,
    FixtureHookRejected(RuntimeHook),
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReviewRequired => write!(f, "Review the selected episode before uploading."),
            Self::ServiceNotConnected => write!(
                f,
                "Private prototype service is not connected. Nothing was uploaded."
            ),
            Self::FixtureHookRejected(_) => write!(
                f,
                "Fixture clock, pause, test redirect, and scripted identity hooks are refused."
            ),
        }
    }
}

impl std::error::Error for LinkError {}

/// The desktop runtime has nowhere to send a command. A fixture hook is
/// refused before that, so it cannot extend a clock or skip the browser.
pub fn runtime_transport_refused(hook: Option<RuntimeHook>) -> LinkError {
    if let Some(hook) = hook {
        return LinkError::FixtureHookRejected(hook);
    }
    LinkError::ServiceNotConnected
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackageError {
    Metadata(String),
    Media(String),
    Limit(String),
    Export(String),
}

impl std::fmt::Display for PackageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Metadata(message) | Self::Media(message) | Self::Limit(message) => {
                write!(f, "{message}")
            }
            Self::Export(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for PackageError {}

pub fn package_chapter(
    chapter: &Chapter,
    assets: &AssetBytes,
    options: &ExportOptions,
    metadata: PublicMetadata,
) -> Result<EpisodePackage, PackageError> {
    let directory = package_directory()?;
    let _cleanup = DirectoryGuard(directory.clone());
    let report = export::export_chapter_with_options(chapter, assets, &directory, options)
        .map_err(|error| PackageError::Export(error.to_string()))?;
    if report.files.is_empty() || report.files.len() > MAX_SLICES {
        return Err(PackageError::Limit(format!(
            "A comic episode needs 1 to {MAX_SLICES} slices."
        )));
    }
    let media_type = match options.format {
        export::ExportFormat::Png => "image/png",
        export::ExportFormat::Jpeg => "image/jpeg",
    };
    let extension = match options.format {
        export::ExportFormat::Png => "png",
        export::ExportFormat::Jpeg => "jpg",
    };
    let mut members = Vec::with_capacity(report.files.len());
    let mut joined = Vec::new();
    for (index, file) in report.files.iter().enumerate() {
        let label = format!("chapter-{:05}.{extension}", index + 1);
        if file.path.file_name().and_then(|name| name.to_str()) != Some(label.as_str()) {
            return Err(PackageError::Media(format!(
                "Export produced an unexpected file name for slice {}.",
                index + 1
            )));
        }
        if file.path == report.report_path {
            return Err(PackageError::Media(
                "The export report cannot be an episode file.".into(),
            ));
        }
        let bytes = fs::read(&file.path)
            .map_err(|error| PackageError::Export(format!("Could not read {label}: {error}")))?;
        if bytes.len() as u64 != file.bytes || bytes.len() as u64 > MAX_SLICE_BYTES {
            return Err(PackageError::Limit(format!(
                "{label} is over the {MAX_SLICE_BYTES} byte slice limit."
            )));
        }
        let (width, height) = decoded_size(&bytes).map_err(|error| {
            PackageError::Media(format!("{label} could not be fully decoded: {error}"))
        })?;
        if width != file.width || height != file.height {
            return Err(PackageError::Media(format!(
                "{label} decoded as {width}×{height}, not {}×{}.",
                file.width, file.height
            )));
        }
        check_dimensions(width, height)?;
        if joined.len() as u64 + bytes.len() as u64 > MAX_EPISODE_BYTES {
            return Err(PackageError::Limit(format!(
                "The episode is over the {MAX_EPISODE_BYTES} byte limit."
            )));
        }
        joined.extend_from_slice(&bytes);
        members.push(PackageMember {
            label,
            media_type,
            width: Some(width),
            height: Some(height),
            sha256_hex: hex(&sha256(&bytes)),
            bytes,
        });
    }
    if report.report_path.exists() {
        let report_bytes = fs::read(&report.report_path).unwrap_or_default();
        if members.iter().any(|member| member.bytes == report_bytes) {
            return Err(PackageError::Media(
                "The export report was included as a slice.".into(),
            ));
        }
    }
    Ok(EpisodePackage {
        kind: EpisodeKind::Comic,
        metadata,
        members,
        content_sha256_hex: hex(&sha256(&joined)),
        source: PackageSource::Chapter(chapter.id),
        profile_label: Some(options.profile.label()),
        format_label: Some(options.format.label()),
    })
}

pub fn package_prose_bytes(
    bytes: &[u8],
    metadata: PublicMetadata,
) -> Result<EpisodePackage, PackageError> {
    if bytes.len() > MAX_PROSE_BYTES {
        return Err(PackageError::Limit(format!(
            "Prose is over the {MAX_PROSE_BYTES} byte limit."
        )));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| {
        PackageError::Media("Prose must be UTF-8 text without a byte order mark.".into())
    })?;
    if text.starts_with('\u{feff}') {
        return Err(PackageError::Media(
            "Prose must be UTF-8 text without a byte order mark.".into(),
        ));
    }
    if text.chars().any(|ch| !prose_char_allowed(ch)) {
        return Err(PackageError::Media(
            "Prose cannot contain carriage returns or other control characters.".into(),
        ));
    }
    if text.is_empty() {
        return Err(PackageError::Media("Prose is empty.".into()));
    }
    let sha = hex(&sha256(bytes));
    Ok(EpisodePackage {
        kind: EpisodeKind::Prose,
        metadata,
        members: vec![PackageMember {
            label: "episode.txt".into(),
            media_type: "text/plain; charset=utf-8",
            width: None,
            height: None,
            sha256_hex: sha.clone(),
            bytes: bytes.to_vec(),
        }],
        content_sha256_hex: sha.clone(),
        source: PackageSource::Prose(sha),
        profile_label: None,
        format_label: None,
    })
}

fn check_title(label: &str, value: &str) -> Result<(), PackageError> {
    if value.is_empty() {
        return Err(PackageError::Metadata(format!("{label} is empty.")));
    }
    if value.chars().count() > MAX_TITLE_CHARS {
        return Err(PackageError::Metadata(format!(
            "{label} is longer than {MAX_TITLE_CHARS} characters."
        )));
    }
    if value.chars().any(|ch| ch.is_control()) {
        return Err(PackageError::Metadata(format!(
            "{label} contains a control character."
        )));
    }
    Ok(())
}

fn check_label(label: &str, value: &str) -> Result<(), PackageError> {
    check_title(label, value)?;
    if value.contains('/') || value.contains('\\') || value.contains("..") {
        return Err(PackageError::Metadata(format!(
            "{label} cannot be a file path."
        )));
    }
    Ok(())
}

fn check_synopsis(value: &str) -> Result<(), PackageError> {
    if value.is_empty() {
        return Err(PackageError::Metadata("Synopsis is empty.".into()));
    }
    if value.chars().count() > MAX_SYNOPSIS_CHARS {
        return Err(PackageError::Metadata(format!(
            "Synopsis is longer than {MAX_SYNOPSIS_CHARS} characters."
        )));
    }
    if value
        .chars()
        .any(|ch| !matches!(ch, '\n' | '\t') && ch.is_control())
    {
        return Err(PackageError::Metadata(
            "Synopsis contains a character the private prototype rejects.".into(),
        ));
    }
    Ok(())
}

fn check_dimensions(width: u32, height: u32) -> Result<(), PackageError> {
    if width == 0
        || height == 0
        || width > MAX_SLICE_EDGE
        || height > MAX_SLICE_EDGE
        || u64::from(width) * u64::from(height) > MAX_SLICE_PIXELS
    {
        return Err(PackageError::Limit(format!(
            "A slice of {width}×{height} is outside the prototype image limits."
        )));
    }
    Ok(())
}

fn prose_char_allowed(ch: char) -> bool {
    matches!(ch, '\n' | '\t') || !ch.is_control()
}

fn decoded_size(bytes: &[u8]) -> Result<(u32, u32), String> {
    let image = image::load_from_memory(bytes).map_err(|error| error.to_string())?;
    Ok((image.width(), image.height()))
}

fn package_id(package: &EpisodePackage) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(package.content_sha256_hex.as_bytes());
    hash_metadata(&mut hasher, &package.metadata);
    for member in &package.members {
        hasher.update(member.label.as_bytes());
        hasher.update(member.sha256_hex.as_bytes());
        hasher.update(member.bytes.len().to_le_bytes());
    }
    hasher.finalize().into()
}

fn destination_id(destination: &LocalDestination) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(destination.account_label.as_bytes());
    hasher.update([0]);
    hasher.update(destination.series_title.as_bytes());
    hasher.update([0]);
    hasher.update(destination.episode_title.as_bytes());
    hasher.finalize().into()
}

fn hash_metadata(hasher: &mut Sha256, metadata: &PublicMetadata) {
    hasher.update(metadata.series_title.as_bytes());
    hasher.update([0]);
    hasher.update(metadata.episode_title.as_bytes());
    hasher.update([0]);
    hasher.update(metadata.synopsis.as_bytes());
    hasher.update([0]);
    hasher.update(metadata.language.as_bytes());
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(DIGITS[(byte >> 4) as usize] as char);
        text.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    text
}

fn contains_slice(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn package_directory() -> Result<PathBuf, PackageError> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| PackageError::Export(error.to_string()))?
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "scroll-publish-{}-{}-{nanos}",
        std::process::id(),
        PACKAGE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).map_err(|error| {
        PackageError::Export(format!("Could not create a package folder: {error}"))
    })?;
    Ok(directory)
}

struct DirectoryGuard(PathBuf);

impl Drop for DirectoryGuard {
    fn drop(&mut self) {
        let _ = remove_tree(&self.0);
    }
}

fn remove_tree(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            remove_tree(&entry.path())?;
        }
        fs::remove_dir(path)
    } else {
        fs::remove_file(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Layer, LayerKind, Story};
    use std::sync::Arc;

    fn metadata() -> PublicMetadata {
        PublicMetadata::new(
            "North line",
            "Platform",
            "A reviewed public synopsis.",
            "en",
        )
        .unwrap()
    }

    fn destination() -> LocalDestination {
        LocalDestination::new("Ada", "North line", "Platform").unwrap()
    }

    fn session_with_package(package: EpisodePackage) -> PublishSession {
        let mut session = PublishSession::new();
        session.set_package(package);
        session.set_destination(destination());
        session
    }

    fn tiny_story() -> (Story, AssetBytes) {
        let mut story = Story::with_canvas("Private story", 64, 64);
        story.synopsis = "SECRET_SYNOPSIS_NOTES".into();
        story.visual_style = "SECRET_VISUAL_STYLE".into();
        story.chapters[0].name = "Visible chapter".into();
        story.chapters[0].panels[0].script = "SECRET_BEAT_SCRIPT".into();
        story.chapters[0].panels[0].notes = "SECRET_BEAT_NOTES".into();
        story.chapters[0].panels[0].prompt = "SECRET_BEAT_PROMPT".into();
        let lore_id = story.alloc_id();
        story.lore.push(crate::model::LoreEntry {
            id: lore_id,
            title: "Bible".into(),
            text: "SECRET_LORE_BIBLE".into(),
            links: Vec::new(),
        });
        let unselected_id = story.alloc_id();
        let layer_id = story.alloc_id();
        let mut unselected = Chapter::new(unselected_id, "SECRET_UNSELECTED_CHAPTER", 64, 64);
        unselected.layers.push(Layer {
            extensions: Default::default(),
            id: layer_id,
            name: "SECRET_UNSELECTED_LAYER".into(),
            visible: true,
            locked: false,
            opacity: 1.0,
            x: 0.0,
            y: 0.0,
            width: 64.0,
            height: 64.0,
            kind: LayerKind::Shape {
                color: [255, 0, 0, 255],
            },
        });
        story.chapters.push(unselected);
        let hidden_asset = b"SECRET_ORIGINAL_ASSET_BYTES".to_vec();
        let asset_id = story.alloc_id();
        story.assets.push(crate::model::Asset {
            extensions: Default::default(),
            provenance: None,
            id: asset_id,
            name: "reference".into(),
            kind: crate::model::AssetKind::Image,
            notes: "SECRET_ASSET_NOTES".into(),
            prompt: "SECRET_ASSET_PROMPT".into(),
            width: 1,
            height: 1,
            character: crate::model::CharacterProfile::default(),
        });
        let mut assets = AssetBytes::new();
        assets.insert(asset_id, Arc::new(hidden_asset));
        (story, assets)
    }

    #[test]
    fn comic_package_keeps_only_rendered_slices_and_leaves_the_story() {
        let (story, assets) = tiny_story();
        let before_story = story.clone();
        let before_assets = assets.clone();
        let options = ExportOptions {
            profile: export::ExportProfile::Original,
            format: export::ExportFormat::Png,
            jpeg_quality: 90,
        };
        let package = package_chapter(&story.chapters[0], &assets, &options, metadata()).unwrap();
        assert_eq!(story, before_story);
        assert_eq!(assets, before_assets);
        assert_eq!(package.kind(), EpisodeKind::Comic);
        assert_eq!(package.members().len(), 1);
        assert_eq!(package.members()[0].label(), "chapter-00001.png");
        assert_eq!(package.members()[0].media_type(), "image/png");
        assert!(package.members()[0].byte_len() > 0);
        assert_eq!(package.chapter_id(), Some(story.chapters[0].id));
        let secrets = [
            "SECRET_SYNOPSIS_NOTES",
            "SECRET_VISUAL_STYLE",
            "SECRET_BEAT_SCRIPT",
            "SECRET_BEAT_NOTES",
            "SECRET_BEAT_PROMPT",
            "SECRET_LORE_BIBLE",
            "SECRET_UNSELECTED_CHAPTER",
            "SECRET_UNSELECTED_LAYER",
            "SECRET_ORIGINAL_ASSET_BYTES",
            "SECRET_ASSET_NOTES",
            "SECRET_ASSET_PROMPT",
            "Scroll Studio export report",
            "export-report.txt",
        ];
        for secret in secrets {
            assert!(
                !package.contains_bytes(secret.as_bytes()),
                "{secret} entered the package"
            );
        }
        let decoded = image::load_from_memory(package.members()[0].bytes()).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (64, 64));
    }

    #[test]
    fn prose_package_rejects_private_text_shapes_and_keeps_exact_utf8() {
        let text = "Line one.\nLine\ttwo.";
        let package = package_prose_bytes(text.as_bytes(), metadata()).unwrap();
        assert_eq!(package.kind(), EpisodeKind::Prose);
        assert_eq!(package.members().len(), 1);
        assert_eq!(package.members()[0].label(), "episode.txt");
        assert_eq!(package.members()[0].bytes(), text.as_bytes());
        assert!(package_prose_bytes(b"\xef\xbb\xbfHello", metadata()).is_err());
        assert!(package_prose_bytes(b"line\r\n", metadata()).is_err());
        assert!(package_prose_bytes(b"bad\x00text", metadata()).is_err());
        assert!(package_prose_bytes(&[0xff, 0xfe], metadata()).is_err());
        assert!(package_prose_bytes(&[], metadata()).is_err());
        let huge = vec![b'a'; MAX_PROSE_BYTES + 1];
        assert!(package_prose_bytes(&huge, metadata()).is_err());
    }

    #[test]
    fn review_seal_breaks_when_package_or_destination_changes() {
        let package = package_prose_bytes(b"Public line.", metadata()).unwrap();
        let mut session = session_with_package(package.clone());
        assert!(!session.is_reviewed());
        assert_eq!(session.upload_draft(), Err(LinkError::ReviewRequired));
        session.confirm_review().unwrap();
        assert!(session.is_reviewed());
        assert_eq!(session.publication_status(), PublicationStatus::NotRead);
        assert_eq!(session.upload_draft(), Err(LinkError::ServiceNotConnected));
        assert_eq!(session.publication_status(), PublicationStatus::NotRead);

        let replacement = package_prose_bytes(b"A different public line.", metadata()).unwrap();
        session.set_package(replacement);
        assert!(!session.is_reviewed());
        session.set_package(package);
        session.set_destination(destination());
        session.confirm_review().unwrap();
        session.set_destination(LocalDestination::new("Bea", "North line", "Platform").unwrap());
        assert!(!session.is_reviewed());
        assert_eq!(session.upload_draft(), Err(LinkError::ReviewRequired));
    }

    #[test]
    fn runtime_refuses_fixture_hooks_and_does_not_cache_publication() {
        for hook in [
            RuntimeHook::Clock,
            RuntimeHook::PauseBeforeCommit,
            RuntimeHook::TestOnlyRedirect,
            RuntimeHook::DeterministicIdentity,
        ] {
            assert_eq!(
                runtime_transport_refused(Some(hook)),
                LinkError::FixtureHookRejected(hook)
            );
        }
        assert_eq!(
            runtime_transport_refused(None),
            LinkError::ServiceNotConnected
        );
        let package = package_prose_bytes(b"Public line.", metadata()).unwrap();
        let mut session = session_with_package(package);
        session.confirm_review().unwrap();
        assert!(session.upload_draft().is_err());
        assert_eq!(session.publication_status(), PublicationStatus::NotRead);
    }

    #[test]
    fn oversized_slice_and_header_only_jpeg_are_rejected() {
        let story = Story::with_canvas("Wide", 4097, 64);
        let options = ExportOptions {
            profile: export::ExportProfile::Original,
            format: export::ExportFormat::Png,
            jpeg_quality: 90,
        };
        let error = package_chapter(&story.chapters[0], &AssetBytes::new(), &options, metadata())
            .unwrap_err();
        assert!(matches!(error, PackageError::Limit(_)), "{error}");

        let header_only = [
            0xff, 0xd8, 0xff, 0xc0, 0x00, 0x07, 0x08, 0x00, 0x01, 0x00, 0x01, 0xff, 0xd9,
        ];
        let error = decoded_size(&header_only).unwrap_err();
        assert!(!error.is_empty());

        let image = image::RgbImage::from_pixel(8, 8, image::Rgb([10, 20, 30]));
        let mut encoded = Vec::new();
        image
            .write_to(
                &mut std::io::Cursor::new(&mut encoded),
                image::ImageFormat::Jpeg,
            )
            .unwrap();
        let (width, height) = decoded_size(&encoded).unwrap();
        assert_eq!((width, height), (8, 8));
    }

    #[test]
    fn metadata_rejects_empty_private_controls_and_paths() {
        assert!(PublicMetadata::new("", "Episode", "Synopsis", "en").is_err());
        assert!(PublicMetadata::new("Series", "Episode", "", "en").is_err());
        assert!(PublicMetadata::new("Series", "Episode", "Synopsis\r", "en").is_err());
        assert!(PublicMetadata::new("Series", "Episode", "Synopsis", "EN").is_err());
        assert!(LocalDestination::new("../story.scro", "Series", "Episode").is_err());
        assert!(LocalDestination::new("Ada", "Series", "Episode").is_ok());
    }
}
