use super::host::{self, Action, Running, Status};
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
    }
}

fn scratch(label: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "scroll-addons-{label}-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
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
    assert_eq!(
        parsed,
        manifest(&[Permission::StoryRead, Permission::StoryPropose])
    );
    let edit = |change: &dyn Fn(&mut Value)| {
        let mut value: Value = serde_json::from_slice(spec).unwrap();
        change(&mut value);
        Manifest::parse(&serde_json::to_vec(&value).unwrap())
    };
    assert!(edit(&|v| v["id"] = json!("AI Lite")).is_err());
    assert!(edit(&|v| v["id"] = json!("../evil")).is_err());
    assert!(edit(&|v| v["entry"] = json!("../outside.exe")).is_err());
    assert!(edit(&|v| v["entry"] = json!("/usr/bin/sh")).is_err());
    assert!(edit(&|v| v["entry"] = json!("C:\\Windows\\cmd.exe")).is_err() || cfg!(not(windows)));
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
            ("data/model.txt", b"x"),
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
    // Reinstalling replaces the folder and keeps the enabled choice.
    registry.install_zip(&zip).unwrap();
    assert!(registry.list()[0].enabled);
    registry.remove("ai-lite").unwrap();
    assert!(registry.list().is_empty());
    assert!(registry.remove("../escape").is_err());
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
    let both = manifest(&[Permission::StoryRead, Permission::StoryPropose]);
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
    // Edits that cannot apply are refused before the user sees them.
    let missing = json!({ "target_id": 999_999, "field": "title", "value": "x" });
    assert_eq!(
        code(host::handle(&story, &both, "edit.set_field", &missing)),
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
    // Locked layers are off limits, however the edit reaches them.
    let mut locked = Story::demo();
    for layer in &mut locked.chapters[0].layers {
        layer.locked = true;
    }
    let layer = locked.chapters[0].layers[0].id;
    for (method, params) in [
        ("edit.update_layer", json!({ "layer_id": layer, "x": 1 })),
        ("edit.delete_layer", json!({ "layer_id": layer })),
        (
            "edit.set_field",
            json!({ "target_id": layer, "field": "name", "value": "x" }),
        ),
        (
            "edit.replace_text",
            json!({ "find": "a", "replace": "b", "scope": "story" }),
        ),
    ] {
        let action = host::handle(&locked, &both, method, &params);
        if method == "edit.replace_text" && !matches!(action, Action::Fail { .. }) {
            continue; // no locked layer happened to contain "a"
        }
        assert_eq!(code(action), host::REFUSED, "{method}");
    }
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
        host::FromAddon::Invalid { id: None, .. }
    ));
    assert!(matches!(
        host::parse_message(r#"{"id":1,"method":"story.summary"}"#),
        host::FromAddon::Invalid { id: Some(_), .. }
    ));
    assert!(matches!(
        host::parse_message(r#"{"jsonrpc":"2.0","method":"log"}"#),
        host::FromAddon::Notification { .. }
    ));
}

/// The test binary acting as an addon. Only runs when started by
/// `a_real_addon_process_reads_proposes_and_hears_changes`.
#[test]
#[ignore]
fn fake_addon() {
    if std::env::var_os("SCROLL_FAKE_ADDON").is_none() {
        return;
    }
    use std::io::BufRead;
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    let mut next = || -> Value { serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap() };
    let out = |value: Value| {
        let mut stdout = std::io::stdout().lock();
        writeln!(stdout, "\n{value}").unwrap();
        stdout.flush().unwrap();
    };
    // The test harness has printed "test ... " without a newline; end that line so the
    // first message stands on its own (the host ignores the harness's non-JSON lines).
    out(json!({ "note": "fake addon starting" }));
    let ready = next();
    assert_eq!(ready["method"], "host.ready");
    out(json!({ "jsonrpc": "2.0", "id": 1, "method": "story.summary" }));
    let summary = next();
    eprintln!("summary-title={}", summary["result"]["title"]);
    out(
        json!({ "jsonrpc": "2.0", "id": 2, "method": "edit.set_field",
                "params": { "target_id": 0, "field": "title", "value": "From addon" } }),
    );
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

#[test]
fn a_real_addon_process_reads_proposes_and_hears_changes() {
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "addons::tests::fake_addon",
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("SCROLL_FAKE_ADDON", "1");
    let process = AddonProcess::spawn(command, Arc::new(|| {})).unwrap();
    let mut running = Running::new(
        manifest(&[Permission::StoryRead, Permission::StoryPropose]),
        process,
    );
    let mut story = Story::demo();
    let deadline = Instant::now() + Duration::from_secs(60);
    while running.proposal.is_none() {
        assert!(Instant::now() < deadline, "no proposal arrived");
        running.poll(&story);
        std::thread::sleep(Duration::from_millis(10));
    }
    // Let the busy refusal go out, then apply the proposal as the app would.
    std::thread::sleep(Duration::from_millis(200));
    running.poll(&story);
    let command = running.proposal.as_ref().unwrap().command.clone();
    let applied = story.apply(command).unwrap();
    running.resolve(Ok(applied));
    running.story_changed(&crate::commands::StoryChanged {
        revision: 1,
        scope: crate::commands::ChangeScope::Objects(vec![0]),
    });
    while running.status == Status::Running {
        assert!(Instant::now() < deadline, "the addon did not finish");
        running.poll(&story);
        std::thread::sleep(Duration::from_millis(10));
    }
    let Status::Stopped(report) = &running.status else {
        unreachable!()
    };
    assert_eq!(story.title, "From addon");
    for expected in [
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
