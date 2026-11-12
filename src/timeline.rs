use crate::model::{ChronologyEntry, Story, StoryEvent, StoryEventKind};
use anyhow::{bail, ensure, Context, Result};
use std::collections::{HashMap, HashSet};

pub const MAX_EVENTS: usize = 4096;
pub const MAX_LINKS: usize = 64;
pub const MAX_TITLE_BYTES: usize = 4096;

/// Newly created chapters appear automatically; older stories need no migration.
pub fn entries(story: &Story) -> Vec<ChronologyEntry> {
    let mut result = story.timeline.clone();
    let existing: HashSet<_> = result
        .iter()
        .filter(|entry| !entry.is_event())
        .map(|entry| entry.chapter_id)
        .collect();
    result.extend(
        story
            .chapters
            .iter()
            .filter(|chapter| !existing.contains(&chapter.id))
            .map(|chapter| ChronologyEntry {
                chapter_id: chapter.id,
                ..Default::default()
            }),
    );
    result
}

/// Chapters in reading order. Events stay immediately before the chapter they precede.
pub fn reading_order(story: &Story) -> Vec<ChronologyEntry> {
    let mut prefixed: HashMap<u64, Vec<ChronologyEntry>> = HashMap::new();
    let mut chapters: HashMap<u64, ChronologyEntry> = HashMap::new();
    let mut pending = Vec::new();
    for item in entries(story) {
        if item.is_event() {
            pending.push(item);
        } else {
            prefixed.insert(item.chapter_id, std::mem::take(&mut pending));
            chapters.insert(item.chapter_id, item);
        }
    }
    let trailing = pending;
    let mut next = Vec::new();
    for chapter in &story.chapters {
        if let Some(events) = prefixed.remove(&chapter.id) {
            next.extend(events);
        }
        if let Some(entry) = chapters.remove(&chapter.id) {
            next.push(entry);
        }
    }
    next.extend(trailing);
    next
}

/// Insert immediately before `before`, or at the end. Reading order is unchanged.
pub fn move_before(story: &mut Story, id: u64, before: Option<u64>) -> Result<bool> {
    let original = entries(story);
    ensure!(
        original.iter().any(|entry| entry.key() == id),
        "Timeline item no longer exists"
    );
    ensure!(
        before.is_none_or(|before| original.iter().any(|entry| entry.key() == before)),
        "Timeline destination no longer exists"
    );
    if before == Some(id) {
        return Ok(false);
    }
    let mut updated = original.clone();
    let index = updated.iter().position(|entry| entry.key() == id).unwrap();
    let entry = updated.remove(index);
    let target = before
        .and_then(|before| updated.iter().position(|entry| entry.key() == before))
        .unwrap_or(updated.len());
    updated.insert(target, entry);
    if updated == original {
        return Ok(false);
    }
    story.timeline = updated;
    Ok(true)
}

/// Place a new in-world moment immediately after `after`, or at the end.
pub fn insert_event(story: &mut Story, after: Option<u64>) -> Result<u64> {
    let mut items = entries(story);
    ensure!(
        items.iter().filter(|entry| entry.is_event()).count() < MAX_EVENTS,
        "This timeline has reached its event limit"
    );
    let index = match after {
        Some(id) => {
            items
                .iter()
                .position(|entry| entry.key() == id)
                .context("Timeline item no longer exists")?
                + 1
        }
        None => items.len(),
    };
    let id = story.alloc_id();
    items.insert(
        index,
        ChronologyEntry {
            event: Some(StoryEvent {
                id,
                title: String::new(),
                kind: StoryEventKind::Other,
                links: Vec::new(),
            }),
            ..Default::default()
        },
    );
    story.timeline = items;
    Ok(id)
}

pub fn delete_event(story: &mut Story, id: u64) -> Result<bool> {
    let mut items = entries(story);
    let Some(index) = items
        .iter()
        .position(|entry| entry.event.as_ref().is_some_and(|event| event.id == id))
    else {
        bail!("Event no longer exists");
    };
    items.remove(index);
    story.timeline = items;
    Ok(true)
}

pub fn validate(story: &Story) -> Result<()> {
    ensure!(
        story.timeline.len() <= story.chapters.len() + MAX_EVENTS,
        "Too many timeline entries"
    );
    let chapters: HashSet<_> = story.chapters.iter().map(|chapter| chapter.id).collect();
    let targets = link_targets(story);
    let mut seen_chapters = HashSet::new();
    let mut seen_events = HashSet::new();
    let mut events = 0usize;
    for entry in &story.timeline {
        ensure!(
            entry.when.len() <= 1024 && entry.summary.len() <= 20_000,
            "Timeline notes are too long"
        );
        if let Some(event) = &entry.event {
            events += 1;
            ensure!(events <= MAX_EVENTS, "Too many story events");
            ensure!(
                entry.chapter_id == 0,
                "A story event cannot replace a chapter"
            );
            ensure!(
                event.id > 0
                    && seen_events.insert(event.id)
                    && !chapters.contains(&event.id)
                    && !seen_chapters.contains(&event.id),
                "Timeline contains a missing or duplicate event"
            );
            ensure!(
                event.title.len() <= MAX_TITLE_BYTES,
                "Event title is too long"
            );
            ensure!(
                event.links.len() <= MAX_LINKS,
                "An event has too many links"
            );
            let mut unique = HashSet::new();
            ensure!(
                event
                    .links
                    .iter()
                    .all(|id| { *id != event.id && targets.contains(id) && unique.insert(*id) }),
                "An event link is missing or duplicated"
            );
        } else {
            ensure!(
                chapters.contains(&entry.chapter_id) && seen_chapters.insert(entry.chapter_id),
                "Timeline contains a missing or duplicate chapter"
            );
        }
    }
    Ok(())
}

fn link_targets(story: &Story) -> HashSet<u64> {
    use crate::model::AssetKind;
    story
        .assets
        .iter()
        .filter(|asset| asset.kind == AssetKind::Character)
        .map(|asset| asset.id)
        .chain(story.locations.iter().map(|location| location.id))
        .chain(story.groups.iter().map(|group| group.id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Chapter;

    #[test]
    fn reorder_preserves_chapter_identity_reading_order_and_notes() {
        let mut story = Story::new("Time");
        let first = story.chapters[0].id;
        let second = story.alloc_id();
        story
            .chapters
            .push(Chapter::new(second, "After", 800, 1000));
        story.timeline = entries(&story);
        story.timeline[1].when = "Before the fall".into();
        let original = story.chapters.clone();
        assert!(move_before(&mut story, second, Some(first)).unwrap());
        assert_eq!(story.timeline[0].when, "Before the fall");
        assert_eq!(story.chapters, original);
        assert!(!move_before(&mut story, second, Some(first)).unwrap());
        let before = story.clone();
        assert!(move_before(&mut story, second, Some(u64::MAX)).is_err());
        assert_eq!(story, before);
        let third = story.alloc_id();
        story.chapters.push(Chapter::new(third, "Later", 800, 1000));
        assert_eq!(entries(&story).last().unwrap().chapter_id, third);
        validate(&story).unwrap();
        story.timeline.push(story.timeline[0].clone());
        assert!(validate(&story).is_err());
    }

    #[test]
    fn events_sit_between_chapters_and_keep_their_anchor_in_reading_order() {
        let mut story = Story::new("Time");
        let first = story.chapters[0].id;
        let second = story.alloc_id();
        story
            .chapters
            .push(Chapter::new(second, "After", 800, 1000));
        let character = story.alloc_id();
        story.assets.push(crate::model::Asset {
            id: character,
            name: "Mira".into(),
            kind: crate::model::AssetKind::Character,
            notes: String::new(),
            prompt: String::new(),
            width: 0,
            height: 0,
            character: Default::default(),
        });
        let event = insert_event(&mut story, Some(first)).unwrap();
        let entry = story
            .timeline
            .iter_mut()
            .find(|entry| entry.key() == event)
            .unwrap();
        entry.when = "Three years earlier".into();
        entry.summary = "The citadel burned.".into();
        entry.event.as_mut().unwrap().title = "The fire".into();
        entry.event.as_mut().unwrap().kind = StoryEventKind::Flashback;
        entry.event.as_mut().unwrap().links = vec![character];
        assert_eq!(
            entries(&story)
                .iter()
                .map(|entry| entry.key())
                .collect::<Vec<_>>(),
            vec![first, event, second]
        );
        assert!(move_before(&mut story, event, Some(first)).unwrap());
        assert_eq!(story.timeline[0].key(), event);
        assert_eq!(reading_order(&story)[0].key(), event);
        assert_eq!(reading_order(&story)[1].chapter_id, first);
        validate(&story).unwrap();
        assert!(delete_event(&mut story, event).unwrap());
        assert!(entries(&story).iter().all(|entry| !entry.is_event()));
        assert!(delete_event(&mut story, event).is_err());
    }

    #[test]
    fn invalid_events_are_rejected() {
        let mut story = Story::new("Time");
        let event = insert_event(&mut story, None).unwrap();
        story
            .timeline
            .iter_mut()
            .find(|entry| entry.key() == event)
            .unwrap()
            .event
            .as_mut()
            .unwrap()
            .links
            .push(story.chapters[0].id);
        assert!(validate(&story).is_err());
        story.timeline[1].event.as_mut().unwrap().links.clear();
        story.timeline[1].chapter_id = story.chapters[0].id;
        assert!(validate(&story).is_err());
    }
}
