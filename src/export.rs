//! Platform-aware export with bounded rasterization and transactional output.
//! Large slices are divided without changing their width or JPEG quality.

use crate::{
    compositor::{output_height, render_with_cache, DecodeCache},
    model::{AssetBytes, Chapter},
};
use anyhow::{bail, ensure, Context, Result};
use image::{codecs::jpeg::JpegEncoder, ImageFormat, RgbaImage};
use std::{
    fmt::Write as _,
    fs::{self, File, OpenOptions},
    io::{Cursor, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const MAX_EXPORT_FILES: usize = 10_000;
const WEBTOON_FILE_LIMIT: u64 = 2_000_000;
const WEBTOON_EPISODE_LIMIT: u64 = 20_000_000;
const WEBTOON_IMAGE_LIMIT: usize = 100;
const TAPAS_FILE_LIMIT: u64 = 10_000_000;
const REPORT_NAME: &str = "export-report.txt";
static STAGE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub const WEBTOON_SOURCE: &str = "https://webtoons-static.pstatic.net/creator101/en/pdf/Before-You-Publish-Checklist-2024.pdf?dt=2024011001";
pub const TAPAS_SOURCE: &str = "https://help.tapas.io/hc/en-us/articles/1260802028970-Series-Basics-How-to-publish-a-comic-episode-on-Tapas";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportProfile {
    Webtoon,
    Tapas,
    Original,
}

impl ExportProfile {
    pub fn label(self) -> &'static str {
        match self {
            Self::Webtoon => "WEBTOON",
            Self::Tapas => "Tapas",
            Self::Original => "Original width",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Png,
    Jpeg,
}

impl ExportFormat {
    pub fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ExportOptions {
    pub profile: ExportProfile,
    pub format: ExportFormat,
    /// Fixed quality, 1–100. Oversized files split; this value is never lowered.
    pub jpeg_quality: u8,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            profile: ExportProfile::Webtoon,
            format: ExportFormat::Png,
            jpeg_quality: 90,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ExportPlan {
    pub width: u32,
    pub height: u32,
    pub slice_height: u32,
    pub minimum_slices: u32,
    pub per_file_limit: Option<u64>,
    pub warnings: Vec<String>,
}

impl ExportOptions {
    pub fn plan(&self, chapter: &Chapter) -> Result<ExportPlan> {
        ensure!(
            (1..=100).contains(&self.jpeg_quality),
            "JPEG quality must be between 1 and 100"
        );
        let (width, slice_height, per_file_limit) = match self.profile {
            ExportProfile::Webtoon => (800, 1280, Some(WEBTOON_FILE_LIMIT)),
            // Tapas has no PNG/JPEG height limit. 1600 is this app's starting slice height.
            ExportProfile::Tapas => (940, 1600, Some(TAPAS_FILE_LIMIT)),
            ExportProfile::Original => (chapter.width, 2000, None),
        };
        let height = output_height(chapter, width)?;
        let minimum_slices = height.div_ceil(slice_height);
        ensure!(
            minimum_slices as usize <= MAX_EXPORT_FILES,
            "Export would need more than {MAX_EXPORT_FILES} files; split the chapter first"
        );
        let warnings = episode_warnings(self.profile, minimum_slices as usize, 0);
        Ok(ExportPlan {
            width,
            height,
            slice_height,
            minimum_slices,
            per_file_limit,
            warnings,
        })
    }
}

#[derive(Clone, Debug)]
pub struct ExportedFile {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    pub jpeg_quality: Option<u8>,
}

#[derive(Clone, Debug)]
pub struct ExportReport {
    pub files: Vec<ExportedFile>,
    pub directory: PathBuf,
    pub report_path: PathBuf,
    /// Image bytes only; the human-readable report is not an uploaded image.
    pub total_bytes: u64,
    pub warnings: Vec<String>,
    /// Pixel dimensions, bytes and count checks for the selected profile passed.
    pub compliant: bool,
    pub profile: ExportProfile,
    pub format: ExportFormat,
}

impl ExportReport {
    pub fn summary(&self) -> String {
        format!(
            "{} {} slices · {:.2} MB · {}",
            self.files.len(),
            self.format.label(),
            self.total_bytes as f64 / 1_000_000.0,
            if self.compliant {
                "profile checks passed"
            } else {
                "review export warnings"
            }
        )
    }

    fn document(&self, chapter_name: &str) -> String {
        let mut text =
            format!(
            "Scroll Studio export report\nChapter: {chapter_name}\nProfile: {}\nFormat: {}\n{}\n\n",
            self.profile.label(), self.format.label(), self.summary()
        );
        match self.profile {
            ExportProfile::Webtoon => {
                text.push_str("Checks: 800 px wide, at most 1280 px per slice, less than 2,000,000 bytes per image, at most 100 images and 20,000,000 image bytes per episode.\n");
                let _ = writeln!(text, "Source: {WEBTOON_SOURCE}");
            }
            ExportProfile::Tapas => {
                text.push_str("Checks: 940 px wide and less than 10,000,000 bytes per image. The initial 1600 px slice height is a Scroll Studio choice; Tapas does not set a PNG/JPEG height limit.\n");
                let _ = writeln!(text, "Source: {TAPAS_SOURCE}");
            }
            ExportProfile::Original => text.push_str("Original canvas width. Platform-specific byte and image-count checks are not applied.\n"),
        }
        text.push_str("MB is treated as 1,000,000 bytes. Oversized slices are split vertically; resolution and the selected JPEG quality are preserved.\n");
        if !self.warnings.is_empty() {
            text.push_str("\nATTENTION\n");
            for warning in &self.warnings {
                let _ = writeln!(text, "- {warning}");
            }
        }
        text.push_str("\nFILES (upload images in this order; do not upload this report)\n");
        for file in &self.files {
            let _ = writeln!(
                text,
                "{}\t{} x {} px\t{} bytes{}",
                file.path.file_name().unwrap_or_default().to_string_lossy(),
                file.width,
                file.height,
                file.bytes,
                file.jpeg_quality
                    .map(|q| format!("\tJPEG quality {q}"))
                    .unwrap_or_default()
            );
        }
        text
    }
}

/// Render to an isolated staging directory first, then publish only complete
/// output. On any failure, only files created by this invocation are removed.
/// Existing destination files are never overwritten. Total episode limits are
/// reported so users can keep the images and choose a smaller format or chapter.
pub fn export_chapter_with_options(
    chapter: &Chapter,
    assets: &AssetBytes,
    directory: &Path,
    options: &ExportOptions,
) -> Result<ExportReport> {
    let plan = options.plan(chapter)?;
    fs::create_dir_all(directory).context("Cannot create export directory")?;
    let directory = fs::canonicalize(directory).context("Cannot resolve export directory")?;
    let report_path = directory.join(REPORT_NAME);
    ensure!(
        !report_path.exists(),
        "An export report already exists in this folder; choose another folder"
    );
    let first = directory.join(format!("chapter-00001.{}", options.format.extension()));
    ensure!(
        !first.exists(),
        "Export file already exists: {}. Choose another folder.",
        first.display()
    );
    let mut staging = Staging::new(&directory)?;
    let mut files = Vec::new();
    let mut cache = DecodeCache::default();
    for y in (0..plan.height).step_by(plan.slice_height as usize) {
        let image = render_with_cache(
            chapter,
            assets,
            y,
            plan.slice_height.min(plan.height - y),
            plan.width,
            &mut cache,
        )?;
        stage_slice(
            &image,
            options,
            plan.per_file_limit,
            &directory,
            &mut staging,
            &mut files,
        )?;
    }
    let total_bytes = files.iter().map(|file| file.bytes).sum();
    let warnings = episode_warnings(options.profile, files.len(), total_bytes);
    let report = ExportReport {
        files,
        directory,
        report_path,
        total_bytes,
        compliant: warnings.is_empty(),
        warnings,
        profile: options.profile,
        format: options.format,
    };
    staging.write(REPORT_NAME, report.document(&chapter.name).as_bytes())?;
    staging.publish(&report.directory)?;
    Ok(report)
}

fn encode(image: &RgbaImage, options: &ExportOptions) -> Result<Vec<u8>> {
    let mut bytes = Cursor::new(Vec::new());
    match options.format {
        ExportFormat::Png => image.write_to(&mut bytes, ImageFormat::Png)?,
        ExportFormat::Jpeg => {
            // Composition is already flattened over white. JPEG has no alpha.
            let rgb = image::RgbImage::from_fn(image.width(), image.height(), |x, y| {
                let pixel = image.get_pixel(x, y);
                image::Rgb([pixel[0], pixel[1], pixel[2]])
            });
            JpegEncoder::new_with_quality(&mut bytes, options.jpeg_quality).encode_image(&rgb)?;
        }
    }
    Ok(bytes.into_inner())
}

fn stage_slice(
    image: &RgbaImage,
    options: &ExportOptions,
    limit: Option<u64>,
    directory: &Path,
    staging: &mut Staging,
    files: &mut Vec<ExportedFile>,
) -> Result<()> {
    let bytes = encode(image, options)?;
    if limit.is_some_and(|limit| bytes.len() as u64 >= limit) {
        ensure!(
            image.height() > 1,
            "A single image row exceeds the selected platform's file limit; choose another format"
        );
        let top_height = image.height() / 2;
        // Drop the large encoded buffer before recursively working on a half.
        drop(bytes);
        let top = image::imageops::crop_imm(image, 0, 0, image.width(), top_height).to_image();
        stage_slice(&top, options, limit, directory, staging, files)?;
        drop(top);
        let bottom = image::imageops::crop_imm(
            image,
            0,
            top_height,
            image.width(),
            image.height() - top_height,
        )
        .to_image();
        return stage_slice(&bottom, options, limit, directory, staging, files);
    }
    ensure!(
        files.len() < MAX_EXPORT_FILES,
        "Export reached the {MAX_EXPORT_FILES} file safety limit; split the chapter first"
    );
    let name = format!(
        "chapter-{:05}.{}",
        files.len() + 1,
        options.format.extension()
    );
    staging.write(&name, &bytes)?;
    files.push(ExportedFile {
        path: directory.join(name),
        width: image.width(),
        height: image.height(),
        bytes: bytes.len() as u64,
        jpeg_quality: (options.format == ExportFormat::Jpeg).then_some(options.jpeg_quality),
    });
    Ok(())
}

fn episode_warnings(profile: ExportProfile, count: usize, total_bytes: u64) -> Vec<String> {
    let mut warnings = Vec::new();
    if profile == ExportProfile::Webtoon {
        if count > WEBTOON_IMAGE_LIMIT {
            warnings.push(format!("This chapter needs {count} images, exceeding WEBTOON's 100-image episode limit. Divide the chapter into shorter episodes; JPEG may also reduce extra splits."));
        }
        if total_bytes > WEBTOON_EPISODE_LIMIT {
            warnings.push(format!("These images total {:.2} MB, exceeding WEBTOON's 20 MB episode limit. Export as JPEG at a lower chosen quality or divide the chapter into shorter episodes.", total_bytes as f64 / 1_000_000.0));
        }
    }
    warnings
}

struct Staging {
    directory: PathBuf,
    files: Vec<PathBuf>,
}

impl Staging {
    fn new(parent: &Path) -> Result<Self> {
        for _ in 0..100 {
            let counter = STAGE_COUNTER.fetch_add(1, Ordering::Relaxed);
            let directory = parent.join(format!(".scroll-export-{}-{counter}", std::process::id()));
            match fs::create_dir(&directory) {
                Ok(()) => {
                    return Ok(Self {
                        directory,
                        files: Vec::new(),
                    })
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error).context("Cannot create export staging directory"),
            }
        }
        bail!("Cannot allocate export staging directory")
    }

    fn write(&mut self, name: &str, bytes: &[u8]) -> Result<()> {
        let path = self.directory.join(name);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        self.files.push(path);
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(())
    }

    fn publish(&self, destination: &Path) -> Result<()> {
        // Check the entire destination set before publishing the first file.
        for source in &self.files {
            let path = destination.join(source.file_name().context("Invalid staged filename")?);
            ensure!(
                !path.exists(),
                "Export file already exists: {}. Choose another folder.",
                path.display()
            );
        }
        let mut created = Vec::new();
        let result = (|| -> Result<()> {
            for source in &self.files {
                let path = destination.join(source.file_name().context("Invalid staged filename")?);
                let mut output = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .with_context(|| format!("Cannot create {}", path.display()))?;
                created.push(path);
                std::io::copy(&mut File::open(source)?, &mut output)?;
                output.flush()?;
                output.sync_all()?;
            }
            Ok(())
        })();
        if result.is_err() {
            for path in created {
                let _ = fs::remove_file(path);
            }
        }
        result
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        for path in &self.files {
            let _ = fs::remove_file(path);
        }
        let _ = fs::remove_dir(&self.directory);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Layer, LayerKind};
    use image::Rgba;
    use std::sync::Arc;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "scroll-platform-export-{}-{}-{}",
                std::process::id(),
                STAGE_COUNTER.fetch_add(1, Ordering::Relaxed),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            if let Ok(entries) = fs::read_dir(&self.0) {
                for entry in entries.flatten() {
                    let _ = fs::remove_file(entry.path());
                }
            }
            let _ = fs::remove_dir(&self.0);
        }
    }

    fn chapter(width: u32, height: u32) -> Chapter {
        Chapter {
            id: 1,
            name: "Export test".into(),
            width,
            height,
            panels: vec![],
            layers: vec![],
        }
    }

    fn image_chapter(image: RgbaImage) -> (Chapter, AssetBytes) {
        let mut chapter = chapter(image.width(), image.height());
        chapter.layers.push(Layer {
            id: 2,
            name: "Texture".into(),
            visible: true,
            locked: false,
            opacity: 1.0,
            x: 0.0,
            y: 0.0,
            width: image.width() as f32,
            height: image.height() as f32,
            kind: LayerKind::Image {
                source_rect: None,
                clip: None,
                asset_id: 3,
                mask: Vec::new(),
            },
        });
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, ImageFormat::Png).unwrap();
        (
            chapter,
            AssetBytes::from([(3, Arc::new(bytes.into_inner()))]),
        )
    }

    fn noisy_image(width: u32, height: u32) -> RgbaImage {
        let mut state = 0x12345678_u32;
        RgbaImage::from_fn(width, height, |_, _| {
            let mut rgb = [0_u8; 4];
            for byte in rgb.iter_mut().take(3) {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                *byte = state as u8;
            }
            rgb[3] = 255;
            Rgba(rgb)
        })
    }

    #[test]
    fn platform_plans_validate_inputs_and_warn_before_large_export() {
        let options = ExportOptions::default();
        let plan = options.plan(&chapter(1600, 9600)).unwrap();
        assert_eq!(
            (
                plan.width,
                plan.height,
                plan.slice_height,
                plan.minimum_slices
            ),
            (800, 4800, 1280, 4)
        );
        assert_eq!(plan.per_file_limit, Some(2_000_000));
        assert!(plan.warnings.is_empty());
        assert_eq!(
            options.plan(&chapter(800, 128001)).unwrap().warnings.len(),
            1
        );
        let tapas = ExportOptions {
            profile: ExportProfile::Tapas,
            ..options.clone()
        }
        .plan(&chapter(940, 3210))
        .unwrap();
        assert_eq!(
            (
                tapas.width,
                tapas.height,
                tapas.minimum_slices,
                tapas.per_file_limit
            ),
            (940, 3210, 3, Some(10_000_000))
        );
        assert!(ExportOptions {
            jpeg_quality: 0,
            ..options.clone()
        }
        .plan(&chapter(800, 100))
        .is_err());
        assert!(options.plan(&chapter(0, 100)).is_err());
    }

    #[test]
    fn noisy_png_splits_under_actual_webtoon_limit_without_changing_pixels() {
        let directory = TestDirectory::new();
        let (chapter, assets) = image_chapter(noisy_image(800, 1280));
        let expected = crate::compositor::render_region(&chapter, &assets, 0, 1280, 800).unwrap();
        let report =
            export_chapter_with_options(&chapter, &assets, &directory.0, &ExportOptions::default())
                .unwrap();
        assert!(
            report.files.len() > 1,
            "Noise must exercise adaptive splitting"
        );
        assert!(report.compliant);
        let mut combined = RgbaImage::new(800, 1280);
        let mut y = 0;
        for file in &report.files {
            assert_eq!(file.width, 800);
            assert!(file.height <= 1280);
            assert!(file.bytes < WEBTOON_FILE_LIMIT);
            assert_eq!(fs::metadata(&file.path).unwrap().len(), file.bytes);
            let image = image::open(&file.path).unwrap().to_rgba8();
            image::imageops::replace(&mut combined, &image, 0, y);
            y += i64::from(file.height);
        }
        assert_eq!(y, 1280);
        assert_eq!(combined, expected);
        assert_eq!(
            report.total_bytes,
            report.files.iter().map(|f| f.bytes).sum::<u64>()
        );
        assert!(fs::read_to_string(&report.report_path)
            .unwrap()
            .contains(WEBTOON_SOURCE));
        assert_eq!(
            fs::read_dir(&directory.0).unwrap().count(),
            report.files.len() + 1
        );
    }

    #[test]
    fn jpeg_preserves_the_requested_quality_and_reports_file_metadata() {
        let directory = TestDirectory::new();
        let (chapter, assets) = image_chapter(noisy_image(24, 27));
        let options = ExportOptions {
            profile: ExportProfile::Original,
            format: ExportFormat::Jpeg,
            jpeg_quality: 73,
        };
        let report =
            export_chapter_with_options(&chapter, &assets, &directory.0, &options).unwrap();
        assert_eq!(report.files.len(), 1);
        let file = &report.files[0];
        assert_eq!(file.jpeg_quality, Some(73));
        assert_eq!(file.path.extension().unwrap(), "jpg");
        let decoded = image::open(&file.path).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (24, 27));
        let expected = crate::compositor::render_region(&chapter, &assets, 0, 27, 24).unwrap();
        assert_eq!(
            fs::read(&file.path).unwrap(),
            encode(&expected, &options).unwrap()
        );
        assert!(fs::read_to_string(report.report_path)
            .unwrap()
            .contains("JPEG quality 73"));
    }

    #[test]
    fn original_png_slices_have_no_missing_or_repeated_boundary_rows() {
        let directory = TestDirectory::new();
        let (chapter, assets) = image_chapter(RgbaImage::from_fn(13, 4001, |x, y| {
            Rgba([x as u8, (y % 251) as u8, (y / 251) as u8, 255])
        }));
        let options = ExportOptions {
            profile: ExportProfile::Original,
            ..Default::default()
        };
        let report =
            export_chapter_with_options(&chapter, &assets, &directory.0, &options).unwrap();
        assert_eq!(
            report.files.iter().map(|f| f.height).collect::<Vec<_>>(),
            vec![2000, 2000, 1]
        );
        let expected = crate::compositor::render_region(&chapter, &assets, 0, 4001, 13).unwrap();
        let mut combined = RgbaImage::new(13, 4001);
        let mut y = 0;
        for file in report.files {
            image::imageops::replace(
                &mut combined,
                &image::open(file.path).unwrap().to_rgba8(),
                0,
                y,
            );
            y += i64::from(file.height);
        }
        assert_eq!(combined, expected);
    }

    #[test]
    fn episode_warnings_cover_total_bytes_and_adaptive_slice_count() {
        let warnings = episode_warnings(ExportProfile::Webtoon, 101, 20_000_001);
        assert_eq!(warnings.len(), 2);
        assert!(warnings.iter().any(|w| w.contains("100-image")));
        assert!(warnings.iter().any(|w| w.contains("20 MB")));
        assert!(episode_warnings(ExportProfile::Webtoon, 100, 20_000_000).is_empty());
        assert!(episode_warnings(ExportProfile::Tapas, 101, 20_000_001).is_empty());
    }

    #[test]
    fn conflicting_later_file_never_overwrites_or_leaves_first_output() {
        let directory = TestDirectory::new();
        let sentinel = directory.0.join("chapter-00002.png");
        fs::write(&sentinel, b"existing artwork").unwrap();
        let options = ExportOptions {
            profile: ExportProfile::Original,
            ..Default::default()
        };
        assert!(export_chapter_with_options(
            &chapter(16, 2001),
            &AssetBytes::new(),
            &directory.0,
            &options
        )
        .is_err());
        assert_eq!(fs::read(&sentinel).unwrap(), b"existing artwork");
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    }

    #[test]
    fn later_render_failure_removes_staged_images_and_report() {
        let directory = TestDirectory::new();
        let mut chapter = chapter(16, 4000);
        chapter.layers.push(Layer {
            id: 2,
            name: "Missing second-slice asset".into(),
            visible: true,
            locked: false,
            opacity: 1.0,
            x: 0.0,
            y: 2500.0,
            width: 16.0,
            height: 16.0,
            kind: LayerKind::Image {
                source_rect: None,
                clip: None,
                asset_id: 99,
                mask: vec![],
            },
        });
        let options = ExportOptions {
            profile: ExportProfile::Original,
            ..Default::default()
        };
        assert!(
            export_chapter_with_options(&chapter, &AssetBytes::new(), &directory.0, &options)
                .is_err()
        );
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
    }
}
