//! A description of every method on the wire: what it does and the parameters it takes,
//! as JSON Schema. Addons use it to offer the API to others (AI Lite builds its MCP tools
//! from it); tests keep it in step with [`super::EDIT_METHODS`] and [`super::READ_METHODS`].

use serde_json::{json, Map, Value};

/// One parameter: its name, type, whether it is required, and what it means.
pub struct Param {
    pub name: &'static str,
    pub kind: Kind,
    pub required: bool,
    pub about: &'static str,
}

/// The shape of a parameter's value.
#[derive(Clone, Copy)]
pub enum Kind {
    /// An object ID (a non-negative integer). The story itself is ID 0.
    Id,
    /// An ID, or `null` for none.
    OptionalId,
    Ids,
    Text,
    /// A string from a fixed list.
    OneOf(&'static [&'static str]),
    /// A non-negative integer.
    Count,
    /// A non-negative integer, or `null`.
    OptionalCount,
    Number,
    Flag,
    /// `[x, y]` on the lore board.
    Point,
    /// `[r, g, b]`, 0–255 each.
    Rgb,
    /// `[r, g, b, a]`, 0–255 each.
    Rgba,
    /// An object described in `about`.
    Object,
    /// A list described in `about`.
    #[allow(dead_code)]
    List,
    /// One of the listed strings, or `null`.
    OptionalOneOf(&'static [&'static str]),
    /// Any JSON value described in `about`.
    Any,
}

pub struct MethodDoc {
    pub method: &'static str,
    pub about: &'static str,
    pub params: &'static [Param],
}

impl MethodDoc {
    pub fn is_edit(&self) -> bool {
        self.method.starts_with(super::EDIT_PREFIX)
    }

    /// The parameters as a JSON Schema object.
    pub fn schema(&self) -> Value {
        if self.method == "edit.batch" {
            // A batch's parameters are a list, which a tool schema cannot be: tools wrap
            // it as `{"commands": [...]}`. See [`MethodDoc::wrapped_params`].
            return json!({
                "type": "object",
                "properties": { "commands": {
                    "type": "array",
                    "description": "Commands applied as one step: [{\"method\": \"set_field\", \"params\": {...}}, ...] using edit method names without the `edit.` prefix.",
                    "items": { "type": "object" }
                }},
                "required": ["commands"],
                "additionalProperties": false
            });
        }
        let mut properties = Map::new();
        let mut required = Vec::new();
        for param in self.params {
            let mut schema = param.kind.schema();
            schema["description"] = param.about.into();
            properties.insert(param.name.into(), schema);
            if param.required {
                required.push(param.name);
            }
        }
        json!({
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false
        })
    }

    /// Turn parameters shaped by [`MethodDoc::schema`] back into wire parameters.
    pub fn wrapped_params(&self, params: Value) -> Value {
        match (self.method, params) {
            ("edit.batch", Value::Object(mut map)) => map.remove("commands").unwrap_or(Value::Null),
            (_, params) => params,
        }
    }
}

impl Kind {
    fn schema(self) -> Value {
        let id = json!({ "type": "integer", "minimum": 0 });
        let byte = json!({ "type": "integer", "minimum": 0, "maximum": 255 });
        match self {
            Kind::Id => id,
            Kind::OptionalId => json!({ "type": ["integer", "null"], "minimum": 0 }),
            Kind::Ids => json!({ "type": "array", "items": id }),
            Kind::Text => json!({ "type": "string" }),
            Kind::OneOf(values) => json!({ "type": "string", "enum": values }),
            Kind::Count => json!({ "type": "integer", "minimum": 0 }),
            Kind::OptionalCount => json!({ "type": ["integer", "null"], "minimum": 0 }),
            Kind::Number => json!({ "type": "number" }),
            Kind::Flag => json!({ "type": "boolean" }),
            Kind::Point => {
                json!({ "type": "array", "items": { "type": "number" }, "minItems": 2, "maxItems": 2 })
            }
            Kind::Rgb => json!({ "type": "array", "items": byte, "minItems": 3, "maxItems": 3 }),
            Kind::Rgba => json!({ "type": "array", "items": byte, "minItems": 4, "maxItems": 4 }),
            Kind::Object => json!({ "type": "object" }),
            Kind::List => json!({ "type": "array" }),
            Kind::OptionalOneOf(values) => {
                let mut values: Vec<Value> = values.iter().map(|v| json!(v)).collect();
                values.push(Value::Null);
                json!({ "type": ["string", "null"], "enum": values })
            }
            Kind::Any => json!({}),
        }
    }
}

const fn p(name: &'static str, kind: Kind, about: &'static str) -> Param {
    Param {
        name,
        kind,
        required: true,
        about,
    }
}

const fn opt(name: &'static str, kind: Kind, about: &'static str) -> Param {
    Param {
        name,
        kind,
        required: false,
        about,
    }
}

const FIELDS: &[&str] = &[
    "title",
    "synopsis",
    "visual_style",
    "script",
    "name",
    "notes",
    "prompt",
    "consistency",
    "avoid",
    "tagline",
    "role",
    "age",
    "pronouns",
    "personality",
    "backstory",
    "relationships",
    "face",
    "hair",
    "eyes",
    "skin",
    "build",
    "distinguishing_features",
    "description",
    "atmosphere",
    "lighting",
    "text",
    "board_note",
];

const CHAPTER: Param = p("chapter_id", Kind::Id, "The chapter.");
const CHARACTER: Param = p("character_id", Kind::Id, "The character sheet.");
const CARD: Param = p("card_id", Kind::Id, "The lore board card.");
const LAYER: Param = p("layer_id", Kind::Id, "The layer.");
const GROUP: Param = p("group_id", Kind::Id, "The group.");
const INDEX: Param = p("index", Kind::Count, "Position in the list, from 0.");
const OUTFIT: Param = p(
    "outfit",
    Kind::Object,
    "{\"name\": string, \"description\": string}.",
);
const PANEL_DRAFT: &str =
    "{\"title\", \"y\", \"height\", \"script\", \"notes\", \"prompt\"}; text may be omitted.";

/// Every method, reads first, in the order of [`super::READ_METHODS`] and
/// [`super::EDIT_METHODS`].
pub const METHODS: &[MethodDoc] = &[
    MethodDoc {
        method: "story.summary",
        about: "The story's title, synopsis, visual style, revision and counts.",
        params: &[],
    },
    MethodDoc {
        method: "chapters.list",
        about: "Chapters in reading order with their size and panel and layer counts.",
        params: &[],
    },
    MethodDoc {
        method: "panels.list",
        about: "A chapter's panels, top to bottom, with their text.",
        params: &[CHAPTER],
    },
    MethodDoc {
        method: "layers.list",
        about: "A chapter's layers, bottom to top.",
        params: &[CHAPTER],
    },
    MethodDoc {
        method: "layer.export",
        about: "One layer in full, as `edit.set_layer` accepts it.",
        params: &[LAYER],
    },
    MethodDoc {
        method: "characters.list",
        about: "Character sheets with their fields, outfits and palette.",
        params: &[],
    },
    MethodDoc {
        method: "assets.list",
        about: "Every library sheet: characters, backgrounds and images.",
        params: &[],
    },
    MethodDoc {
        method: "locations.list",
        about: "Locations with their fields and backgrounds.",
        params: &[],
    },
    MethodDoc {
        method: "groups.list",
        about: "Groups with their members and references.",
        params: &[],
    },
    MethodDoc {
        method: "timeline.list",
        about: "Chapters in story-time order with when each happens.",
        params: &[],
    },
    MethodDoc {
        method: "object.get",
        about: "Any object by ID, with its kind and editable text fields.",
        params: &[p("id", Kind::Id, "The object; 0 is the story.")],
    },
    MethodDoc {
        method: "lore.board",
        about: "Lore notes, board cards and connections.",
        params: &[],
    },
    MethodDoc {
        method: "panels.at",
        about: "The panels of a chapter at a height.",
        params: &[
            CHAPTER,
            p("y", Kind::Number, "Height in the chapter, in pixels."),
        ],
    },
    MethodDoc {
        method: "edit.set_field",
        about: "Replace one text field. `object.get` lists the fields an object has.",
        params: &[
            p("target_id", Kind::Id, "The object; 0 is the story."),
            p("field", Kind::OneOf(FIELDS), "The field."),
            p("value", Kind::Text, "The new text."),
        ],
    },
    MethodDoc {
        method: "edit.replace_text",
        about: "Find and replace literal, case-sensitive text in text fields.",
        params: &[
            p("find", Kind::Text, "Text to find."),
            p("replace", Kind::Text, "Replacement."),
            p(
                "scope",
                Kind::Any,
                "\"story\" for every field, or {\"target_id\": id} for one object.",
            ),
        ],
    },
    MethodDoc {
        method: "edit.add_panel",
        about: "Insert a panel after another, or at the end of the chapter.",
        params: &[
            CHAPTER,
            opt("after_id", Kind::OptionalId, "The panel to insert after."),
            p("panel", Kind::Object, PANEL_DRAFT),
        ],
    },
    MethodDoc {
        method: "edit.set_gender",
        about: "Set or clear a character's gender.",
        params: &[
            CHARACTER,
            p("gender", Kind::OptionalOneOf(&["female", "male"]), "The gender, or null to clear it."),
        ],
    },
    MethodDoc {
        method: "edit.add_outfit",
        about: "Add an outfit; the first one becomes the active look.",
        params: &[CHARACTER, OUTFIT],
    },
    MethodDoc {
        method: "edit.set_outfit",
        about: "Replace an outfit.",
        params: &[CHARACTER, INDEX, OUTFIT],
    },
    MethodDoc {
        method: "edit.remove_outfit",
        about: "Remove an outfit.",
        params: &[CHARACTER, INDEX],
    },
    MethodDoc {
        method: "edit.set_active_outfit",
        about: "Choose the active look, or null for none.",
        params: &[
            CHARACTER,
            p("index", Kind::OptionalCount, "The outfit, or null."),
        ],
    },
    MethodDoc {
        method: "edit.add_palette_color",
        about: "Add a named color to a character's palette.",
        params: &[
            CHARACTER,
            p("name", Kind::Text, "Color name."),
            p("color", Kind::Rgba, "The color."),
        ],
    },
    MethodDoc {
        method: "edit.set_palette_color",
        about: "Replace a palette color.",
        params: &[
            CHARACTER,
            INDEX,
            p("name", Kind::Text, "Color name."),
            p("color", Kind::Rgba, "The color."),
        ],
    },
    MethodDoc {
        method: "edit.remove_palette_color",
        about: "Remove a palette color.",
        params: &[CHARACTER, INDEX],
    },
    MethodDoc {
        method: "edit.set_reference",
        about: "Edit a reference image's label and notes.",
        params: &[
            CHARACTER,
            p("asset_id", Kind::Id, "The reference image."),
            p("label", Kind::Text, "Label."),
            p("notes", Kind::Text, "Notes."),
        ],
    },
    MethodDoc {
        method: "edit.remove_reference",
        about: "Detach a reference image from a character; the image stays.",
        params: &[CHARACTER, p("asset_id", Kind::Id, "The reference image.")],
    },
    MethodDoc {
        method: "edit.add_lore_note",
        about: "Create a lore note and its card on the board.",
        params: &[
            p("title", Kind::Text, "The note's title."),
            p("position", Kind::Point, "Where the card goes."),
        ],
    },
    MethodDoc {
        method: "edit.add_lore_card",
        about: "Pin a character, location, group or note to the lore board.",
        params: &[
            p("target_id", Kind::Id, "What to pin."),
            p("position", Kind::Point, "Where the card goes."),
        ],
    },
    MethodDoc {
        method: "edit.remove_lore_card",
        about: "Remove a card and its connections.",
        params: &[
            CARD,
            p(
                "delete_note",
                Kind::Flag,
                "Also delete a note card's note everywhere.",
            ),
        ],
    },
    MethodDoc {
        method: "edit.move_lore_card",
        about: "Move a card on the board.",
        params: &[CARD, p("position", Kind::Point, "New position.")],
    },
    MethodDoc {
        method: "edit.set_lore_card_color",
        about: "Color a card.",
        params: &[CARD, p("color", Kind::Rgb, "The color.")],
    },
    MethodDoc {
        method: "edit.connect_lore",
        about: "Connect two cards with a labelled line.",
        params: &[
            p("from_id", Kind::Id, "First card."),
            p("to_id", Kind::Id, "Second card."),
            p("label", Kind::Text, "What connects them."),
        ],
    },
    MethodDoc {
        method: "edit.set_lore_connection",
        about: "Change a connection's label and color.",
        params: &[
            p("connection_id", Kind::Id, "The connection."),
            p("label", Kind::Text, "Label."),
            p("color", Kind::Rgb, "The color."),
        ],
    },
    MethodDoc {
        method: "edit.disconnect_lore",
        about: "Remove a connection.",
        params: &[p("connection_id", Kind::Id, "The connection.")],
    },
    MethodDoc {
        method: "edit.set_chapter_height",
        about: "Change a chapter's canvas height; panels and layers stay where they are.",
        params: &[CHAPTER, p("height", Kind::Count, "Height in pixels.")],
    },
    MethodDoc {
        method: "edit.add_chapter",
        about: "Append a chapter, optionally with an opening panel.",
        params: &[
            p("name", Kind::Text, "Chapter name."),
            p("width", Kind::Count, "Width in pixels."),
            p("height", Kind::Count, "Height in pixels."),
            opt("opening", Kind::Object, PANEL_DRAFT),
        ],
    },
    MethodDoc {
        method: "edit.set_panel_geometry",
        about: "Move or resize a panel vertically.",
        params: &[
            p("panel_id", Kind::Id, "The panel."),
            p("y", Kind::Number, "Top, in pixels."),
            p("height", Kind::Number, "Height, in pixels."),
        ],
    },
    MethodDoc {
        method: "edit.delete_panel",
        about: "Delete a panel.",
        params: &[p("panel_id", Kind::Id, "The panel.")],
    },
    MethodDoc {
        method: "edit.sort_panels",
        about: "Order a chapter's panels top to bottom.",
        params: &[CHAPTER],
    },
    MethodDoc {
        method: "edit.add_asset",
        about: "Create a library sheet without an image.",
        params: &[
            p("name", Kind::Text, "Sheet name."),
            p(
                "kind",
                Kind::OneOf(&["character", "background", "image"]),
                "Sheet kind.",
            ),
            opt(
                "location_id",
                Kind::OptionalId,
                "File a background under this location.",
            ),
        ],
    },
    MethodDoc {
        method: "edit.add_image_asset",
        about: "Create the sheet for an imported image. Needs image data, which addons cannot send yet.",
        params: &[
            p("name", Kind::Text, "Sheet name."),
            p("width", Kind::Count, "Image width."),
            p("height", Kind::Count, "Image height."),
            p(
                "attach",
                Kind::Any,
                "\"library\", {\"character_id\": id} or {\"group_id\": id}.",
            ),
        ],
    },
    MethodDoc {
        method: "edit.add_image_version",
        about: "Give a sheet a new image. Needs image data, which addons cannot send yet.",
        params: &[
            p("asset_id", Kind::Id, "The sheet."),
            p("width", Kind::Count, "Image width."),
            p("height", Kind::Count, "Image height."),
        ],
    },
    MethodDoc {
        method: "edit.add_layer",
        about: "Add a layer, on top or at `index` (0 is the bottom).",
        params: &[
            CHAPTER,
            opt("index", Kind::OptionalCount, "Position in the stack."),
            p(
                "layer",
                Kind::Object,
                "A layer as `layer.export` returns it; its id is replaced.",
            ),
        ],
    },
    MethodDoc {
        method: "edit.set_layer",
        about: "Replace a layer's settings and content, keeping its ID and place.",
        params: &[p(
            "layer",
            Kind::Object,
            "A layer as `layer.export` returns it.",
        )],
    },
    MethodDoc {
        method: "edit.update_layer",
        about: "Change some of a layer's settings. `text` applies to text layers only.",
        params: &[
            LAYER,
            opt("name", Kind::Text, "Name."),
            opt("visible", Kind::Flag, "Shown."),
            opt("locked", Kind::Flag, "Locked."),
            opt("opacity", Kind::Number, "0 to 1."),
            opt("x", Kind::Number, "Left, in pixels."),
            opt("y", Kind::Number, "Top, in pixels."),
            opt("width", Kind::Number, "Width, in pixels."),
            opt("height", Kind::Number, "Height, in pixels."),
            opt("text", Kind::Text, "Text of a text layer."),
        ],
    },
    MethodDoc {
        method: "edit.delete_layer",
        about: "Delete a layer.",
        params: &[LAYER],
    },
    MethodDoc {
        method: "edit.move_layer",
        about: "Move a layer in its chapter's stack (0 is the bottom).",
        params: &[LAYER, INDEX],
    },
    MethodDoc {
        method: "edit.add_linked_bubble",
        about: "Add a speech bubble joined to another.",
        params: &[p("parent_id", Kind::Id, "The bubble to join.")],
    },
    MethodDoc {
        method: "edit.add_group",
        about: "Create an empty group.",
        params: &[],
    },
    MethodDoc {
        method: "edit.set_group_color",
        about: "Color a group.",
        params: &[GROUP, p("color", Kind::Rgb, "The color.")],
    },
    MethodDoc {
        method: "edit.set_group_members",
        about: "Replace a group's members and references.",
        params: &[
            GROUP,
            p("character_ids", Kind::Ids, "Characters."),
            p("location_ids", Kind::Ids, "Locations."),
            p("reference_ids", Kind::Ids, "Reference images."),
        ],
    },
    MethodDoc {
        method: "edit.remove_group",
        about: "Delete a group, its lore links and its card.",
        params: &[GROUP],
    },
    MethodDoc {
        method: "edit.move_chronology",
        about: "Move a chapter in story time (not reading order), before another or last.",
        params: &[
            CHAPTER,
            opt("before_id", Kind::OptionalId, "The chapter to go before."),
        ],
    },
    MethodDoc {
        method: "edit.set_chronology_entry",
        about: "Say when a chapter happens.",
        params: &[
            CHAPTER,
            p("when", Kind::Text, "When it happens."),
            p("summary", Kind::Text, "What happens."),
        ],
    },
    MethodDoc {
        method: "edit.order_chronology_by_reading",
        about: "Arrange story time to match the reading order.",
        params: &[],
    },
    MethodDoc {
        method: "edit.batch",
        about: "Several edits applied and undone as one step; if one fails, nothing changes.",
        params: &[],
    },
];

/// The description of one method.
pub fn method(name: &str) -> Option<&'static MethodDoc> {
    METHODS.iter().find(|doc| doc.method == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::tests::{commands_for, world};
    use crate::commands::Command;
    use std::collections::BTreeSet;

    #[test]
    fn every_method_is_described_once_in_wire_order() {
        let documented: Vec<_> = METHODS.iter().map(|d| d.method).collect();
        let wire: Vec<_> = super::super::READ_METHODS
            .iter()
            .chain(super::super::EDIT_METHODS.iter())
            .copied()
            .collect();
        assert_eq!(documented, wire);
        for doc in METHODS {
            assert!(!doc.about.is_empty(), "{}", doc.method);
            assert_eq!(doc.schema()["type"], "object", "{}", doc.method);
        }
    }

    #[test]
    fn documented_parameters_match_what_commands_accept() {
        let mut seen = BTreeSet::new();
        for command in commands_for(&world()) {
            let commands = match command {
                Command::Batch(parts) => parts,
                other => vec![other],
            };
            for command in commands {
                let Ok((method, params)) = super::super::command_request(&command) else {
                    continue;
                };
                let doc = method_doc(&method);
                let given: BTreeSet<_> = params
                    .as_object()
                    .map(|p| p.keys().cloned().collect())
                    .unwrap_or_default();
                let names: BTreeSet<_> = doc.params.iter().map(|p| p.name.to_owned()).collect();
                assert!(
                    given.is_subset(&names),
                    "{method}: undocumented {:?}",
                    given.difference(&names).collect::<Vec<_>>()
                );
                for param in doc.params.iter().filter(|p| p.required) {
                    assert!(
                        given.contains(param.name),
                        "{method}: `{}` is not required",
                        param.name
                    );
                }
                // Parameters built from the schema's names parse back.
                super::super::parse_request(&method, &params, Default::default())
                    .or_else(|e| {
                        if command.needs_image_bytes() {
                            Ok(super::super::Request::Edit(Box::new(command.clone())))
                        } else {
                            Err(e)
                        }
                    })
                    .unwrap();
                seen.insert(method);
            }
        }
        assert!(seen.len() > 30, "only {} methods exercised", seen.len());
    }

    #[test]
    fn a_batch_is_wrapped_for_tools() {
        let doc = method_doc("edit.batch");
        assert_eq!(doc.schema()["required"][0], "commands");
        let params = doc.wrapped_params(serde_json::json!({ "commands": [] }));
        assert_eq!(params, serde_json::json!([]));
    }

    fn method_doc(name: &str) -> &'static MethodDoc {
        method(name).unwrap_or_else(|| panic!("{name} is not documented"))
    }
}
