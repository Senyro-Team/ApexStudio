#![allow(clippy::expect_used)]

use apex_application::{
    team_sync::{
        project::{RemoteProject, SyncEntry, SyncPreview, SyncReport},
        ConnectionStatus, Invitation, ServerInfo, Team, TeamMember, User,
    },
    ApplicationServices, RobloxGroupRevenueData, RobloxGroupSale, RobloxRevenueGroup,
    StatisticsComparison, StatisticsComparisonDay, StatisticsPost,
};
use apex_core::ProjectInput;
use apex_storage::{ProjectSync, RobloxRevenuePass};
use serde::Serialize;
use serde_json::Value;
use std::{fs, path::PathBuf};
use tempfile::tempdir;

fn schema_root(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/cli/schemas/v1")
        .join(name)
}

fn read_schema(name: &str) -> Value {
    serde_json::from_slice(&fs::read(schema_root(name)).expect("read JSON schema"))
        .expect("parse JSON schema")
}

fn schema_definition(file: &str, definition: &str) -> Value {
    let schema = read_schema(file);
    let mut extracted = schema["$defs"][definition].clone();
    extracted["$schema"] = schema["$schema"].clone();
    extracted["$defs"] = schema["$defs"].clone();
    extracted
}

fn assert_valid(schema: &Value, instance: &impl Serialize) {
    let value = serde_json::to_value(instance).expect("serialize fixture");
    let validator = jsonschema::validator_for(schema).expect("compile schema");
    let errors = validator
        .iter_errors(&value)
        .map(|error| format!("{} at {}", error, error.instance_path()))
        .collect::<Vec<_>>();
    assert!(
        errors.is_empty(),
        "schema validation failed: {}",
        errors.join("; ")
    );
}

fn assert_schema_definition_valid(file: &str, definition: &str, instance: &impl Serialize) {
    assert_valid(&schema_definition(file, definition), instance);
}

#[test]
fn all_contract_files_are_valid_draft_2020_12_schemas() {
    let directory = schema_root("");
    for entry in fs::read_dir(directory).expect("read schema directory") {
        let path = entry.expect("read schema entry").path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let schema: Value =
            serde_json::from_slice(&fs::read(&path).expect("read schema")).expect("parse schema");
        jsonschema::meta::validate(&schema)
            .unwrap_or_else(|error| panic!("invalid JSON Schema {}: {error}", path.display()));
        jsonschema::validator_for(&schema)
            .unwrap_or_else(|error| panic!("schema does not compile {}: {error}", path.display()));
    }
}

#[test]
fn core_cli_schemas_accept_real_disposable_workspace_outputs() {
    let temp = tempdir().expect("temporary workspace root");
    let app = ApplicationServices::open(temp.path()).expect("open disposable workspace");
    let project = app
        .save_project(
            None,
            ProjectInput {
                name: "JSON Contract Fixture".into(),
                description: "Temporary schema fixture".into(),
                ..Default::default()
            },
        )
        .expect("create fixture project");

    assert_valid(
        &read_schema("workspace.schema.json"),
        &app.workspace_info().expect("workspace info"),
    );
    assert_valid(&read_schema("project.schema.json"), &project);
    assert_valid(
        &read_schema("project.schema.json"),
        &app.project(&project.id).expect("read project"),
    );
    assert_valid(
        &read_schema("asset-board.schema.json"),
        &app.asset_board(&project.id).expect("asset board"),
    );

    let note = app
        .create_note(Some(&project.id))
        .expect("create project note");
    assert_valid(&read_schema("note.schema.json"), &note);

    let post_id = app.create_post().expect("create publishing draft");
    let draft = app.post(&post_id).expect("read publishing draft");
    assert_valid(&read_schema("publishing-draft.schema.json"), &draft);
    assert_valid(
        &read_schema("post-summary.schema.json"),
        &app.posts().expect("post list")[0],
    );
    assert_valid(
        &read_schema("roblox-queue.schema.json"),
        &app.upload_queue().expect("empty queue"),
    );

    app.set_statistics_profile("123456")
        .expect("set statistics profile");
    app.workspace()
        .save_statistics_snapshot(
            "123456",
            &[StatisticsPost {
                bundle_id: 55,
                title: "Fixture".into(),
                views: Some(20),
                likes: Some(3),
                copies: None,
                favourites: Some(1),
                rating_count: Some(2),
                rating_average: Some(4.5),
            }],
        )
        .expect("save statistics snapshot");
    assert_valid(
        &read_schema("statistics-snapshot.schema.json"),
        &app.statistics_history().expect("statistics history")[0],
    );

    let folder = app
        .save_folder(&project.id, None, None, "Trash fixture")
        .expect("create trash fixture folder");
    app.trash_item(&folder.id, true)
        .expect("trash fixture folder");
    assert_valid(
        &read_schema("trash.schema.json"),
        &app.trash_items().expect("trash list"),
    );
}

#[test]
fn sync_revenue_and_comparison_schemas_accept_typed_results() {
    let server = ServerInfo {
        product: "Apex Sync".into(),
        api_version: "v1".into(),
        server_version: "1.0.0".into(),
        server_instance_id: "instance".into(),
        origin: "https://sync.example.test".into(),
    };
    let team = Team {
        id: "team".into(),
        name: "Designers".into(),
        role: "owner".into(),
    };
    let user = User {
        id: "user".into(),
        email: "designer@example.test".into(),
        display_name: "Designer".into(),
        teams: vec![team.clone()],
    };
    assert_schema_definition_valid(
        "team-sync.schema.json",
        "connectionStatus",
        &ConnectionStatus {
            server: server.clone(),
            user: Some(user),
            session_expired: false,
            saved_session_for_different_server: false,
        },
    );
    assert_schema_definition_valid("team-sync.schema.json", "serverInfo", &server);
    assert_schema_definition_valid(
        "team-sync.schema.json",
        "invitation",
        &Invitation {
            invitation_token: "DEMO-INVITE-CODE".into(),
            role: "designer".into(),
            expires_in_seconds: 3600,
        },
    );
    assert_schema_definition_valid(
        "team-sync.schema.json",
        "teamMember",
        &TeamMember {
            id: "member".into(),
            email: "member@example.test".into(),
            display_name: "Member".into(),
            role: "designer".into(),
        },
    );
    let linked = ProjectSync {
        project_id: "project".into(),
        origin: server.origin.clone(),
        server_instance: server.server_instance_id.clone(),
        team_id: team.id.clone(),
        enabled: true,
        received_from_team: false,
        local_hash: None,
        revision: 2,
        cursor: 3,
        message: "Up to date".into(),
        synced_at: Some("2026-01-01T00:00:00Z".into()),
    };
    assert_schema_definition_valid("team-sync.schema.json", "projectSync", &linked);
    assert_schema_definition_valid(
        "team-sync.schema.json",
        "syncPreview",
        &SyncPreview {
            team: team.clone(),
            remote_projects: vec![RemoteProject {
                id: "remote".into(),
                name: "Remote".into(),
            }],
            linked_projects: vec![linked],
        },
    );
    assert_schema_definition_valid(
        "team-sync.schema.json",
        "syncReport",
        &SyncReport {
            uploaded: 1,
            downloaded: 0,
            checked: 2,
            up_to_date: 1,
            shared_local: 0,
            remote_projects: 1,
            errors: vec![],
            entries: vec![SyncEntry {
                project_id: "project".into(),
                project_name: "Fixture".into(),
                status: "uploaded",
                message: "Uploaded".into(),
            }],
        },
    );

    let revenue = RobloxGroupRevenueData {
        groups: vec![RobloxRevenueGroup {
            group_id: "100".into(),
            name: "Fixture Group".into(),
            experience_ids: vec!["200".into()],
            pass_mappings: Default::default(),
            selected_pass_ids: vec!["300".into()],
            api_updated_at: None,
            api_coverage_from: None,
            api_coverage_to: None,
            api_summary: None,
            api_status: Some("success".into()),
            updated_at: None,
            last_import_at: None,
        }],
        sales: vec![RobloxGroupSale {
            group_id: "100".into(),
            sale_fingerprint: "a".repeat(64),
            universe_id: "200".into(),
            asset_id: "300".into(),
            sale_at: "2026-01-01T00:00:00Z".into(),
            sale_location: "Game".into(),
            hold_status: "Released".into(),
            price: 10,
            revenue: 7,
            import_batch_id: "batch".into(),
            external_transaction_id: None,
            source: "csv".into(),
            dedupe_confidence: "exact_fields".into(),
            imported_at: None,
        }],
        passes: vec![RobloxRevenuePass {
            group_id: "100".into(),
            asset_id: "300".into(),
            universe_id: "200".into(),
            name: "Pass".into(),
            source: "api".into(),
        }],
    };
    assert_schema_definition_valid("group-revenue.schema.json", "data", &revenue);

    assert_valid(
        &read_schema("statistics-comparison.schema.json"),
        &StatisticsComparison {
            start_date: "2026-01-01".into(),
            end_date: "2026-01-07".into(),
            range_days: Some(7),
            erlcx_sales: 1,
            erlcx_net_robux: 7,
            erlcx_gross_robux_snapshot: 10,
            roblox_selected_groups: 1,
            roblox_selected_passes: 1,
            roblox_sale_rows: 1,
            roblox_price: 10,
            roblox_revenue: 7,
            roblox_coverage: "observed_rows".into(),
            daily: vec![StatisticsComparisonDay {
                date: "2026-01-01".into(),
                erlcx_sales: 1,
                erlcx_net_robux: 7,
                roblox_price: Some(10),
                roblox_revenue: Some(7),
            }],
        },
    );
}

#[test]
fn malformed_cli_values_do_not_pass_strict_contract_schemas() {
    let schema = read_schema("project.schema.json");
    let validator = jsonschema::validator_for(&schema).expect("compile Project schema");
    let invalid =
        serde_json::json!({"id":"p1","name":"Missing required properties","unknown":true});
    assert!(!validator.is_valid(&invalid));
}

#[test]
fn ad_hoc_cli_operation_reports_match_the_documented_schemas() {
    let archive = serde_json::json!({
        "title": "Fixture archive",
        "assets": 2,
        "notes": 1,
        "bytes": 4096,
        "kind": "project"
    });
    let workspace = serde_json::json!({
        "name": "Fixture workspace",
        "root": "C:/temp/workspace",
        "schemaVersion": 18,
        "managedFileCount": 2
    });
    let asset = serde_json::json!({
        "id": "asset-1",
        "projectId": "project-1",
        "name": "Left 1.png",
        "hash": "a".repeat(64),
        "size": 128,
        "importedAt": "2026-01-01T00:00:00Z",
        "preview": null
    });
    let schema = read_schema("cli-reports.schema.json");
    for (definition, value) in [
        (
            "action",
            serde_json::json!({"ok":true,"message":"Completed."}),
        ),
        ("packagePreview", archive.clone()),
        (
            "packageImport",
            serde_json::json!({"project":{"id":"project-1"},"archive":archive.clone()}),
        ),
        (
            "workspaceRestore",
            serde_json::json!({"workspace":workspace,"safetyBackup":"C:/temp/pre-restore.apexbackup","archive":archive}),
        ),
        (
            "importReport",
            serde_json::json!({"imported":[asset.clone()],"skipped":[],"failed":[]}),
        ),
        (
            "postCreate",
            serde_json::json!({"id":"post-1","draft":{"data":{},"revision":1}}),
        ),
        (
            "postHandoff",
            serde_json::json!({"ok":true,"postId":"post-1"}),
        ),
        ("statisticsProfile", serde_json::json!({"profileId":null})),
        (
            "doubleUploadSetting",
            serde_json::json!({"doubleUpload":true}),
        ),
        (
            "postSource",
            serde_json::json!({"folderId":"folder-1","vehicle":"Falcon","state":"current"}),
        ),
        (
            "publishingPreview",
            serde_json::json!({"hash":"b".repeat(64),"name":"Preview.png","size":1024,"preview":null}),
        ),
        (
            "codesExport",
            serde_json::json!({"path":"C:/temp/codes.md","posts":2,"liveries":3,"codes":9,"missing":1}),
        ),
        (
            "robloxLivery",
            serde_json::json!({"id":"folder-1","projectId":"project-1","projectName":"Fixture","name":"Livery","vehicleId":null,"textures":2,"uploaded":1}),
        ),
        (
            "robloxCredentials",
            serde_json::json!({"configured":true,"accountId":"12345"}),
        ),
        (
            "credentialsSet",
            serde_json::json!({"configured":true,"accountId":"12345"}),
        ),
        (
            "robloxBatchCreated",
            serde_json::json!({"batchId":"batch-1","accountId":"12345","liveries":1,"textures":2,"reusedIds":1,"uploadPasses":2,"doubleUpload":true}),
        ),
        (
            "liveryImport",
            serde_json::json!({"folderId":"folder-1","name":"Livery","vehicleId":"vehicle-1","skippedEntries":1,"assets":[asset]}),
        ),
        (
            "trashCreated",
            serde_json::json!({"ok":true,"trashId":"trash-1","preview":{"name":"Livery","files":2,"folders":1}}),
        ),
        (
            "statisticsSummary",
            serde_json::json!({
                "profileId":"12345","snapshotCount":1,"collectedAt":"2026-01-01T00:00:00Z","freshness":"fresh",
                "previousCollectedAt":null,"creatorSalesCollectedAt":null,"creatorSalesFreshness":"never","postCount":1,
                "views":{"total":20,"knownPosts":1},"likes":{"total":4,"knownPosts":1},"copies":{"total":null,"knownPosts":0},
                "favourites":{"total":2,"knownPosts":1},"ratingCount":{"total":1,"knownPosts":1},"ratingAverage":4.5,
                "changes":{"views":null,"likes":null,"copies":null,"favourites":null},
                "topPosts":[{"bundleId":12,"title":"Fixture","views":20,"likes":4,"copies":null,"favourites":2,"ratingCount":1,"ratingAverage":4.5}]
            }),
        ),
        (
            "revenueGroupSummary",
            serde_json::json!([{"group":{"groupId":"100","name":"Fixture"},"sales":5}]),
        ),
        ("revenueRemove", serde_json::json!({"removedGroupId":"100"})),
        (
            "revenueCsvPreview",
            serde_json::json!({
                "groupId":"100","files":["sales.csv"],"rows":3,"skippedRows":1,
                "potentialDuplicates":1,"fromUtc":"2026-01-01T00:00:00Z",
                "toUtc":"2026-01-02T00:00:00Z","warnings":[]
            }),
        ),
        (
            "revenueCsvImport",
            serde_json::json!({
                "preview": {
                    "groupId":"100","files":["sales.csv"],"rows":3,"skippedRows":1,
                    "potentialDuplicates":1,"fromUtc":"2026-01-01T00:00:00Z",
                    "toUtc":"2026-01-02T00:00:00Z","warnings":[]
                },
                "import":{"inserted":2,"duplicates":1}
            }),
        ),
        (
            "revenueGroupDetails",
            serde_json::json!({"group":{"groupId":"100"},"sales":[],"passes":[]}),
        ),
        (
            "syncResolution",
            serde_json::json!({"projectId":"project-1","resolution":"server","previousRevision":3}),
        ),
        (
            "syncRun",
            serde_json::json!({
                "preview":{"team":{"id":"team-1"},"remoteProjects":[],"linkedProjects":[]},
                "report":{"uploaded":1,"downloaded":0,"checked":1,"upToDate":0,"sharedLocal":0,"remoteProjects":0,"errors":[],"entries":[]}
            }),
        ),
        (
            "liveryUploads",
            serde_json::json!({
                "folderId":"folder-1","name":"Livery","vehicle":"Falcon","accountId":"12345",
                "textures":[{"assetId":"asset-1","name":"Left.png","hash":"c".repeat(64),"side":"Left 1","upload":null}]
            }),
        ),
    ] {
        assert_valid(
            &schema_definition("cli-reports.schema.json", definition),
            &value,
        );
    }
    let validator = jsonschema::validator_for(&schema).expect("compile CLI report definitions");
    assert!(validator.is_valid(&serde_json::json!({"ok":true})));

    assert_schema_definition_valid(
        "asset-board.schema.json",
        "version",
        &serde_json::json!({
            "id":"version-1","assetId":"asset-1","hash":"d".repeat(64),
            "name":"Left 1.png","createdAt":"2026-01-01T00:00:00Z"
        }),
    );
    assert_valid(
        &read_schema("vehicle-catalogue.schema.json"),
        &serde_json::json!([{
            "id":"vehicle-1","team":"Police","name":"Falcon","source":"official",
            "liverySupport":"yes","bodyStatus":"available","windowStatus":null,
            "notes":[],"slots":[{"key":"Left 1","status":"available","kind":"body"}]
        }]),
    );
    assert_valid(
        &read_schema("statistics-sales.schema.json"),
        &serde_json::json!({
            "collectedAt":"2026-01-01T00:00:00Z",
            "data":{"sales":1,"allTimeSales":10,"netRobux":7,"grossRobux":10,"series":[{"date":"2026-01-01","sales":1,"netRobux":7}]}
        }),
    );
    assert_valid(
        &read_schema("statistics-sales.schema.json"),
        &serde_json::Value::Null,
    );
}
