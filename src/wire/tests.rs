use super::*;
use crate::commands::{tests::commands_for, tests::world, Field, Target};
use crate::model::Story;

fn all_commands() -> Vec<Command> {
    let mut commands = Vec::new();
    for story in [Story::new("Untitled"), Story::demo(), world()] {
        commands.extend(commands_for(&story));
    }
    commands
}

#[test]
fn every_public_command_round_trips_through_json() {
    let allow = Policy {
        allow_image_commands: true,
    };
    let mut methods = std::collections::HashSet::new();
    for command in all_commands() {
        let (method, params) = command_request(&command).unwrap();
        assert_eq!(method, format!("edit.{}", command.name()));
        assert!(
            EDIT_METHODS.contains(&method.as_str()),
            "{method} not listed"
        );
        let back = match parse_request(&method, &params, allow).unwrap() {
            Request::Edit(command) => *command,
            other => panic!("{method} parsed as {other:?}"),
        };
        assert_eq!(
            back, command,
            "{method} changed in the round trip: {params}"
        );
        methods.insert(method);
    }
    for method in EDIT_METHODS {
        assert!(methods.contains(method), "{method} is never exercised");
    }
}

#[test]
fn the_wire_format_uses_the_read_api_conventions() {
    let (method, params) = command_request(&Command::SetField {
        target: Target::Story,
        field: Field::VisualStyle,
        value: "Ink".into(),
    })
    .unwrap();
    assert_eq!(method, "edit.set_field");
    assert_eq!(
        params,
        json!({ "target_id": 0, "field": "visual_style", "value": "Ink" })
    );
    let (_, params) = command_request(&Command::SetGender {
        character: 7,
        gender: Some(crate::model::CharacterGender::Female),
    })
    .unwrap();
    assert_eq!(params, json!({ "character_id": 7, "gender": "female" }));
    let (_, params) = command_request(&Command::AddAsset {
        name: "Bg".into(),
        kind: crate::model::AssetKind::Background,
        location: None,
    })
    .unwrap();
    assert_eq!(
        params,
        json!({ "name": "Bg", "kind": "background", "location_id": null })
    );
    let (method, params) = command_request(&Command::AddGroup).unwrap();
    assert_eq!((method.as_str(), params), ("edit.add_group", Value::Null));
}

#[test]
fn hand_written_requests_parse_with_optional_parts_left_out() {
    let none = Policy::default();
    let parse = |method: &str, params: Value| match parse_request(method, &params, none) {
        Ok(Request::Edit(command)) => *command,
        other => panic!("{method}: {other:?}"),
    };
    assert_eq!(
        parse(
            "edit.add_panel",
            json!({ "chapter_id": 3, "panel": { "y": 0, "height": 400, "script": "WIDE" } })
        ),
        Command::AddPanel {
            chapter: 3,
            after: None,
            panel: crate::commands::PanelDraft {
                y: 0.0,
                height: 400.0,
                script: "WIDE".into(),
                ..Default::default()
            },
        }
    );
    assert_eq!(parse("edit.add_group", Value::Null), Command::AddGroup);
    assert_eq!(parse("edit.add_group", json!({})), Command::AddGroup);
    assert_eq!(
        parse(
            "edit.replace_text",
            json!({ "find": "a", "replace": "b", "scope": "story" })
        ),
        Command::ReplaceText {
            find: "a".into(),
            replace: "b".into(),
            scope: crate::commands::Scope::Story,
        }
    );
    assert_eq!(
        parse(
            "edit.batch",
            json!([
                { "method": "add_group" },
                { "method": "set_field", "params": { "target_id": 0, "field": "title", "value": "Rain" } }
            ])
        ),
        Command::Batch(vec![
            Command::AddGroup,
            Command::SetField {
                target: Target::Story,
                field: Field::Title,
                value: "Rain".into(),
            },
        ])
    );
}

#[test]
fn the_boundary_refuses_what_must_not_arrive_from_outside() {
    let none = Policy::default();
    let refused = |method: &str, params: Value| {
        let result = parse_request(method, &params, none);
        assert!(
            result.is_err(),
            "{method} {params} was accepted: {result:?}"
        );
        result.unwrap_err().to_string()
    };
    // Internal inverses have no wire form, by name or inside a batch.
    for name in [
        "restore",
        "restore_lore",
        "insert_layer",
        "remove_layer",
        "remove_last_chapter",
    ] {
        refused(&format!("edit.{name}"), json!({}));
        refused("edit.batch", json!([{ "method": name, "params": {} }]));
    }
    // Image commands need a host that stores the bytes.
    let image = json!({ "asset_id": 3, "width": 10, "height": 10 });
    assert!(refused("edit.add_image_version", image.clone()).contains("image data"));
    refused(
        "edit.batch",
        json!([{ "method": "add_image_version", "params": image.clone() }]),
    );
    let allow = Policy {
        allow_image_commands: true,
    };
    assert!(parse_request("edit.add_image_version", &image, allow).is_ok());
    // Misspelt or unknown parameters are refused instead of silently ignored.
    let typo = refused(
        "edit.set_active_outfit",
        json!({ "character_id": 1, "index": null, "idx": 2 }),
    );
    assert!(typo.contains("idx"), "{typo}");
    refused(
        "edit.add_panel",
        json!({ "chapter_id": 1, "after": 2, "panel": {} }),
    );
    refused(
        "edit.add_panel",
        json!({ "chapter_id": 1, "panel": { "y": 0, "height": 1, "colour": 1 } }),
    );
    // Wrong types, unknown fields and kinds, unknown methods.
    refused(
        "edit.set_field",
        json!({ "target_id": "1", "field": "title", "value": "x" }),
    );
    refused(
        "edit.set_field",
        json!({ "target_id": 1, "field": "next_id", "value": "x" }),
    );
    refused(
        "edit.set_gender",
        json!({ "character_id": 1, "gender": "Female" }),
    );
    // Leaving these out must never mean "clear": null is required for that.
    refused("edit.set_gender", json!({ "character_id": 1 }));
    refused("edit.set_active_outfit", json!({ "character_id": 1 }));
    // A different shape is not an alias.
    refused(
        "edit.replace_text",
        json!({ "find": "a", "replace": "b", "scope": { "story": null } }),
    );
    refused("edit.update_layer", json!({ "layer_id": 1, "opacty": 0.5 }));
    // Batches are flat and bounded.
    refused("edit.batch", json!([{ "method": "batch", "params": [] }]));
    refused(
        "edit.batch",
        Value::Array(vec![json!({ "method": "add_group" }); MAX_BATCH + 1]),
    );
    refused("edit.batch", json!({ "method": "add_group" }));
    refused(
        "edit.add_asset",
        json!({ "name": "x", "kind": "Character" }),
    );
    refused("edit.rewrite_story", json!({}));
    refused("edit.", json!({}));
    refused("story.rewrite", json!({}));
}

#[test]
fn read_requests_go_to_the_read_api() {
    let story = world();
    for method in READ_METHODS {
        let params = match method {
            "panels.list" | "layers.list" => json!({ "chapter_id": story.chapters[0].id }),
            "layer.export" => json!({ "layer_id": story.chapters[0].layers[0].id }),
            "object.get" => json!({ "id": 0 }),
            "panels.at" => json!({ "chapter_id": story.chapters[0].id, "y": 10 }),
            _ => Value::Null,
        };
        let Request::Read(query) = parse_request(method, &params, Policy::default()).unwrap()
        else {
            panic!("{method} is not a read");
        };
        assert_eq!(query.method(), method);
        story.query(&query).unwrap();
    }
}

#[test]
fn optional_parts_may_be_null_or_left_out_but_required_ones_may_not() {
    let none = Policy::default();
    let parse = |method: &str, params: Value| match parse_request(method, &params, none) {
        Ok(Request::Edit(command)) => *command,
        other => panic!("{method} {params}: {other:?}"),
    };
    // Unit commands in batches, with params missing, null or empty.
    for entry in [
        json!({ "method": "add_group" }),
        json!({ "method": "add_group", "params": null }),
        json!({ "method": "add_group", "params": {} }),
    ] {
        assert_eq!(
            parse("edit.batch", json!([entry])),
            Command::Batch(vec![Command::AddGroup])
        );
    }
    // The encoder's own batch entries parse back.
    let parts = vec![Command::AddGroup, Command::OrderChronologyByReading];
    let entries: Vec<_> = parts.iter().map(|c| batch_entry(c).unwrap()).collect();
    assert_eq!(
        parse("edit.batch", Value::Array(entries)),
        Command::Batch(parts)
    );
    // Explicit nulls for optional values, and integral float IDs.
    assert_eq!(
        parse(
            "edit.add_panel",
            json!({ "chapter_id": 3.0, "after_id": null, "panel": { "y": 0, "height": 10 } })
        ),
        Command::AddPanel {
            chapter: 3,
            after: None,
            panel: crate::commands::PanelDraft {
                height: 10.0,
                ..Default::default()
            },
        }
    );
    assert_eq!(
        parse(
            "edit.set_active_outfit",
            json!({ "character_id": 1, "index": null })
        ),
        Command::SetActiveOutfit {
            character: 1,
            index: None
        }
    );
    assert_eq!(
        parse(
            "edit.set_gender",
            json!({ "character_id": 1, "gender": null })
        ),
        Command::SetGender {
            character: 1,
            gender: None
        }
    );
    assert_eq!(
        parse(
            "edit.replace_text",
            json!({ "find": "a", "replace": "b", "scope": { "target_id": 5 } })
        ),
        Command::ReplaceText {
            find: "a".into(),
            replace: "b".into(),
            scope: crate::commands::Scope::Target(Target::Object(5)),
        }
    );
    let allow = Policy {
        allow_image_commands: true,
    };
    let attached = parse_request(
        "edit.add_image_asset",
        &json!({ "name": "x", "width": 1, "height": 1, "attach": { "group_id": 4 } }),
        allow,
    )
    .unwrap();
    assert_eq!(
        attached,
        Request::Edit(Box::new(Command::AddImageAsset {
            name: "x".into(),
            width: 1,
            height: 1,
            attach: crate::commands::Attach::GroupReference(4),
        }))
    );
    assert!(parse_request("edit.delete_layer", &json!({ "layer_id": 1.5 }), none).is_err());
}

#[test]
fn an_exported_layer_can_be_edited_and_written_back() {
    let mut story = Story::demo();
    let none = Policy::default();
    for layer in story.chapters[0].layers.clone() {
        let Request::Read(query) =
            parse_request("layer.export", &json!({ "layer_id": layer.id }), none).unwrap()
        else {
            panic!("layer.export is a read");
        };
        let mut exported = story.query(&query).unwrap()["layer"].clone();
        exported["x"] = json!(layer.x + 12.0);
        // Clients that write optional values as null are accepted.
        if let Some(image) = exported["kind"].get_mut("Image") {
            image["clip"] = Value::Null;
            image["source_rect"] = Value::Null;
        }
        let Request::Edit(command) =
            parse_request("edit.set_layer", &json!({ "layer": exported }), none).unwrap()
        else {
            panic!("edit.set_layer is an edit");
        };
        story.apply(*command).unwrap();
        let moved = story.chapters[0]
            .layers
            .iter()
            .find(|l| l.id == layer.id)
            .unwrap();
        assert_eq!(moved.x, layer.x + 12.0);
        assert_eq!(moved.kind, layer.kind, "only the position changed");
    }
    // Typos deep inside a layer are still caught.
    let text = story.chapters[0]
        .layers
        .iter()
        .find(|l| matches!(l.kind, crate::model::LayerKind::Text { .. }))
        .unwrap();
    let mut exported = serde_json::to_value(text).unwrap();
    exported["kind"]["Text"]["style"]["paddng"] = json!(3);
    assert!(parse_request("edit.set_layer", &json!({ "layer": exported }), none).is_err());
}

#[test]
fn update_layer_changes_only_what_it_names() {
    let mut story = Story::demo();
    let layer = story.chapters[0].layers[0].clone();
    let Request::Edit(command) = parse_request(
        "edit.update_layer",
        &json!({ "layer_id": layer.id, "visible": false, "x": 5 }),
        Policy::default(),
    )
    .unwrap() else {
        panic!("an edit");
    };
    let applied = story.apply(*command).unwrap();
    let updated = &story.chapters[0].layers[0];
    assert!(!updated.visible);
    assert_eq!(updated.x, 5.0);
    assert_eq!(
        (updated.y, &updated.name, &updated.kind),
        (layer.y, &layer.name, &layer.kind)
    );
    story.apply(applied.inverse).unwrap();
    assert_eq!(story.chapters[0].layers[0], layer);
    // Text only applies to text layers.
    let shape = story.chapters[0]
        .layers
        .iter()
        .find(|l| !matches!(l.kind, crate::model::LayerKind::Text { .. }))
        .unwrap()
        .id;
    let text = Command::UpdateLayer {
        layer: shape,
        patch: crate::commands::LayerPatch {
            text: Some("x".into()),
            ..Default::default()
        },
    };
    assert!(story.apply(text).is_err());
}

#[test]
fn scro_files_keep_their_own_spellings() {
    // The wire uses lowercase names; saved stories must not change.
    let mut story = world();
    let mira = story
        .assets
        .iter()
        .position(|a| a.name == "Mira Vale")
        .unwrap();
    story.assets[mira].character.gender = Some(crate::model::CharacterGender::Female);
    let saved = serde_json::to_value(&story).unwrap();
    assert_eq!(saved["assets"][mira]["character"]["gender"], "Female");
    assert_eq!(saved["assets"][mira]["kind"], "Character");
}
