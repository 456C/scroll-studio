use super::*;
use scroll_studio::characters;

impl Studio {
    pub(super) fn character_reference_picker(&mut self, character_id: u64) {
        if let Some(paths) = rfd::FileDialog::new()
            .set_title("Add character references")
            .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
            .pick_files()
        {
            for path in paths {
                self.import_character_reference(character_id, &path);
            }
        }
    }

    pub(super) fn import_character_reference(&mut self, character_id: u64, path: &std::path::Path) {
        let result = (|| -> anyhow::Result<Vec<u8>> {
            anyhow::ensure!(
                std::fs::metadata(path)?.len() <= 64 * 1024 * 1024,
                "Reference image exceeds 64 MB"
            );
            Ok(std::fs::read(path)?)
        })();
        match result {
            Ok(bytes) => self.import_character_reference_bytes(
                character_id,
                bytes,
                path.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            ),
            Err(e) => self.error = Some(format!("Could not add reference: {e:#}")),
        }
    }

    pub(super) fn import_character_reference_bytes(
        &mut self,
        character_id: u64,
        bytes: Vec<u8>,
        name: String,
    ) {
        let result = (|| -> anyhow::Result<_> {
            let index = self
                .story
                .assets
                .iter()
                .position(|asset| asset.id == character_id && asset.kind == AssetKind::Character)
                .ok_or_else(|| anyhow::anyhow!("Choose a character first"))?;
            anyhow::ensure!(
                self.story.assets[index].character.references.len() < 24,
                "Each character can hold 24 additional references"
            );
            self.check_import_budget(bytes.len())?;
            let image = compositor::decode_image(&bytes)?;
            Ok((index, image.width(), image.height()))
        })();
        match result {
            Ok((index, width, height)) => {
                let command = Command::AddImageAsset {
                    name: name.clone(),
                    width,
                    height,
                    attach: commands::Attach::CharacterReference(character_id),
                };
                let mut fallback_id = None;
                let applied = self.commit(Some(command), |studio| {
                    let id = studio.story.alloc_id();
                    studio.story.assets.push(Asset {
                        id,
                        name: name.clone(),
                        kind: AssetKind::Image,
                        notes: String::new(),
                        prompt: String::new(),
                        width,
                        height,
                        character: CharacterProfile::default(),
                    });
                    studio.story.assets[index]
                        .character
                        .references
                        .push(CharacterReference {
                            asset_id: id,
                            label: name,
                            notes: String::new(),
                        });
                    fallback_id = Some(id);
                });
                let id = applied
                    .and_then(|a| a.created.first().copied())
                    .or(fallback_id)
                    .expect("a reference was added");
                // Image bytes live with the app, outside the command layer.
                self.assets.insert(id, Arc::new(bytes));
                self.selected_asset = Some(character_id);
                self.status = "Reference added".into();
            }
            Err(e) => self.error = Some(format!("Could not add character reference: {e:#}")),
        }
    }

    pub(super) fn place_character_reference(&mut self, character_id: u64, reference_asset_id: u64) {
        if self.assets_only {
            return;
        }
        if self
            .story
            .assets
            .iter()
            .find(|asset| asset.id == character_id)
            .is_some_and(|asset| characters::reference_ids(asset).contains(&reference_asset_id))
        {
            self.place_asset(reference_asset_id, None);
            self.tab = LibraryTab::Chapters;
        }
    }

    pub(super) fn copy_character_sheet(&mut self, character_id: u64) {
        self.start_character_sheet(character_id, None);
    }

    pub(super) fn export_character_sheet(&mut self, character_id: u64) {
        if self.job.is_some() {
            self.status = "Wait for the current file operation to finish.".into();
            return;
        }
        if let Some(mut path) = rfd::FileDialog::new()
            .set_title("Export character reference sheet")
            .add_filter("PNG reference sheet", &["png"])
            .set_file_name("Character references.png")
            .save_file()
        {
            path.set_extension("png");
            self.start_character_sheet(character_id, Some(path));
        }
    }

    fn start_character_sheet(&mut self, character_id: u64, output: Option<PathBuf>) {
        if self.job.is_some() {
            self.status = "Wait for the current file operation to finish.".into();
            return;
        }
        let Some(character) = self
            .story
            .assets
            .iter()
            .find(|asset| asset.id == character_id && asset.kind == AssetKind::Character)
            .cloned()
        else {
            return;
        };
        if characters::reference_ids(&character).is_empty() {
            self.status = "Add reference images to this character first.".into();
            return;
        }
        let story = self.story.clone();
        let assets: AssetBytes = characters::reference_ids(&character)
            .into_iter()
            .filter_map(|id| self.assets.get(&id).map(|bytes| (id, Arc::clone(bytes))))
            .collect();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<_> {
                let sheet = characters::reference_sheet(&story, &assets, &character)?;
                let message = if let Some(path) = output {
                    sheet.save_with_format(&path, image::ImageFormat::Png)?;
                    format!("Reference sheet saved to {}", path.display())
                } else {
                    arboard::Clipboard::new()?.set_image(arboard::ImageData {
                        width: sheet.width() as usize,
                        height: sheet.height() as usize,
                        bytes: std::borrow::Cow::Owned(sheet.into_raw()),
                    })?;
                    "Reference sheet copied".into()
                };
                Ok(JobResult::CharacterSheetDone(message))
            })();
            let _ = tx.send(result);
        });
        self.job = Some(rx);
        self.busy_label = "Preparing character reference sheet…".into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn character_reference_import_preserves_identity_and_is_undoable() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.story = Story::demo();
        let character_id = studio
            .story
            .assets
            .iter()
            .find(|a| a.kind == AssetKind::Character)
            .unwrap()
            .id;
        let original = studio.story.clone();
        let image = image::RgbaImage::from_pixel(4, 8, image::Rgba([10, 20, 30, 255]));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        studio.import_character_reference_bytes(
            character_id,
            bytes.into_inner(),
            "Front view".into(),
        );
        assert!(studio.error.is_none());
        let character = studio
            .story
            .assets
            .iter()
            .find(|a| a.id == character_id)
            .unwrap();
        assert_eq!(character.character.references.len(), 1);
        let reference_id = character.character.references[0].asset_id;
        let pixels = Arc::clone(studio.assets.get(&reference_id).unwrap());
        storage::validate_story(
            &studio.story,
            &assets_for_story(&studio.story, &studio.assets),
        )
        .unwrap();
        let imported = studio.story.clone();
        studio.undo();
        assert_eq!(studio.story, original);
        studio.redo();
        assert_eq!(studio.story, imported);
        assert!(Arc::ptr_eq(
            &pixels,
            studio.assets.get(&reference_id).unwrap()
        ));
    }
}
