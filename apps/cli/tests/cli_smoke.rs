#![allow(clippy::expect_used)]

use apex_core::{
    ProjectInput, PublishingImage, PublishingInput, PublishingLivery, PublishingSide,
    WorkspaceRepository,
};
use apex_storage::Workspace;
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::{Command, Stdio},
};
use tempfile::tempdir;

#[test]
fn team_sync_help_exposes_member_workflows_without_password_arguments() {
    let sync_help = Command::new(env!("CARGO_BIN_EXE_apex-studio-cli"))
        .args(["sync", "--help"])
        .output()
        .expect("run sync help");
    assert!(sync_help.status.success());
    let sync_help = String::from_utf8(sync_help.stdout).expect("sync help is UTF-8");
    for command in [
        "check",
        "status",
        "sign-in",
        "accept-invitation",
        "invite",
        "members",
        "remove-member",
        "disconnect",
    ] {
        assert!(sync_help.contains(command), "missing command {command}");
    }

    let sign_in_help = Command::new(env!("CARGO_BIN_EXE_apex-studio-cli"))
        .args(["sync", "sign-in", "--help"])
        .output()
        .expect("run sign-in help");
    assert!(sign_in_help.status.success());
    assert!(!String::from_utf8_lossy(&sign_in_help.stdout).contains("--password"));

    let password_arg = Command::new(env!("CARGO_BIN_EXE_apex-studio-cli"))
        .args([
            "sync",
            "sign-in",
            "https://sync.example.test",
            "--email",
            "user@example.test",
            "--password",
            "not-accepted",
        ])
        .output()
        .expect("reject password argument");
    assert!(!password_arg.status.success());
    assert!(String::from_utf8_lossy(&password_arg.stderr).contains("--password"));
}

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
fn statistics_and_group_revenue_read_saved_snapshots_from_a_dummy_workspace() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path().join("workspace");
    let workspace = Workspace::open(&root).expect("dummy workspace");
    workspace
        .set_statistics_profile("2919579206")
        .expect("set dummy ERLCX profile");
    let old = apex_storage::StatisticsPost {
        bundle_id: 11,
        title: "Older post".into(),
        views: Some(100),
        likes: Some(9),
        copies: None,
        favourites: Some(4),
        rating_count: Some(2),
        rating_average: Some(4.5),
    };
    let latest = apex_storage::StatisticsPost {
        bundle_id: 11,
        title: "Current post".into(),
        views: Some(125),
        likes: Some(12),
        copies: Some(3),
        favourites: None,
        rating_count: Some(4),
        rating_average: Some(4.0),
    };
    workspace
        .save_statistics_snapshot("2919579206", &[old])
        .expect("first dummy profile snapshot");
    workspace
        .save_statistics_snapshot("2919579206", &[latest])
        .expect("latest dummy profile snapshot");
    workspace
        .save_statistics_sales(
            "2919579206",
            &apex_storage::SalesData {
                sales: 1,
                all_time_sales: 1,
                net_robux: 70,
                gross_robux: 100,
                series: vec![apex_storage::SalesPoint {
                    date: "2026-11-09".into(),
                    sales: 1,
                    net_robux: 70,
                }],
            },
        )
        .expect("seed dummy creator sales");
    let summary = cli_json(&root, &["statistics", "summary"]);
    assert!(summary.status.success());
    let summary: serde_json::Value = serde_json::from_slice(&summary.stdout).expect("summary JSON");
    assert_eq!(summary["changes"]["views"], "+25");
    assert_eq!(summary["topPosts"][0]["bundleId"], 11);
    assert_eq!(summary["topPosts"][0]["title"], "Current post");
    let group = apex_storage::RobloxRevenueGroup {
        group_id: "12345".into(),
        name: "Apex Dummy Group".into(),
        experience_ids: vec!["67890".into()],
        pass_mappings: BTreeMap::new(),
        selected_pass_ids: vec!["45678".into()],
        api_updated_at: None,
        api_coverage_from: None,
        api_coverage_to: None,
        api_summary: None,
        api_status: None,
        updated_at: None,
        last_import_at: None,
    };
    let sale = apex_storage::RobloxGroupSale {
        group_id: "12345".into(),
        sale_fingerprint: "a".repeat(64),
        universe_id: "67890".into(),
        asset_id: "45678".into(),
        sale_at: "2026-11-09T12:00:00Z".into(),
        sale_location: "Game".into(),
        hold_status: "Released".into(),
        price: 100,
        revenue: 70,
        import_batch_id: "dummy-batch".into(),
        external_transaction_id: None,
        source: "csv".into(),
        dedupe_confidence: "exact_fields".into(),
        imported_at: None,
    };
    workspace
        .import_roblox_group_sales(&group, "dummy-batch", &[sale])
        .expect("import dummy group sale");
    drop(workspace);

    let output = Command::new(env!("CARGO_BIN_EXE_apex-studio-cli"))
        .args([
            "--workspace",
            root.to_str().expect("workspace path is UTF-8"),
            "statistics",
            "summary",
            "--json",
        ])
        .output()
        .expect("run statistics summary");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid summary JSON");
    assert_eq!(summary["profileId"], "2919579206");
    assert_eq!(summary["snapshotCount"], 2);
    assert_eq!(summary["postCount"], 1);
    assert_eq!(summary["views"]["total"], 125);
    assert_eq!(summary["views"]["knownPosts"], 1);
    assert_eq!(summary["copies"]["knownPosts"], 1);
    assert_eq!(summary["favourites"]["total"], serde_json::Value::Null);
    assert!(matches!(
        summary["freshness"].as_str(),
        Some("fresh" | "stale" | "clock skew" | "unknown")
    ));
    assert!(matches!(
        summary["creatorSalesFreshness"].as_str(),
        Some("fresh" | "stale" | "clock skew" | "unknown")
    ));

    let output = Command::new(env!("CARGO_BIN_EXE_apex-studio-cli"))
        .args([
            "--workspace",
            root.to_str().expect("workspace path is UTF-8"),
            "revenue",
            "show",
            "12345",
            "--json",
        ])
        .output()
        .expect("run Group Revenue detail");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let group: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid revenue JSON");
    assert_eq!(group["group"]["name"], "Apex Dummy Group");
    assert_eq!(group["sales"][0]["price"], 100);

    let comparison = cli_json(&root, &["statistics", "compare", "--range", "all"]);
    assert!(
        comparison.status.success(),
        "{}",
        String::from_utf8_lossy(&comparison.stderr)
    );
    let comparison: serde_json::Value =
        serde_json::from_slice(&comparison.stdout).expect("valid comparison JSON");
    assert_eq!(comparison["startDate"], "2026-11-09");
    assert_eq!(comparison["endDate"], "2026-11-09");
    assert_eq!(comparison["robloxCoverage"], "observed_rows_only");
    assert_eq!(comparison["robloxPrice"], 100);
    assert_eq!(comparison["robloxRevenue"], 70);
    assert_eq!(group["sales"][0]["revenue"], 70);
}

#[test]
fn creator_sales_refresh_requires_a_configured_profile_before_opening_chrome() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path().join("workspace");
    let output = cli_json(&root, &["statistics", "refresh-sales"]);
    assert!(!output.status.success());
    let error: serde_json::Value =
        serde_json::from_slice(&output.stderr).expect("profile validation error JSON");
    assert_eq!(error["error"]["code"], "statistics_profile_missing");
    assert!(error["error"]["message"]
        .as_str()
        .expect("error message")
        .contains("Set the Roblox user ID"));
}

#[test]
fn revenue_cli_configures_groups_selects_passes_and_confirms_removal() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path().join("workspace");
    let configured = cli_json(
        &root,
        &[
            "revenue",
            "configure",
            "--group-id",
            "16538500",
            "--name",
            "APX Dummy",
            "--experience-id",
            "84560840200510",
        ],
    );
    assert!(
        configured.status.success(),
        "{}",
        String::from_utf8_lossy(&configured.stderr)
    );
    let config: serde_json::Value =
        serde_json::from_slice(&configured.stdout).expect("configured group JSON");
    assert_eq!(config["name"], "APX Dummy");
    assert_eq!(config["experienceIds"][0], "84560840200510");

    let workspace = Workspace::open(&root).expect("dummy workspace");
    let group = workspace
        .roblox_group_revenue()
        .expect("configured revenue state")
        .groups
        .remove(0);
    workspace
        .import_roblox_group_sales(
            &group,
            "fixture-batch",
            &[apex_storage::RobloxGroupSale {
                group_id: group.group_id.clone(),
                sale_fingerprint: "b".repeat(64),
                universe_id: "84560840200510".into(),
                asset_id: "2053361604".into(),
                sale_at: "2026-11-10T12:00:00Z".into(),
                sale_location: "Website".into(),
                hold_status: "Released".into(),
                price: 1000,
                revenue: 700,
                import_batch_id: "fixture-batch".into(),
                external_transaction_id: None,
                source: "csv".into(),
                dedupe_confidence: "exact_fields".into(),
                imported_at: None,
            }],
        )
        .expect("seed a dummy sale row");
    drop(workspace);

    let selected = cli_json(
        &root,
        &[
            "revenue",
            "select-pass",
            "--group-id",
            "16538500",
            "--asset-id",
            "2053361604",
        ],
    );
    assert!(selected.status.success());
    let selected: serde_json::Value =
        serde_json::from_slice(&selected.stdout).expect("selected group JSON");
    assert_eq!(selected["selectedPassIds"][0], "2053361604");

    let deselected = cli_json(
        &root,
        &[
            "revenue",
            "deselect-pass",
            "--group-id",
            "16538500",
            "--asset-id",
            "2053361604",
        ],
    );
    assert!(deselected.status.success());
    let deselected: serde_json::Value =
        serde_json::from_slice(&deselected.stdout).expect("deselected group JSON");
    assert_eq!(deselected["selectedPassIds"], serde_json::json!([]));

    let blocked_remove = cli_json(&root, &["revenue", "remove", "--group-id", "16538500"]);
    assert!(!blocked_remove.status.success());
    let error: serde_json::Value =
        serde_json::from_slice(&blocked_remove.stderr).expect("confirmation error JSON");
    assert_eq!(error["error"]["code"], "confirmation_required");

    let removed = cli_json(
        &root,
        &["revenue", "remove", "--group-id", "16538500", "--yes"],
    );
    assert!(removed.status.success());
    let removed: serde_json::Value =
        serde_json::from_slice(&removed.stdout).expect("remove result JSON");
    assert_eq!(removed["removedGroupId"], "16538500");
}

#[test]
fn revenue_csv_preview_import_and_reimport_are_safe_in_a_dummy_workspace() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path().join("workspace");
    let configured = cli_json(
        &root,
        &[
            "revenue",
            "configure",
            "--group-id",
            "16538500",
            "--name",
            "APX Dummy",
            "--experience-id",
            "84560840200510",
        ],
    );
    assert!(configured.status.success());
    let csv_path = temp.path().join("creator-sales.csv");
    let csv = "Id,Sale Date and Time,Sale Location,Universe Id,Asset Id,Asset Type,Hold Status,Revenue,Price,Buyer User ID\r\ntransaction-1,2026-11-07T22:31:20Z,\"Website, Premium\",84560840200510,2053361604,Game Pass,Released,700,1000,private-buyer\r\ntransaction-2,2026-11-08T12:00:00Z,Website,84560840200510,100,Clothing,Released,10,15,another-buyer\r\n";
    fs::write(&csv_path, csv).expect("write dummy Roblox export");
    let path = csv_path.to_str().expect("CSV path is UTF-8");

    let preview = cli_json(
        &root,
        &[
            "revenue",
            "import-csv",
            "--group-id",
            "16538500",
            "--preview-only",
            path,
        ],
    );
    assert!(preview.status.success());
    let preview: serde_json::Value = serde_json::from_slice(&preview.stdout).expect("preview JSON");
    assert_eq!(preview["rows"], 1);
    assert_eq!(preview["skippedRows"], 1);
    assert!(!preview.to_string().contains("private-buyer"));
    assert_eq!(
        fs::read_to_string(&csv_path).expect("CSV remains original"),
        csv
    );

    let unconfirmed = cli_json(
        &root,
        &["revenue", "import-csv", "--group-id", "16538500", path],
    );
    assert!(!unconfirmed.status.success());
    let error: serde_json::Value = serde_json::from_slice(&unconfirmed.stderr).expect("error JSON");
    assert_eq!(error["error"]["code"], "confirmation_required");

    let imported = cli_json(
        &root,
        &[
            "revenue",
            "import-csv",
            "--group-id",
            "16538500",
            "--yes",
            path,
        ],
    );
    assert!(
        imported.status.success(),
        "{}",
        String::from_utf8_lossy(&imported.stderr)
    );
    let imported: serde_json::Value =
        serde_json::from_slice(&imported.stdout).expect("import JSON");
    assert_eq!(imported["import"]["inserted"], 1);
    assert_eq!(imported["import"]["duplicates"], 0);
    assert!(!imported.to_string().contains("private-buyer"));

    let repeated = cli_json(
        &root,
        &[
            "revenue",
            "import-csv",
            "--group-id",
            "16538500",
            "--yes",
            path,
        ],
    );
    assert!(repeated.status.success());
    let repeated: serde_json::Value =
        serde_json::from_slice(&repeated.stdout).expect("repeat import JSON");
    assert_eq!(repeated["import"]["inserted"], 0);
    assert_eq!(repeated["import"]["duplicates"], 1);
}

#[test]
fn code_export_combines_selected_saved_posts_as_markdown_without_overwriting() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path().join("workspace");
    let workspace = Workspace::open(&root).expect("dummy workspace");
    let mut post_ids = Vec::new();
    for (title, vehicle, id) in [
        ("Patrol *One*", "Falcon Interceptor", "123456"),
        ("Fire Support", "Rescue Truck", "987654"),
    ] {
        let id_post = workspace.create_post().expect("create post");
        let revision = workspace.post(&id_post).expect("read new post").revision;
        workspace
            .save_post(
                &id_post,
                revision,
                PublishingInput {
                    title: title.into(),
                    liveries: vec![PublishingLivery {
                        source: None,
                        vehicle: vehicle.into(),
                        primary_color: "#FFFFFF".into(),
                        secondary_color: None,
                        opacity: 100,
                        sides: vec![PublishingSide {
                            side: "Left 1".into(),
                            decal_id: id.into(),
                        }],
                    }],
                    ..Default::default()
                },
            )
            .expect("save post");
        post_ids.push(id_post);
    }
    drop(workspace);

    let output_path = temp.path().join("codes.md");
    let mut command = Command::new(env!("CARGO_BIN_EXE_apex-studio-cli"));
    command.args([
        "--workspace",
        root.to_str().expect("workspace path is UTF-8"),
        "codes",
        "export",
        output_path.to_str().expect("output path is UTF-8"),
        "--post",
        &post_ids[0],
        "--post",
        &post_ids[1],
    ]);
    let exported = command.output().expect("run code export");
    assert!(
        exported.status.success(),
        "{}",
        String::from_utf8_lossy(&exported.stderr)
    );
    let markdown = fs::read_to_string(&output_path).expect("read Markdown output");
    assert!(markdown.contains("# Patrol \\*One\\*"));
    assert!(markdown.contains("`123456`"));
    assert!(markdown.contains("`987654`"));

    let duplicate = Command::new(env!("CARGO_BIN_EXE_apex-studio-cli"))
        .args([
            "--workspace",
            root.to_str().expect("workspace path is UTF-8"),
            "codes",
            "export",
            output_path.to_str().expect("output path is UTF-8"),
            "--post",
            &post_ids[0],
            "--json",
        ])
        .output()
        .expect("run duplicate export");
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("output_exists"));
}

#[test]
fn imports_a_public_apexpublish_bundle_as_a_new_draft() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path().join("workspace");
    let workspace = Workspace::open(&root).expect("dummy workspace");
    let media_paths = [
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../design/Logo_Main.png"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../design/Banner_Small.png"),
    ];
    let media = media_paths
        .iter()
        .map(|path| PublishingImage {
            hash: workspace.import_file(path).expect("import fixture image"),
            name: path
                .file_name()
                .expect("fixture filename")
                .to_string_lossy()
                .into_owned(),
        })
        .collect();
    let original = workspace.create_post().expect("create original draft");
    let draft = workspace.post(&original).expect("read original draft");
    let saved = workspace
        .save_post(
            &original,
            draft.revision,
            PublishingInput {
                title: "Imported bundle test".into(),
                description: "Disposable CLI integration fixture".into(),
                media,
                liveries: vec![PublishingLivery {
                    source: None,
                    vehicle: "Falcon Interceptor".into(),
                    primary_color: "#FFFFFF".into(),
                    secondary_color: None,
                    opacity: 100,
                    sides: vec![PublishingSide {
                        side: "Left 1".into(),
                        decal_id: "123456".into(),
                    }],
                }],
                ..Default::default()
            },
        )
        .expect("save original draft");
    let bundle = temp.path().join("post.apexpublish");
    workspace
        .export_post(&original, saved.revision, false, &bundle)
        .expect("export fixture bundle");
    drop(workspace);

    let imported = Command::new(env!("CARGO_BIN_EXE_apex-studio-cli"))
        .args([
            "--workspace",
            root.to_str().expect("workspace path is UTF-8"),
            "posts",
            "import",
            bundle.to_str().expect("bundle path is UTF-8"),
            "--json",
        ])
        .output()
        .expect("run bundle import");
    assert!(
        imported.status.success(),
        "{}",
        String::from_utf8_lossy(&imported.stderr)
    );
    let result: serde_json::Value =
        serde_json::from_slice(&imported.stdout).expect("valid import JSON");
    let imported_id = result["id"].as_str().expect("imported draft ID");
    let reopened = Workspace::open(&root).expect("reopen dummy workspace");
    let imported = reopened.post(imported_id).expect("read imported draft");
    assert_eq!(imported.data.title, "Imported bundle test");
    assert_eq!(imported.data.media.len(), 2);
    assert_eq!(imported.data.liveries[0].sides[0].decal_id, "123456");
}

#[test]
fn roblox_commands_show_liveries_and_queue_without_starting_network_uploads() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path().join("workspace");
    let workspace = Workspace::open(&root).expect("dummy workspace");
    let project = workspace
        .save_project(
            None,
            ProjectInput {
                name: "EMS Test Project".into(),
                ..Default::default()
            },
        )
        .expect("create project");
    let vehicle = apex_core::vehicle_catalogue()
        .expect("official vehicle catalogue")
        .vehicles
        .into_iter()
        .find(|vehicle| vehicle.livery_support == "supported")
        .expect("a livery-supported vehicle");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../design/Banner_Small.png");
    workspace
        .import_livery(
            &project.id,
            None,
            None,
            "Highway Patrol",
            &vehicle.id,
            &[(source.clone(), "Left1.png".into(), Some("Left 1".into()))],
            None,
        )
        .expect("import fixture livery");
    drop(workspace);

    let liveries = Command::new(env!("CARGO_BIN_EXE_apex-studio-cli"))
        .args([
            "--workspace",
            root.to_str().expect("workspace path is UTF-8"),
            "roblox",
            "liveries",
            "--account-id",
            "123456",
            "--json",
        ])
        .output()
        .expect("list Roblox livery candidates");
    assert!(liveries.status.success());
    let liveries: serde_json::Value =
        serde_json::from_slice(&liveries.stdout).expect("livery JSON");
    assert_eq!(liveries[0]["name"], "Highway Patrol");
    assert_eq!(liveries[0]["textures"], 1);
    let livery_id = liveries[0]["id"].as_str().expect("livery ID");

    let queue = Command::new(env!("CARGO_BIN_EXE_apex-studio-cli"))
        .args([
            "--workspace",
            root.to_str().expect("workspace path is UTF-8"),
            "roblox",
            "queue",
            "--json",
        ])
        .output()
        .expect("inspect Roblox queue");
    assert!(queue.status.success());
    let queue: serde_json::Value = serde_json::from_slice(&queue.stdout).expect("queue JSON");
    assert_eq!(queue["batches"].as_array().expect("batch list").len(), 0);
    assert_eq!(queue["jobs"].as_array().expect("job list").len(), 0);

    let empty_run = Command::new(env!("CARGO_BIN_EXE_apex-studio-cli"))
        .args([
            "--workspace",
            root.to_str().expect("workspace path is UTF-8"),
            "--json",
            "roblox",
            "run",
            "--yes",
        ])
        .output()
        .expect("run the empty Roblox queue");
    assert!(empty_run.status.success());
    assert!(empty_run.stderr.is_empty());
    let empty_run: serde_json::Value =
        serde_json::from_slice(&empty_run.stdout).expect("empty-run JSON");
    assert_eq!(
        empty_run["batches"].as_array().expect("batch list").len(),
        0
    );

    let batch = Command::new(env!("CARGO_BIN_EXE_apex-studio-cli"))
        .args([
            "--workspace",
            root.to_str().expect("workspace path is UTF-8"),
            "roblox",
            "batch",
            "start",
            "--account-id",
            "123456",
            "--livery",
            livery_id,
            "--yes",
            "--json",
        ])
        .output()
        .expect("create offline batch");
    assert!(batch.status.success());
    let batch: serde_json::Value = serde_json::from_slice(&batch.stdout).expect("batch JSON");
    assert_eq!(batch["uploadPasses"], 2);
    let batch_id = batch["batchId"].as_str().expect("batch ID");

    let paused = Command::new(env!("CARGO_BIN_EXE_apex-studio-cli"))
        .args([
            "--workspace",
            root.to_str().expect("workspace path is UTF-8"),
            "roblox",
            "batch",
            "pause",
            batch_id,
            "--json",
        ])
        .output()
        .expect("pause offline batch");
    assert!(paused.status.success());
    let workspace = Workspace::open(&root).expect("reopen workspace");
    let snapshot = workspace.upload_snapshot().expect("queue snapshot");
    assert_eq!(snapshot.batches[0].state, "paused");
    assert_eq!(snapshot.jobs.len(), 1);

    let double_upload = Command::new(env!("CARGO_BIN_EXE_apex-studio-cli"))
        .args([
            "--workspace",
            root.to_str().expect("workspace path is UTF-8"),
            "roblox",
            "double-upload",
            "--disable",
            "--json",
        ])
        .output()
        .expect("disable double upload");
    assert!(double_upload.status.success());
    let setting: serde_json::Value =
        serde_json::from_slice(&double_upload.stdout).expect("setting JSON");
    assert_eq!(setting["doubleUpload"], false);
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

#[test]
fn project_commands_create_edit_archive_restore_and_confirm_delete() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path().join("workspace");
    let input = temp.path().join("project.json");
    fs::write(
        &input,
        serde_json::to_vec(&ProjectInput {
            name: "Created from JSON".into(),
            description: "Before edit".into(),
            ..Default::default()
        })
        .expect("serialize input"),
    )
    .expect("write input");

    let created = cli_json(
        &root,
        &["projects", "create", input.to_str().expect("input path")],
    );
    assert!(created.status.success());
    let created: serde_json::Value = serde_json::from_slice(&created.stdout).expect("project JSON");
    let id = created["id"].as_str().expect("project ID");

    fs::write(
        &input,
        serde_json::to_vec(&ProjectInput {
            name: "Edited from JSON".into(),
            tags: vec!["cli".into()],
            ..Default::default()
        })
        .expect("serialize updated input"),
    )
    .expect("write updated input");
    let updated = cli_json(
        &root,
        &[
            "projects",
            "update",
            id,
            input.to_str().expect("input path"),
        ],
    );
    assert!(updated.status.success());
    let updated: serde_json::Value = serde_json::from_slice(&updated.stdout).expect("updated JSON");
    assert_eq!(updated["name"], "Edited from JSON");
    assert_eq!(updated["tags"][0], "cli");

    assert!(cli_json(&root, &["projects", "archive", id])
        .status
        .success());
    assert!(cli_json(&root, &["projects", "restore", id])
        .status
        .success());
    assert!(cli_json(&root, &["projects", "archive", id])
        .status
        .success());
    let refused = cli_json(&root, &["projects", "delete", id]);
    assert!(!refused.status.success());
    let error: serde_json::Value = serde_json::from_slice(&refused.stderr).expect("JSON error");
    assert_eq!(error["error"]["code"], "confirmation_required");
    assert!(cli_json(&root, &["projects", "delete", id, "--yes"])
        .status
        .success());
    assert!(Workspace::open(&root)
        .expect("reopen workspace")
        .projects()
        .expect("list projects")
        .is_empty());
}

#[test]
fn markdown_notes_and_asset_commands_preserve_sources_and_support_trash_recovery() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path().join("workspace");
    let workspace = Workspace::open(&root).expect("workspace");
    let project = workspace
        .save_project(
            None,
            ProjectInput {
                name: "CLI asset fixture".into(),
                ..Default::default()
            },
        )
        .expect("project");
    drop(workspace);

    let source_image = temp.path().join("preview.png");
    let image_bytes = include_bytes!("../../erlcx-extension/icon32.png");
    fs::write(&source_image, image_bytes).expect("dummy PNG");
    let markdown_path = temp.path().join("note.md");
    fs::write(
        &markdown_path,
        "# Livery\n\nSee **details** and ![preview](preview.png).\n",
    )
    .expect("Markdown source");
    let note = cli_json(
        &root,
        &[
            "notes",
            "create",
            "--project",
            &project.id,
            "--title",
            "CLI note",
            "--input",
            markdown_path.to_str().expect("Markdown path"),
        ],
    );
    assert!(
        note.status.success(),
        "{}",
        String::from_utf8_lossy(&note.stderr)
    );
    let note: serde_json::Value = serde_json::from_slice(&note.stdout).expect("note JSON");
    assert_eq!(note["projectId"], project.id);
    assert!(note["document"].to_string().contains("apex-image:"));
    assert_eq!(
        fs::read(&source_image).expect("source PNG unchanged"),
        image_bytes
    );
    let note_id = note["id"].as_str().expect("note ID");

    let exported_markdown = temp.path().join("exported-note.md");
    let export = cli_json(
        &root,
        &[
            "notes",
            "export",
            note_id,
            exported_markdown.to_str().expect("export path"),
        ],
    );
    assert!(
        export.status.success(),
        "{}",
        String::from_utf8_lossy(&export.stderr)
    );
    let note_document = note["document"].to_string();
    let image_hash = note_document
        .split("apex-image:")
        .nth(1)
        .and_then(|rest| rest.split(['"', '\\']).next())
        .expect("embedded image source");
    let exported_image = temp
        .path()
        .join(format!(".apex-note-{note_id}.assets"))
        .join(format!("{image_hash}.png"));
    assert_eq!(
        fs::read(exported_image).expect("exported note image"),
        image_bytes
    );
    let exported_text = fs::read_to_string(&exported_markdown).expect("exported Markdown");
    assert!(exported_text.contains(".apex-note-"));
    let round_trip = cli_json(
        &root,
        &[
            "notes",
            "create",
            "--project",
            &project.id,
            "--title",
            "Round trip",
            "--input",
            exported_markdown.to_str().expect("exported Markdown path"),
        ],
    );
    assert!(
        round_trip.status.success(),
        "{}",
        String::from_utf8_lossy(&round_trip.stderr)
    );
    let round_trip: serde_json::Value =
        serde_json::from_slice(&round_trip.stdout).expect("round-trip note JSON");
    assert!(round_trip["document"].to_string().contains("apex-image:"));

    let source_file = temp.path().join("original.psd");
    fs::write(&source_file, b"dummy arbitrary asset bytes").expect("dummy asset");
    let folder = cli_json(&root, &["assets", "new-folder", &project.id, "Research"]);
    assert!(folder.status.success());
    let folder: serde_json::Value = serde_json::from_slice(&folder.stdout).expect("folder JSON");
    let folder_id = folder["id"].as_str().expect("folder ID");
    let imported = cli_json(
        &root,
        &[
            "assets",
            "import",
            &project.id,
            "--folder",
            folder_id,
            source_file.to_str().expect("source path"),
        ],
    );
    assert!(
        imported.status.success(),
        "{}",
        String::from_utf8_lossy(&imported.stderr)
    );
    let imported: serde_json::Value =
        serde_json::from_slice(&imported.stdout).expect("import JSON");
    assert_eq!(
        imported["imported"]
            .as_array()
            .expect("imported list")
            .len(),
        1
    );
    assert_eq!(imported["failed"].as_array().expect("failed list").len(), 0);
    assert_eq!(
        fs::read(&source_file).expect("source asset unchanged"),
        b"dummy arbitrary asset bytes"
    );
    let asset_id = imported["imported"][0]["id"].as_str().expect("asset ID");
    assert!(cli_json(&root, &["assets", "move", asset_id])
        .status
        .success());
    assert!(
        cli_json(&root, &["assets", "move", asset_id, "--folder", folder_id])
            .status
            .success()
    );

    let exported = temp.path().join("export.psd");
    assert!(cli_json(
        &root,
        &[
            "assets",
            "export",
            asset_id,
            exported.to_str().expect("export path")
        ]
    )
    .status
    .success());
    assert_eq!(
        fs::read(&exported).expect("exported file"),
        b"dummy arbitrary asset bytes"
    );
    let copied = cli_json(
        &root,
        &[
            "assets",
            "copy",
            &project.id,
            asset_id,
            "--folder",
            folder_id,
        ],
    );
    assert!(copied.status.success());
    let trashed = cli_json(&root, &["assets", "trash", asset_id, "--yes"]);
    assert!(trashed.status.success());
    let trashed: serde_json::Value = serde_json::from_slice(&trashed.stdout).expect("trash result");
    let trash_id = trashed["trashId"].as_str().expect("trash ID");
    let restored = cli_json(&root, &["trash", "restore", trash_id]);
    assert!(restored.status.success());
    let listed = cli_json(&root, &["assets", "list", &project.id]);
    assert!(listed.status.success());
    let board: serde_json::Value = serde_json::from_slice(&listed.stdout).expect("asset board");
    assert_eq!(board["assets"].as_array().expect("asset list").len(), 2);

    let purge = cli_json(&root, &["assets", "trash", asset_id, "--yes"]);
    assert!(purge.status.success());
    let purge: serde_json::Value = serde_json::from_slice(&purge.stdout).expect("trash result");
    assert!(cli_json(
        &root,
        &[
            "trash",
            "purge",
            purge["trashId"].as_str().expect("trash ID"),
            "--yes"
        ]
    )
    .status
    .success());
    let listed = cli_json(&root, &["assets", "list", &project.id]);
    let board: serde_json::Value =
        serde_json::from_slice(&listed.stdout).expect("post-purge board");
    assert_eq!(board["assets"].as_array().expect("asset list").len(), 1);

    let archived = cli_json(&root, &["notes", "archive", note_id]);
    assert!(archived.status.success());
    let notes = cli_json(
        &root,
        &["notes", "list", "--project", &project.id, "--archived"],
    );
    assert!(notes.status.success());
    let notes: serde_json::Value = serde_json::from_slice(&notes.stdout).expect("archived notes");
    assert_eq!(notes.as_array().expect("note list").len(), 1);
    assert!(cli_json(&root, &["notes", "delete", note_id, "--yes"])
        .status
        .success());
}

#[test]
fn livery_folder_import_uses_official_vehicle_and_filename_slot_suggestions() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path().join("workspace");
    let workspace = Workspace::open(&root).expect("workspace");
    let project = workspace
        .save_project(
            None,
            ProjectInput {
                name: "Livery import fixture".into(),
                ..Default::default()
            },
        )
        .expect("project");
    drop(workspace);

    let vehicle = apex_core::vehicle_catalogue()
        .expect("official catalogue")
        .vehicles
        .into_iter()
        .find(|vehicle| vehicle.livery_support == "supported")
        .expect("supported vehicle");
    let source_dir = temp.path().join("Patrol Livery");
    fs::create_dir(&source_dir).expect("livery folder");
    let source_png = source_dir.join("Patrol LEFT1.png");
    let image_bytes = include_bytes!("../../erlcx-extension/icon32.png");
    fs::write(&source_png, image_bytes).expect("dummy livery PNG");
    fs::write(source_dir.join("readme.txt"), b"skip this").expect("non-PNG file");

    let imported = cli_json(
        &root,
        &[
            "assets",
            "import-livery",
            &project.id,
            source_dir.to_str().expect("livery folder path"),
            "--vehicle-id",
            &vehicle.id,
            "--cover",
            "Patrol LEFT1.png",
        ],
    );
    assert!(
        imported.status.success(),
        "{}",
        String::from_utf8_lossy(&imported.stderr)
    );
    let imported: serde_json::Value =
        serde_json::from_slice(&imported.stdout).expect("livery JSON");
    assert_eq!(imported["name"], "Patrol Livery");
    assert_eq!(imported["skippedEntries"], 1);
    assert_eq!(imported["assets"][0]["name"], "Patrol LEFT1.png");
    let folder_id = imported["folderId"].as_str().expect("livery folder ID");
    assert_eq!(
        fs::read(&source_png).expect("source preserved"),
        image_bytes
    );

    let board = cli_json(&root, &["assets", "list", &project.id]);
    assert!(board.status.success());
    let board: serde_json::Value = serde_json::from_slice(&board.stdout).expect("asset board");
    let folder = board["folders"]
        .as_array()
        .expect("folders")
        .iter()
        .find(|folder| folder["id"] == folder_id)
        .expect("imported livery folder");
    assert_eq!(folder["kind"], "livery");
    assert_eq!(folder["vehicleId"], vehicle.id);
    assert_eq!(board["placements"][0]["slot"], "Left 1");
    let original_asset = board["assets"]
        .as_array()
        .expect("livery assets")
        .iter()
        .find(|asset| asset["name"] == "Patrol LEFT1.png")
        .expect("texture asset");
    let asset_id = original_asset["id"].as_str().expect("texture asset ID");
    let original_hash = original_asset["hash"].as_str().expect("original hash");

    fs::write(
        &source_png,
        include_bytes!("../../erlcx-extension/icon128.png"),
    )
    .expect("revised dummy PNG");
    let revised = cli_json(
        &root,
        &[
            "assets",
            "import-livery",
            &project.id,
            source_dir.to_str().expect("livery folder path"),
            "--vehicle-id",
            &vehicle.id,
            "--cover",
            "Patrol LEFT1.png",
            "--existing",
            folder_id,
        ],
    );
    assert!(
        revised.status.success(),
        "{}",
        String::from_utf8_lossy(&revised.stderr)
    );
    let versions = cli_json(&root, &["assets", "versions", asset_id]);
    assert!(versions.status.success());
    let versions: serde_json::Value = serde_json::from_slice(&versions.stdout).expect("versions");
    let original_version = versions
        .as_array()
        .expect("version list")
        .iter()
        .find(|version| version["hash"] == original_hash)
        .expect("retained original version");
    assert!(cli_json(
        &root,
        &[
            "assets",
            "restore-version",
            asset_id,
            original_version["id"].as_str().expect("version ID")
        ]
    )
    .status
    .success());
    let restored_board = cli_json(&root, &["assets", "list", &project.id]);
    let restored_board: serde_json::Value =
        serde_json::from_slice(&restored_board.stdout).expect("restored version board");
    assert_eq!(
        restored_board["assets"]
            .as_array()
            .expect("assets")
            .iter()
            .find(|asset| asset["id"] == asset_id)
            .expect("restored asset")["hash"],
        original_hash
    );

    let supported = cli_json(&root, &["vehicles", "list", "--livery-support"]);
    assert!(supported.status.success());
    let vehicles: serde_json::Value =
        serde_json::from_slice(&supported.stdout).expect("vehicles JSON");
    assert!(vehicles
        .as_array()
        .expect("vehicle list")
        .iter()
        .any(|item| item["id"] == vehicle.id));
}

#[test]
fn package_backup_and_workspace_commands_preview_confirm_and_roundtrip_safely() {
    let temp = tempdir().expect("temporary directory");
    let root = temp.path().join("workspace");
    let workspace = Workspace::open(&root).expect("workspace");
    let project = workspace
        .save_project(
            None,
            ProjectInput {
                name: "Packaged project".into(),
                description: "Disposable package fixture".into(),
                ..Default::default()
            },
        )
        .expect("project");
    drop(workspace);

    let source_file = temp.path().join("design.psd");
    fs::write(&source_file, b"package asset bytes").expect("fixture asset");
    let imported = cli_json(
        &root,
        &[
            "assets",
            "import",
            &project.id,
            source_file.to_str().expect("asset path"),
        ],
    );
    assert!(imported.status.success());
    let imported: serde_json::Value = serde_json::from_slice(&imported.stdout).expect("asset");
    let source_hash = imported["imported"][0]["hash"]
        .as_str()
        .expect("asset hash")
        .to_owned();

    let note_source = temp.path().join("package-note.md");
    fs::write(&note_source, "# Package note\n\nPackage round-trip.").expect("note source");
    let note = cli_json(
        &root,
        &[
            "notes",
            "create",
            "--project",
            &project.id,
            "--title",
            "Package note",
            "--input",
            note_source.to_str().expect("note path"),
        ],
    );
    assert!(
        note.status.success(),
        "{}",
        String::from_utf8_lossy(&note.stderr)
    );

    let package = temp.path().join("project.apexstd");
    let exported = cli_json(
        &root,
        &[
            "packages",
            "export",
            &project.id,
            package.to_str().expect("package path"),
        ],
    );
    assert!(exported.status.success());
    let inspected = cli_json(
        &root,
        &[
            "packages",
            "inspect",
            package.to_str().expect("package path"),
        ],
    );
    assert!(inspected.status.success());
    let preview: serde_json::Value = serde_json::from_slice(&inspected.stdout).expect("preview");
    assert_eq!(preview["kind"], "project");
    assert_eq!(preview["assets"], 1);
    assert_eq!(preview["notes"], 1);
    let metadata_only = temp.path().join("metadata-only.apexstd");
    assert!(cli_json(
        &root,
        &[
            "packages",
            "export",
            &project.id,
            metadata_only.to_str().expect("metadata package path"),
            "--metadata-only"
        ]
    )
    .status
    .success());
    let metadata_preview = cli_json(
        &root,
        &[
            "packages",
            "inspect",
            metadata_only.to_str().expect("metadata package path"),
        ],
    );
    let metadata_preview: serde_json::Value =
        serde_json::from_slice(&metadata_preview.stdout).expect("metadata preview");
    assert_eq!(metadata_preview["assets"], 0);
    assert_eq!(metadata_preview["notes"], 0);

    let refused = cli_json(
        &root,
        &[
            "packages",
            "import",
            package.to_str().expect("package path"),
        ],
    );
    assert!(!refused.status.success());
    let error: serde_json::Value = serde_json::from_slice(&refused.stderr).expect("JSON error");
    assert_eq!(error["error"]["code"], "confirmation_required");
    let imported_package = cli_json(
        &root,
        &[
            "packages",
            "import",
            package.to_str().expect("package path"),
            "--yes",
        ],
    );
    assert!(
        imported_package.status.success(),
        "{}",
        String::from_utf8_lossy(&imported_package.stderr)
    );
    let imported_package: serde_json::Value =
        serde_json::from_slice(&imported_package.stdout).expect("imported package");
    let imported_id = imported_package["project"]["id"]
        .as_str()
        .expect("new project ID");
    let imported_board = cli_json(&root, &["assets", "list", imported_id]);
    assert!(imported_board.status.success());
    let imported_board: serde_json::Value =
        serde_json::from_slice(&imported_board.stdout).expect("imported board");
    assert_eq!(imported_board["assets"][0]["hash"], source_hash);
    let imported_notes = cli_json(&root, &["notes", "list", "--project", imported_id]);
    assert!(imported_notes.status.success());
    let imported_notes: serde_json::Value =
        serde_json::from_slice(&imported_notes.stdout).expect("imported notes");
    assert_eq!(imported_notes[0]["title"], "Package note");

    let backup = temp.path().join("workspace.apexbackup");
    let created = cli_json(
        &root,
        &["backups", "create", backup.to_str().expect("backup path")],
    );
    assert!(created.status.success());
    let backup_preview = cli_json(
        &root,
        &["backups", "inspect", backup.to_str().expect("backup path")],
    );
    assert!(backup_preview.status.success());
    let backup_preview: serde_json::Value =
        serde_json::from_slice(&backup_preview.stdout).expect("backup preview");
    assert_eq!(backup_preview["kind"], "workspace");
    assert!(backup_preview["bytes"].as_u64().expect("archive bytes") > 0);
    assert!(!cli_json(
        &root,
        &["packages", "inspect", backup.to_str().expect("backup path")]
    )
    .status
    .success());

    let renamed = cli_json(&root, &["workspace", "rename", "Changed Workspace"]);
    assert!(renamed.status.success());
    let renamed_info = cli_json(&root, &["workspace"]);
    assert!(renamed_info.status.success());
    assert!(String::from_utf8_lossy(&renamed_info.stdout).contains("Changed Workspace"));
    let extra = Workspace::open(&root)
        .expect("workspace after rename")
        .save_project(
            None,
            ProjectInput {
                name: "Created after backup".into(),
                ..Default::default()
            },
        )
        .expect("post-backup project");
    let restore_refused = cli_json(
        &root,
        &["backups", "restore", backup.to_str().expect("backup path")],
    );
    assert!(!restore_refused.status.success());
    let error: serde_json::Value =
        serde_json::from_slice(&restore_refused.stderr).expect("restore refusal");
    assert_eq!(error["error"]["code"], "confirmation_required");

    let restored = cli_json(
        &root,
        &[
            "backups",
            "restore",
            backup.to_str().expect("backup path"),
            "--yes",
        ],
    );
    assert!(
        restored.status.success(),
        "{}",
        String::from_utf8_lossy(&restored.stderr)
    );
    let restored: serde_json::Value = serde_json::from_slice(&restored.stdout).expect("restore");
    assert_eq!(restored["workspace"]["name"], "My workspace");
    let safety_backup = restored["safetyBackup"]
        .as_str()
        .expect("safety backup path");
    assert!(Path::new(safety_backup).is_file());
    let workspace = Workspace::open(&root).expect("reopened restored workspace");
    assert!(workspace.project(&extra.id).is_err());
    let restored_project = workspace.project(&project.id).expect("project restored");
    assert_eq!(restored_project.asset_count, 1);
    let file = root.join("files").join(&source_hash);
    assert_eq!(
        fs::read(file).expect("restored managed bytes"),
        b"package asset bytes"
    );
    assert_eq!(
        workspace
            .notes(Some(&project.id))
            .expect("restored notes")
            .len(),
        1
    );

    let corrupt = temp.path().join("broken.apexstd");
    fs::write(&corrupt, b"not an Apex archive").expect("corrupt fixture");
    assert!(!cli_json(
        &root,
        &[
            "packages",
            "inspect",
            corrupt.to_str().expect("corrupt path")
        ]
    )
    .status
    .success());
    let oversized = temp.path().join("oversized.apexbackup");
    let oversized_file = fs::File::create(&oversized).expect("oversized sparse file");
    oversized_file
        .set_len(2 * 1024 * 1024 * 1024 + 1)
        .expect("extend sparse archive fixture");
    let oversized_result = cli_json(
        &root,
        &[
            "backups",
            "inspect",
            oversized.to_str().expect("large path"),
        ],
    );
    assert!(!oversized_result.status.success());
    let oversized_error: serde_json::Value =
        serde_json::from_slice(&oversized_result.stderr).expect("large archive error");
    assert_eq!(oversized_error["error"]["code"], "archive_too_large");
    let corrupt_backup = temp.path().join("broken.apexbackup");
    fs::write(&corrupt_backup, b"not a workspace backup").expect("corrupt backup fixture");
    assert!(!cli_json(
        &root,
        &[
            "backups",
            "restore",
            corrupt_backup.to_str().expect("corrupt backup path"),
            "--yes"
        ]
    )
    .status
    .success());
    assert_eq!(
        Workspace::open(&root)
            .expect("unchanged workspace")
            .info()
            .expect("info")
            .name,
        "My workspace"
    );

    let existing_backup = temp.path().join("existing.apexbackup");
    fs::write(&existing_backup, b"keep this backup destination").expect("existing backup");
    assert!(!cli_json(
        &root,
        &[
            "backups",
            "create",
            existing_backup.to_str().expect("existing backup path")
        ]
    )
    .status
    .success());
    assert_eq!(
        fs::read(&existing_backup).expect("destination unchanged"),
        b"keep this backup destination"
    );
    let existing = temp.path().join("existing.apexstd");
    fs::write(&existing, b"keep this destination").expect("existing destination");
    assert!(!cli_json(
        &root,
        &[
            "packages",
            "export",
            &project.id,
            existing.to_str().expect("existing path")
        ]
    )
    .status
    .success());
    assert_eq!(
        fs::read(existing).expect("destination remains unchanged"),
        b"keep this destination"
    );
}

fn cli_json(root: &Path, args: &[&str]) -> std::process::Output {
    let root = root.to_str().expect("workspace path");
    let mut command = Command::new(env!("CARGO_BIN_EXE_apex-studio-cli"));
    command.args(["--workspace", root]);
    command.args(args);
    command.arg("--json");
    command.output().expect("run CLI command")
}
