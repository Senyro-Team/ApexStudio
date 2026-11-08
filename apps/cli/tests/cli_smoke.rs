#![allow(clippy::expect_used)]

use apex_core::ProjectInput;
use apex_storage::Workspace;
use std::process::{Command, Stdio};
use tempfile::tempdir;

#[test]
fn lists_dummy_projects_as_json_from_a_disposable_workspace() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path().join("workspace");
    let workspace = Workspace::open(&root).expect("dummy workspace");
    let project = workspace
        .save_project(
            None,
            ProjectInput {
                name: "EMS Dummy Project".into(),
                description: "Generated only for the CLI smoke test".into(),
                tags: vec!["test".into()],
                ..Default::default()
            },
        )
        .expect("dummy project");
    drop(workspace);

    let output = Command::new(env!("CARGO_BIN_EXE_apex-studio-cli"))
        .args([
            "--workspace",
            root.to_str().expect("workspace path is UTF-8"),
            "projects",
            "list",
            "--json",
        ])
        .output()
        .expect("run CLI");

    assert!(output.status.success());
    let projects: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    assert_eq!(projects[0]["name"], "EMS Dummy Project");
    assert_eq!(projects[0]["tags"][0], "test");
    assert!(output.stderr.is_empty());

    let details = Command::new(env!("CARGO_BIN_EXE_apex-studio-cli"))
        .args([
            "--workspace",
            root.to_str().expect("workspace path is UTF-8"),
            "projects",
            "show",
            &project.id,
            "--json",
        ])
        .output()
        .expect("read dummy project details");
    assert!(details.status.success());
    let details: serde_json::Value =
        serde_json::from_slice(&details.stdout).expect("valid project JSON");
    assert_eq!(
        details["description"],
        "Generated only for the CLI smoke test"
    );

    let filtered = Command::new(env!("CARGO_BIN_EXE_apex-studio-cli"))
        .args([
            "--workspace",
            root.to_str().expect("workspace path is UTF-8"),
            "projects",
            "list",
            "--search",
            "no-match",
            "--json",
        ])
        .output()
        .expect("filter dummy projects");
    assert!(filtered.status.success());
    assert_eq!(filtered.stdout, b"[]\n");
}

#[test]
fn separate_cli_processes_can_open_the_same_workspace_during_first_start() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path().join("first-start-workspace");
    let root = root.to_str().expect("workspace path is UTF-8");
    let mut children = Vec::new();

    for _ in 0..4 {
        children.push(
            Command::new(env!("CARGO_BIN_EXE_apex-studio-cli"))
                .args(["--workspace", root, "workspace", "--json"])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("spawn simultaneous CLI client"),
        );
    }

    for child in children {
        let output = child.wait_with_output().expect("wait for CLI client");
        assert!(
            output.status.success(),
            "concurrent workspace open failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let info: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("valid workspace JSON");
        assert!(info["schemaVersion"].as_u64().unwrap_or_default() > 0);
    }
}

#[test]
fn desktop_storage_writes_can_run_while_separate_cli_processes_read() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path().join("concurrent-workspace");
    let workspace = Workspace::open(&root).expect("initialize dummy workspace");
    workspace
        .save_project(
            None,
            ProjectInput {
                name: "Seed project".into(),
                ..Default::default()
            },
        )
        .expect("seed project");
    let root = root.to_str().expect("workspace path is UTF-8").to_owned();
    let mut readers = Vec::new();

    for _ in 0..4 {
        readers.push(
            Command::new(env!("CARGO_BIN_EXE_apex-studio-cli"))
                .args(["--workspace", &root, "projects", "list", "--json"])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("spawn CLI reader"),
        );
    }

    let writer = std::thread::spawn(move || {
        for index in 0..30 {
            workspace
                .save_project(
                    None,
                    ProjectInput {
                        name: format!("Concurrent desktop project {index}"),
                        ..Default::default()
                    },
                )
                .expect("write while CLI clients read");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    });

    for reader in readers {
        let output = reader.wait_with_output().expect("wait for CLI reader");
        assert!(
            output.status.success(),
            "concurrent CLI read failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let projects: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("project list JSON");
        assert!(!projects.as_array().expect("project array").is_empty());
    }
    writer.join().expect("desktop writer thread");
}

#[test]
fn json_command_errors_include_a_stable_error_code() {
    let temp = tempdir().expect("temporary directory");
    let output = Command::new(env!("CARGO_BIN_EXE_apex-studio-cli"))
        .args([
            "--workspace",
            temp.path().to_str().expect("workspace path is UTF-8"),
            "projects",
            "show",
            "missing-project",
            "--json",
        ])
        .output()
        .expect("run failing JSON command");
    assert!(!output.status.success());
    let error: serde_json::Value =
        serde_json::from_slice(&output.stderr).expect("structured JSON error");
    assert_eq!(error["error"]["code"], "not_found");
}
