use crate::model::{ChronologyEntry, Story};
use anyhow::{ensure, Result};
use std::collections::HashSet;

/// Newly created chapters appear automatically; older stories need no migration.
pub fn entries(story: &Story) -> Vec<ChronologyEntry> {
    let mut result = story.timeline.clone();
    let existing: HashSet<_> = result.iter().map(|e| e.chapter_id).collect();
    result.extend(
        story
            .chapters
            .iter()
            .filter(|c| !existing.contains(&c.id))
            .map(|c| ChronologyEntry {
                chapter_id: c.id,
                ..Default::default()
            }),
    );
    result
}
/// Insert immediately before `before`, or at the end. Reading order is unchanged.
pub fn move_before(story: &mut Story, chapter: u64, before: Option<u64>) -> Result<bool> {
    ensure!(
        story.chapters.iter().any(|c| c.id == chapter),
        "Chapter no longer exists"
    );
    ensure!(
        before.is_none_or(|id| story.chapters.iter().any(|c| c.id == id)),
        "Timeline destination no longer exists"
    );
    if before == Some(chapter) {
        return Ok(false);
    }
    let original = entries(story);
    let mut updated = original.clone();
    let index = updated
        .iter()
        .position(|e| e.chapter_id == chapter)
        .unwrap();
    let entry = updated.remove(index);
    let target = before
        .and_then(|id| updated.iter().position(|e| e.chapter_id == id))
        .unwrap_or(updated.len());
    updated.insert(target, entry);
    if updated == original {
        return Ok(false);
    }
    story.timeline = updated;
    Ok(true)
}
pub fn validate(story: &Story) -> Result<()> {
    ensure!(
        story.timeline.len() <= story.chapters.len(),
        "Too many timeline entries"
    );
    let chapters: HashSet<_> = story.chapters.iter().map(|c| c.id).collect();
    let mut seen = HashSet::new();
    for entry in &story.timeline {
        ensure!(
            chapters.contains(&entry.chapter_id) && seen.insert(entry.chapter_id),
            "Timeline contains a missing or duplicate chapter"
        );
        ensure!(
            entry.when.len() <= 1024 && entry.summary.len() <= 20000,
            "Timeline notes are too long"
        );
    }
    Ok(())
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
}
