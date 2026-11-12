//! Read-only queries over a story, answered as JSON.
//!
//! This is the read half of the command layer: addons (and later the MCP server) call
//! these by name with JSON parameters, using the same transport as commands. Answers
//! describe text and structure only; image bytes are never returned.

use crate::{
    commands::{Field, ObjectKind, Target, STORY_ID},
    lore,
    model::{AssetKind, Story},
};
use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};

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
    CharactersList,
    LocationsList,
    GroupsList,
    /// The whitelisted text of one object (or the story itself, with [`STORY_ID`]) plus
    /// the small structural details that give it context.
    ObjectGet {
        id: u64,
    },
    /// Lore board cards and connections.
    LoreBoard,
    /// The panel covering a canvas position, or `null`.
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
            Query::CharactersList => "characters.list",
            Query::LocationsList => "locations.list",
            Query::GroupsList => "groups.list",
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
                .and_then(Value::as_u64)
                .with_context(|| format!("{method} needs a numeric `{key}`"))
        };
        Ok(match method {
            "story.summary" => Query::StorySummary,
            "chapters.list" => Query::ChaptersList,
            "panels.list" => Query::PanelsList {
                chapter: id("chapter")?,
            },
            "characters.list" => Query::CharactersList,
            "locations.list" => Query::LocationsList,
            "groups.list" => Query::GroupsList,
            "object.get" => Query::ObjectGet { id: id("id")? },
            "lore.board" => Query::LoreBoard,
            "panels.at" => {
                let y = params
                    .get("y")
                    .and_then(Value::as_f64)
                    .filter(|y| y.is_finite())
                    .context("panels.at needs a finite `y`")?;
                Query::PanelsAt {
                    chapter: id("chapter")?,
                    y: y as f32,
                }
            }
            other => bail!("Unknown query `{other}`"),
        })
    }

    /// The parameters [`Query::parse`] accepts for this query.
    pub fn params(&self) -> Value {
        match self {
            Query::PanelsList { chapter } => json!({ "chapter": chapter }),
            Query::ObjectGet { id } => json!({ "id": id }),
            Query::PanelsAt { chapter, y } => json!({ "chapter": chapter, "y": y }),
            _ => Value::Null,
        }
    }
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

fn panel_json(panel: &crate::model::Panel) -> Value {
    json!({
        "id": panel.id,
        "title": panel.title,
        "y": panel.y,
        "height": panel.height,
        "script": panel.script,
        "notes": panel.notes,
        "prompt": panel.prompt,
    })
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
            Query::ChaptersList => Value::Array(
                self.chapters
                    .iter()
                    .map(|c| {
                        json!({
                            "id": c.id,
                            "name": c.name,
                            "width": c.width,
                            "height": c.height,
                            "panel_count": c.panels.len(),
                        })
                    })
                    .collect(),
            ),
            Query::PanelsList { chapter } => {
                let chapter = self.chapter(*chapter)?;
                Value::Array(chapter.panels.iter().map(panel_json).collect())
            }
            Query::CharactersList => Value::Array(
                self.assets
                    .iter()
                    .filter(|a| a.kind == AssetKind::Character)
                    .map(|a| json!({ "id": a.id, "name": a.name, "tagline": a.character.tagline }))
                    .collect(),
            ),
            Query::LocationsList => Value::Array(
                self.locations
                    .iter()
                    .map(|l| json!({ "id": l.id, "name": l.name, "description": l.profile.description }))
                    .collect(),
            ),
            Query::GroupsList => Value::Array(
                self.groups
                    .iter()
                    .map(|g| json!({ "id": g.id, "name": g.name, "description": g.description }))
                    .collect(),
            ),
            Query::ObjectGet { id } => self.object_json(*id)?,
            Query::LoreBoard => {
                let targets = lore::targets(self);
                json!({
                    "cards": self.lore_board.cards.iter().map(|card| {
                        let target = targets.get(&card.target);
                        json!({
                            "id": card.id,
                            "target": card.target,
                            "kind": target.map(|t| t.kind.name().to_lowercase()),
                            "title": target.map(|t| t.title),
                            "position": card.position,
                            "note": card.note,
                            "color": card.color,
                        })
                    }).collect::<Vec<_>>(),
                    "connections": self.lore_board.connections.iter().map(|e| json!({
                        "id": e.id,
                        "from": e.from,
                        "to": e.to,
                        "label": e.label,
                        "color": e.color,
                    })).collect::<Vec<_>>(),
                })
            }
            Query::PanelsAt { chapter, y } => {
                let chapter = self.chapter(*chapter)?;
                // Where panels overlap, the one starting lowest (the innermost) wins.
                chapter
                    .panels
                    .iter()
                    .filter(|p| *y >= p.y && *y < p.y + p.height)
                    .max_by(|a, b| a.y.total_cmp(&b.y))
                    .map_or(Value::Null, panel_json)
            }
        })
    }

    fn chapter(&self, id: u64) -> Result<&crate::model::Chapter> {
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
        let kind = self
            .object_kind(target)
            .context("That object no longer exists")?;
        let mut fields = Map::new();
        for field in Field::allowed_on(kind) {
            fields.insert(field.as_str().into(), self.field(target, *field)?.into());
        }
        let mut object = json!({
            "id": id,
            "kind": kind_name(kind),
            "fields": fields,
        });
        let extra = match kind {
            ObjectKind::Panel => self
                .chapters
                .iter()
                .find_map(|c| {
                    c.panels
                        .iter()
                        .find(|p| p.id == id)
                        .map(|p| json!({ "chapter": c.id, "y": p.y, "height": p.height }))
                })
                .unwrap_or(Value::Null),
            ObjectKind::Character | ObjectKind::Asset => {
                let asset = self
                    .assets
                    .iter()
                    .find(|a| a.id == id)
                    .expect("kind checked");
                let p = &asset.character;
                let mut extra = json!({
                    "asset_kind": format!("{:?}", asset.kind).to_lowercase(),
                    "has_image": asset.width > 0 && asset.height > 0,
                    "width": asset.width,
                    "height": asset.height,
                });
                if kind == ObjectKind::Character {
                    extra["gender"] = json!(p.gender.map(|g| g.label().to_lowercase()));
                    extra["outfits"] = json!(p
                        .outfits
                        .iter()
                        .map(|o| json!({ "name": o.name, "description": o.description }))
                        .collect::<Vec<_>>());
                    extra["active_outfit"] = json!(p.active_outfit);
                    extra["palette"] = json!(p
                        .palette
                        .iter()
                        .map(|s| json!({ "name": s.name, "color": s.color }))
                        .collect::<Vec<_>>());
                    extra["references"] = json!(p
                        .references
                        .iter()
                        .map(|r| json!({ "asset": r.asset_id, "label": r.label, "notes": r.notes }))
                        .collect::<Vec<_>>());
                }
                extra
            }
            ObjectKind::Location => {
                let location = self
                    .locations
                    .iter()
                    .find(|l| l.id == id)
                    .expect("kind checked");
                json!({ "assets": location.asset_ids })
            }
            ObjectKind::Group => {
                let group = self
                    .groups
                    .iter()
                    .find(|g| g.id == id)
                    .expect("kind checked");
                json!({
                    "color": group.color,
                    "characters": group.character_ids,
                    "locations": group.location_ids,
                    "references": group.reference_ids,
                })
            }
            ObjectKind::Chapter => {
                let chapter = self.chapter(id)?;
                json!({
                    "width": chapter.width,
                    "height": chapter.height,
                    "panels": chapter.panels.iter().map(|p| p.id).collect::<Vec<_>>(),
                })
            }
            ObjectKind::LoreNote => {
                let note = self.lore.iter().find(|n| n.id == id).expect("kind checked");
                json!({ "links": note.links })
            }
            ObjectKind::LoreCard => {
                let card = self
                    .lore_board
                    .cards
                    .iter()
                    .find(|c| c.id == id)
                    .expect("kind checked");
                json!({ "target": card.target, "position": card.position, "color": card.color })
            }
            ObjectKind::LoreConnection => {
                let edge = self
                    .lore_board
                    .connections
                    .iter()
                    .find(|e| e.id == id)
                    .expect("kind checked");
                json!({ "from": edge.from, "to": edge.to, "label": edge.label, "color": edge.color })
            }
            ObjectKind::Story => Value::Null,
        };
        if !extra.is_null() {
            object["details"] = extra;
        }
        Ok(object)
    }
}

#[cfg(test)]
mod tests;
