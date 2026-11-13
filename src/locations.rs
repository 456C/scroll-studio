//! Named, story-owned collections of background assets. No image bytes are duplicated.
use crate::model::{AssetKind, BackgroundLocation, Story};
use anyhow::{ensure, Context, Result};
use std::collections::HashSet;

pub const MAX_LOCATIONS: usize = 4096;
pub const MAX_LOCATION_NAME_BYTES: usize = 4096;
pub const MAX_LOCATION_ASSETS: usize = 4096;

pub fn compose_prompt(story: &Story, location: &BackgroundLocation, angle: &str) -> String {
    let mut parts = Vec::new();
    for (label, value) in [
        ("Visual style", story.visual_style.as_str()),
        ("Location", location.name.as_str()),
        ("Description", location.profile.description.as_str()),
        ("Atmosphere", location.profile.atmosphere.as_str()),
        ("Lighting", location.profile.lighting.as_str()),
        ("Keep consistent", location.profile.consistency.as_str()),
        ("View", angle),
        ("Instructions", location.profile.prompt.as_str()),
        ("Avoid", location.profile.avoid.as_str()),
    ] {
        if !value.trim().is_empty() {
            parts.push(format!("{label}: {}", value.trim()));
        }
    }
    let groups = crate::groups::prompt(story, crate::groups::Subject::Location(location.id));
    if !groups.is_empty() {
        parts.push(format!("Affiliations: {groups}"));
    }
    parts.join("\n\n")
}

pub(crate) fn checked_name(name: &str) -> Result<&str> {
    let name = name.trim();
    ensure!(!name.is_empty(), "Enter a location name");
    ensure!(
        name.len() <= MAX_LOCATION_NAME_BYTES,
        "Location name is too long"
    );
    Ok(name)
}

pub fn add_location(story: &mut Story, name: &str) -> Result<u64> {
    let name = checked_name(name)?;
    ensure!(story.locations.len() < MAX_LOCATIONS, "Too many locations");
    ensure!(
        story.next_id > 0 && story.next_id < u64::MAX - 1,
        "Story ID space exhausted"
    );
    let id = story.alloc_id();
    story.locations.push(BackgroundLocation {
        extensions: Default::default(),
        id,
        name: name.to_owned(),
        asset_ids: Vec::new(),
        profile: Default::default(),
    });
    Ok(id)
}

/// Return whether the name changed, so callers can avoid empty undo steps.
pub fn rename_location(story: &mut Story, id: u64, name: &str) -> Result<bool> {
    let name = checked_name(name)?;
    let location = story
        .locations
        .iter_mut()
        .find(|location| location.id == id)
        .context("Location no longer exists")?;
    if location.name == name {
        return Ok(false);
    }
    location.name = name.to_owned();
    Ok(true)
}

pub fn location_for_asset(story: &Story, asset_id: u64) -> Option<u64> {
    if !story
        .assets
        .iter()
        .any(|asset| asset.id == asset_id && asset.kind == AssetKind::Background)
    {
        return None;
    }
    story
        .locations
        .iter()
        .find(|location| location.asset_ids.contains(&asset_id))
        .map(|location| location.id)
}

/// Move one existing background into a location, or None for Unsorted.
/// Invalid requests leave the story unchanged; a no-op returns false.
pub fn move_asset(story: &mut Story, asset_id: u64, destination: Option<u64>) -> Result<bool> {
    ensure!(
        story
            .assets
            .iter()
            .any(|asset| asset.id == asset_id && asset.kind == AssetKind::Background),
        "Choose a background image"
    );
    let target = destination
        .map(|id| {
            story
                .locations
                .iter()
                .position(|location| location.id == id)
                .context("Location no longer exists")
        })
        .transpose()?;
    let memberships = story
        .locations
        .iter()
        .flat_map(|location| &location.asset_ids)
        .filter(|&&id| id == asset_id)
        .count();
    if match target {
        Some(index) => memberships == 1 && story.locations[index].asset_ids.contains(&asset_id),
        None => memberships == 0,
    } {
        return Ok(false);
    }
    if let Some(index) = target {
        ensure!(
            story.locations[index]
                .asset_ids
                .iter()
                .filter(|&&id| id != asset_id)
                .count()
                < MAX_LOCATION_ASSETS,
            "Location contains too many images"
        );
    }
    for location in &mut story.locations {
        location.asset_ids.retain(|&id| id != asset_id);
    }
    if let Some(index) = target {
        story.locations[index].asset_ids.push(asset_id);
    }
    Ok(true)
}

/// Validate folder metadata and references without silently modifying saved data.
/// Global ID uniqueness, including chapters and layers, is checked by storage.
pub fn validate_locations(story: &Story) -> Result<()> {
    ensure!(story.locations.len() <= MAX_LOCATIONS, "Too many locations");
    let background_ids: HashSet<_> = story
        .assets
        .iter()
        .filter(|asset| asset.kind == AssetKind::Background)
        .map(|asset| asset.id)
        .collect();
    let mut folder_ids = HashSet::new();
    let mut assigned = HashSet::new();
    for location in &story.locations {
        ensure!(
            [
                &location.profile.description,
                &location.profile.atmosphere,
                &location.profile.lighting,
                &location.profile.consistency,
                &location.profile.notes,
                &location.profile.prompt,
                &location.profile.avoid
            ]
            .iter()
            .all(|field| field.len() <= 64 * 1024),
            "Location description is too long"
        );
        ensure!(
            location.id > 0 && folder_ids.insert(location.id),
            "Location contains a missing or duplicate ID"
        );
        ensure!(
            !location.name.trim().is_empty() && location.name.len() <= MAX_LOCATION_NAME_BYTES,
            "Invalid location name"
        );
        ensure!(
            location.asset_ids.len() <= MAX_LOCATION_ASSETS,
            "Location contains too many images"
        );
        for &id in &location.asset_ids {
            ensure!(
                background_ids.contains(&id),
                "Location references a missing background"
            );
            ensure!(
                assigned.insert(id),
                "A background belongs to more than one location"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Asset, CharacterProfile};

    fn story_with_backgrounds() -> (Story, [u64; 2]) {
        let mut story = Story::new("Locations");
        let ids = [story.alloc_id(), story.alloc_id()];
        for (id, name) in ids.into_iter().zip(["Station day", "Station night"]) {
            story.assets.push(Asset {
                extensions: Default::default(),
                provenance: None,
                id,
                name: name.into(),
                kind: AssetKind::Background,
                notes: String::new(),
                prompt: String::new(),
                width: 0,
                height: 0,
                character: CharacterProfile::default(),
            });
        }
        (story, ids)
    }

    #[test]
    fn moving_backgrounds_preserves_assets_order_and_unrelated_story_content() {
        let (mut story, [day, night]) = story_with_backgrounds();
        let assets = story.assets.clone();
        let chapters = story.chapters.clone();
        let next_id = story.next_id;
        let station = add_location(&mut story, " Station ").unwrap();
        let city = add_location(&mut story, "City").unwrap();
        assert_eq!(station, next_id);
        assert_eq!(story.locations[0].name, "Station");
        assert!(move_asset(&mut story, day, Some(station)).unwrap());
        assert!(move_asset(&mut story, night, Some(station)).unwrap());
        assert!(!move_asset(&mut story, day, Some(station)).unwrap());
        assert_eq!(story.locations[0].asset_ids, [day, night]);
        assert!(move_asset(&mut story, day, Some(city)).unwrap());
        assert_eq!(story.locations[0].asset_ids, [night]);
        assert_eq!(story.locations[1].asset_ids, [day]);
        assert_eq!(location_for_asset(&story, day), Some(city));
        assert!(move_asset(&mut story, day, None).unwrap());
        assert!(!move_asset(&mut story, day, None).unwrap());
        assert_eq!(location_for_asset(&story, day), None);
        assert_eq!(story.assets, assets);
        assert_eq!(story.chapters, chapters);
        validate_locations(&story).unwrap();
    }

    #[test]
    fn invalid_location_edits_are_atomic_and_renaming_preserves_membership() {
        let (mut story, [day, night]) = story_with_backgrounds();
        let location = add_location(&mut story, "Station").unwrap();
        move_asset(&mut story, day, Some(location)).unwrap();
        story.assets[1].kind = AssetKind::Image;
        let before = story.clone();
        assert!(move_asset(&mut story, day, Some(999_999)).is_err());
        assert!(move_asset(&mut story, night, Some(location)).is_err());
        assert!(move_asset(&mut story, 999_999, Some(location)).is_err());
        assert!(rename_location(&mut story, location, "   ").is_err());
        assert!(add_location(&mut story, &"x".repeat(MAX_LOCATION_NAME_BYTES + 1)).is_err());
        assert_eq!(story, before);
        assert!(rename_location(&mut story, location, "Old station").unwrap());
        assert!(!rename_location(&mut story, location, " Old station ").unwrap());
        assert_eq!(story.locations[0].asset_ids, [day]);
        assert_eq!(location_for_asset(&story, night), None);
    }

    #[test]
    fn locations_reject_limits_and_duplicate_memberships() {
        let (mut story, [day, _]) = story_with_backgrounds();
        let location = add_location(&mut story, "Station").unwrap();
        move_asset(&mut story, day, Some(location)).unwrap();
        story.locations[0].asset_ids.push(day);
        assert!(validate_locations(&story).is_err());
        story.locations[0].asset_ids.pop();
        story.locations[0].name = "x".repeat(MAX_LOCATION_NAME_BYTES + 1);
        assert!(validate_locations(&story).is_err());
        story.locations[0].name = "Station".into();
        let folder = story.locations[0].clone();
        story.locations = vec![folder; MAX_LOCATIONS];
        let before = story.clone();
        assert!(add_location(&mut story, "Another").is_err());
        assert_eq!(story, before);
        story.locations.push(story.locations[0].clone());
        assert!(validate_locations(&story).is_err());
    }
}
