//! Generate an editable character-planning example without placeholder artwork.
mod support;
use scroll_studio::{locations, model::*};
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let directory = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/sample-package"));
    std::fs::create_dir_all(&directory)?;
    let mut story = Story::with_canvas("After the Rain · Character studio", 1600, 5800);
    story.synopsis = "Mira returns to a rain-soaked city. An envelope and an old friend lead her toward the past she left behind.".into();
    story.visual_style = "Modern urban mystery webtoon. Clean expressive ink outlines with varied line weight, believable adult proportions, restrained facial acting, and soft cel shading. Muted teal and blue-gray shadows with small warm amber lights. Rain-softened city backgrounds, readable silhouettes, atmospheric negative space, and cinematic vertical compositions. Keep character designs consistent from panel to panel. No rendered lettering, speech bubbles, logos, or watermarks.".into();
    story.chapters[0].name = "01 · The missed train".into();
    for (panel, (title, script, notes, prompt)) in story.chapters[0].panels.iter_mut().zip([
        (
            "A platform after midnight",
            "WIDE SHOT. Mira stands alone beneath a station lamp, the final train already gone. She checks the folded envelope in her hand.",
            "Mira wears her Rain commute outfit. Keep the amber earring visible on her left ear. Begin with a quiet, spacious composition.",
            "A nearly empty city train platform after midnight, rain on the pavement, one warm station lamp, an adult woman in a charcoal raincoat holding a cream envelope. Wide establishing shot with space above her silhouette.",
        ),
        (
            "A familiar voice",
            "REN: You always take the long way home. Mira turns. Ren is leaning beside a closed café, one hand on his bicycle.",
            "Ren wears his Café closing outfit. Keep his circular glasses and the lighter streak at his right temple. Mira stays in the same coat.",
            "An adult man with round bronze glasses and a dark green apron leans beside a bicycle outside a small closed café, looking toward an adult woman in a charcoal raincoat. Eye-level medium shot, rain reflections and warm doorway light.",
        ),
        (
            "The envelope",
            "CLOSE-UP. Mira turns the envelope over. The café symbol on its seal matches the sign behind Ren. MIRA: Then you know where this came from.",
            "Focus on the hands and envelope. Dialogue is added separately with a bubble preset. Keep costume colors unchanged.",
            "Close-up of a woman's hand holding an unmarked cream envelope sealed with a simple circular café emblem, charcoal raincoat sleeve and narrow amber bracelet, warm café light against cool rain shadows. No written words.",
        ),
    ]) {
        panel.title = title.into();
        panel.script = script.into();
        panel.notes = notes.into();
        panel.prompt = prompt.into();
    }
    let mira_id = story.alloc_id();
    story.assets.push(Asset { extensions: Default::default(), provenance: None,
        id: mira_id,
        name: "Mira".into(),
        kind: AssetKind::Character,
        notes: "The charcoal coat belongs to the opening-night scenes. Her left amber earring, bag strap, and short bob should remain recognizable even in distant panels.".into(),
        prompt: "Keep the expression subtle and observant. Her posture is self-contained, with relaxed shoulders and one hand often around the bag strap.".into(),
        width: 0,
        height: 0,
        character: CharacterProfile {
            tagline: "A quiet commuter who notices what everyone else misses.".into(),
            role: "Lead · returning city resident".into(),
            age: "28, clearly an adult".into(),
            gender: Some(CharacterGender::Female),
            pronouns: String::new(),
            personality: "Observant, practical, quietly stubborn; restrained gestures and a dry half-smile.".into(),
            backstory: "Mira left the city five years ago. She has returned to clear her late aunt's apartment and keeps postponing the train home.".into(),
            relationships: "Ren was her childhood neighbor. They speak easily about small things and avoid the reason she left.".into(),
            face: "Soft angular face, slightly pointed chin, straight brows, small nose with a rounded tip.".into(),
            hair: "Chin-length black bob, side part on her right, one tucked strand behind the left ear.".into(),
            eyes: "Dark brown almond-shaped eyes, steady gaze, minimal lashes.".into(),
            skin: "Medium warm brown, with a few faint freckles across the nose.".into(),
            build: "Average height, lean adult build, narrow shoulders; clear coat-and-bag silhouette.".into(),
            distinguishing_features: "Small amber drop earring on the LEFT ear, narrow amber bracelet on the right wrist.".into(),
            consistency: "Keep her adult proportions, bob length and right-side part, dark brown eyes, and left amber earring. Her bag strap runs from right shoulder to left hip. Preserve these details in every view.".into(),
            avoid: "Long hair, bangs covering both eyes, oversized childlike eyes, changing the earring side, bright neon accents.".into(),
            outfits: vec![
                CharacterOutfit {
                    name: "Rain commute".into(),
                    description: "Oversized charcoal raincoat to the knees, muted teal knit top, straight black trousers, dark ankle boots, and a tan canvas crossbody bag.".into(),
                },
                CharacterOutfit {
                    name: "Apartment morning".into(),
                    description: "Loose cream shirt with rolled sleeves over a muted teal tank, dark relaxed trousers, simple slippers. Keep the left amber earring and bracelet; no coat or bag.".into(),
                },
            ],
            active_outfit: Some(0),
            palette: vec![
                CharacterSwatch { name: "Coat · charcoal".into(), color: [40, 49, 54, 255] },
                CharacterSwatch { name: "Knit · muted teal".into(), color: [70, 105, 108, 255] },
                CharacterSwatch { name: "Jewelry · amber".into(), color: [211, 159, 72, 255] },
                CharacterSwatch { name: "Bag · canvas tan".into(), color: [180, 155, 118, 255] },
            ],
            references: Vec::new(),
        },
    });
    let ren_id = story.alloc_id();
    story.assets.push(Asset { extensions: Default::default(), provenance: None,
        id: ren_id,
        name: "Ren".into(),
        kind: AssetKind::Character,
        notes: "Ren's bicycle and café apron recur in the opening chapter. His smile is usually warm but guarded. His glasses should not disappear in expression sheets.".into(),
        prompt: "Use natural adult facial proportions, gentle expression changes, and a relaxed standing pose. Keep the bronze glasses frames thin and round.".into(),
        width: 0,
        height: 0,
        character: CharacterProfile {
            tagline: "The café owner who always seems to be waiting for someone.".into(),
            role: "Supporting lead · café owner".into(),
            age: "30, clearly an adult".into(),
            gender: Some(CharacterGender::Male),
            pronouns: String::new(),
            personality: "Patient, sociable, gently teasing; open hands, easy posture, attentive eye contact.".into(),
            backstory: "Ren took over his family's café after university. He has kept its old sign, its bicycle delivery route, and several letters he never sent.".into(),
            relationships: "Mira's former neighbor and closest childhood friend. He remembers the city as it was when she left.".into(),
            face: "Long oval face, broad soft jaw, straight nose, expressive curved eyebrows.".into(),
            hair: "Short wavy dark brown hair, slightly tousled; a small lighter streak at his RIGHT temple.".into(),
            eyes: "Warm hazel eyes behind thin round bronze glasses.".into(),
            skin: "Light olive complexion, soft under-eye shadows, usually clean shaven.".into(),
            build: "Tall, lightly athletic adult build, long forearms and a relaxed forward lean.".into(),
            distinguishing_features: "Round bronze glasses and the lighter streak at his right temple. A small café emblem is stitched on the apron chest.".into(),
            consistency: "Preserve the round glasses, hazel eyes, right-temple streak, and tall proportions. The apron is dark forest green with brown ties; its emblem is a simple circle without readable text.".into(),
            avoid: "Square glasses, heavy facial hair, removing the hair streak, changing the apron to bright green, readable branding.".into(),
            outfits: vec![
                CharacterOutfit {
                    name: "Café closing".into(),
                    description: "Cream short-sleeve shirt, dark forest green waist-to-chest apron with brown ties, tapered dark blue trousers, worn brown work shoes.".into(),
                },
                CharacterOutfit {
                    name: "Delivery ride".into(),
                    description: "Faded indigo jacket over a cream shirt, dark blue trousers with the right cuff rolled, brown shoes and a small leather satchel. No apron; keep glasses.".into(),
                },
            ],
            active_outfit: Some(0),
            palette: vec![
                CharacterSwatch { name: "Apron · forest".into(), color: [48, 79, 63, 255] },
                CharacterSwatch { name: "Shirt · cream".into(), color: [232, 223, 202, 255] },
                CharacterSwatch { name: "Glasses · bronze".into(), color: [151, 111, 63, 255] },
                CharacterSwatch { name: "Trousers · indigo".into(), color: [56, 70, 93, 255] },
            ],
            references: Vec::new(),
        },
    });
    for name in ["Night platform", "Ren's café", "Aunt's apartment"] {
        locations::add_location(&mut story, name)?;
    }
    support::save(
        &directory,
        "Character studio playground",
        &story,
        &AssetBytes::new(),
    )
}
