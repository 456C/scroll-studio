use super::host::{self, Action, Decision, Running, Status};
use super::process::AddonProcess;
use super::*;
use crate::commands::tests::world;
use crate::commands::{Command, Field, Target};
use crate::model::Story;
use serde_json::{json, Value};
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn manifest(permissions: &[Permission]) -> Manifest {
    Manifest {
        id: "ai-lite".into(),
        name: "AI Lite".into(),
        version: "0.1.0".into(),
        min_app: "0.13.0".into(),
        requires: Vec::new(),
        entry: "ai-lite.exe".into(),
        permissions: permissions.to_vec(),
        download_mb: 4.0,
        hardware: "Any PC that runs Scroll Studio".into(),
        assist: None,
    }
}

const BOTH: [Permission; 2] = [Permission::StoryRead, Permission::StoryPropose];

fn scratch(label: &str) -> std::path::PathBuf {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let path = std::env::temp_dir().join(format!(
        "scroll-addons-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn package(dir: &Path, files: &[(&str, &[u8])]) -> std::path::PathBuf {
    let path = dir.join("addon.zip");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let options = zip::write::SimpleFileOptions::default().unix_permissions(0o755);
    for (name, bytes) in files {
        zip.start_file(*name, options).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
    path
}

#[test]
fn manifests_follow_the_spec_and_refuse_unsafe_values() {
    let spec = br#"{
        "id": "ai-lite", "name": "AI Lite", "version": "0.1.0", "min_app": "0.13.0",
        "requires": [], "entry": "ai-lite.exe", "permissions": ["story.read", "story.propose"],
        "download_mb": 4, "hardware": "Any PC that runs Scroll Studio", "future_field": 1
    }"#;
    let parsed = Manifest::parse(spec).unwrap();
    assert_eq!(parsed, manifest(&BOTH));
    let edit = |change: &dyn Fn(&mut Value)| {
        let mut value: Value = serde_json::from_slice(spec).unwrap();
        change(&mut value);
        Manifest::parse(&serde_json::to_vec(&value).unwrap())
    };
    assert!(edit(&|v| v["id"] = json!("AI Lite")).is_err());
    assert!(edit(&|v| v["id"] = json!("../evil")).is_err());
    assert!(edit(&|v| v["id"] = json!("C:")).is_err());
    assert!(edit(&|v| v["entry"] = json!("../outside.exe")).is_err());
    assert!(edit(&|v| v["entry"] = json!("/usr/bin/sh")).is_err());
    assert!(edit(&|v| v["version"] = json!("one")).is_err());
    assert!(edit(&|v| v["min_app"] = json!("99.0.0")).is_err());
    assert!(edit(&|v| v["permissions"] = json!(["story.apply"])).is_err());
    assert!(edit(&|v| v["permissions"] = json!(["files.delete"])).is_err());
    assert!(Manifest::parse(b"not json").is_err());
}

#[test]
fn installing_listing_enabling_and_removing() {
    let dir = scratch("install");
    let registry = Registry::new(dir.join("addons"));
    assert!(
        registry.list().is_empty(),
        "no addons folder means no addons"
    );
    let manifest_json = serde_json::to_vec(&manifest(&[Permission::StoryRead])).unwrap();
    let zip = package(
        &dir,
        &[
            ("addon.json", &manifest_json),
            ("ai-lite.exe", b"#!/bin/sh\n"),
            ("assets/model.txt", b"x"),
        ],
    );
    let installed = registry.install_zip(&zip).unwrap();
    assert_eq!(installed.id, "ai-lite");
    let list = registry.list();
    assert_eq!(list.len(), 1);
    assert!(
        list[0].manifest.is_ok() && !list[0].enabled,
        "new addons start disabled"
    );
    assert!(list[0].size_bytes > 0);
    registry.set_enabled("ai-lite", true).unwrap();
    assert!(registry.list()[0].enabled);
    // Reinstalling replaces the folder and keeps the enabled choice and the addon's data.
    let data = dir.join("addons/ai-lite/data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("config.json"), b"{\"mcp\":true}").unwrap();
    std::fs::write(dir.join("addons/ai-lite/old-file.txt"), b"x").unwrap();
    registry.install_zip(&zip).unwrap();
    assert!(registry.list()[0].enabled);
    assert_eq!(
        std::fs::read(data.join("config.json")).unwrap(),
        b"{\"mcp\":true}"
    );
    assert!(
        !dir.join("addons/ai-lite/old-file.txt").exists(),
        "other files are replaced"
    );
    // A broken update leaves the installed version as it was.
    let broken = dir.join("broken");
    std::fs::create_dir_all(&broken).unwrap();
    let broken = package(&broken, &[("addon.json", &manifest_json)]);
    assert!(registry.install_zip(&broken).is_err());
    let list = registry.list();
    assert_eq!(list.len(), 1);
    assert!(list[0].folder.join("ai-lite.exe").exists() && list[0].manifest.is_ok());
    // An update interrupted between its two renames is put back, not lost.
    std::fs::rename(
        dir.join("addons/ai-lite"),
        dir.join("addons/.previous-ai-lite"),
    )
    .unwrap();
    let list = registry.list();
    assert_eq!(list.len(), 1);
    assert!(list[0].manifest.is_ok() && list[0].enabled);
    registry.remove("ai-lite").unwrap();
    assert!(registry.list().is_empty());
    assert!(registry.remove("../escape").is_err());
    assert!(registry.remove(".installing-x").is_err());
    // A folder with a name no addon could have is listed, and can be removed.
    std::fs::create_dir_all(dir.join("addons/Not An ID")).unwrap();
    assert_eq!(registry.list().len(), 1);
    registry.remove("Not An ID").unwrap();
    assert!(registry.list().is_empty());
    assert!(registry.set_enabled("C:", true).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bad_packages_install_nothing() {
    let dir = scratch("bad");
    let registry = Registry::new(dir.join("addons"));
    let good = serde_json::to_vec(&manifest(&[])).unwrap();
    let cases: Vec<Vec<(&str, &[u8])>> = vec![
        vec![("ai-lite.exe", b"x")],
        vec![("addon.json", b"{}"), ("ai-lite.exe", b"x")],
        vec![("addon.json", &good)],
        vec![
            ("addon.json", &good),
            ("ai-lite.exe", b"x"),
            ("../escape.txt", b"x"),
        ],
        // Same file on case-insensitive filesystems.
        vec![
            ("addon.json", &good),
            ("ai-lite.exe", b"x"),
            ("AI-LITE.EXE", b"y"),
        ],
    ];
    for (i, files) in cases.iter().enumerate() {
        let case = dir.join(format!("case{i}"));
        std::fs::create_dir_all(&case).unwrap();
        let zip = package(&case, files);
        assert!(registry.install_zip(&zip).is_err(), "case {i} installed");
        assert!(registry.list().is_empty(), "case {i} left something behind");
        assert!(!dir.join("escape.txt").exists() && !dir.join("addons/escape.txt").exists());
    }
    let not_zip = dir.join("not.zip");
    std::fs::write(&not_zip, b"hello").unwrap();
    assert!(registry.install_zip(&not_zip).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn requests_respect_permissions_and_locks() {
    let story = world();
    let read_only = manifest(&[Permission::StoryRead]);
    let both = manifest(&BOTH);
    let none = manifest(&[]);
    let code = |action: Action| match action {
        Action::Fail { code, .. } => code,
        other => panic!("expected a failure, got {other:?}"),
    };
    assert!(matches!(
        host::handle(&story, &read_only, "story.summary", &Value::Null),
        Action::Reply(answer) if answer["title"] == story.title
    ));
    assert_eq!(
        code(host::handle(&story, &none, "story.summary", &Value::Null)),
        host::PERMISSION_DENIED
    );
    let title = json!({ "target_id": 0, "field": "title", "value": "Rain" });
    assert_eq!(
        code(host::handle(&story, &read_only, "edit.set_field", &title)),
        host::PERMISSION_DENIED
    );
    assert_eq!(
        host::handle(&story, &both, "edit.set_field", &title),
        Action::Propose(Box::new(Command::SetField {
            target: Target::Story,
            field: Field::Title,
            value: "Rain".into(),
        }))
    );
    assert_eq!(
        code(host::handle(&story, &both, "edit.unknown", &Value::Null)),
        host::METHOD_NOT_FOUND
    );
    assert_eq!(
        code(host::handle(&story, &both, "files.read", &Value::Null)),
        host::METHOD_NOT_FOUND
    );
    // Edits that cannot apply, or change nothing, are refused before the user sees them.
    let missing = json!({ "target_id": 999_999, "field": "title", "value": "x" });
    assert_eq!(
        code(host::handle(&story, &both, "edit.set_field", &missing)),
        host::REFUSED
    );
    assert_eq!(
        code(host::handle(&story, &both, "edit.batch", &json!([]))),
        host::REFUSED
    );
    // Image commands need bytes this connection cannot carry.
    let image = json!({ "asset_id": story.assets[0].id, "width": 1, "height": 1 });
    assert_eq!(
        code(host::handle(
            &story,
            &both,
            "edit.add_image_version",
            &image
        )),
        host::INVALID_PARAMS
    );
    // host.methods lists exactly what this addon may call.
    let Action::Reply(listed) = host::handle(&story, &read_only, "host.methods", &Value::Null)
    else {
        panic!()
    };
    assert!(listed["methods"]
        .as_array()
        .unwrap()
        .iter()
        .all(|m| !m.as_str().unwrap().starts_with("edit.")));
}

#[test]
fn locked_layers_are_matched_by_id_not_position() {
    // A locked top layer over an unlocked bottom one.
    let mut story = Story::demo();
    let layers = &mut story.chapters[0].layers;
    let top = layers.len() - 1;
    layers[top].locked = true;
    let locked = layers[top].id;
    let unlocked = layers[0].id;
    let both = manifest(&BOTH);
    let allowed = |params: Value, method: &str| {
        matches!(
            host::handle(&story, &both, method, &params),
            Action::Propose(_)
        )
    };
    // Changing unlocked layers that only shift the locked one's position is fine.
    assert!(allowed(
        json!({ "layer_id": unlocked }),
        "edit.delete_layer"
    ));
    assert!(allowed(
        json!({ "layer_id": unlocked, "index": 1 }),
        "edit.move_layer"
    ));
    assert!(allowed(
        json!({ "layer_id": unlocked, "x": 3 }),
        "edit.update_layer"
    ));
    // Changing the locked layer, however it is reached, is refused.
    for (method, params) in [
        ("edit.update_layer", json!({ "layer_id": locked, "x": 1 })),
        ("edit.delete_layer", json!({ "layer_id": locked })),
        (
            "edit.set_field",
            json!({ "target_id": locked, "field": "name", "value": "x" }),
        ),
    ] {
        assert!(
            matches!(host::handle(&story, &both, method, &params), Action::Fail { code, .. } if code == host::REFUSED),
            "{method}"
        );
    }
    // check_edit (run again at Apply) sees a layer locked after the proposal arrived.
    let command = Command::UpdateLayer {
        layer: unlocked,
        patch: crate::commands::LayerPatch {
            x: Some(9.0),
            ..Default::default()
        },
    };
    assert!(host::check_edit(&story, &command).is_ok());
    story.chapters[0].layers[0].locked = true;
    assert!(host::check_edit(&story, &command).is_err());
}

#[test]
fn messages_are_strict_json_rpc_lines() {
    assert_eq!(
        host::parse_message(r#"{"jsonrpc":"2.0","id":7,"method":"story.summary"}"#),
        host::FromAddon::Request {
            id: json!(7),
            method: "story.summary".into(),
            params: Value::Null,
        }
    );
    assert!(matches!(
        host::parse_message("running 1 test"),
        host::FromAddon::Invalid {
            id: None,
            code: host::PARSE_ERROR,
            ..
        }
    ));
    assert!(matches!(
        host::parse_message(r#"{"id":1,"method":"story.summary"}"#),
        host::FromAddon::Invalid { id: Some(_), .. }
    ));
    assert!(matches!(
        host::parse_message(r#"{"jsonrpc":"2.0","id":{"x":1},"method":"story.summary"}"#),
        host::FromAddon::Invalid { id: None, .. }
    ));
    assert!(matches!(
        host::parse_message(r#"{"jsonrpc":"2.0","method":"log"}"#),
        host::FromAddon::Notification { .. }
    ));
    assert_eq!(
        host::parse_message(r#"{"jsonrpc":"2.0","id":5,"result":{}}"#),
        host::FromAddon::Response {
            id: json!(5),
            outcome: Ok(json!({}))
        }
    );
    assert_eq!(
        host::parse_message(r#"{"jsonrpc":"2.0","id":6,"error":{"code":1,"message":"no model"}}"#),
        host::FromAddon::Response {
            id: json!(6),
            outcome: Err("no model".into())
        }
    );
    // A refusal at Apply is told apart from the user saying no.
    let refused: Value = serde_json::from_str(&host::decision_line(
        &json!(1),
        &Decision::Failed("x".into()),
    ))
    .unwrap();
    assert_eq!(refused["error"]["code"], host::REFUSED);
    let cancelled: Value =
        serde_json::from_str(&host::decision_line(&json!(1), &Decision::Cancelled)).unwrap();
    assert_eq!(cancelled["error"]["code"], host::CANCELLED);
}

/// The test binary acting as an addon, in the mode named by `SCROLL_FAKE_ADDON`. Only runs
/// when started by the process tests below.
#[test]
#[ignore]
fn fake_addon() {
    let Some(mode) = std::env::var_os("SCROLL_FAKE_ADDON") else {
        return;
    };
    use std::io::BufRead;
    let out = |value: Value| {
        let mut stdout = std::io::stdout().lock();
        // A leading newline ends the harness's "test ... " line, which the host ignores.
        writeln!(stdout, "\n{value}").unwrap();
        stdout.flush().unwrap();
    };
    let edit = |id: u64| {
        json!({ "jsonrpc": "2.0", "id": id, "method": "edit.set_field",
                "params": { "target_id": 0, "field": "title", "value": "From addon" } })
    };
    match mode.to_str().unwrap() {
        "deaf" => {
            // Never reads its input.
            std::thread::sleep(Duration::from_secs(30));
            return;
        }
        "flood" => {
            for i in 0..20_000 {
                out(json!({ "jsonrpc": "2.0", "method": "log", "params": i }));
            }
            std::thread::sleep(Duration::from_secs(30));
            return;
        }
        _ => {}
    }
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    // Skip the host's answers to the harness's stray output (errors with a null id).
    let mut next = || -> Value {
        loop {
            let value: Value = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
            if !(value["id"].is_null() && value.get("error").is_some()) {
                return value;
            }
        }
    };
    let ready = next();
    assert_eq!(ready["method"], "host.ready");
    eprintln!("ready-revision={}", ready["params"]["revision"]);
    match mode.to_str().unwrap() {
        "crash" => {
            out(edit(1));
            std::thread::sleep(Duration::from_millis(300));
            std::process::exit(3);
        }
        "assistant" => {
            // Answers the app's requests: settings at once, assist after proposing.
            loop {
                let request = next();
                match request["method"].as_str() {
                    Some("addon.settings") => out(json!({ "jsonrpc": "2.0", "id": request["id"],
                        "result": { "fields": [{ "key": "model", "label": "Model", "kind": "text", "value": "tiny" }] } })),
                    Some("addon.assist") => {
                        eprintln!("assist-request={}", request["params"]["request"]);
                        out(edit(10));
                        let decision = next();
                        out(json!({ "jsonrpc": "2.0", "id": request["id"],
                            "result": { "applied": decision["result"]["applied"] } }));
                    }
                    Some("addon.cancel") => eprintln!("cancelled={}", request["params"]["id"]),
                    _ => {}
                }
            }
        }
        "proposer" => {
            // Proposes until its input closes, pausing after a refusal.
            for id in 1.. {
                out(edit(id));
                let decision = next();
                if decision["error"]["code"] == host::REFUSED {
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        }
        _ => {
            out(json!({ "jsonrpc": "2.0", "id": 1, "method": "story.summary" }));
            let summary = next();
            eprintln!("summary-title={}", summary["result"]["title"]);
            out(edit(2));
            // A second edit while the first waits is refused.
            out(json!({ "jsonrpc": "2.0", "id": 3, "method": "edit.add_group" }));
            let busy = next();
            eprintln!("busy-code={}", busy["error"]["code"]);
            let decision = next();
            eprintln!("applied={}", decision["result"]["applied"]);
            let changed = next();
            eprintln!("changed={}", changed["method"]);
            eprintln!("fake-addon-done");
        }
    }
}

fn fake(mode: &str) -> Running {
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "addons::tests::fake_addon",
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("SCROLL_FAKE_ADDON", mode);
    let process = AddonProcess::spawn(command, Arc::new(|| {})).unwrap();
    Running::new(manifest(&BOTH), process, 41)
}

fn until(running: &mut Running, story: &Story, done: impl Fn(&Running) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !done(running) {
        assert!(Instant::now() < deadline, "timed out");
        running.poll(story);
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_real_addon_process_reads_proposes_and_hears_changes() {
    let mut running = fake("full");
    let mut story = Story::demo();
    until(&mut running, &story, |r| r.proposal.is_some());
    // Let the busy refusal go out, then apply the proposal as the app would.
    std::thread::sleep(Duration::from_millis(200));
    running.poll(&story);
    let command = running.proposal.as_ref().unwrap().command.clone();
    let applied = story.apply(command).unwrap();
    running.resolve(Decision::Applied(Box::new(applied)));
    running.story_changed(&crate::commands::StoryChanged {
        revision: 1,
        scope: crate::commands::ChangeScope::Objects(vec![0]),
    });
    until(&mut running, &story, |r| r.status != Status::Running);
    let Status::Stopped(report) = &running.status else {
        unreachable!()
    };
    assert_eq!(story.title, "From addon");
    for expected in [
        "ready-revision=41",
        "summary-title=\"After the rain\"",
        &format!("busy-code={}", host::BUSY),
        "applied=true",
        "changed=\"story.changed\"",
        "fake-addon-done",
    ] {
        assert!(
            report.contains(expected),
            "missing `{expected}` in: {report}"
        );
    }
}

/// Spec release test: killing the addon mid-request leaves the story unchanged and the
/// app responsive.
#[test]
fn an_addon_dying_mid_request_leaves_the_story_unchanged() {
    let mut running = fake("crash");
    let story = Story::demo();
    let before = serde_json::to_vec(&story).unwrap();
    until(&mut running, &story, |r| {
        r.proposal.is_some() || r.status != Status::Running
    });
    until(&mut running, &story, |r| r.status != Status::Running);
    assert!(running.proposal.is_none(), "its proposal went with it");
    // Resolving afterwards is harmless.
    running.resolve(Decision::Cancelled);
    assert_eq!(serde_json::to_vec(&story).unwrap(), before);
    // Killing one directly also leaves nothing behind.
    let mut running = fake("proposer");
    until(&mut running, &story, |r| r.proposal.is_some());
    running.stop("Killed by the test".into());
    assert!(running.proposal.is_none());
    let started = Instant::now();
    running.poll(&story);
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(serde_json::to_vec(&story).unwrap(), before);
}

#[test]
fn an_addon_that_stops_reading_cannot_freeze_the_app() {
    let mut running = fake("deaf");
    let story = Story::demo();
    let started = Instant::now();
    let change = crate::commands::StoryChanged {
        revision: 1,
        scope: crate::commands::ChangeScope::Everything,
    };
    for _ in 0..2000 {
        running.story_changed(&change);
        if running.status != Status::Running {
            break;
        }
    }
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "sending blocked"
    );
    assert!(
        matches!(&running.status, Status::Stopped(reason) if reason.contains("stopped reading")),
        "{:?}",
        running.status
    );
    let _ = story;
}

#[test]
fn a_flooding_addon_cannot_hold_up_a_frame() {
    let mut running = fake("flood");
    let story = Story::demo();
    std::thread::sleep(Duration::from_millis(500));
    let started = Instant::now();
    let polled = running.poll(&story);
    assert!(started.elapsed() < Duration::from_millis(500));
    assert!(polled.more, "the rest waits for the next frame");
}

#[test]
fn repeated_cancels_quiet_the_addon_until_the_user_asks() {
    let mut running = fake("proposer");
    let story = Story::demo();
    for _ in 0..host::MAX_CANCELLED {
        until(&mut running, &story, |r| r.proposal.is_some());
        running.resolve(Decision::Cancelled);
    }
    // Further unrequested suggestions are turned away, and the addon keeps running.
    let quiet = Instant::now() + Duration::from_millis(400);
    while Instant::now() < quiet {
        running.poll(&story);
        assert!(running.proposal.is_none());
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(running.status, Status::Running);
    // Once the user asks, suggestions are shown again, and cancelling them never counts.
    running
        .request("addon.assist", json!({ "request": "x" }))
        .unwrap();
    for _ in 0..host::MAX_CANCELLED + 1 {
        until(&mut running, &story, |r| r.proposal.is_some());
        assert!(running.proposal.as_ref().unwrap().requested);
        running.resolve(Decision::Cancelled);
    }
    assert_eq!(running.status, Status::Running);
}

#[test]
fn the_app_asks_addons_and_hears_their_answers() {
    let mut running = fake("assistant");
    let mut story = Story::demo();
    let settings = running.request("addon.settings", json!({})).unwrap();
    let mut answers = Vec::new();
    until(&mut running, &story, |r| !r.is_pending(settings));
    answers.extend(running.take_answers());
    assert_eq!(answers[0].0, settings);
    assert_eq!(answers[0].1.as_ref().unwrap()["fields"][0]["key"], "model");
    // An answer to a request the app never sent is ignored.
    assert!(running.take_answers().is_empty());

    let assist = running
        .request(
            "addon.assist",
            json!({ "request": "Rename it", "target": { "kind": "story" } }),
        )
        .unwrap();
    until(&mut running, &story, |r| r.proposal.is_some());
    let command = running.proposal.as_ref().unwrap().command.clone();
    let applied = story.apply(command).unwrap();
    running.resolve(Decision::Applied(Box::new(applied)));
    until(&mut running, &story, |r| !r.is_pending(assist));
    let answers = running.take_answers();
    assert_eq!(answers[0].1.as_ref().unwrap()["applied"], true);
    assert_eq!(story.title, "From addon");

    // Cancelling tells the addon; stopping answers whatever is still waiting.
    let gone = running
        .request("addon.assist", json!({ "request": "x" }))
        .unwrap();
    running.cancel_request(gone);
    assert!(!running.is_pending(gone));
    let waiting = running
        .request("addon.settings_never_answered", json!({}))
        .unwrap();
    running.stop("Stopped for the test".into());
    let answers = running.take_answers();
    assert!(answers
        .iter()
        .any(|(id, outcome)| *id == waiting && outcome.is_err()));
    assert!(running.request("addon.settings", json!({})).is_none());
}

#[test]
fn addons_can_read_the_api_description_they_may_use() {
    let story = Story::demo();
    let reader = manifest(&[Permission::StoryRead]);
    let Action::Reply(described) = host::handle(&story, &reader, "host.describe", &Value::Null)
    else {
        panic!("host.describe failed")
    };
    let methods = described["methods"].as_array().unwrap();
    assert_eq!(methods.len(), crate::wire::READ_METHODS.len());
    assert!(methods
        .iter()
        .all(|m| !m["method"].as_str().unwrap().starts_with("edit.")));
    assert_eq!(methods[0]["params"]["type"], "object");
    let both = manifest(&BOTH);
    let Action::Reply(described) = host::handle(&story, &both, "host.describe", &Value::Null)
    else {
        panic!("host.describe failed")
    };
    assert_eq!(
        described["methods"].as_array().unwrap().len(),
        crate::wire::READ_METHODS.len() + crate::wire::EDIT_METHODS.len()
            - host::IMAGE_METHODS.len()
    );
}

#[test]
fn only_proposing_addons_may_assist() {
    let mut m = manifest(&[Permission::StoryRead]);
    m.assist = Some("Ask AI Lite".into());
    let bytes = serde_json::to_vec(&m).unwrap();
    assert!(Manifest::parse(&bytes).is_err());
    m.permissions = BOTH.to_vec();
    assert!(Manifest::parse(&serde_json::to_vec(&m).unwrap()).is_ok());
    m.assist = Some(String::new());
    assert!(Manifest::parse(&serde_json::to_vec(&m).unwrap()).is_err());
}

#[test]
fn edits_that_change_nothing_are_refused_and_batches_may_be_wrapped() {
    let story = Story::demo();
    let both = manifest(&BOTH);
    let same = json!({ "target_id": 0, "field": "title", "value": story.title });
    assert!(matches!(
        host::handle(&story, &both, "edit.set_field", &same),
        Action::Fail {
            code: host::REFUSED,
            ..
        }
    ));
    let rename = json!({ "method": "set_field",
        "params": { "target_id": 0, "field": "title", "value": "Renamed" } });
    for params in [json!([rename.clone()]), json!({ "commands": [rename] })] {
        assert!(matches!(
            host::handle(&story, &both, "edit.batch", &params),
            Action::Propose(_)
        ));
    }
}

#[test]
fn image_commands_are_not_offered_to_addons() {
    // The list matches exactly the commands that need image bytes.
    let mut needing = std::collections::BTreeSet::new();
    for command in crate::commands::tests::commands_for(&world()) {
        let parts = match command {
            Command::Batch(parts) => parts,
            other => vec![other],
        };
        for part in parts {
            if let Ok((method, _)) = crate::wire::command_request(&part) {
                if part.needs_image_bytes() {
                    needing.insert(method);
                }
            }
        }
    }
    let listed: std::collections::BTreeSet<_> =
        host::IMAGE_METHODS.iter().map(|m| m.to_string()).collect();
    assert_eq!(needing, listed);
    let story = Story::demo();
    for method in ["host.methods", "host.describe"] {
        let Action::Reply(answer) = host::handle(&story, &manifest(&BOTH), method, &Value::Null)
        else {
            panic!("{method} failed")
        };
        let text = answer.to_string();
        assert!(
            host::IMAGE_METHODS.iter().all(|m| !text.contains(m)),
            "{method}"
        );
    }
}

#[test]
fn the_real_ai_lite_manifest_is_valid() {
    let bytes = include_bytes!("../../addons/ai-lite/addon.json");
    let mut manifest: Manifest = serde_json::from_slice(bytes).unwrap();
    // Checked against the app version it targets, which may be newer than this build.
    assert!(manifest::parse_version(&manifest.min_app).is_some());
    manifest.min_app = env!("CARGO_PKG_VERSION").into();
    let checked = Manifest::parse(&serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert_eq!(checked.id, "ai-lite");
    assert!(checked.has(Permission::StoryRead) && checked.has(Permission::StoryPropose));
    assert!(checked.assist.is_some());
}
