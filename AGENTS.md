# Scroll Studio product direction

- Keep the first view extremely simple. Put common actions and useful defaults in front; reveal substantial depth through clearly named optional sections.
- Use Adobe-inspired graphite panels and precise visual hierarchy with Procreate-inspired simplicity: compact controls, clear vector icons, restrained blue accents, and depth revealed on demand.
- Keep motion brief and responsive, with stable hit targets and no idle animation loop. Use selective frosted surfaces for controls; keep artwork crisp. Offer solid surfaces and reduced motion in app settings.
- Prefer layout, selection, previews, and direct controls over explanatory prose. Do not add persistent tutorial sentences, marketing copy, duplicate headings, or repeated actions to compensate for unclear UX.
- Characters opens as a scrollable gallery of visual character cards. Open a card for its fixed sheet and provide a clear return to the gallery, preserving browse position. Virtualize large collections. Character sheets use always-visible category navigation and focused detail editors; never stack expanding sections into a scrolling detail page. Keep long text scrolling inside its own editor so navigation and actions stay visible.
- Check the actual native interface at the minimum supported window size as well as a typical laptop size. Preserve comfortable targets and readable labels; hiding overflow or shrinking text is not a substitute for a fitting layout.
- Always inspect empty as well as populated workspaces. Center compact compositions inside a bounded child layout; never justify a call-to-action across the whole workspace. Check painted areas and hit targets, not just text bounds, including large windows and display scaling.
- This progressive disclosure philosophy applies to every future tool. A new user should be able to act immediately without a tutorial, while experienced creators can control details.
- Character sheets and reusable story style are central to the workflow with external image generators. Make copying prompts and reference images easy; combine current story style with character/scene prompts without stale copies.
- Preserve story-wide definitions inside `.scro`, existing story compatibility, undo, and the lightweight native Rust architecture. Do not introduce a browser runtime or an AI service requirement.
- Keep appearance definitions in the story's Bubbles section; chapters edit content, preset choice, and placement.
