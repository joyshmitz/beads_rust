mod common;

use common::cli::{BrWorkspace, run_br};
use std::collections::HashMap;
use std::fs;

fn parse_created_id(stdout: &str) -> String {
    let line = stdout.lines().next().unwrap_or("");
    let id_part = line
        .strip_prefix("Created ")
        .and_then(|rest| rest.split(':').next())
        .unwrap_or("");
    id_part.trim().to_string()
}

#[test]
fn e2e_error_handling() {
    let workspace = BrWorkspace::new();

    let list_uninit = run_br(&workspace, ["list"], "list_uninitialized");
    assert!(!list_uninit.status.success());

    let init = run_br(&workspace, ["init"], "init");
    assert!(init.status.success(), "init failed: {}", init.stderr);

    let create = run_br(&workspace, ["create", "Bad status"], "create");
    assert!(create.status.success(), "create failed: {}", create.stderr);
    let id = parse_created_id(&create.stdout);

    let bad_status = run_br(
        &workspace,
        ["update", &id, "--status", "not_a_status"],
        "update_bad_status",
    );
    assert!(!bad_status.status.success());

    let bad_priority = run_br(
        &workspace,
        ["list", "--priority-min", "9"],
        "list_bad_priority",
    );
    assert!(!bad_priority.status.success());

    let bad_label = run_br(
        &workspace,
        ["update", &id, "--add-label", "bad label"],
        "update_bad_label",
    );
    assert!(!bad_label.status.success());

    let show_missing = run_br(&workspace, ["show", "bd-doesnotexist"], "show_missing");
    assert!(!show_missing.status.success());

    let delete_missing = run_br(&workspace, ["delete", "bd-doesnotexist"], "delete_missing");
    assert!(!delete_missing.status.success());

    let beads_dir = workspace.root.join(".beads");
    let issues_path = beads_dir.join("issues.jsonl");
    fs::write(
        &issues_path,
        "<<<<<<< HEAD\n{}\n=======\n{}\n>>>>>>> branch\n",
    )
    .expect("write conflict jsonl");

    let sync_bad = run_br(&workspace, ["sync", "--import-only"], "sync_bad_jsonl");
    assert!(!sync_bad.status.success());
}

#[test]
fn e2e_dependency_errors() {
    let workspace = BrWorkspace::new();

    let init = run_br(&workspace, ["init"], "init");
    assert!(init.status.success(), "init failed: {}", init.stderr);

    let issue_a = run_br(&workspace, ["create", "Issue A"], "create_a");
    assert!(
        issue_a.status.success(),
        "create A failed: {}",
        issue_a.stderr
    );
    let id_a = parse_created_id(&issue_a.stdout);

    let issue_b = run_br(&workspace, ["create", "Issue B"], "create_b");
    assert!(
        issue_b.status.success(),
        "create B failed: {}",
        issue_b.stderr
    );
    let id_b = parse_created_id(&issue_b.stdout);

    let self_dep = run_br(&workspace, ["dep", "add", &id_a, &id_a], "dep_self");
    assert!(!self_dep.status.success(), "self dependency should fail");

    let add = run_br(&workspace, ["dep", "add", &id_a, &id_b], "dep_add");
    assert!(add.status.success(), "dep add failed: {}", add.stderr);

    let cycle = run_br(&workspace, ["dep", "add", &id_b, &id_a], "dep_cycle");
    assert!(!cycle.status.success(), "cycle dependency should fail");
}

#[test]
fn e2e_sync_invalid_orphans() {
    let workspace = BrWorkspace::new();

    let init = run_br(&workspace, ["init"], "init");
    assert!(init.status.success(), "init failed: {}", init.stderr);

    let create = run_br(&workspace, ["create", "Sync issue"], "create");
    assert!(create.status.success(), "create failed: {}", create.stderr);

    let flush = run_br(&workspace, ["sync", "--flush-only"], "sync_flush");
    assert!(
        flush.status.success(),
        "sync flush failed: {}",
        flush.stderr
    );

    let bad_orphans = run_br(
        &workspace,
        ["sync", "--import-only", "--force", "--orphans", "weird"],
        "sync_bad_orphans",
    );
    assert!(
        !bad_orphans.status.success(),
        "invalid orphans mode should fail"
    );
}

#[test]
fn e2e_ambiguous_id() {
    let workspace = BrWorkspace::new();

    let init = run_br(&workspace, ["init"], "init");
    assert!(init.status.success(), "init failed: {}", init.stderr);

    let mut ids: Vec<String> = Vec::new();
    let mut attempt = 0;
    let mut ambiguous_char: Option<char> = None;

    while ambiguous_char.is_none() && attempt < 20 {
        let title = format!("Ambiguous {attempt}");
        let create = run_br(&workspace, ["create", &title], "create_ambiguous");
        assert!(create.status.success(), "create failed: {}", create.stderr);
        let id = parse_created_id(&create.stdout);
        ids.push(id);

        let mut matches: HashMap<char, std::collections::HashSet<String>> = HashMap::new();
        for id in &ids {
            let hash = id.split('-').nth(1).unwrap_or("");
            for ch in hash.chars() {
                matches.entry(ch).or_default().insert(id.clone());
            }
        }

        ambiguous_char = matches
            .iter()
            .find(|(_, ids)| ids.len() >= 2)
            .map(|(ch, _)| *ch);

        attempt += 1;
    }

    let ambiguous_char = ambiguous_char.expect("failed to find ambiguous char");
    let ambiguous_input = ambiguous_char.to_string();

    let show = run_br(&workspace, ["show", &ambiguous_input], "show_ambiguous");
    assert!(!show.status.success(), "ambiguous id should fail");
}
