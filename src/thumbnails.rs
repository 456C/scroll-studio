//! Card covers reference existing originals; they never replace artwork or reorder references.
use crate::model::{AssetKind, CardThumbnail, Story};
use std::collections::HashSet;
fn references(story: &Story, target: u64) -> Vec<u64> {
    if let Some(a) = story
        .assets
        .iter()
        .find(|a| a.id == target && a.kind == AssetKind::Character)
    {
        return crate::characters::reference_ids(a);
    }
    if let Some(l) = story.locations.iter().find(|l| l.id == target) {
        return l.asset_ids.clone();
    }
    story
        .groups
        .iter()
        .find(|g| g.id == target)
        .map(|g| g.reference_ids.clone())
        .unwrap_or_default()
}
pub fn selected(story: &Story, target: u64) -> Option<u64> {
    story
        .card_thumbnails
        .iter()
        .find(|c| c.target == target)
        .map(|c| c.asset_id)
        .filter(|id| references(story, target).contains(id))
        .filter(|id| {
            story
                .assets
                .iter()
                .any(|a| a.id == *id && a.width > 0 && a.height > 0)
        })
}
pub fn image(story: &Story, target: u64) -> Option<u64> {
    selected(story, target).or_else(|| references(story, target).first().copied())
}
pub fn set(story: &mut Story, target: u64, asset_id: u64) -> bool {
    if !references(story, target).contains(&asset_id)
        || !story
            .assets
            .iter()
            .any(|a| a.id == asset_id && a.width > 0 && a.height > 0)
        || selected(story, target) == Some(asset_id)
    {
        return false;
    }
    story.card_thumbnails.retain(|c| c.target != target);
    story
        .card_thumbnails
        .push(CardThumbnail { target, asset_id });
    true
}
pub fn prune(story: &mut Story) {
    let valid: HashSet<_> = story
        .card_thumbnails
        .iter()
        .filter(|c| selected(story, c.target) == Some(c.asset_id))
        .map(|c| (c.target, c.asset_id))
        .collect();
    story
        .card_thumbnails
        .retain(|c| valid.contains(&(c.target, c.asset_id)));
}
pub fn validate(story: &Story) -> anyhow::Result<()> {
    anyhow::ensure!(
        story.card_thumbnails.len() <= 12288,
        "Too many card thumbnails"
    );
    let mut seen = HashSet::new();
    for c in &story.card_thumbnails {
        anyhow::ensure!(
            seen.insert(c.target) && selected(story, c.target) == Some(c.asset_id),
            "Invalid card thumbnail"
        );
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Asset, CharacterProfile, CharacterReference};
    #[test]
    fn chosen_cover_preserves_original_identity_and_reference_order() {
        let mut s = Story::new("Covers");
        let char_id = s.alloc_id();
        let first = s.alloc_id();
        let second = s.alloc_id();
        for id in [char_id, first, second] {
            s.assets.push(Asset {
                id,
                name: "Reference".into(),
                kind: if id == char_id {
                    AssetKind::Character
                } else {
                    AssetKind::Image
                },
                notes: String::new(),
                prompt: String::new(),
                width: 20,
                height: 20,
                character: CharacterProfile::default(),
            });
        }
        s.assets[0].character.references = vec![
            CharacterReference {
                asset_id: first,
                label: "Front".into(),
                notes: String::new(),
            },
            CharacterReference {
                asset_id: second,
                label: "Side".into(),
                notes: String::new(),
            },
        ];
        let originals = s.assets.clone();
        assert!(set(&mut s, char_id, second));
        assert_eq!(image(&s, char_id), Some(second));
        assert_eq!(s.assets, originals);
        let loaded: Story = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        assert_eq!(image(&loaded, char_id), Some(second));
        s.assets[0].character.references.pop();
        prune(&mut s);
        assert!(s.card_thumbnails.is_empty());
        assert_eq!(image(&s, char_id), Some(char_id));
    }
}
