# Demo website decision

## Boundary

`prototypes/publishing/demo_site.py` resolves `GET /` and `GET /ss-publish/demo/**`. It takes a method and a path. It does not receive the creator world. Pictures live in `prototypes/publishing/demo_assets/`.

`prototypes/publishing/reader_content_kit/` is reserved for a separate content lane. This service does not create, edit, import, or depend on it. The pages shipped here are the fallback samples.

## Two shapes

An immutable catalogue serves the story from files under its own paths. A seeded publication would insert the same story into the fixture world and reuse `/ss-publish/browser/reader/`.

## Synthesis

The catalogue stays immutable and outside creator runtime. Seeding would share publication ids, hold, removal, and the session cookie with the fixture gate, so a visitor could not read without that gate. The homepage is the one local URL. Reading starts only after **Enter the synthetic demo**. Fixture sign-in remains a separate test-only link. Preview and reader command fields stay as they are.
