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
        json!({ "character_id": 1, "idx": 2 }),
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
