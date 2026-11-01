//! Recovery copies are separate from user saves. A small atomic pointer commits
//! each immutable .scro snapshot; a failed write keeps the previous one usable.
//! An OS file lock prevents another running instance from offering or deleting
//! this instance's recovery data. No image decoding is done during discovery.

use crate::{
    model::{AssetBytes, Story},
    storage,
};
use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};

const RECORD_FILE: &str = "current.json";
const OWNER_FILE: &str = "owner.lock";
const RECORD_LIMIT: u64 = 32 * 1024;
static UNIQUE: AtomicU64 = AtomicU64::new(0);

/// Resolve the per-user recovery folder without creating or scanning it.
pub fn default_directory() -> PathBuf {
    if let Some(path) = std::env::var_os("LOCALAPPDATA").filter(|s| !s.is_empty()) {
        return PathBuf::from(path).join("Scroll Studio").join("recovery");
    }
    if let Some(path) = std::env::var_os("XDG_DATA_HOME").filter(|s| !s.is_empty()) {
        return PathBuf::from(path).join("scroll-studio").join("recovery");
    }
    if let Some(path) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        return PathBuf::from(path)
            .join(".local")
            .join("share")
            .join("scroll-studio")
            .join("recovery");
    }
    std::env::temp_dir().join("Scroll Studio").join("recovery")
}

#[derive(Clone, Debug)]
pub struct RecoverySession(Arc<SessionInner>);

#[derive(Debug)]
struct SessionInner {
    root: PathBuf,
    directory: PathBuf,
    session_id: String,
    // Holding this handle keeps the exclusive OS lock alive, including while a
    // worker owns a cloned RecoverySession after the document has been switched.
    _owner: File,
    operation: Mutex<()>,
}

#[derive(Clone, Debug)]
pub struct RecoveryEntry {
    pub session_id: String,
    pub title: String,
    pub original_path: Option<PathBuf>,
    pub saved_unix_seconds: u64,
    pub revision: u64,
    root: PathBuf,
    directory: PathBuf,
    snapshot_file: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u32,
    session_id: String,
    title: String,
    original_path: Option<PathBuf>,
    saved_unix_seconds: u64,
    revision: u64,
    snapshot_file: String,
}

impl RecoverySession {
    /// Create one independently locked session for one open document.
    /// Each caller/process gets a unique directory, even for untitled stories.
    pub fn new(root: &Path) -> Result<Self> {
        fs::create_dir_all(root).context("Could not create recovery folder")?;
        let root = resolve_write_directory(root)?;
        for _ in 0..32 {
            let session_id = format!("session-{}", unique_token());
            let directory = root.join(&session_id);
            match fs::create_dir(&directory) {
                Ok(()) => {
                    let owner = OpenOptions::new()
                        .read(true)
                        .write(true)
                        .create_new(true)
                        .open(directory.join(OWNER_FILE))?;
                    owner
                        .try_lock()
                        .context("Could not lock this recovery session")?;
                    return Ok(Self(Arc::new(SessionInner {
                        root,
                        directory,
                        session_id,
                        _owner: owner,
                        operation: Mutex::new(()),
                    })));
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error).context("Could not create recovery session"),
            }
        }
        bail!("Could not allocate a unique recovery session")
    }

    pub fn id(&self) -> &str {
        &self.0.session_id
    }

    /// Call on a worker with an immutable document snapshot. This never writes
    /// to original_path; that path is descriptive metadata only.
    pub fn save(
        &self,
        story: &Story,
        assets: &AssetBytes,
        original_path: Option<&Path>,
        revision: u64,
    ) -> Result<RecoveryEntry> {
        let _operation = self
            .0
            .operation
            .lock()
            .map_err(|_| anyhow::anyhow!("Recovery worker lock was poisoned"))?;
        check_session_directory(&self.0.root, &self.0.directory, &self.0.session_id)?;
        let previous = read_optional_record(&self.0.directory, &self.0.session_id)?;
        let record = Record {
            version: 1,
            session_id: self.0.session_id.clone(),
            title: story.title.chars().take(4096).collect(),
            original_path: original_path.map(Path::to_path_buf),
            saved_unix_seconds: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
            revision,
            snapshot_file: format!("snapshot-{}.scro", unique_token()),
        };
        let encoded = serde_json::to_vec(&record)?;
        ensure!(
            encoded.len() as u64 <= RECORD_LIMIT,
            "Recovery metadata is too large"
        );
        let snapshot = self.0.directory.join(&record.snapshot_file);
        storage::save_story(&snapshot, story, assets)?;
        if let Err(error) = commit_record(&self.0.directory, &encoded) {
            let _ = fs::remove_file(&snapshot);
            return Err(error);
        }
        // Cleanup only after committing the new pointer. Failure to remove an
        // older snapshot must not report a successfully saved copy as failed.
        if let Some(previous) = previous {
            if previous.snapshot_file != record.snapshot_file {
                let _ = remove_known_file(&self.0.directory, &previous.snapshot_file);
            }
        }
        Ok(entry_from_record(&self.0.root, &self.0.directory, record))
    }

    /// Remove this session's committed recovery after an explicit discard or
    /// a fully current manual save. Serialized with outstanding save workers.
    /// It leaves the locked session reusable and never recurses through folders.
    pub fn clear(&self) -> Result<()> {
        let _operation = self
            .0
            .operation
            .lock()
            .map_err(|_| anyhow::anyhow!("Recovery worker lock was poisoned"))?;
        check_session_directory(&self.0.root, &self.0.directory, &self.0.session_id)?;
        let record = read_optional_record(&self.0.directory, &self.0.session_id)?;
        remove_if_exists(&self.0.directory.join(RECORD_FILE))?;
        if let Some(record) = record {
            remove_known_file(&self.0.directory, &record.snapshot_file)?;
        }
        Ok(())
    }
}

impl RecoveryEntry {
    pub fn snapshot_path(&self) -> PathBuf {
        self.directory.join(&self.snapshot_file)
    }

    /// Recovery loads into the caller's new, dirty document. Do not silently
    /// replace the original document path with this recovery file's location.
    pub fn load(&self) -> Result<(Story, AssetBytes)> {
        let _owner = self.lock_inactive()?;
        self.ensure_current()?;
        storage::load_story(&self.snapshot_path())
    }

    /// Remove only this exact generation. A stale entry is a no-op if a newer
    /// committed snapshot has appeared since discovery. Active sessions refuse.
    pub fn discard(&self) -> Result<()> {
        let _owner = self.lock_inactive()?;
        let Some(current) = read_optional_record(&self.directory, &self.session_id)? else {
            return Ok(());
        };
        if current.snapshot_file != self.snapshot_file || current.revision != self.revision {
            return Ok(());
        }
        remove_if_exists(&self.directory.join(RECORD_FILE))?;
        remove_known_file(&self.directory, &self.snapshot_file)
    }

    fn lock_inactive(&self) -> Result<File> {
        check_session_directory(&self.root, &self.directory, &self.session_id)?;
        let owner = open_owner(&self.directory)?;
        owner
            .try_lock()
            .context("This recovery belongs to a running Scroll Studio instance")?;
        Ok(owner)
    }

    fn ensure_current(&self) -> Result<()> {
        let current = read_record(&self.directory, &self.session_id)?;
        ensure!(
            current.snapshot_file == self.snapshot_file && current.revision == self.revision,
            "This recovery has changed; refresh the recovery list"
        );
        ensure!(
            self.snapshot_path().is_file(),
            "The recovery snapshot is missing"
        );
        ensure!(
            self.snapshot_path().canonicalize()?.parent() == Some(self.directory.as_path()),
            "Recovery snapshot leaves its session folder"
        );
        Ok(())
    }
}

/// Discover only inactive, committed sessions, newest first. This reads a
/// bounded metadata record per session and never expands the .scro image data.
/// Incomplete/corrupt entries are ignored so one bad copy cannot hide others.
pub fn list(root: &Path) -> Result<Vec<RecoveryEntry>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let root = resolve_write_directory(root).context("Could not locate recovery folder")?;
    let mut entries = Vec::new();
    for item in fs::read_dir(&root)? {
        let Ok(item) = item else {
            continue;
        };
        let Some(session_id) = item.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !valid_session_name(&session_id) {
            continue;
        }
        let directory = item.path();
        if check_session_directory(&root, &directory, &session_id).is_err() {
            continue;
        }
        let Ok(owner) = open_owner(&directory) else {
            continue;
        };
        if owner.try_lock().is_err() {
            continue;
        }
        let Ok(record) = read_record(&directory, &session_id) else {
            continue;
        };
        let snapshot = directory.join(&record.snapshot_file);
        if !snapshot.is_file()
            || snapshot
                .canonicalize()
                .ok()
                .as_deref()
                .and_then(Path::parent)
                != Some(directory.as_path())
        {
            continue;
        }
        entries.push(entry_from_record(&root, &directory, record));
    }
    entries.sort_by(|a, b| {
        b.saved_unix_seconds
            .cmp(&a.saved_unix_seconds)
            .then_with(|| b.snapshot_file.cmp(&a.snapshot_file))
    });
    Ok(entries)
}

fn entry_from_record(root: &Path, directory: &Path, record: Record) -> RecoveryEntry {
    RecoveryEntry {
        session_id: record.session_id,
        title: record.title,
        original_path: record.original_path,
        saved_unix_seconds: record.saved_unix_seconds,
        revision: record.revision,
        root: root.to_path_buf(),
        directory: directory.to_path_buf(),
        snapshot_file: record.snapshot_file,
    }
}

fn unique_token() -> String {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "{time:032x}-{:x}-{:x}",
        std::process::id(),
        UNIQUE.fetch_add(1, Ordering::Relaxed)
    )
}

fn valid_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= 96
        && token.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
}
fn valid_session_name(name: &str) -> bool {
    name.strip_prefix("session-").is_some_and(valid_token)
}
fn valid_snapshot_name(name: &str) -> bool {
    name.strip_prefix("snapshot-")
        .and_then(|s| s.strip_suffix(".scro"))
        .is_some_and(valid_token)
}

/// Resolve the actual write destination before applying containment checks.
/// Packaged Windows processes can see a merged LocalAppData folder whose
/// directory handle resolves to the system path, while new files go to the
/// package's LocalCache. A newly created private probe exposes that destination;
/// subsequent session/snapshot checks still require strict physical containment.
fn resolve_write_directory(root: &Path) -> Result<PathBuf> {
    let probe = root.join(format!(".path-probe-{}", unique_token()));
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .context("Could not verify recovery storage")?;
    let result = probe.canonicalize().and_then(|path| {
        path.parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| std::io::Error::other("Recovery storage has no parent"))
    });
    drop(file);
    // This is only the unique zero-byte file created above, never an existing file.
    fs::remove_file(&probe).context("Could not finish recovery storage check")?;
    Ok(result?)
}

fn check_session_directory(root: &Path, directory: &Path, session_id: &str) -> Result<()> {
    ensure!(
        valid_session_name(session_id),
        "Invalid recovery session name"
    );
    ensure!(
        directory.file_name().and_then(|n| n.to_str()) == Some(session_id),
        "Mismatched recovery session"
    );
    ensure!(
        directory.canonicalize()?.parent() == Some(root),
        "Recovery session leaves its root folder"
    );
    ensure!(
        !fs::symlink_metadata(directory)?.file_type().is_symlink(),
        "Recovery session cannot be a symbolic link"
    );
    Ok(())
}

fn open_owner(directory: &Path) -> Result<File> {
    let path = directory.join(OWNER_FILE);
    ensure!(
        !fs::symlink_metadata(&path)?.file_type().is_symlink(),
        "Invalid recovery owner file"
    );
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .context("Could not inspect recovery owner")
}

fn read_optional_record(directory: &Path, session_id: &str) -> Result<Option<Record>> {
    if !directory.join(RECORD_FILE).try_exists()? {
        return Ok(None);
    }
    read_record(directory, session_id).map(Some)
}

fn read_record(directory: &Path, session_id: &str) -> Result<Record> {
    let path = directory.join(RECORD_FILE);
    ensure!(
        !fs::symlink_metadata(&path)?.file_type().is_symlink(),
        "Invalid recovery metadata file"
    );
    let file = File::open(path)?;
    ensure!(
        file.metadata()?.len() <= RECORD_LIMIT,
        "Recovery metadata is too large"
    );
    let mut bytes = Vec::new();
    file.take(RECORD_LIMIT + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= RECORD_LIMIT,
        "Recovery metadata is too large"
    );
    let record: Record = serde_json::from_slice(&bytes).context("Recovery metadata is invalid")?;
    ensure!(
        record.version == 1
            && record.session_id == session_id
            && valid_snapshot_name(&record.snapshot_file),
        "Recovery metadata identifies an invalid snapshot"
    );
    Ok(record)
}

fn commit_record(directory: &Path, encoded: &[u8]) -> Result<()> {
    let temporary = directory.join(format!(".record-{}.tmp", unique_token()));
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(encoded)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, directory.join(RECORD_FILE))
            .context("Could not commit recovery metadata")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn remove_known_file(directory: &Path, name: &str) -> Result<()> {
    ensure!(
        valid_snapshot_name(name),
        "Refusing to remove an unrelated recovery file"
    );
    let path = directory.join(name);
    if path.try_exists()? {
        ensure!(
            !fs::symlink_metadata(&path)?.file_type().is_symlink(),
            "Refusing to remove a linked recovery file"
        );
        ensure!(
            path.canonicalize()?.parent() == Some(directory),
            "Refusing to remove a file outside this recovery session"
        );
    }
    remove_if_exists(&path)
}

fn remove_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestRoot(PathBuf);
    impl TestRoot {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("scroll-recovery-test-{}", unique_token()));
            fs::create_dir(&path).unwrap();
            Self(path.canonicalize().unwrap())
        }
    }
    impl Drop for TestRoot {
        fn drop(&mut self) {
            let temp = std::env::temp_dir().canonicalize().unwrap();
            if self.0.parent() == Some(temp.as_path())
                && self
                    .0
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("scroll-recovery-test-")
            {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
    }

    #[test]
    fn untitled_recovery_reopens_after_session_drop_without_touching_user_save() {
        let root = TestRoot::new();
        let user_file = root.0.join("User story.scro");
        fs::write(&user_file, b"original user file").unwrap();
        let session = RecoverySession::new(&root.0).unwrap();
        let mut story = Story::new("An untitled recovered story");
        story.chapters[0].panels[0].script = "Saved script".into();
        session
            .save(&story, &AssetBytes::new(), Some(&user_file), 7)
            .unwrap();
        assert!(
            list(&root.0).unwrap().is_empty(),
            "live sessions are hidden"
        );
        drop(session);
        let entries = list(&root.0).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].title, story.title);
        assert_eq!(entries[0].original_path, Some(user_file.clone()));
        assert_eq!(entries[0].revision, 7);
        assert_eq!(entries[0].load().unwrap().0, story);
        assert_eq!(fs::read(user_file).unwrap(), b"original user file");
    }

    #[test]
    fn multiple_sessions_and_active_owner_are_independent() {
        let root = TestRoot::new();
        let a = RecoverySession::new(&root.0).unwrap();
        let b = RecoverySession::new(&root.0).unwrap();
        assert_ne!(a.id(), b.id());
        let ea = a
            .save(&Story::new("A"), &AssetBytes::new(), None, 1)
            .unwrap();
        b.save(&Story::new("B"), &AssetBytes::new(), None, 1)
            .unwrap();
        assert!(ea.discard().is_err());
        assert!(ea.load().is_err());
        drop(a);
        assert_eq!(list(&root.0).unwrap().len(), 1);
        drop(b);
        assert_eq!(list(&root.0).unwrap().len(), 2);
        ea.discard().unwrap();
        assert_eq!(list(&root.0).unwrap()[0].title, "B");
    }

    #[test]
    fn invalid_save_preserves_previous_committed_snapshot() {
        let root = TestRoot::new();
        let session = RecoverySession::new(&root.0).unwrap();
        let story = Story::new("Valid story");
        let first = session.save(&story, &AssetBytes::new(), None, 1).unwrap();
        let mut invalid = story.clone();
        invalid.chapters.clear();
        assert!(session.save(&invalid, &AssetBytes::new(), None, 2).is_err());
        assert!(first.snapshot_path().exists());
        drop(session);
        assert_eq!(list(&root.0).unwrap()[0].load().unwrap().0, story);
    }

    #[test]
    fn failed_metadata_commit_keeps_previous_pointer_and_snapshot() {
        let root = TestRoot::new();
        let session = RecoverySession::new(&root.0).unwrap();
        let original = Story::new("Original");
        let first = session
            .save(&original, &AssetBytes::new(), None, 1)
            .unwrap();
        let before = fs::read(session.0.directory.join(RECORD_FILE)).unwrap();
        // Hold the current metadata without delete sharing so Windows refuses
        // rename; on other systems a read-only record directory is unreliable
        // under privileged test users, so test commit's direct failure path.
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            let blocked = OpenOptions::new()
                .read(true)
                .share_mode(1)
                .open(session.0.directory.join(RECORD_FILE))
                .unwrap();
            assert!(session
                .save(&Story::new("Uncommitted"), &AssetBytes::new(), None, 2)
                .is_err());
            drop(blocked);
        }
        #[cfg(not(windows))]
        {
            assert!(commit_record(&session.0.directory.join("missing"), b"{}").is_err());
        }
        assert_eq!(
            fs::read(session.0.directory.join(RECORD_FILE)).unwrap(),
            before
        );
        assert!(first.snapshot_path().exists());
        drop(session);
        assert_eq!(list(&root.0).unwrap()[0].load().unwrap().0, original);
    }

    #[test]
    fn stale_entry_cannot_discard_a_newer_generation() {
        let root = TestRoot::new();
        let session = RecoverySession::new(&root.0).unwrap();
        let old = session
            .save(&Story::new("First"), &AssetBytes::new(), None, 1)
            .unwrap();
        let newest = session
            .save(&Story::new("Second"), &AssetBytes::new(), None, 2)
            .unwrap();
        assert!(!old.snapshot_path().exists());
        drop(session);
        old.discard().unwrap();
        assert!(newest.snapshot_path().exists());
        assert_eq!(list(&root.0).unwrap()[0].revision, 2);
    }

    #[test]
    fn clear_is_targeted_and_reusable() {
        let root = TestRoot::new();
        let session = RecoverySession::new(&root.0).unwrap();
        let first = session
            .save(&Story::new("First"), &AssetBytes::new(), None, 1)
            .unwrap();
        let unrelated = session.0.directory.join("keep.txt");
        fs::write(&unrelated, "keep").unwrap();
        session.clear().unwrap();
        assert!(!first.snapshot_path().exists());
        assert!(unrelated.exists());
        session
            .save(&Story::new("After clear"), &AssetBytes::new(), None, 3)
            .unwrap();
        drop(session);
        assert_eq!(list(&root.0).unwrap()[0].revision, 3);
    }

    #[test]
    fn discovery_skips_invalid_and_oversized_metadata() {
        let root = TestRoot::new();
        let valid = RecoverySession::new(&root.0).unwrap();
        valid
            .save(&Story::new("Good"), &AssetBytes::new(), None, 1)
            .unwrap();
        let bad = RecoverySession::new(&root.0).unwrap();
        bad.save(&Story::new("Bad"), &AssetBytes::new(), None, 1)
            .unwrap();
        let directory = bad.0.directory.clone();
        let id = bad.id().to_owned();
        drop(bad);
        fs::write(
            directory.join(RECORD_FILE),
            vec![b'x'; RECORD_LIMIT as usize + 1],
        )
        .unwrap();
        drop(valid);
        assert_eq!(list(&root.0).unwrap().len(), 1);
        let invalid = Record {
            version: 1,
            session_id: id,
            title: "Malicious".into(),
            original_path: None,
            saved_unix_seconds: 0,
            revision: 0,
            snapshot_file: "../../outside.scro".into(),
        };
        fs::write(
            directory.join(RECORD_FILE),
            serde_json::to_vec(&invalid).unwrap(),
        )
        .unwrap();
        assert_eq!(list(&root.0).unwrap().len(), 1);
    }
    #[test]
    fn write_directory_probe_leaves_no_files_and_preserves_containment() {
        let root = TestRoot::new();
        let resolved = resolve_write_directory(&root.0).unwrap();
        assert_eq!(resolved, root.0);
        assert_eq!(fs::read_dir(&root.0).unwrap().count(), 0);
        let other = TestRoot::new();
        let session = RecoverySession::new(&root.0).unwrap();
        assert!(check_session_directory(&other.0, &session.0.directory, session.id()).is_err());
    }
}
