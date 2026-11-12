use super::*;
use crate::commands::{tests::world, Command};

fn ask(story: &Story, method: &str, params: Value) -> Value {
    let query = Query::parse(method, &params).unwrap_or_else(|e| panic!("{method}: {e:#}"));
    assert_eq!(query.method(), method);
    // Parameters round-trip, so a host can echo a query back.
    assert_eq!(Query::parse(method, &query.params()).unwrap(), query);
    story
        .query(&query)
        .unwrap_or_else(|e| panic!("{method}: {e:#}"))
}

#[test]
fn summary_and_lists_describe_the_story() {
    let story = world();
    let summary = ask(&story, "story.summary", Value::Null);
    assert_eq!(summary["title"], story.title);
    assert_eq!(summary["visual_style"], story.visual_style);
    assert_eq!(summary["counts"]["chapters"], story.chapters.len());
    assert_eq!(
        summary["counts"]["lore_cards"],
        story.lore_board.cards.len()
    );

    let chapters = ask(&story, "chapters.list", Value::Null);
    assert_eq!(chapters[0]["id"], story.chapters[0].id);
    assert_eq!(chapters[0]["panel_count"], story.chapters[0].panels.len());

    let chapter = story.chapters[0].id;
    let panels = ask(&story, "panels.list", json!({ "chapter": chapter }));
    assert_eq!(
        panels.as_array().unwrap().len(),
        story.chapters[0].panels.len()
    );
    assert_eq!(panels[0]["script"], story.chapters[0].panels[0].script);

    let characters = ask(&story, "characters.list", Value::Null);
    let mira = characters
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Mira Vale")
        .unwrap();
    assert_eq!(mira["tagline"], "Misses the last train on purpose");
    assert!(characters.as_array().unwrap().iter().all(|c| story
        .assets
        .iter()
        .any(|a| a.id == c["id"] && a.kind == AssetKind::Character)));

    let locations = ask(&story, "locations.list", Value::Null);
    assert_eq!(
        locations[0]["description"],
        story.locations[0].profile.description
    );
    let groups = ask(&story, "groups.list", Value::Null);
    assert_eq!(groups[0]["name"], "Night shift");
}

#[test]
fn object_get_returns_every_whitelisted_field_for_every_kind() {
    let story = world();
    let mut kinds = std::collections::HashSet::new();
    for (target, field) in story.text_fields() {
        let id = match target {
            Target::Story => STORY_ID,
            Target::Object(id) => id,
        };
        let object = ask(&story, "object.get", json!({ "id": id }));
        assert_eq!(object["id"], id);
        assert_eq!(
            object["fields"][field.as_str()],
            story.field(target, field).unwrap()
        );
        kinds.insert(object["kind"].as_str().unwrap().to_owned());
    }
    for kind in [
        "story",
        "panel",
        "character",
        "asset",
        "location",
        "lore_note",
        "lore_card",
        "chapter",
        "group",
    ] {
        assert!(kinds.contains(kind), "{kind} not covered");
    }
    let edge = story.lore_board.connections[0].id;
    let connection = ask(&story, "object.get", json!({ "id": edge }));
    assert_eq!(connection["kind"], "lore_connection");
    assert_eq!(connection["details"]["label"], "Received");
    let mira = story
        .assets
        .iter()
        .find(|a| a.name == "Mira Vale")
        .unwrap()
        .id;
    let character = ask(&story, "object.get", json!({ "id": mira }));
    assert_eq!(character["details"]["outfits"][1]["name"], "Café");
    assert_eq!(character["details"]["active_outfit"], 1);
    assert_eq!(character["details"]["gender"], "female");
}

#[test]
fn lore_board_names_each_card() {
    let story = world();
    let board = ask(&story, "lore.board", Value::Null);
    let cards = board["cards"].as_array().unwrap();
    assert_eq!(cards.len(), story.lore_board.cards.len());
    let note = cards.iter().find(|c| c["kind"] == "note").unwrap();
    assert_eq!(note["title"], "The letter");
    assert_eq!(
        board["connections"].as_array().unwrap().len(),
        story.lore_board.connections.len()
    );
}

#[test]
fn panels_at_finds_the_innermost_panel_or_null() {
    let mut story = world();
    let chapter = story.chapters[0].id;
    let outer = story.chapters[0].panels[0].clone();
    let inner = story
        .apply(Command::AddPanel {
            chapter,
            after: None,
            panel: crate::commands::PanelDraft {
                title: "Inset".into(),
                y: outer.y + 10.0,
                height: 20.0,
                script: String::new(),
                notes: String::new(),
                prompt: String::new(),
            },
        })
        .unwrap()
        .created[0];
    let at = |y: f32| ask(&story, "panels.at", json!({ "chapter": chapter, "y": y }));
    assert_eq!(at(outer.y + 15.0)["id"], inner);
    assert_eq!(at(outer.y + 1.0)["id"], outer.id);
    assert_eq!(at(-50.0), Value::Null);
}

#[test]
fn bad_queries_are_errors_and_answers_never_carry_image_bytes() {
    let story = world();
    for (method, params) in [
        ("story.rewrite", Value::Null),
        ("panels.list", Value::Null),
        (
            "panels.at",
            json!({ "chapter": story.chapters[0].id, "y": "top" }),
        ),
        ("object.get", json!({ "id": -1 })),
    ] {
        assert!(Query::parse(method, &params).is_err(), "{method} {params}");
    }
    assert!(story.query(&Query::ObjectGet { id: 999_999 }).is_err());
    assert!(story
        .query(&Query::PanelsList { chapter: 999_999 })
        .is_err());
    // Answers are text and numbers only: nothing resembling encoded image data.
    for method in [
        "story.summary",
        "chapters.list",
        "characters.list",
        "lore.board",
    ] {
        let text = ask(&story, method, Value::Null).to_string();
        assert!(
            text.len() < 64 * 1024,
            "{method} answer is suspiciously large"
        );
        assert!(
            !text.contains("PNG") && !text.contains("base64"),
            "{method}"
        );
    }
}
