use crate::model::{AssetKind, Story, WorldGroup};
use anyhow::{ensure, Result};
use std::collections::HashSet;

pub const MAX_GROUPS: usize = 4096;
pub const MAX_REFERENCES: usize = 64;

#[derive(Clone, Copy)]
pub enum Subject {
    Character(u64),
    Location(u64),
}

pub fn members(group: &WorldGroup, subject: Subject) -> &[u64] {
    match subject {
        Subject::Character(_) => &group.character_ids,
        Subject::Location(_) => &group.location_ids,
    }
}
pub fn affiliated(group: &WorldGroup, subject: Subject) -> bool {
    let id = match subject {
        Subject::Character(id) | Subject::Location(id) => id,
    };
    members(group, subject).contains(&id)
}
pub fn set_affiliation(group: &mut WorldGroup, subject: Subject, enabled: bool) {
    let (list, id) = match subject {
        Subject::Character(id) => (&mut group.character_ids, id),
        Subject::Location(id) => (&mut group.location_ids, id),
    };
    list.retain(|&value| value != id);
    if enabled {
        list.push(id);
    }
}
pub fn add(story: &mut Story) -> Result<u64> {
    ensure!(
        story.groups.len() < MAX_GROUPS,
        "This story has reached its group limit"
    );
    ensure!(story.next_id < u64::MAX - 1, "Story ID space exhausted");
    let id = story.alloc_id();
    story.groups.push(WorldGroup {
        id,
        name: "New group".into(),
        description: String::new(),
        color: [114, 190, 182],
        reference_ids: vec![],
        character_ids: vec![],
        location_ids: vec![],
    });
    Ok(id)
}
pub fn prompt(story: &Story, subject: Subject) -> String {
    story
        .groups
        .iter()
        .filter(|g| affiliated(g, subject))
        .map(|g| {
            if g.description.trim().is_empty() {
                g.name.clone()
            } else {
                format!("{}: {}", g.name, g.description.trim())
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
pub fn validate(story: &Story) -> Result<()> {
    ensure!(story.groups.len() <= MAX_GROUPS, "Too many groups");
    let characters: HashSet<_> = story
        .assets
        .iter()
        .filter(|a| a.kind == AssetKind::Character)
        .map(|a| a.id)
        .collect();
    let locations: HashSet<_> = story.locations.iter().map(|l| l.id).collect();
    let images: HashSet<_> = story
        .assets
        .iter()
        .filter(|a| a.width > 0 && a.height > 0)
        .map(|a| a.id)
        .collect();
    for group in &story.groups {
        ensure!(
            !group.name.trim().is_empty()
                && group.name.len() <= 4096
                && group.description.len() <= 20_000,
            "Invalid group details"
        );
        ensure!(
            group.reference_ids.len() <= MAX_REFERENCES,
            "Too many group references"
        );
        for (list, allowed) in [
            (&group.character_ids, &characters),
            (&group.location_ids, &locations),
            (&group.reference_ids, &images),
        ] {
            let mut seen = HashSet::new();
            ensure!(
                list.iter()
                    .all(|id| allowed.contains(id) && seen.insert(id)),
                "Group contains a missing or duplicate reference"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn affiliation_links_and_shared_names_round_trip_without_duplicating_images() {
        let mut story = Story::new("World");
        let location = crate::locations::add_location(&mut story, "Citadel").unwrap();
        let id = add(&mut story).unwrap();
        set_affiliation(&mut story.groups[0], Subject::Location(location), true);
        set_affiliation(&mut story.groups[0], Subject::Location(location), true);
        assert_eq!(story.groups[0].location_ids, [location]);
        story.groups[0].name = "Angels".into();
        assert_eq!(prompt(&story, Subject::Location(location)), "Angels");
        validate(&story).unwrap();
        assert_eq!(
            serde_json::from_slice::<Story>(&serde_json::to_vec(&story).unwrap()).unwrap(),
            story
        );
        set_affiliation(&mut story.groups[0], Subject::Location(location), false);
        assert!(!affiliated(&story.groups[0], Subject::Location(location)));
        assert_eq!(story.groups[0].id, id);
        story.groups[0].reference_ids.push(u64::MAX);
        assert!(validate(&story).is_err());
    }
}
