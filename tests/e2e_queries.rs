mod common;

use common::cli::{BrWorkspace, extract_json_payload, run_br};
use serde_json::Value;

fn parse_created_id(stdout: &str) -> String {
    let line = stdout.lines().next().unwrap_or("");
    let id_part = line
        .strip_prefix("Created ")
        .and_then(|rest| rest.split(':').next())
        .unwrap_or("");
    id_part.trim().to_string()
}

#[test]
#[allow(clippy::similar_names, clippy::too_many_lines)]
fn e2e_queries_ready_stale_count_search() {
    let workspace = BrWorkspace::new();

    let init = run_br(&workspace, ["init"], "init");
    assert!(init.status.success(), "init failed: {}", init.stderr);

    let blocker = run_br(
        &workspace,
        ["create", "Blocker issue", "-p", "1"],
        "create_blocker",
    );
    assert!(
        blocker.status.success(),
        "blocker create failed: {}",
        blocker.stderr
    );
    let blocker_id = parse_created_id(&blocker.stdout);

    let blocked = run_br(
        &workspace,
        ["create", "Blocked issue", "-p", "2"],
        "create_blocked",
    );
    assert!(
        blocked.status.success(),
        "blocked create failed: {}",
        blocked.stderr
    );
    let blocked_id = parse_created_id(&blocked.stdout);

    let deferred = run_br(
        &workspace,
        ["create", "Deferred issue", "-p", "3"],
        "create_deferred",
    );
    assert!(
        deferred.status.success(),
        "deferred create failed: {}",
        deferred.stderr
    );
    let deferred_id = parse_created_id(&deferred.stdout);

    let closed = run_br(
        &workspace,
        ["create", "Closed issue", "-p", "0"],
        "create_closed",
    );
    assert!(
        closed.status.success(),
        "closed create failed: {}",
        closed.stderr
    );
    let closed_id = parse_created_id(&closed.stdout);

    let label_blocker = run_br(
        &workspace,
        ["update", &blocker_id, "--add-label", "core"],
        "label_blocker",
    );
    assert!(
        label_blocker.status.success(),
        "label update failed: {}",
        label_blocker.stderr
    );

    let dep_add = run_br(
        &workspace,
        ["dep", "add", &blocked_id, &blocker_id],
        "dep_add",
    );
    assert!(
        dep_add.status.success(),
        "dep add failed: {}",
        dep_add.stderr
    );

    let defer_issue = run_br(
        &workspace,
        [
            "update",
            &deferred_id,
            "--status",
            "deferred",
            "--defer",
            "2100-01-01T00:00:00Z",
        ],
        "defer_issue",
    );
    assert!(
        defer_issue.status.success(),
        "defer update failed: {}",
        defer_issue.stderr
    );

    let close_issue = run_br(
        &workspace,
        ["update", &closed_id, "--status", "closed"],
        "close_issue",
    );
    assert!(
        close_issue.status.success(),
        "close update failed: {}",
        close_issue.stderr
    );

    let ready = run_br(&workspace, ["ready", "--json"], "ready");
    assert!(ready.status.success(), "ready failed: {}", ready.stderr);
    let ready_payload = extract_json_payload(&ready.stdout);
    let ready_json: Vec<Value> = serde_json::from_str(&ready_payload).expect("ready json");
    assert!(ready_json.iter().any(|item| item["id"] == blocker_id));
    assert!(!ready_json.iter().any(|item| item["id"] == blocked_id));
    assert!(!ready_json.iter().any(|item| item["id"] == deferred_id));

    let ready_text = run_br(&workspace, ["ready"], "ready_text");
    assert!(
        ready_text.status.success(),
        "ready text failed: {}",
        ready_text.stderr
    );
    assert!(
        ready_text.stdout.contains("Ready to work"),
        "ready text missing header"
    );

    let ready_core = run_br(
        &workspace,
        ["ready", "--json", "--label", "core"],
        "ready_label",
    );
    assert!(
        ready_core.status.success(),
        "ready label failed: {}",
        ready_core.stderr
    );
    let ready_core_payload = extract_json_payload(&ready_core.stdout);
    let ready_core_json: Vec<Value> =
        serde_json::from_str(&ready_core_payload).expect("ready label json");
    assert_eq!(ready_core_json.len(), 1);
    assert_eq!(ready_core_json[0]["id"], blocker_id);

    let blocked = run_br(&workspace, ["blocked", "--json"], "blocked");
    assert!(
        blocked.status.success(),
        "blocked failed: {}",
        blocked.stderr
    );
    let blocked_payload = extract_json_payload(&blocked.stdout);
    let blocked_json: Vec<Value> = serde_json::from_str(&blocked_payload).expect("blocked json");
    assert!(blocked_json.iter().any(|item| item["id"] == blocked_id));

    let blocked_text = run_br(&workspace, ["blocked"], "blocked_text");
    assert!(
        blocked_text.status.success(),
        "blocked text failed: {}",
        blocked_text.stderr
    );
    assert!(
        blocked_text.stdout.contains("Blocked Issues"),
        "blocked text missing header"
    );

    let search = run_br(
        &workspace,
        ["search", "Blocker", "--status", "open", "--json"],
        "search",
    );
    assert!(search.status.success(), "search failed: {}", search.stderr);
    let search_payload = extract_json_payload(&search.stdout);
    let search_json: Vec<Value> = serde_json::from_str(&search_payload).expect("search json");
    assert!(search_json.iter().any(|item| item["id"] == blocker_id));

    let search_text = run_br(&workspace, ["search", "Blocker"], "search_text");
    assert!(
        search_text.status.success(),
        "search text failed: {}",
        search_text.stderr
    );
    assert!(
        search_text.stdout.contains("Blocker issue"),
        "search text missing issue title"
    );

    let count = run_br(
        &workspace,
        ["count", "--by", "status", "--include-closed", "--json"],
        "count",
    );
    assert!(count.status.success(), "count failed: {}", count.stderr);
    let count_payload = extract_json_payload(&count.stdout);
    let count_json: Value = serde_json::from_str(&count_payload).expect("count json");
    assert_eq!(count_json["total"], 4);

    let groups = count_json["groups"].as_array().expect("count groups array");
    let mut counts = std::collections::BTreeMap::new();
    for group in groups {
        let key = group["group"].as_str().unwrap_or("").to_string();
        let value = group["count"].as_u64().unwrap_or(0);
        counts.insert(key, value);
    }
    assert_eq!(counts.get("open"), Some(&2));
    assert_eq!(counts.get("deferred"), Some(&1));
    assert_eq!(counts.get("closed"), Some(&1));

    let count_text = run_br(
        &workspace,
        ["count", "--by", "status", "--include-closed"],
        "count_text",
    );
    assert!(
        count_text.status.success(),
        "count text failed: {}",
        count_text.stderr
    );
    assert!(
        count_text.stdout.contains("Total:"),
        "count text missing total"
    );

    let count_priority = run_br(
        &workspace,
        [
            "count",
            "--by",
            "priority",
            "--priority",
            "0",
            "--include-closed",
            "--json",
        ],
        "count_priority",
    );
    assert!(
        count_priority.status.success(),
        "count priority failed: {}",
        count_priority.stderr
    );
    let count_priority_payload = extract_json_payload(&count_priority.stdout);
    let count_priority_json: Value =
        serde_json::from_str(&count_priority_payload).expect("count priority json");
    assert_eq!(count_priority_json["total"], 1);

    let stale = run_br(&workspace, ["stale", "--days", "0", "--json"], "stale");
    assert!(stale.status.success(), "stale failed: {}", stale.stderr);
    let stale_payload = extract_json_payload(&stale.stdout);
    let stale_json: Vec<Value> = serde_json::from_str(&stale_payload).expect("stale json");
    assert!(stale_json.len() >= 2);
    assert!(stale_json.iter().any(|item| item["id"] == blocker_id));
    assert!(stale_json.iter().any(|item| item["id"] == blocked_id));
}
