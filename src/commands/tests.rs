use super::*;
use crate::model::{
    Asset, AssetKind, BackgroundLocation, CharacterProfile, LocationProfile, WorldGroup,
};

/// The manifest bytes exactly as `storage::save_story` writes them.
fn manifest(story: &Story) -> Vec<u8> {
    serde_json::to_vec(story).unwrap()
}

fn asset(story: &mut Story, name: &str, kind: AssetKind) -> u64 {
    let id = story.alloc_id();
    story.assets.push(Asset {
        id,
        name: name.into(),
        kind,
        notes: String::new(),
        prompt: String::new(),
        width: 640,
        height: 960,
        character: CharacterProfile::default(),
    });
    id
}

/// A world that uses every object a stage 1 command can touch.
fn world() -> Story {
    let mut story = Story::demo();
    let reference = asset(&mut story, "Mira turnaround", AssetKind::Image);
    let mira = asset(&mut story, "Mira Vale", AssetKind::Character);
    let jun = asset(&mut story, "Jun", AssetKind::Character);
    asset(&mut story, "Station", AssetKind::Background);
    {
        let m = &mut story.assets.iter_mut().find(|a| a.id == mira).unwrap();
        m.notes = "Charcoal coat in every scene.".into();
        m.prompt = "Young woman, short dark bob".into();
        let p = &mut m.character;
        p.tagline = "Misses the last train on purpose".into();
        p.role = "Lead".into();
        p.age = "24".into();
        p.gender = Some(CharacterGender::Female);
        p.pronouns = "she / her".into();
        p.personality = "Dry, curious, kind under pressure".into();
        p.hair = "Short dark bob".into();
        p.eyes = "Grey".into();
        p.consistency = "Cream envelope stays visible".into();
        p.avoid = "Bright colors".into();
        p.outfits = vec![
            CharacterOutfit {
                name: "Rain commute".into(),
                description: "Charcoal coat, cream scarf".into(),
            },
            CharacterOutfit {
                name: "Café".into(),
                description: "Sweater".into(),
            },
        ];
        p.active_outfit = Some(1);
        p.palette = vec![
            CharacterSwatch {
                name: "Coat".into(),
                color: [40, 49, 54, 255],
            },
            CharacterSwatch {
                name: "Scarf".into(),
                color: [230, 220, 200, 255],
            },
        ];
        p.references = vec![CharacterReference {
            asset_id: reference,
            label: "Turnaround".into(),
            notes: "Front and back".into(),
        }];
    }
    let station = locations::add_location(&mut story, "Central station").unwrap();
    story
        .locations
        .iter_mut()
        .find(|l| l.id == station)
        .unwrap()
        .profile = LocationProfile {
        description: "Glass roof, one platform lit".into(),
        atmosphere: "Hushed".into(),
        lighting: "Blue hour, amber windows".into(),
        consistency: "Clock reads 11:58".into(),
        notes: String::new(),
        prompt: "Empty train station at night".into(),
        avoid: "Crowds".into(),
    };
    let group = story.alloc_id();
    story.groups.push(WorldGroup {
        id: group,
        name: "Night shift".into(),
        description: "Everyone still awake".into(),
        color: [120, 140, 200],
        reference_ids: Vec::new(),
        character_ids: vec![mira],
        location_ids: vec![station],
    });
    let note = story.alloc_id();
    story.lore.push(LoreEntry {
        id: note,
        title: "The letter".into(),
        text: "Nobody knows who wrote it.".into(),
        links: vec![mira, station],
    });
    let note_card = lore::pin(&mut story, note, [0.0, 0.0]).unwrap();
    let mira_card = lore::pin(&mut story, mira, [300.0, 40.0]).unwrap();
    let station_card = lore::pin(&mut story, station, [-200.0, 220.0]).unwrap();
    let edge = lore::connect(&mut story, note_card, mira_card).unwrap();
    story
        .lore_board
        .connections
        .iter_mut()
        .find(|e| e.id == edge)
        .unwrap()
        .label = "Received".into();
    lore::connect(&mut story, note_card, station_card).unwrap();
    let _ = jun;
    story
}

fn fixtures() -> Vec<(String, Story)> {
    let mut stories = vec![
        ("empty".to_owned(), Story::new("Untitled")),
        ("demo".to_owned(), Story::demo()),
        ("world".to_owned(), world()),
    ];
    // Real stories: SCROLL_SAMPLE_STORIES=path/to/folder cargo test commands
    if let Some(folder) = std::env::var_os("SCROLL_SAMPLE_STORIES") {
        let mut paths: Vec<_> = std::fs::read_dir(&folder)
            .expect("SCROLL_SAMPLE_STORIES is not a readable folder")
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|path| path.extension().is_some_and(|e| e == "scro"))
            .collect();
        paths.sort();
        assert!(
            !paths.is_empty(),
            "SCROLL_SAMPLE_STORIES has no .scro files"
        );
        for path in paths {
            let (story, _) = crate::storage::load_story(&path)
                .unwrap_or_else(|e| panic!("{}: {e:#}", path.display()));
            stories.push((path.display().to_string(), story));
        }
    }
    stories
}

/// One valid instance of every command that applies to `story`.
fn commands_for(story: &Story) -> Vec<Command> {
    let mut commands = Vec::new();
    for (target, field) in story.text_fields() {
        let current = story.field(target, field).unwrap();
        let value =
            if field == Field::Name && story.object_kind(target) == Some(ObjectKind::Location) {
                format!("{current} (edited)")
            } else {
                format!("{current} edited ✎")
            };
        commands.push(Command::SetField {
            target,
            field,
            value,
        });
        // Clearing is a distinct case: undo must restore the old text exactly.
        if !(field == Field::Name && story.object_kind(target) == Some(ObjectKind::Location)) {
            commands.push(Command::SetField {
                target,
                field,
                value: String::new(),
            });
        }
    }
    for find in ["e", "the", "zzz-not-present"] {
        commands.push(Command::ReplaceText {
            find: find.into(),
            replace: "Ë".into(),
            scope: Scope::Story,
        });
    }
    if let Some(asset) = story.assets.first() {
        commands.push(Command::ReplaceText {
            find: "a".into(),
            replace: "A".into(),
            scope: Scope::Target(Target::Object(asset.id)),
        });
    }
    for chapter in &story.chapters {
        let draft = PanelDraft {
            title: "Inserted".into(),
            y: 0.0,
            height: (chapter.height as f32).min(400.0),
            script: "WIDE".into(),
            notes: String::new(),
            prompt: "Rain".into(),
        };
        if chapter.height >= 1 {
            commands.push(Command::AddPanel {
                chapter: chapter.id,
                after: None,
                panel: draft.clone(),
            });
            if let Some(first) = chapter.panels.first() {
                commands.push(Command::AddPanel {
                    chapter: chapter.id,
                    after: Some(first.id),
                    panel: draft,
                });
            }
        }
    }
    for asset in story
        .assets
        .iter()
        .filter(|a| a.kind == AssetKind::Character)
    {
        let id = asset.id;
        let p = &asset.character;
        for gender in [
            None,
            Some(CharacterGender::Female),
            Some(CharacterGender::Male),
        ] {
            commands.push(Command::SetGender {
                character: id,
                gender,
            });
        }
        if p.outfits.len() < MAX_OUTFITS {
            commands.push(Command::AddOutfit {
                character: id,
                outfit: CharacterOutfit {
                    name: "New look".into(),
                    description: "Raincoat".into(),
                },
            });
        }
        for index in 0..p.outfits.len() {
            commands.push(Command::SetOutfit {
                character: id,
                index,
                outfit: CharacterOutfit {
                    name: "Renamed".into(),
                    description: String::new(),
                },
            });
            commands.push(Command::RemoveOutfit {
                character: id,
                index,
            });
            commands.push(Command::SetActiveOutfit {
                character: id,
                index: Some(index),
            });
        }
        commands.push(Command::SetActiveOutfit {
            character: id,
            index: None,
        });
        if p.palette.len() < MAX_SWATCHES {
            commands.push(Command::AddPaletteColor {
                character: id,
                name: "Accent".into(),
                color: [200, 40, 40, 255],
            });
        }
        for index in 0..p.palette.len() {
            commands.push(Command::SetPaletteColor {
                character: id,
                index,
                name: "Changed".into(),
                color: [1, 2, 3, 4],
            });
            commands.push(Command::RemovePaletteColor {
                character: id,
                index,
            });
        }
        for reference in &p.references {
            commands.push(Command::SetReference {
                character: id,
                asset: reference.asset_id,
                label: "Relabelled".into(),
                notes: String::new(),
            });
            commands.push(Command::RemoveReference {
                character: id,
                asset: reference.asset_id,
            });
        }
    }
    if story.lore.len() < MAX_LORE_NOTES && story.lore_board.cards.len() < lore::MAX_CARDS {
        commands.push(Command::AddLoreNote {
            title: "Untitled note".into(),
            position: [12.5, -40.0],
        });
    }
    let pinned: Vec<_> = story.lore_board.cards.iter().map(|c| c.target).collect();
    for target in lore::targets(story).into_keys() {
        commands.push(Command::AddLoreCard {
            target,
            position: [64.0, 64.0],
        });
        let _ = pinned.contains(&target);
    }
    let cards = &story.lore_board.cards;
    for card in cards {
        for delete_note in [false, true] {
            commands.push(Command::RemoveLoreCard {
                card: card.id,
                delete_note,
            });
        }
        commands.push(Command::MoveLoreCard {
            card: card.id,
            position: [card.position[0] + 33.0, card.position[1] - 7.0],
        });
        commands.push(Command::SetLoreCardColor {
            card: card.id,
            color: [9, 8, 7],
        });
    }
    for (i, a) in cards.iter().enumerate() {
        for b in cards.iter().skip(i + 1).take(3) {
            commands.push(Command::ConnectLore {
                from: a.id,
                to: b.id,
                label: "knows".into(),
            });
        }
    }
    for edge in &story.lore_board.connections {
        commands.push(Command::SetLoreConnection {
            connection: edge.id,
            label: "Changed".into(),
            color: [1, 1, 1],
        });
        commands.push(Command::DisconnectLore {
            connection: edge.id,
        });
    }
    // A batch mixing creation, edits and removal must undo as one step.
    let mut batch = commands.iter().take(6).cloned().collect::<Vec<_>>();
    batch.extend(
        commands
            .iter()
            .filter(|c| {
                matches!(
                    c,
                    Command::AddLoreNote { .. } | Command::RemoveLoreCard { .. }
                )
            })
            .take(2)
            .cloned(),
    );
    commands.push(Command::Batch(batch));
    commands.push(Command::Batch(Vec::new()));
    commands
}

#[test]
fn every_command_then_its_inverse_leaves_the_manifest_byte_identical() {
    let mut checked = 0;
    for (name, original) in fixtures() {
        let before = manifest(&original);
        let commands = commands_for(&original);
        assert!(!commands.is_empty(), "{name}: no commands generated");
        for command in commands {
            let label = format!("{name}: {}", command.name());
            let mut story = original.clone();
            let applied = story
                .apply(command.clone())
                .unwrap_or_else(|e| panic!("{label} failed: {e:#}"));
            let after = manifest(&story);
            let redo = story
                .apply(applied.inverse.clone())
                .unwrap_or_else(|e| panic!("{label} inverse failed: {e:#}"));
            assert!(
                manifest(&story) == before,
                "{label}: inverse did not restore the manifest"
            );
            // The inverse of the inverse is a redo: it reproduces the edited story exactly.
            story
                .apply(redo.inverse)
                .unwrap_or_else(|e| panic!("{label} redo failed: {e:#}"));
            assert!(
                manifest(&story) == after,
                "{label}: redo did not reproduce the edit"
            );
            checked += 1;
        }
    }
    assert!(checked > 100, "only {checked} commands were checked");
}

#[test]
fn edits_from_commands_still_pass_storage_validation() {
    for (name, original) in fixtures() {
        // Image bytes are not part of the command layer; give every asset a valid image.
        let assets: crate::model::AssetBytes = original
            .assets
            .iter()
            .filter(|a| a.width > 0 && a.height > 0)
            .map(|a| (a.id, png_bytes(a.width, a.height).into()))
            .collect();
        for command in commands_for(&original) {
            let mut story = original.clone();
            story.apply(command.clone()).unwrap();
            crate::storage::validate_story(&story, &assets).unwrap_or_else(|e| {
                panic!("{name}: {} made an invalid story: {e:#}", command.name())
            });
        }
    }
}

fn png_bytes(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbaImage::new(width, height);
    let mut bytes = Vec::new();
    image
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
    bytes
}

#[test]
fn failed_commands_change_nothing() {
    let original = world();
    let before = manifest(&original);
    let mira = original
        .assets
        .iter()
        .find(|a| a.name == "Mira Vale")
        .unwrap()
        .id;
    let station = original.locations[0].id;
    let card = original.lore_board.cards[0].id;
    let failing = vec![
        Command::SetField {
            target: Target::Object(999_999),
            field: Field::Name,
            value: "x".into(),
        },
        // A whitelisted field on the wrong kind of object.
        Command::SetField {
            target: Target::Object(station),
            field: Field::Hair,
            value: "x".into(),
        },
        Command::SetField {
            target: Target::Story,
            field: Field::Title,
            value: "x".repeat(SHORT + 1),
        },
        Command::SetField {
            target: Target::Object(station),
            field: Field::Name,
            value: "   ".into(),
        },
        Command::RemoveOutfit {
            character: mira,
            index: 99,
        },
        Command::SetActiveOutfit {
            character: mira,
            index: Some(99),
        },
        Command::AddOutfit {
            character: station,
            outfit: CharacterOutfit::default(),
        },
        Command::MoveLoreCard {
            card,
            position: [f32::NAN, 0.0],
        },
        Command::ConnectLore {
            from: card,
            to: card,
            label: String::new(),
        },
        Command::ReplaceText {
            find: String::new(),
            replace: "x".into(),
            scope: Scope::Story,
        },
        Command::AddPanel {
            chapter: original.chapters[0].id,
            after: None,
            panel: PanelDraft {
                title: String::new(),
                y: 0.0,
                height: original.chapters[0].height as f32 + 1.0,
                script: String::new(),
                notes: String::new(),
                prompt: String::new(),
            },
        },
        // A batch that fails part-way through rolls back its earlier parts.
        Command::Batch(vec![
            Command::SetField {
                target: Target::Story,
                field: Field::Title,
                value: "Changed".into(),
            },
            Command::AddLoreNote {
                title: "Kept?".into(),
                position: [0.0, 0.0],
            },
            Command::RemovePaletteColor {
                character: mira,
                index: 42,
            },
        ]),
        // Internal inverse commands are still validated.
        Command::RestoreLore {
            lore: Vec::new(),
            board: original.lore_board.clone(),
            next_id: original.next_id,
        },
    ];
    for command in failing {
        let mut story = original.clone();
        let name = command.name();
        assert!(story.apply(command).is_err(), "{name} should fail");
        assert!(
            manifest(&story) == before,
            "{name} changed the story while failing"
        );
    }
}

#[test]
fn created_ids_are_reported_and_reused_after_undo_and_redo() {
    let mut story = world();
    let applied = story
        .apply(Command::AddLoreNote {
            title: "Clue".into(),
            position: [0.0, 0.0],
        })
        .unwrap();
    let [note, card] = applied.created[..] else {
        panic!("a note and its card")
    };
    assert!(story.lore.iter().any(|n| n.id == note && n.title == "Clue"));
    assert!(story
        .lore_board
        .cards
        .iter()
        .any(|c| c.id == card && c.target == note));
    let redo = story.apply(applied.inverse).unwrap();
    assert!(!story.lore.iter().any(|n| n.id == note));
    story.apply(redo.inverse).unwrap();
    assert!(story
        .lore_board
        .cards
        .iter()
        .any(|c| c.id == card && c.target == note));
}

#[test]
fn duplicate_pins_and_connections_report_the_existing_object_without_changing_anything() {
    let mut story = world();
    let before = manifest(&story);
    let card = story.lore_board.cards[1].clone();
    let applied = story
        .apply(Command::AddLoreCard {
            target: card.target,
            position: [9.0, 9.0],
        })
        .unwrap();
    assert_eq!(applied.touched, vec![card.id]);
    let edge = story.lore_board.connections[0].clone();
    let applied = story
        .apply(Command::ConnectLore {
            from: edge.to,
            to: edge.from,
            label: "ignored".into(),
        })
        .unwrap();
    assert_eq!(applied.touched, vec![edge.id]);
    assert!(manifest(&story) == before);
}

#[test]
fn deleting_a_note_card_unlinks_it_and_undo_restores_every_link() {
    let mut story = world();
    let before = manifest(&story);
    let note = story.lore[0].id;
    let card = story
        .lore_board
        .cards
        .iter()
        .find(|c| c.target == note)
        .unwrap()
        .id;
    let applied = story
        .apply(Command::RemoveLoreCard {
            card,
            delete_note: true,
        })
        .unwrap();
    assert!(story.lore.iter().all(|n| n.id != note));
    assert!(story
        .lore_board
        .connections
        .iter()
        .all(|e| e.from != card && e.to != card));
    story.apply(applied.inverse).unwrap();
    assert!(manifest(&story) == before);
}

#[test]
fn field_names_round_trip_and_the_whitelist_matches_the_spec() {
    for field in Field::ALL {
        assert_eq!(Field::parse(field.as_str()), Some(field));
    }
    assert_eq!(Field::parse("next_id"), None);
    let names = |kind| {
        Field::allowed_on(kind)
            .iter()
            .map(|f| f.as_str())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(ObjectKind::Story),
        ["title", "synopsis", "visual_style"]
    );
    assert_eq!(
        names(ObjectKind::Panel),
        ["title", "script", "notes", "prompt"]
    );
    for spec in [
        "name",
        "tagline",
        "role",
        "age",
        "pronouns",
        "personality",
        "backstory",
        "relationships",
        "hair",
        "eyes",
        "face",
        "skin",
        "build",
        "distinguishing_features",
        "consistency",
        "avoid",
        "prompt",
        "notes",
    ] {
        assert!(names(ObjectKind::Character).contains(&spec), "{spec}");
    }
    for spec in [
        "name",
        "description",
        "atmosphere",
        "lighting",
        "consistency",
        "avoid",
        "prompt",
        "notes",
    ] {
        assert!(names(ObjectKind::Location).contains(&spec), "{spec}");
    }
}

#[test]
fn internal_inverse_commands_are_marked() {
    let mut story = world();
    let applied = story
        .apply(Command::AddLoreNote {
            title: "x".into(),
            position: [0.0, 0.0],
        })
        .unwrap();
    assert!(applied.inverse.is_internal());
    assert!(!Command::SetField {
        target: Target::Story,
        field: Field::Title,
        value: String::new()
    }
    .is_internal());
}

/// Typical single-frame sheet edits, described as commands, reproduce the edited copy.
#[test]
fn sheet_edits_become_commands_that_reproduce_the_edited_copy() {
    let story = world();
    let original = story
        .assets
        .iter()
        .find(|a| a.name == "Mira Vale")
        .unwrap()
        .clone();
    type Edit = Box<dyn Fn(&mut Asset)>;
    let edits: Vec<Edit> = vec![
        Box::new(|a| a.name.push('!')),
        Box::new(|a| a.character.backstory = "New".into()),
        Box::new(|a| a.notes.clear()),
        Box::new(|a| a.character.gender = Some(CharacterGender::Male)),
        Box::new(|a| {
            a.character.outfits.push(CharacterOutfit {
                name: "Look 3".into(),
                description: String::new(),
            })
        }),
        Box::new(|a| {
            // Removing the active outfit clears the active look, as the sheet does.
            a.character.outfits.remove(1);
            a.character.active_outfit = None;
        }),
        Box::new(|a| {
            a.character.outfits.remove(0);
            a.character.active_outfit = Some(0);
        }),
        Box::new(|a| a.character.outfits[0].description = "Wet coat".into()),
        Box::new(|a| a.character.active_outfit = Some(0)),
        Box::new(|a| {
            a.character.palette.push(CharacterSwatch {
                name: "New".into(),
                color: [0, 0, 0, 255],
            })
        }),
        Box::new(|a| {
            a.character.palette.remove(0);
        }),
        Box::new(|a| a.character.palette[1].color = [1, 2, 3, 255]),
        Box::new(|a| a.character.references[0].notes = "Side view".into()),
        Box::new(|a| a.character.references.clear()),
        Box::new(|a| {
            a.character.tagline.push('x');
            a.prompt = "Changed".into();
            a.character.palette[0].name = "Wool".into();
        }),
    ];
    for (i, edit) in edits.iter().enumerate() {
        let mut edited = original.clone();
        edit(&mut edited);
        let command = character_edit(&original, &edited)
            .unwrap_or_else(|| panic!("edit {i} could not be described"));
        let mut changed = story.clone();
        changed
            .apply(command)
            .unwrap_or_else(|e| panic!("edit {i}: {e:#}"));
        let result = changed.assets.iter().find(|a| a.id == original.id).unwrap();
        assert_eq!(
            result, &edited,
            "edit {i} did not reproduce the sheet's copy"
        );
    }
    let mut other = original.clone();
    other.width += 1;
    assert!(character_edit(&original, &other).is_none());
}

#[test]
fn location_sheet_edits_become_field_commands() {
    let story = world();
    let original: BackgroundLocation = story.locations[0].clone();
    let mut edited = original.clone();
    edited.profile.lighting = "Dawn".into();
    edited.profile.notes = "New".into();
    let command = location_edit(&original, &edited).unwrap();
    let Command::Batch(parts) = &command else {
        panic!("a batch")
    };
    assert_eq!(parts.len(), 2);
    let mut changed = story.clone();
    changed.apply(command).unwrap();
    assert_eq!(changed.locations[0], edited);
}
