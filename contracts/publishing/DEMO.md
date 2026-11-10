# Demo website decision

## Boundary

`prototypes/publishing/demo_site.py` resolves `GET /` and `GET /ss-publish/demo/**`. It takes a method and a path. It does not receive the creator world. Pictures live in `prototypes/publishing/demo_assets/`.

`prototypes/publishing/reader_content_kit/` is reserved for a separate content lane. This service does not create, edit, import, or depend on it. The pages shipped here are the fallback samples.

## Two shapes

An immutable catalogue serves the story from files under its own paths. A seeded publication would insert the same story into the fixture world and reuse `/ss-publish/browser/reader/`.

## Synthesis

The catalogue stays immutable and outside creator runtime. Seeding would share publication ids, hold, removal, and the session cookie with the fixture gate, so a visitor could not read without that gate. The homepage is the one local URL. Reading starts only after **Enter the synthetic demo**. Fixture sign-in remains a separate test-only link. Preview and reader command fields stay as they are.

## Follow-up after 91ba489

The first homepage led with a story headline and then listed what the page was not. The follow-up puts the purpose in the heading: read the sample comic and prose. One notice remains: local prototype, synthetic demo, not a real publication. Story pictures stay off the landing page. After the visitor enters, browse and the series page use the original panels as cards and offer Read the comic and Read the prose.

The four JPEGs were created for this synthetic demo. This note does not claim a legal clearance, a license review, or a right to use them anywhere else.

Keyboard and narrow checks use the existing Linux Chrome helper (`google-chrome`, `os.killpg`). They are not Kaijin’s Windows real-Chrome path. Do not treat a skip of `test_demo_reading_in_chrome` or `test_demo_keyboard_and_narrow_layout` as browser proof on Windows. The same exclusion already covers `test_chrome_sign_in_sends_service_origin` and `test_phone_viewport_screenshot`.
