//! Read-only queries over a story, answered as JSON.
//!
//! This is the read half of the command layer: addons (and later the MCP server) call
//! these by name with JSON parameters, using the same transport as commands. Answers
//! describe text and structure only; image bytes, masks and clips are never returned.
//!
//! The answer format is a contract with addons:
//! - List methods answer an object holding one named array (`{"chapters": [...]}`), so
//!   fields can be added later without breaking readers.
//! - References to other objects end in `_id` or `_ids`.
//! - Kinds and enum values use the lowercase wire names below, never UI labels.
//! - `object.get` always answers `{id, kind, fields, details}`: `fields` holds the
//!   whitelisted text an addon may edit with `SetField`, `details` the read-only rest.
//! - Coordinates are stored as 32-bit floats and printed in their shortest exact form.
//! - The story itself is object [`STORY_ID`] (0); every other ID is positive.

use crate::{
    commands::{Field, ObjectKind, Target, STORY_ID},
    lore,
    model::{AssetKind, Chapter, Layer, LayerKind, Panel, Story},
    timeline,
};
use anyhow::{bail, ensure, Context, Result};
use serde_json::{json, Map, Value};

/// Largest integer every JSON runtime represents exactly (2^53).
const MAX_JSON_INTEGER: f64 = 9_007_199_254_740_992.0;
/// Canvas coordinates beyond this are outside any chapter.
const COORDINATE_LIMIT: f64 = 1_000_000.0;

/// A read-only question about the story.
#[derive(Clone, Debug, PartialEq)]
pub enum Query {
    /// Title, synopsis, visual style and object counts.
    StorySummary,
    /// Chapters in reading order.
    ChaptersList,
    /// One chapter's panels, in stored order.
    PanelsList {
        chapter: u64,
    },
    /// One chapter's layers, bottom to top, with their lettered text.
    LayersList {
        chapter: u64,
    },
    CharactersList,
    /// Backgrounds and images (characters have their own list).
    AssetsList,
    LocationsList,
    GroupsList,
    /// Every chapter in chronological (not reading) order, with its time and summary.
    TimelineList,
    /// The text of one object (or the story itself, with [`STORY_ID`]) and its details.
    ObjectGet {
        id: u64,
    },
    /// Lore board cards and connections.
    LoreBoard,
    /// The panel covering a canvas position, or `null`. Where panels overlap, the shortest
    /// covering panel wins, and among equals the last in stored order. A panel covers
    /// `y` from its top up to, not including, its bottom.
    PanelsAt {
        chapter: u64,
        y: f32,
    },
}

impl Query {
    /// The method name used on the wire.
    pub fn method(&self) -> &'static str {
        match self {
            Query::StorySummary => "story.summary",
            Query::ChaptersList => "chapters.list",
            Query::PanelsList { .. } => "panels.list",
            Query::LayersList { .. } => "layers.list",
            Query::CharactersList => "characters.list",
            Query::AssetsList => "assets.list",
            Query::LocationsList => "locations.list",
            Query::GroupsList => "groups.list",
            Query::TimelineList => "timeline.list",
            Query::ObjectGet { .. } => "object.get",
            Query::LoreBoard => "lore.board",
            Query::PanelsAt { .. } => "panels.at",
        }
    }

    /// Build a query from a method name and its JSON parameters (an object, or `null` for
    /// methods without parameters).
    pub fn parse(method: &str, params: &Value) -> Result<Query> {
        let id = |key: &str| -> Result<u64> {
            params
                .get(key)
                .and_then(json_id)
                .with_context(|| format!("{method} needs `{key}` as a non-negative integer"))
        };
        Ok(match method {
            "story.summary" => Query::StorySummary,
            "chapters.list" => Query::ChaptersList,
            "panels.list" => Query::PanelsList {
                chapter: id("chapter_id")?,
            },
            "layers.list" => Query::LayersList {
                chapter: id("chapter_id")?,
            },
            "characters.list" => Query::CharactersList,
            "assets.list" => Query::AssetsList,
            "locations.list" => Query::LocationsList,
            "groups.list" => Query::GroupsList,
            "timeline.list" => Query::TimelineList,
            "object.get" => Query::ObjectGet { id: id("id")? },
            "lore.board" => Query::LoreBoard,
            "panels.at" => {
                let y = params
                    .get("y")
                    .and_then(Value::as_f64)
                    .filter(|y| y.abs() <= COORDINATE_LIMIT)
                    .context("panels.at needs `y` as a number within ±1,000,000")?;
                Query::PanelsAt {
                    chapter: id("chapter_id")?,
                    y: y as f32,
                }
            }
            other => bail!("Unknown query `{other}`"),
        })
    }

    /// The parameters [`Query::parse`] accepts for this query.
    pub fn params(&self) -> Value {
        match self {
            Query::PanelsList { chapter } | Query::LayersList { chapter } => {
                json!({ "chapter_id": chapter })
            }
            Query::ObjectGet { id } => json!({ "id": id }),
            Query::PanelsAt { chapter, y } => json!({ "chapter_id": chapter, "y": num(*y) }),
            _ => Value::Null,
        }
    }
}

/// An ID from JSON: a non-negative integer, or an integral float that JSON runtimes
/// without integers produce (up to 2^53).
fn json_id(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| {
        value
            .as_f64()
            .filter(|v| v.fract() == 0.0 && (0.0..=MAX_JSON_INTEGER).contains(v))
            .map(|v| v as u64)
    })
}

/// A stored 32-bit float in its shortest exact decimal form (0.1, not 0.10000000149…).
fn num(value: f32) -> Value {
    value
        .to_string()
        .parse::<f64>()
        .ok()
        .and_then(serde_json::Number::from_f64)
        .map_or(Value::Null, Value::Number)
}

fn kind_name(kind: ObjectKind) -> &'static str {
    match kind {
        ObjectKind::Story => "story",
        ObjectKind::Panel => "panel",
        ObjectKind::Character => "character",
        ObjectKind::Asset => "asset",
        ObjectKind::Location => "location",
        ObjectKind::LoreNote => "lore_note",
        ObjectKind::LoreCard => "lore_card",
        ObjectKind::LoreConnection => "lore_connection",
        ObjectKind::Chapter => "chapter",
        ObjectKind::Group => "group",
    }
}

/// The object kind a lore card points at, in the same vocabulary as `object.get`.
fn lore_kind_name(kind: lore::Kind) -> &'static str {
    match kind {
        lore::Kind::Note => "lore_note",
        lore::Kind::Character => "character",
        lore::Kind::Location => "location",
        lore::Kind::Group => "group",
    }
}

fn asset_kind_name(kind: AssetKind) -> &'static str {
    match kind {
        AssetKind::Character => "character",
        AssetKind::Background => "background",
        AssetKind::Image => "image",
    }
}

fn gender_name(gender: crate::model::CharacterGender) -> &'static str {
    match gender {
        crate::model::CharacterGender::Female => "female",
        crate::model::CharacterGender::Male => "male",
    }
}

fn panel_json(chapter: &Chapter, index: usize, panel: &Panel) -> Value {
    json!({
        "id": panel.id,
        "chapter_id": chapter.id,
        "index": index,
        "title": panel.title,
        "y": num(panel.y),
        "height": num(panel.height),
        "script": panel.script,
        "notes": panel.notes,
        "prompt": panel.prompt,
    })
}

fn layer_json(chapter: &Chapter, index: usize, layer: &Layer) -> Value {
    let mut value = json!({
        "id": layer.id,
        "chapter_id": chapter.id,
        "index": index,
        "name": layer.name,
        "visible": layer.visible,
        "locked": layer.locked,
        "opacity": num(layer.opacity),
        "x": num(layer.x),
        "y": num(layer.y),
        "width": num(layer.width),
        "height": num(layer.height),
    });
    let (kind, extra) = match &layer.kind {
        LayerKind::Text { text, bubble, .. } => ("text", json!({ "text": text, "bubble": bubble })),
        // Only the reference; never pixels, masks or clips.
        LayerKind::Image { asset_id, .. } => ("image", json!({ "asset_id": asset_id })),
        LayerKind::Stroke { .. } => ("stroke", Value::Null),
        LayerKind::Shape { .. } => ("shape", Value::Null),
        LayerKind::Effect { .. } => ("effect", Value::Null),
    };
    value["kind"] = kind.into();
    if let Value::Object(extra) = extra {
        value.as_object_mut().expect("object").extend(extra);
    }
    value
}

impl Story {
    /// Answer a read-only query. Never changes the story.
    pub fn query(&self, query: &Query) -> Result<Value> {
        Ok(match query {
            Query::StorySummary => {
                let characters = self
                    .assets
                    .iter()
                    .filter(|a| a.kind == AssetKind::Character)
                    .count();
                json!({
                    "title": self.title,
                    "synopsis": self.synopsis,
                    "visual_style": self.visual_style,
                    "counts": {
                        "chapters": self.chapters.len(),
                        "panels": self.chapters.iter().map(|c| c.panels.len()).sum::<usize>(),
                        "layers": self.chapters.iter().map(|c| c.layers.len()).sum::<usize>(),
                        "characters": characters,
                        "assets": self.assets.len() - characters,
                        "locations": self.locations.len(),
                        "groups": self.groups.len(),
                        "lore_notes": self.lore.len(),
                        "lore_cards": self.lore_board.cards.len(),
                        "lore_connections": self.lore_board.connections.len(),
                    },
                })
            }
            Query::ChaptersList => json!({
                "chapters": self.chapters.iter().enumerate().map(|(index, c)| json!({
                    "id": c.id,
                    "index": index,
                    "name": c.name,
                    "width": c.width,
                    "height": c.height,
                    "panel_count": c.panels.len(),
                    "layer_count": c.layers.len(),
                })).collect::<Vec<_>>(),
            }),
            Query::PanelsList { chapter } => {
                let chapter = self.chapter(*chapter)?;
                json!({
                    "chapter_id": chapter.id,
                    "panels": chapter.panels.iter().enumerate()
                        .map(|(i, p)| panel_json(chapter, i, p)).collect::<Vec<_>>(),
                })
            }
            Query::LayersList { chapter } => {
                let chapter = self.chapter(*chapter)?;
                json!({
                    "chapter_id": chapter.id,
                    "layers": chapter.layers.iter().enumerate()
                        .map(|(i, l)| layer_json(chapter, i, l)).collect::<Vec<_>>(),
                })
            }
            Query::CharactersList => json!({
                "characters": self.assets.iter()
                    .filter(|a| a.kind == AssetKind::Character)
                    .map(|a| json!({
                        "id": a.id,
                        "name": a.name,
                        "tagline": a.character.tagline,
                        "has_image": a.width > 0 && a.height > 0,
                    }))
                    .collect::<Vec<_>>(),
            }),
            Query::AssetsList => json!({
                "assets": self.assets.iter()
                    .filter(|a| a.kind != AssetKind::Character)
                    .map(|a| json!({
                        "id": a.id,
                        "name": a.name,
                        "asset_kind": asset_kind_name(a.kind),
                        "has_image": a.width > 0 && a.height > 0,
                    }))
                    .collect::<Vec<_>>(),
            }),
            Query::LocationsList => json!({
                "locations": self.locations.iter().map(|l| json!({
                    "id": l.id,
                    "name": l.name,
                    "description": l.profile.description,
                })).collect::<Vec<_>>(),
            }),
            Query::GroupsList => json!({
                "groups": self.groups.iter().map(|g| json!({
                    "id": g.id,
                    "name": g.name,
                    "description": g.description,
                })).collect::<Vec<_>>(),
            }),
            Query::TimelineList => json!({
                "entries": timeline::entries(self).iter().enumerate().map(|(index, e)| json!({
                    "chapter_id": e.chapter_id,
                    "index": index,
                    "when": e.when,
                    "summary": e.summary,
                })).collect::<Vec<_>>(),
            }),
            Query::ObjectGet { id } => self.object_json(*id)?,
            Query::LoreBoard => {
                let targets = lore::targets(self);
                json!({
                    "cards": self.lore_board.cards.iter().map(|card| {
                        let target = targets.get(&card.target);
                        json!({
                            "id": card.id,
                            "target_id": card.target,
                            "target_kind": target.map(|t| lore_kind_name(t.kind)),
                            "title": target.map(|t| t.title),
                            "position": card.position.map(num),
                            "note": card.note,
                            "color": card.color,
                        })
                    }).collect::<Vec<_>>(),
                    "connections": self.lore_board.connections.iter().map(|e| json!({
                        "id": e.id,
                        "from_id": e.from,
                        "to_id": e.to,
                        "label": e.label,
                        "color": e.color,
                    })).collect::<Vec<_>>(),
                })
            }
            Query::PanelsAt { chapter, y } => {
                ensure!(y.is_finite(), "panels.at needs a finite `y`");
                let chapter = self.chapter(*chapter)?;
                let covering = chapter
                    .panels
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| *y >= p.y && *y < p.y + p.height)
                    // Shortest wins; `max_by` keeps the last of equals, i.e. the topmost.
                    .max_by(|(_, a), (_, b)| b.height.total_cmp(&a.height));
                json!({
                    "chapter_id": chapter.id,
                    "panel": covering.map_or(Value::Null, |(i, p)| panel_json(chapter, i, p)),
                })
            }
        })
    }

    fn chapter(&self, id: u64) -> Result<&Chapter> {
        self.chapters
            .iter()
            .find(|c| c.id == id)
            .context("That chapter no longer exists")
    }

    fn object_json(&self, id: u64) -> Result<Value> {
        let target = if id == STORY_ID {
            Target::Story
        } else {
            Target::Object(id)
        };
        let Some(kind) = self.object_kind(target) else {
            let layer = self
                .chapters
                .iter()
                .any(|c| c.layers.iter().any(|l| l.id == id));
            let preset = self.bubble_presets.iter().any(|p| p.id == id);
            if layer {
                bail!("Object {id} is a layer; read it with layers.list");
            }
            if preset {
                bail!("Object {id} is a bubble preset, which the read API does not cover yet");
            }
            bail!("Object {id} does not exist");
        };
        let mut fields = Map::new();
        for field in Field::allowed_on(kind) {
            fields.insert(field.as_str().into(), self.field(target, *field)?.into());
        }
        let details = match kind {
            ObjectKind::Story => json!({}),
            ObjectKind::Panel => self
                .chapters
                .iter()
                .find_map(|c| {
                    let i = c.panels.iter().position(|p| p.id == id)?;
                    let p = &c.panels[i];
                    Some(json!({
                        "chapter_id": c.id,
                        "index": i,
                        "y": num(p.y),
                        "height": num(p.height),
                    }))
                })
                .context("That panel no longer exists")?,
            ObjectKind::Character | ObjectKind::Asset => {
                let asset = self
                    .assets
                    .iter()
                    .find(|a| a.id == id)
                    .context("That asset no longer exists")?;
                let p = &asset.character;
                let mut details = json!({
                    "asset_kind": asset_kind_name(asset.kind),
                    "has_image": asset.width > 0 && asset.height > 0,
                    "width": asset.width,
                    "height": asset.height,
                });
                if kind == ObjectKind::Character {
                    details["gender"] = json!(p.gender.map(gender_name));
                    details["outfits"] = json!(p
                        .outfits
                        .iter()
                        .map(|o| json!({ "name": o.name, "description": o.description }))
                        .collect::<Vec<_>>());
                    details["active_outfit"] = json!(p.active_outfit);
                    details["palette"] = json!(p
                        .palette
                        .iter()
                        .map(|s| json!({ "name": s.name, "color": s.color }))
                        .collect::<Vec<_>>());
                    details["references"] = json!(p
                        .references
                        .iter()
                        .map(|r| json!({ "asset_id": r.asset_id, "label": r.label, "notes": r.notes }))
                        .collect::<Vec<_>>());
                }
                details
            }
            ObjectKind::Location => {
                let location = self
                    .locations
                    .iter()
                    .find(|l| l.id == id)
                    .context("That location no longer exists")?;
                json!({ "asset_ids": location.asset_ids })
            }
            ObjectKind::Group => {
                let group = self
                    .groups
                    .iter()
                    .find(|g| g.id == id)
                    .context("That group no longer exists")?;
                json!({
                    "color": group.color,
                    "character_ids": group.character_ids,
                    "location_ids": group.location_ids,
                    "reference_ids": group.reference_ids,
                })
            }
            ObjectKind::Chapter => {
                let chapter = self.chapter(id)?;
                let entry = timeline::entries(self)
                    .into_iter()
                    .enumerate()
                    .find(|(_, e)| e.chapter_id == id);
                json!({
                    "index": self.chapters.iter().position(|c| c.id == id),
                    "width": chapter.width,
                    "height": chapter.height,
                    "panel_ids": chapter.panels.iter().map(|p| p.id).collect::<Vec<_>>(),
                    "layer_ids": chapter.layers.iter().map(|l| l.id).collect::<Vec<_>>(),
                    "timeline_index": entry.as_ref().map(|(i, _)| *i),
                    "when": entry.as_ref().map(|(_, e)| e.when.as_str()),
                    "summary": entry.as_ref().map(|(_, e)| e.summary.as_str()),
                })
            }
            ObjectKind::LoreNote => {
                let note = self
                    .lore
                    .iter()
                    .find(|n| n.id == id)
                    .context("That note no longer exists")?;
                json!({ "link_ids": note.links })
            }
            ObjectKind::LoreCard => {
                let card = self
                    .lore_board
                    .cards
                    .iter()
                    .find(|c| c.id == id)
                    .context("That card no longer exists")?;
                json!({
                    "target_id": card.target,
                    "position": card.position.map(num),
                    "color": card.color,
                })
            }
            ObjectKind::LoreConnection => {
                let edge = self
                    .lore_board
                    .connections
                    .iter()
                    .find(|e| e.id == id)
                    .context("That connection no longer exists")?;
                json!({ "from_id": edge.from, "to_id": edge.to, "label": edge.label, "color": edge.color })
            }
        };
        Ok(json!({
            "id": id,
            "kind": kind_name(kind),
            "fields": fields,
            "details": details,
        }))
    }
}

#[cfg(test)]
mod tests;
