//! The investigation board references story entities without duplicating their images or profiles.
use crate::model::{AssetKind, LoreCard, LoreConnection, Story};
use anyhow::{ensure, Result};
use std::collections::{HashMap, HashSet};

pub const MAX_CARDS: usize = 8192;
pub const MAX_CONNECTIONS: usize = 16384;
pub const NOTE_COLOR: [u8; 3] = [222, 181, 105];
pub const STRING_COLOR: [u8; 3] = [190, 103, 106];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Note,
    Character,
    Location,
    Group,
}
impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Note => "Note",
            Self::Character => "Character",
            Self::Location => "Location",
            Self::Group => "Group",
        }
    }
    pub fn height(self) -> f32 {
        if self == Self::Note {
            164.0
        } else {
            236.0
        }
    }
}

pub struct Target<'a> {
    pub kind: Kind,
    pub title: &'a str,
    pub text: &'a str,
    pub portrait: Option<u64>,
}
pub fn targets(story: &Story) -> HashMap<u64, Target<'_>> {
    let mut targets = HashMap::new();
    for note in &story.lore {
        targets.insert(
            note.id,
            Target {
                kind: Kind::Note,
                title: &note.title,
                text: &note.text,
                portrait: None,
            },
        );
    }
    for character in story
        .assets
        .iter()
        .filter(|a| a.kind == AssetKind::Character)
    {
        targets.insert(
            character.id,
            Target {
                kind: Kind::Character,
                title: &character.name,
                text: &character.notes,
                portrait: crate::characters::reference_ids(character).first().copied(),
            },
        );
    }
    for location in &story.locations {
        targets.insert(
            location.id,
            Target {
                kind: Kind::Location,
                title: &location.name,
                text: &location.profile.description,
                portrait: location.asset_ids.first().copied(),
            },
        );
    }
    for group in &story.groups {
        targets.insert(
            group.id,
            Target {
                kind: Kind::Group,
                title: &group.name,
                text: &group.description,
                portrait: group.reference_ids.first().copied(),
            },
        );
    }
    targets
}

pub fn pin(story: &mut Story, target: u64, position: [f32; 2]) -> Option<u64> {
    if let Some(card) = story.lore_board.cards.iter().find(|c| c.target == target) {
        return Some(card.id);
    }
    if story.lore_board.cards.len() >= MAX_CARDS || !targets(story).contains_key(&target) {
        return None;
    }
    let id = story.alloc_id();
    let note = story.lore.iter().any(|n| n.id == target);
    story.lore_board.cards.push(LoreCard {
        id,
        target,
        position: position.map(|v| v.clamp(-1_000_000.0, 1_000_000.0)),
        note: String::new(),
        color: if note { NOTE_COLOR } else { [114, 168, 213] },
    });
    Some(id)
}

pub fn connect(story: &mut Story, from: u64, to: u64) -> Option<u64> {
    if from == to || story.lore_board.connections.len() >= MAX_CONNECTIONS {
        return None;
    }
    if let Some(edge) = story
        .lore_board
        .connections
        .iter()
        .find(|e| (e.from == from && e.to == to) || (e.from == to && e.to == from))
    {
        return Some(edge.id);
    }
    if ![from, to]
        .iter()
        .all(|id| story.lore_board.cards.iter().any(|c| c.id == *id))
    {
        return None;
    }
    let id = story.alloc_id();
    story.lore_board.connections.push(LoreConnection {
        id,
        from,
        to,
        label: String::new(),
        color: STRING_COLOR,
    });
    Some(id)
}

pub fn unpin(story: &mut Story, card: u64) {
    story.lore_board.cards.retain(|c| c.id != card);
    story
        .lore_board
        .connections
        .retain(|e| e.from != card && e.to != card);
}

pub fn unlink_target(story: &mut Story, target: u64) {
    for entry in &mut story.lore {
        entry.links.retain(|id| *id != target);
    }
    let ids: Vec<_> = story
        .lore_board
        .cards
        .iter()
        .filter(|c| c.target == target)
        .map(|c| c.id)
        .collect();
    for id in ids {
        unpin(story, id);
    }
}

pub fn migrate_board(story: &mut Story) -> Result<()> {
    if !story.lore_board.cards.is_empty() {
        return Ok(());
    }
    let mut ids: Vec<_> = story.lore.iter().map(|n| n.id).collect();
    for target in story.lore.iter().flat_map(|n| &n.links) {
        if !ids.contains(target) {
            ids.push(*target);
        }
    }
    ensure!(ids.len() <= MAX_CARDS, "Too many linked lore objects");
    for (index, target) in ids.iter().enumerate() {
        pin(
            story,
            *target,
            [(index % 4) as f32 * 264.0, (index / 4) as f32 * 300.0],
        );
    }
    let cards: HashMap<_, _> = story
        .lore_board
        .cards
        .iter()
        .map(|c| (c.target, c.id))
        .collect();
    let edges: Vec<_> = story
        .lore
        .iter()
        .flat_map(|note| note.links.iter().map(|id| (cards[&note.id], cards[id])))
        .collect();
    for (from, to) in edges {
        ensure!(
            connect(story, from, to).is_some(),
            "Too many lore connections"
        );
    }
    Ok(())
}

pub fn validate_board(story: &Story) -> Result<()> {
    let board = &story.lore_board;
    ensure!(
        board.cards.len() <= MAX_CARDS && board.connections.len() <= MAX_CONNECTIONS,
        "Lore board is too large"
    );
    let targets = targets(story);
    let mut pinned = HashSet::new();
    let mut ids = HashSet::new();
    for card in &board.cards {
        ensure!(
            targets.contains_key(&card.target) && pinned.insert(card.target) && ids.insert(card.id),
            "Invalid lore card target"
        );
        ensure!(
            card.position
                .iter()
                .all(|v| v.is_finite() && v.abs() <= 1_000_000.0)
                && card.note.len() <= 100_000,
            "Invalid lore card details"
        );
    }
    let mut pairs = HashSet::new();
    for edge in &board.connections {
        ensure!(
            edge.from != edge.to
                && ids.contains(&edge.from)
                && ids.contains(&edge.to)
                && pairs.insert((edge.from.min(edge.to), edge.from.max(edge.to))),
            "Invalid lore board connection"
        );
        ensure!(
            edge.label.len() <= 4096,
            "Lore connection label is too long"
        );
    }
    Ok(())
}
