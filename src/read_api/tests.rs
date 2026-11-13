use super::*;
use crate::commands::{tests::world, Command, PanelDraft};

fn ask(story: &Story, method: &str, params: Value) -> Value {
    let query = Query::parse(method, &params).unwrap_or_else(|e| panic!("{method}: {e:#}"));
    assert_eq!(query.method(), method);
    // Parameters round-trip, so a host can echo a query back.
    assert_eq!(Query::parse(method, &query.params()).unwrap(), query);
    story
        .query(&query)
        .unwrap_or_else(|e| panic!("{method}: {e:#}"))
}

fn list<'a>(answer: &'a Value, key: &str) -> &'a Vec<Value> {
    answer[key]
        .as_array()
        .unwrap_or_else(|| panic!("`{key}` is not a list in {answer}"))
}

const ALL: [&str; 12] = [
    "story.summary",
    "chapters.list",
    "panels.list",
    "layers.list",
    "characters.list",
    "assets.list",
    "locations.list",
    "groups.list",
    "timeline.list",
    "object.get",
    "lore.board",
    "panels.at",
];

fn params_for(story: &Story, method: &str) -> Value {
    let chapter = story.chapters[0].id;
    match method {
        "panels.list" | "layers.list" => json!({ "chapter_id": chapter }),
        "object.get" => json!({ "id": STORY_ID }),
        "panels.at" => json!({ "chapter_id": chapter, "y": 200.0 }),
        _ => Value::Null,
    }
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
    assert_eq!(list(&chapters, "chapters")[0]["id"], story.chapters[0].id);
    assert_eq!(
        list(&chapters, "chapters")[0]["panel_count"],
        story.chapters[0].panels.len()
    );

    let chapter = story.chapters[0].id;
    let panels = ask(&story, "panels.list", json!({ "chapter_id": chapter }));
    assert_eq!(panels["chapter_id"], chapter);
    let panels = list(&panels, "panels");
    assert_eq!(panels.len(), story.chapters[0].panels.len());
    assert_eq!(panels[0]["script"], story.chapters[0].panels[0].script);
    assert_eq!(panels[1]["index"], 1);

    let characters = ask(&story, "characters.list", Value::Null);
    let characters = list(&characters, "characters");
    let mira = characters
        .iter()
        .find(|c| c["name"] == "Mira Vale")
        .unwrap();
    assert_eq!(mira["tagline"], "Misses the last train on purpose");
    assert_eq!(mira["has_image"], true);
    let sketch = characters
        .iter()
        .find(|c| c["name"] == "Ari (sketch)")
        .unwrap();
    assert_eq!(sketch["has_image"], false);

    let assets = ask(&story, "assets.list", Value::Null);
    let assets = list(&assets, "assets");
    assert!(assets.iter().any(|a| a["asset_kind"] == "background"));
    assert!(assets.iter().any(|a| a["asset_kind"] == "image"));
    assert!(assets.iter().all(|a| a["asset_kind"] != "character"));

    let locations = ask(&story, "locations.list", Value::Null);
    assert_eq!(
        list(&locations, "locations")[0]["description"],
        story.locations[0].profile.description
    );
    let groups = ask(&story, "groups.list", Value::Null);
    assert_eq!(list(&groups, "groups")[0]["name"], "Night shift");
}

#[test]
fn layers_list_exposes_lettering_but_never_pixels() {
    let mut story = Story::demo();
    let id = story.alloc_id();
    story.chapters[0].layers.push(Layer {
        extensions: Default::default(),
        id,
        name: "Mira's line".into(),
        visible: true,
        locked: false,
        opacity: 1.0,
        x: 10.0,
        y: 20.0,
        width: 300.0,
        height: 120.0,
        kind: LayerKind::Text {
            text: "Then I guess I'm walking.".into(),
            color: [0, 0, 0, 255],
            size: 32.0,
            bubble: true,
            style: Default::default(),
        },
    });
    let chapter = &story.chapters[0];
    assert!(chapter.layers.len() > 1);
    let answer = ask(&story, "layers.list", json!({ "chapter_id": chapter.id }));
    let layers = list(&answer, "layers");
    assert_eq!(layers.len(), chapter.layers.len());
    for (layer, value) in chapter.layers.iter().zip(layers) {
        assert_eq!(value["id"], layer.id);
        assert_eq!(value["name"], layer.name);
        if let LayerKind::Text { text, .. } = &layer.kind {
            assert_eq!(value["kind"], "text");
            assert_eq!(value["text"], *text);
        }
    }
    let line = layers.iter().find(|l| l["id"] == id).unwrap();
    assert_eq!(line["text"], "Then I guess I'm walking.");
    assert_eq!(line["bubble"], true);
    // Layers are objects too: their name and lettered text are editable fields.
    let object = ask(&story, "object.get", json!({ "id": id }));
    assert_eq!(object["kind"], "text_layer");
    assert_eq!(object["fields"]["text"], "Then I guess I'm walking.");
    assert_eq!(object["details"]["chapter_id"], story.chapters[0].id);
}

#[test]
fn timeline_lists_chapters_in_chronological_order() {
    let mut story = world();
    let first = story.chapters[0].id;
    story
        .apply(Command::AddChapter {
            name: "Two".into(),
            width: 800,
            height: 6400,
            opening: None,
        })
        .unwrap();
    story
        .apply(Command::MoveChronology {
            chapter: first,
            before: None,
        })
        .unwrap();
    let answer = ask(&story, "timeline.list", Value::Null);
    let entries = list(&answer, "entries");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[1]["chapter_id"], first);
    assert_eq!(entries[1]["when"], "Before dawn");
    let chapter = ask(&story, "object.get", json!({ "id": first }));
    assert_eq!(chapter["details"]["timeline_index"], 1);
    assert_eq!(chapter["details"]["summary"], "The last train");
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
        assert!(
            object["details"].is_object(),
            "details always present: {object}"
        );
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
        "layer",
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
    assert_eq!(character["details"]["asset_kind"], "character");
}

#[test]
fn lore_board_uses_the_object_get_vocabulary() {
    let story = world();
    let board = ask(&story, "lore.board", Value::Null);
    let cards = list(&board, "cards");
    assert_eq!(cards.len(), story.lore_board.cards.len());
    let note = cards
        .iter()
        .find(|c| c["target_kind"] == "lore_note")
        .unwrap();
    assert_eq!(note["title"], "The letter");
    let target = ask(&story, "object.get", json!({ "id": note["target_id"] }));
    assert_eq!(target["kind"], note["target_kind"]);
    assert_eq!(
        list(&board, "connections").len(),
        story.lore_board.connections.len()
    );
}

#[test]
fn panels_at_prefers_the_shortest_covering_panel() {
    let mut story = world();
    let chapter = story.chapters[0].id;
    let outer = story.chapters[0].panels[0].clone();
    let draft = |y: f32, height: f32| PanelDraft {
        title: "Inset".into(),
        y,
        height,
        script: String::new(),
        notes: String::new(),
        prompt: String::new(),
    };
    let add = |story: &mut Story, y, height| {
        story
            .apply(Command::AddPanel {
                chapter,
                after: None,
                panel: draft(y, height),
            })
            .unwrap()
            .created[0]
    };
    // A partial overlap that reaches far down must not win over the panel it overlaps.
    let long = add(&mut story, outer.y + 50.0, 3000.0);
    let inset = add(&mut story, outer.y + 10.0, 20.0);
    let tie = add(&mut story, outer.y + 10.0, 20.0);
    let at =
        |story: &Story, y: f32| ask(story, "panels.at", json!({ "chapter_id": chapter, "y": y }));
    assert_eq!(
        at(&story, outer.y + 15.0)["panel"]["id"],
        tie,
        "equal panels: last wins"
    );
    assert_eq!(at(&story, outer.y + 60.0)["panel"]["id"], outer.id);
    assert_eq!(
        at(&story, outer.y + outer.height + 5.0)["panel"]["id"],
        long
    );
    assert_eq!(at(&story, outer.y + 1.0)["panel"]["id"], outer.id);
    // The bottom edge belongs to the next panel down, not this one.
    assert_ne!(at(&story, outer.y + 10.0 + 20.0)["panel"]["id"], inset);
    let empty = at(&story, -50.0);
    assert_eq!(empty["panel"], Value::Null);
    assert_eq!(empty["chapter_id"], chapter);
}

#[test]
fn numbers_print_in_their_shortest_form() {
    let mut story = world();
    let panel = story.chapters[0].panels[0].id;
    story
        .apply(Command::SetPanelGeometry {
            panel,
            y: 0.1,
            height: 1.3,
        })
        .unwrap();
    let object = ask(&story, "object.get", json!({ "id": panel }));
    assert_eq!(object["details"]["y"].to_string(), "0.1");
    assert_eq!(object["details"]["height"].to_string(), "1.3");
}

#[test]
fn ids_accept_integral_floats_and_reject_everything_else() {
    let story = world();
    let id = story.assets[0].id;
    let as_float = Query::parse("object.get", &json!({ "id": id as f64 })).unwrap();
    assert_eq!(as_float, Query::ObjectGet { id });
    for bad in [
        json!({ "id": -1 }),
        json!({ "id": 1.5 }),
        json!({ "id": "1" }),
        json!({ "id": 1e300 }),
        json!({ "id": null }),
        json!([1]),
        json!("id"),
        Value::Null,
    ] {
        assert!(Query::parse("object.get", &bad).is_err(), "{bad}");
    }
    let chapter = story.chapters[0].id;
    for y in [json!(1e39), json!(-2e6), json!("top"), Value::Null] {
        let params = json!({ "chapter_id": chapter, "y": y });
        assert!(Query::parse("panels.at", &params).is_err(), "{params}");
    }
    assert!(Query::parse("story.rewrite", &Value::Null).is_err());
    assert!(story.query(&Query::ObjectGet { id: 999_999 }).is_err());
    assert!(story
        .query(&Query::PanelsList { chapter: 999_999 })
        .is_err());
}

#[test]
fn every_query_survives_hostile_params_without_panicking() {
    let story = world();
    let hostile = [
        Value::Null,
        json!([]),
        json!("x"),
        json!(42),
        json!({ "id": u64::MAX, "chapter_id": u64::MAX, "y": 1e300 }),
        json!({ "id": 0, "chapter_id": 0, "y": -0.0 }),
        json!({ "chapter_id": story.chapters[0].id, "y": f32::MAX as f64 }),
    ];
    for method in ALL {
        for params in &hostile {
            if let Ok(query) = Query::parse(method, params) {
                let _ = story.query(&query);
            }
        }
    }
    // A card whose target vanished (an older story) still answers.
    let mut odd = story.clone();
    odd.lore_board.cards[0].target = 999_999;
    let board = odd.query(&Query::LoreBoard).unwrap();
    assert_eq!(board["cards"][0]["target_kind"], Value::Null);
}

/// Answers may only use these keys, so pixels, masks, clips or file paths cannot leak in
/// through a new field without this test noticing.
#[test]
fn answers_only_use_known_keys() {
    const KEYS: &[&str] = &[
        "active_outfit",
        "asset_id",
        "asset_ids",
        "asset_kind",
        "assets",
        "atmosphere",
        "avoid",
        "backstory",
        "board_note",
        "bubble",
        "build",
        "cards",
        "chapter_id",
        "chapters",
        "character_ids",
        "characters",
        "color",
        "connections",
        "consistency",
        "counts",
        "description",
        "details",
        "distinguishing_features",
        "entries",
        "eyes",
        "face",
        "fields",
        "from_id",
        "gender",
        "groups",
        "hair",
        "has_image",
        "height",
        "id",
        "index",
        "kind",
        "label",
        "layer_count",
        "layer_ids",
        "layers",
        "lighting",
        "link_ids",
        "location_ids",
        "locations",
        "locked",
        "lore_cards",
        "lore_connections",
        "lore_notes",
        "name",
        "note",
        "notes",
        "opacity",
        "outfits",
        "palette",
        "panel",
        "panel_count",
        "panel_ids",
        "panels",
        "personality",
        "position",
        "prompt",
        "pronouns",
        "reference_ids",
        "references",
        "relationships",
        "role",
        "age",
        "script",
        "skin",
        "summary",
        "synopsis",
        "tagline",
        "target_id",
        "target_kind",
        "text",
        "timeline_index",
        "title",
        "to_id",
        "visible",
        "visual_style",
        "when",
        "width",
        "x",
        "y",
    ];
    fn walk(value: &Value, method: &str) {
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    assert!(
                        KEYS.contains(&key.as_str()),
                        "{method}: unexpected key `{key}`"
                    );
                    walk(child, method);
                }
            }
            Value::Array(items) => items.iter().for_each(|item| walk(item, method)),
            _ => {}
        }
    }
    for story in [world(), Story::demo()] {
        for method in ALL {
            walk(&ask(&story, method, params_for(&story, method)), method);
        }
        for (target, _) in story.text_fields() {
            let id = match target {
                Target::Story => STORY_ID,
                Target::Object(id) => id,
            };
            walk(
                &ask(&story, "object.get", json!({ "id": id })),
                "object.get",
            );
        }
    }
}

/// `layer.export` is left out of the key whitelist on purpose: it returns the `.scro` layer
/// format so it can be written back with `edit.set_layer`.
#[test]
fn layer_export_returns_the_write_form_and_survives_hostile_params() {
    let story = Story::demo();
    let layer = &story.chapters[0].layers[0];
    let answer = ask(&story, "layer.export", json!({ "layer_id": layer.id }));
    assert_eq!(answer["chapter_id"], story.chapters[0].id);
    let back: Layer = serde_json::from_value(answer["layer"].clone()).unwrap();
    assert_eq!(&back, layer);
    for params in [
        Value::Null,
        json!({ "layer_id": u64::MAX }),
        json!({ "layer_id": -1 }),
        json!({ "layer_id": "1" }),
    ] {
        if let Ok(query) = Query::parse("layer.export", &params) {
            assert!(story.query(&query).is_err());
        }
    }
}
