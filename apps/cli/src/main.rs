use apex_application::{
    ApplicationServices, PostSummary, QueueSnapshot, RobloxGroupRevenueData, RobloxLiverySummary,
    RobloxRevenueGroup, StatisticsPost, StatisticsSnapshot, TrashItem, UploadJob,
};
use apex_core::{
    AssetBoard, Error, Note, Project, ProjectInput, PublishingDraft, PublishingImage,
    PublishingInput, PublishingLivery, PublishingSide, PublishingSource, Vehicle,
};
use base64::Engine as _;
use clap::{Parser, Subcommand};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    fmt, fs,
    io::{self, IsTerminal, Read, Write},
    path::PathBuf,
    process::ExitCode,
};

const APP_IDENTIFIER: &str = "studio.apexdesigns.apexstudio";

#[cfg(windows)]
fn chrome_pipe_name() -> Result<String, Error> {
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| Error::InvalidContent("Windows local app data is unavailable.".into()))?;
    let config_path = PathBuf::from(local_app_data)
        .join("ApexStudio")
        .join("chrome")
        .join("settings.json");
    let bytes = fs::read(&config_path).map_err(|_| {
        Error::InvalidContent(
            "Open Apex Studio and configure the Chrome connection in Settings first.".into(),
        )
    })?;
    let config: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| Error::InvalidContent("Chrome bridge settings are invalid.".into()))?;
    let prefix = r"\\.\pipe\ApexStudio-";
    let pipe = config["pipe"]
        .as_str()
        .filter(|pipe| {
            pipe.starts_with(prefix)
                && pipe.len() <= 160
                && pipe[prefix.len()..]
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
        .ok_or_else(|| {
            Error::InvalidContent(
                "The desktop Chrome bridge is not running. Open Apex Studio and try again.".into(),
            )
        })?;
    Ok(pipe.to_owned())
}

#[cfg(windows)]
fn request_chrome_handoff(id: &str, revision: u64, reviewed: bool) -> Result<(), Error> {
    let pipe_name = chrome_pipe_name()?;
    request_chrome_handoff_on_pipe(&pipe_name, id, revision, reviewed)
}

#[cfg(windows)]
fn request_chrome_handoff_on_pipe(
    pipe_name: &str,
    id: &str,
    revision: u64,
    reviewed: bool,
) -> Result<(), Error> {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::windows::named_pipe::ClientOptions,
    };

    let request = serde_json::json!({
        "origin": "apex-cli",
        "kind": "prepare_post",
        "id": id,
        "revision": revision,
        "reviewed": reviewed,
    });
    let payload =
        serde_json::to_vec(&request).map_err(|error| Error::Storage(error.to_string()))?;
    if payload.is_empty() || payload.len() > 16 * 1024 {
        return Err(Error::InvalidContent(
            "Chrome handoff request is too large.".into(),
        ));
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| Error::Storage(error.to_string()))?;
    let response = runtime.block_on(async {
        let mut pipe = ClientOptions::new().open(pipe_name).map_err(|error| {
            Error::Storage(format!(
                "Cannot reach the Apex Studio browser bridge: {error}"
            ))
        })?;
        tokio::time::timeout(std::time::Duration::from_secs(60), async {
            pipe.write_u32_le(payload.len() as u32).await?;
            pipe.write_all(&payload).await?;
            pipe.flush().await?;
            let length = pipe.read_u32_le().await? as usize;
            if length == 0 || length > 16 * 1024 {
                return Err(std::io::Error::other("Invalid browser bridge response."));
            }
            let mut bytes = vec![0; length];
            pipe.read_exact(&mut bytes).await?;
            Ok::<_, std::io::Error>(bytes)
        })
        .await
        .map_err(|_| Error::InvalidContent("The Apex Studio browser bridge timed out.".into()))?
        .map_err(|error| Error::Storage(error.to_string()))
    })?;
    let response: serde_json::Value =
        serde_json::from_slice(&response).map_err(|error| Error::Storage(error.to_string()))?;
    if let Some(message) = response["error"].as_str() {
        return Err(Error::InvalidContent(message.to_owned()));
    }
    if response["ok"] != true || response["postId"].as_str() != Some(id) {
        return Err(Error::InvalidContent(
            "The desktop rejected the ERLCX handoff.".into(),
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn request_chrome_handoff(_id: &str, _revision: u64, _reviewed: bool) -> Result<(), Error> {
    Err(Error::InvalidContent(
        "Direct Chrome handoff is currently supported on Windows only.".into(),
    ))
}

#[derive(Debug, Parser)]
#[command(
    name = "apex-studio-cli",
    version,
    about = "Apex Studio terminal workspace"
)]
struct Args {
    /// Use a specific Apex Studio workspace directory.
    #[arg(long, global = true)]
    workspace: Option<PathBuf>,

    /// Print command results as JSON.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Browse and inspect projects.
    Projects {
        #[command(subcommand)]
        action: ProjectCommand,
    },
    /// Manage project and general notes.
    Notes {
        #[command(subcommand)]
        action: NoteCommand,
    },
    /// Browse and manage project assets.
    Assets {
        #[command(subcommand)]
        action: AssetCommand,
    },
    /// Recover or permanently remove trashed project assets.
    Trash {
        #[command(subcommand)]
        action: TrashCommand,
    },
    /// Search the official ER:LC vehicle catalogue.
    Vehicles {
        #[command(subcommand)]
        action: VehicleCommand,
    },
    /// Inspect, export, and import project packages.
    Packages {
        #[command(subcommand)]
        action: PackageCommand,
    },
    /// Create, inspect, and restore whole-workspace backups.
    Backups {
        #[command(subcommand)]
        action: BackupCommand,
    },
    /// Manage global ERLCX post drafts and decal-code exports.
    Posts {
        #[command(subcommand)]
        action: PostCommand,
    },
    /// Export Roblox decal IDs from saved post snapshots as Markdown.
    Codes {
        #[command(subcommand)]
        action: CodeCommand,
    },
    /// Inspect Roblox upload candidates and the durable upload queue.
    Roblox {
        #[command(subcommand)]
        action: RobloxCommand,
    },
    /// Inspect ERLCX profile metrics and locally collected history.
    Statistics {
        #[command(subcommand)]
        action: StatisticsCommand,
    },
    /// Inspect locally stored Roblox Group Revenue data.
    Revenue {
        #[command(subcommand)]
        action: RevenueCommand,
    },
    /// Show the workspace name and storage summary.
    Workspace {
        #[command(subcommand)]
        action: Option<WorkspaceCommand>,
    },
}

#[derive(Debug, Subcommand)]
enum PostCommand {
    /// List global ERLCX post drafts.
    List,
    /// Create an empty global ERLCX post draft.
    Create,
    /// Import a validated .apexpublish bundle as a new global draft.
    Import { source: PathBuf },
    /// Open a saved draft in the configured Chrome ERLCX assistant.
    Handoff {
        id: String,
        /// Confirm use of saved codes when linked livery sources changed or are unavailable.
        #[arg(long)]
        reviewed: bool,
    },
    /// Show a post draft and its saved revision.
    Show { id: String },
    /// Save a full post JSON snapshot (the format returned by `posts show`).
    Save { id: String, input: PathBuf },
    /// Show whether linked livery sources still match the saved codes.
    Sources { id: String },
    /// Import and pin preview PNGs for this draft; add their returned hashes to the JSON before saving.
    AddImages {
        id: String,
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
    /// Export a validated .apexpublish bundle without overwriting an existing file.
    Export {
        id: String,
        destination: PathBuf,
        /// Confirm use of saved codes when source liveries changed or are unavailable.
        #[arg(long)]
        reviewed: bool,
    },
}

#[derive(Debug, Subcommand)]
enum CodeCommand {
    /// Combine saved post codes into one Markdown file.
    Export {
        destination: PathBuf,
        /// Include a global ERLCX post draft (repeatable).
        #[arg(long = "post")]
        posts: Vec<String>,
        /// Confirm use of saved code snapshots when linked liveries changed or are unavailable.
        #[arg(long)]
        reviewed: bool,
    },
}

#[derive(Debug, Subcommand)]
enum RobloxCommand {
    /// List livery folders and upload coverage for an account ID.
    Liveries {
        #[arg(long)]
        account_id: String,
    },
    /// Show durable batches, upload jobs, and livery assignments.
    Queue,
    /// Run confirmed, already-created batches without requiring Apex Studio to stay open.
    Run {
        /// Confirm network uploads in non-interactive use.
        #[arg(long)]
        yes: bool,
    },
    /// Create a reviewed durable batch for the CLI or desktop upload worker.
    Batch {
        #[command(subcommand)]
        action: UploadBatchCommand,
    },
    /// Show or change whether newly created Roblox batches use two upload passes.
    DoubleUpload {
        #[arg(long, conflicts_with = "disable")]
        enable: bool,
        #[arg(long)]
        disable: bool,
    },
    /// Read or manage personal Roblox credentials in Windows Credential Manager.
    Credentials {
        #[command(subcommand)]
        action: RobloxCredentialsCommand,
    },
}

#[derive(Debug, Subcommand)]
enum StatisticsCommand {
    /// Show or set the Roblox profile whose public ERLCX posts are tracked.
    Profile {
        #[arg(long)]
        set: Option<String>,
    },
    /// Show totals from the most recent public ERLCX profile snapshot.
    Summary,
    /// Show locally saved public-profile snapshots.
    History,
    /// Fetch the public profile bundle list without opening individual posts.
    Refresh,
    /// Show the latest authenticated creator-sales snapshot, if available.
    Sales,
}

#[derive(Debug, Subcommand)]
enum RevenueCommand {
    /// List configured Roblox groups and their imported sale counts.
    Groups,
    /// Show one group's settings, imported sales and discovered passes.
    Show { group_id: String },
    /// Add or update a group and its experience IDs.
    Configure {
        #[arg(long)]
        group_id: String,
        #[arg(long)]
        name: String,
        /// Repeat this option for each experience ID belonging to the group.
        #[arg(long = "experience-id", required = true)]
        experience_ids: Vec<String>,
    },
    /// Include a discovered Game Pass in group revenue totals.
    SelectPass {
        #[arg(long)]
        group_id: String,
        #[arg(long)]
        asset_id: String,
    },
    /// Exclude a Game Pass from group revenue totals.
    DeselectPass {
        #[arg(long)]
        group_id: String,
        #[arg(long)]
        asset_id: String,
    },
    /// Remove a group's stored settings and sales.
    Remove {
        #[arg(long)]
        group_id: String,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Subcommand)]
enum RobloxCredentialsCommand {
    /// Show whether credentials are configured and which account is selected.
    Status,
    /// Validate and replace credentials; the API key is prompted without echo.
    Set {
        #[arg(long)]
        account_id: String,
    },
    /// Remove credentials from Windows Credential Manager.
    Clear {
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Subcommand)]
enum UploadBatchCommand {
    /// Queue one or more livery folders for a Roblox account.
    Start {
        #[arg(long)]
        account_id: String,
        #[arg(long = "livery", required = true)]
        liveries: Vec<String>,
        /// Upload again even when a usable account-specific ID already exists.
        #[arg(long)]
        reupload: bool,
        /// Confirm network work without an interactive prompt.
        #[arg(long)]
        yes: bool,
    },
    /// Pause the selected batch or all batches.
    Pause { id: Option<String> },
    /// Resume the selected batch or all batches.
    Resume { id: Option<String> },
    /// Cancel pending work in the selected batch or all batches.
    Cancel {
        id: Option<String>,
        #[arg(long)]
        yes: bool,
    },
    /// Retry failed passes in the selected batch or all batches.
    Retry { id: Option<String> },
}

#[derive(Debug, Subcommand)]
enum PackageCommand {
    /// Preview an .apexstd archive without importing it.
    Inspect { path: PathBuf },
    /// Export a project without overwriting an existing destination.
    Export {
        project: String,
        destination: PathBuf,
        /// Export project metadata and folder structure without file contents.
        #[arg(long)]
        metadata_only: bool,
        /// Omit notes when embedding assets.
        #[arg(long)]
        exclude_notes: bool,
    },
    /// Import a package as a new project after reviewing its contents.
    Import {
        path: PathBuf,
        /// Confirm the import without an interactive prompt.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Subcommand)]
enum BackupCommand {
    /// Create a complete workspace backup without overwriting an existing file.
    Create { destination: PathBuf },
    /// Preview a .apexbackup archive without restoring it.
    Inspect { path: PathBuf },
    /// Replace this workspace from a backup after reviewing the target summary.
    Restore {
        path: PathBuf,
        /// Confirm the full workspace replacement without an interactive prompt.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Subcommand)]
enum WorkspaceCommand {
    /// Rename the current workspace.
    Rename { name: String },
}

#[derive(Debug, Subcommand)]
enum ProjectCommand {
    /// List projects, optionally filtering by a case-insensitive substring.
    List {
        #[arg(long, short)]
        search: Option<String>,
    },
    /// Show a project's details by its ID.
    Show { id: String },
    /// Create a project using a JSON ProjectInput file (use - for stdin).
    Create { input: PathBuf },
    /// Update a project's fields from a JSON ProjectInput file (use - for stdin).
    Update { id: String, input: PathBuf },
    /// Archive a project.
    Archive { id: String },
    /// Restore an archived project.
    Restore { id: String },
    /// Permanently delete an archived project.
    Delete {
        id: String,
        /// Confirm the permanent deletion without an interactive prompt.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Subcommand)]
enum NoteCommand {
    /// List notes. Omit --project to list general notes.
    List {
        #[arg(long, conflicts_with = "general")]
        project: Option<String>,
        #[arg(long)]
        general: bool,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        archived: bool,
    },
    /// Show a note's Markdown and metadata.
    Show { id: String },
    /// Export Markdown and its embedded images without overwriting existing files.
    Export { id: String, output: PathBuf },
    /// Create a project note or, with --general, a general note.
    Create {
        #[arg(long, conflicts_with = "general")]
        project: Option<String>,
        #[arg(long)]
        general: bool,
        #[arg(long)]
        title: Option<String>,
        /// Optional Markdown source file. Relative images resolve beside this file.
        #[arg(long)]
        input: Option<PathBuf>,
    },
    /// Edit a note from a Markdown file with revision checking.
    Edit {
        id: String,
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        title: Option<String>,
    },
    /// Archive a note.
    Archive { id: String },
    /// Restore an archived note.
    Restore { id: String },
    /// Permanently delete an archived note.
    Delete {
        id: String,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Subcommand)]
enum AssetCommand {
    /// List folders, liveries, files, and assigned slots in a project.
    List { project: String },
    /// Import arbitrary files into a project or folder; originals are preserved.
    Import {
        project: String,
        #[arg(long)]
        folder: Option<String>,
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
    /// Import one vehicle-specific livery from a folder of PNG textures.
    ImportLivery {
        project: String,
        path: PathBuf,
        #[arg(long)]
        vehicle_id: String,
        #[arg(long)]
        name: Option<String>,
        /// PNG filename to use as the livery cover.
        #[arg(long)]
        cover: Option<String>,
        /// Override a filename suggestion with FILE=SLOT (repeatable).
        #[arg(long = "assign")]
        assignments: Vec<String>,
        /// Leave a suggested slot unassigned for this PNG filename (repeatable).
        #[arg(long = "unassigned")]
        unassigned: Vec<String>,
        #[arg(long)]
        parent: Option<String>,
        /// Replace matching slots in an existing livery and retain prior versions.
        #[arg(long)]
        existing: Option<String>,
    },
    /// Create a regular folder.
    NewFolder {
        project: String,
        name: String,
        #[arg(long)]
        parent: Option<String>,
    },
    /// Rename or move a folder. Omit --parent to move it to the project root.
    EditFolder {
        project: String,
        id: String,
        name: String,
        #[arg(long)]
        parent: Option<String>,
    },
    /// Move an asset to a folder or project root.
    Move {
        id: String,
        #[arg(long)]
        folder: Option<String>,
    },
    /// Copy a file or folder tree within its project.
    Copy {
        project: String,
        id: String,
        #[arg(long)]
        folder_item: bool,
        #[arg(long)]
        folder: Option<String>,
    },
    /// Assign a livery side slot or clear the assignment.
    Slot {
        id: String,
        #[arg(long, required_unless_present = "clear", conflicts_with = "clear")]
        slot: Option<String>,
        #[arg(long)]
        clear: bool,
    },
    /// List recoverable versions of an asset.
    Versions { id: String },
    /// Restore a previous asset version.
    RestoreVersion { id: String, version: String },
    /// Export a managed file without overwriting an existing file.
    Export { id: String, output: PathBuf },
    /// Move an asset or folder tree into project trash.
    Trash {
        id: String,
        #[arg(long)]
        folder: bool,
        #[arg(long)]
        yes: bool,
    },
    /// Set a livery or folder cover from a PNG file.
    SetCover { id: String, input: PathBuf },
}

#[derive(Debug, Subcommand)]
enum TrashCommand {
    /// List trashed files and folders.
    List {
        #[arg(long)]
        project: Option<String>,
    },
    /// Restore a trashed item to its original location where possible.
    Restore { id: String },
    /// Permanently delete a trashed item and unreferenced content.
    Purge {
        id: String,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Subcommand)]
enum VehicleCommand {
    /// List official vehicles and their livery support.
    List {
        #[arg(long)]
        search: Option<String>,
        /// Show only catalogue entries that allow custom liveries.
        #[arg(long)]
        livery_support: bool,
    },
}

fn main() -> ExitCode {
    let args = Args::parse();
    let json = args.json;
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if json {
                eprintln!(
                    "{}",
                    serde_json::json!({
                        "ok": false,
                        "error": { "code": error.code, "message": error.message }
                    })
                );
            } else {
                eprintln!("Error: {}", error.message);
            }
            ExitCode::FAILURE
        }
    }
}

#[derive(Debug)]
struct CliError {
    code: String,
    message: String,
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl From<Error> for CliError {
    fn from(error: Error) -> Self {
        Self {
            code: error.code().into(),
            message: error.to_string(),
        }
    }
}

fn run(args: Args) -> Result<(), CliError> {
    let json = args.json;
    let root = args
        .workspace
        .or_else(default_workspace_path)
        .ok_or_else(|| {
            CliError {
                code: "workspace_path_unavailable".into(),
                message: "Could not find this user's local application data directory. Use --workspace <PATH>.".into(),
            }
        })?;
    let app = ApplicationServices::open(&root).map_err(CliError::from)?;

    match args.command {
        Some(Command::Projects { action }) => match action {
            ProjectCommand::List { search } => {
                let mut projects = app.projects().map_err(CliError::from)?;
                if let Some(search) = search.filter(|query| !query.trim().is_empty()) {
                    let query = search.trim().to_lowercase();
                    projects.retain(|project| project_matches(project, &query));
                }
                print_projects(&projects, args.json);
            }
            ProjectCommand::Show { id } => {
                let project = app.project(&id).map_err(CliError::from)?;
                print_project(&project, args.json);
            }
            ProjectCommand::Create { input } => {
                let fields: ProjectInput = read_json_input(&input)?;
                let project = app.save_project(None, fields).map_err(CliError::from)?;
                print_project(&project, args.json);
            }
            ProjectCommand::Update { id, input } => {
                let fields: ProjectInput = read_json_input(&input)?;
                let project = app
                    .save_project(Some(&id), fields)
                    .map_err(CliError::from)?;
                print_project(&project, args.json);
            }
            ProjectCommand::Archive { id } => {
                let project = app.archive_project(&id, true).map_err(CliError::from)?;
                print_project(&project, args.json);
            }
            ProjectCommand::Restore { id } => {
                let project = app.archive_project(&id, false).map_err(CliError::from)?;
                print_project(&project, args.json);
            }
            ProjectCommand::Delete { id, yes } => {
                let project = app.project(&id).map_err(CliError::from)?;
                confirm_action(
                    &format!(
                        "permanently delete archived project '{}'",
                        project.fields.name
                    ),
                    yes,
                    json,
                )?;
                app.delete_archived_project(&id).map_err(CliError::from)?;
                print_action("Project permanently deleted.", json);
            }
        },
        Some(Command::Notes { action }) => run_note_command(&app, action, json)?,
        Some(Command::Assets { action }) => run_asset_command(&app, action, json)?,
        Some(Command::Trash { action }) => run_trash_command(&app, action, json)?,
        Some(Command::Vehicles { action }) => run_vehicle_command(action, json)?,
        Some(Command::Packages { action }) => run_package_command(&app, action, json)?,
        Some(Command::Backups { action }) => run_backup_command(&app, action, json)?,
        Some(Command::Posts { action }) => run_post_command(&app, action, json)?,
        Some(Command::Codes { action }) => run_code_command(&app, action, json)?,
        Some(Command::Roblox { action }) => run_roblox_command(&app, action, json)?,
        Some(Command::Statistics { action }) => run_statistics_command(&app, action, json)?,
        Some(Command::Revenue { action }) => run_revenue_command(&app, action, json)?,
        Some(Command::Workspace { action: None }) => print_workspace(&app, json)?,
        Some(Command::Workspace {
            action: Some(WorkspaceCommand::Rename { name }),
        }) => {
            let info = app.rename_workspace(&name).map_err(CliError::from)?;
            if json {
                print_json(&info)?;
            } else {
                println!("Workspace renamed to {}", info.name);
            }
        }
        None => {
            if args.json {
                return Err(CliError {
                    code: "invalid_mode".into(),
                    message: "--json requires a non-interactive command.".into(),
                });
            }
            if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
                return Err(CliError {
                    code: "interactive_terminal_required".into(),
                    message: "Run this command in a terminal, or use a command such as `projects list --json`.".into(),
                });
            }
            run_tui(app).map_err(|error| CliError {
                code: "terminal_error".into(),
                message: error.to_string(),
            })?;
        }
    }
    Ok(())
}

fn print_workspace(app: &ApplicationServices, json: bool) -> Result<(), CliError> {
    let info = app.workspace_info().map_err(CliError::from)?;
    if json {
        print_json(&info)?;
    } else {
        println!("{}", info.name);
        println!("Path: {}", info.root);
        println!("Database schema: {}", info.schema_version);
        println!("Managed files: {}", info.managed_file_count);
    }
    Ok(())
}

fn run_package_command(
    app: &ApplicationServices,
    action: PackageCommand,
    json: bool,
) -> Result<(), CliError> {
    match action {
        PackageCommand::Inspect { path } => {
            let (_archive, preview) = snapshot_archive(app, &path)?;
            if preview.kind != "project" {
                return Err(CliError {
                    code: "wrong_archive_kind".into(),
                    message: "Choose an .apexstd project package.".into(),
                });
            }
            print_archive_preview(&preview, json)?;
        }
        PackageCommand::Export {
            project,
            destination,
            metadata_only,
            exclude_notes,
        } => {
            app.export_project_package(
                &project,
                !metadata_only,
                !metadata_only && !exclude_notes,
                &destination,
            )
            .map_err(CliError::from)?;
            print_action(
                &format!("Exported project package to {}", destination.display()),
                json,
            );
        }
        PackageCommand::Import { path, yes } => {
            let (archive, preview) = snapshot_archive(app, &path)?;
            if preview.kind != "project" {
                return Err(CliError {
                    code: "wrong_archive_kind".into(),
                    message: "Choose an .apexstd project package.".into(),
                });
            }
            confirm_action(
                &format!(
                    "import '{}' as a new project ({} assets, {} notes, {} bytes)",
                    preview.title, preview.assets, preview.notes, preview.bytes
                ),
                yes,
                json,
            )?;
            let id = app
                .import_project_package(&archive)
                .map_err(CliError::from)?;
            let project = app.project(&id).map_err(CliError::from)?;
            if json {
                print_json(&serde_json::json!({"project":project,"archive":preview}))?;
            } else {
                println!("Imported {} ({})", project.fields.name, project.id);
            }
        }
    }
    Ok(())
}

fn run_backup_command(
    app: &ApplicationServices,
    action: BackupCommand,
    json: bool,
) -> Result<(), CliError> {
    match action {
        BackupCommand::Create { destination } => {
            app.create_workspace_backup(&destination)
                .map_err(CliError::from)?;
            print_action(
                &format!("Created workspace backup at {}", destination.display()),
                json,
            );
        }
        BackupCommand::Inspect { path } => {
            let (_archive, preview) = snapshot_archive(app, &path)?;
            if preview.kind != "workspace" {
                return Err(CliError {
                    code: "wrong_archive_kind".into(),
                    message: "Choose an .apexbackup workspace backup.".into(),
                });
            }
            print_archive_preview(&preview, json)?;
        }
        BackupCommand::Restore { path, yes } => {
            let (archive, preview) = snapshot_archive(app, &path)?;
            if preview.kind != "workspace" {
                return Err(CliError {
                    code: "wrong_archive_kind".into(),
                    message: "Choose an .apexbackup workspace backup.".into(),
                });
            }
            let current = app.workspace_info().map_err(CliError::from)?;
            confirm_action(
                &format!(
                    "replace workspace '{}' with '{}' ({} bytes); a safety backup will be created first",
                    current.name, preview.title, preview.bytes
                ),
                yes,
                json,
            )?;
            let safety_backup = app
                .restore_workspace_backup(&archive)
                .map_err(CliError::from)?;
            let restored = app.workspace_info().map_err(CliError::from)?;
            if json {
                print_json(&serde_json::json!({
                    "workspace": restored,
                    "safetyBackup": safety_backup,
                    "archive": preview
                }))?;
            } else {
                println!("Restored workspace {}", restored.name);
                println!("Safety backup: {}", safety_backup.display());
            }
        }
    }
    Ok(())
}

fn run_post_command(
    app: &ApplicationServices,
    action: PostCommand,
    json: bool,
) -> Result<(), CliError> {
    match action {
        PostCommand::List => {
            let posts = app.posts().map_err(CliError::from)?;
            if json {
                print_json(&posts)?;
            } else if posts.is_empty() {
                println!("No ERLCX drafts.");
            } else {
                for post in posts {
                    println!(
                        "{}\t{}\t{} livery(s)\t{}",
                        post.id, post.title, post.liveries, post.updated_at
                    );
                }
            }
        }
        PostCommand::Create => {
            let id = app.create_post().map_err(CliError::from)?;
            let draft = app.post(&id).map_err(CliError::from)?;
            if json {
                print_json(&serde_json::json!({"id":id,"draft":draft}))?;
            } else {
                println!("Created ERLCX draft {id}");
            }
        }
        PostCommand::Import { source } => {
            let id = app.import_post_bundle(&source).map_err(CliError::from)?;
            let draft = app.post(&id).map_err(CliError::from)?;
            if json {
                print_json(&serde_json::json!({"id":id,"draft":draft}))?;
            } else {
                println!(
                    "Imported .apexpublish bundle as ERLCX draft {id} ({} liveries, {} previews).",
                    draft.data.liveries.len(),
                    draft.data.media.len()
                );
            }
        }
        PostCommand::Handoff { id, reviewed } => {
            let draft = app.post(&id).map_err(CliError::from)?;
            request_chrome_handoff(&id, draft.revision, reviewed).map_err(CliError::from)?;
            if json {
                print_json(&serde_json::json!({"ok":true,"postId":id}))?;
            } else {
                println!(
                    "Opened the ERLCX assistant in Chrome for draft {}. Review the final page yourself; Apex Studio will not submit or publish it.",
                    draft.data.title
                );
            }
        }
        PostCommand::Show { id } => {
            let draft = app.post(&id).map_err(CliError::from)?;
            if json {
                print_json(&draft)?;
            } else {
                println!("{}\nRevision: {}\n", draft.data.title, draft.revision);
                println!(
                    "{} liveries · {} preview image(s)",
                    draft.data.liveries.len(),
                    draft.data.media.len()
                );
                println!("Save the JSON shown by `posts show {id} --json` to edit this draft.");
            }
        }
        PostCommand::Save { id, input } => {
            let source = fs::read(&input).map_err(|error| CliError {
                code: "input_read_failed".into(),
                message: format!("{}: {error}", input.display()),
            })?;
            let draft: apex_core::PublishingDraft =
                serde_json::from_slice(&source).map_err(|error| CliError {
                    code: "invalid_post_input".into(),
                    message: error.to_string(),
                })?;
            let saved = app
                .save_post(&id, draft.revision, draft.data)
                .map_err(CliError::from)?;
            let _ = app.release_post_image_pins(&id);
            if json {
                print_json(&saved)?;
            } else {
                println!("Saved ERLCX draft at revision {}", saved.revision);
            }
        }
        PostCommand::Sources { id } => {
            let sources = app.post_sources(&id).map_err(CliError::from)?;
            if json {
                print_json(&sources)?;
            } else if sources.is_empty() {
                println!("This draft has no linked livery sources.");
            } else {
                for source in sources {
                    println!("{}\t{}\t{}", source.folder_id, source.vehicle, source.state);
                }
            }
        }
        PostCommand::AddImages { id, paths } => {
            app.post(&id).map_err(CliError::from)?;
            let previews = app.add_post_images(&id, &paths).map_err(CliError::from)?;
            if json {
                print_json(&previews)?;
            } else {
                for preview in previews {
                    println!("{}\t{}\t{} bytes", preview.hash, preview.name, preview.size);
                }
                println!("Add these hashes to the draft's media list, then save it.");
            }
        }
        PostCommand::Export {
            id,
            destination,
            reviewed,
        } => {
            let draft = app.post(&id).map_err(CliError::from)?;
            app.export_post(&id, draft.revision, reviewed, &destination)
                .map_err(CliError::from)?;
            print_action(
                &format!("Exported .apexpublish bundle to {}", destination.display()),
                json,
            );
        }
    }
    Ok(())
}

fn run_code_command(
    app: &ApplicationServices,
    action: CodeCommand,
    json: bool,
) -> Result<(), CliError> {
    match action {
        CodeCommand::Export {
            destination,
            posts,
            reviewed,
        } => {
            if posts.is_empty() || posts.len() > 50 {
                return Err(CliError {
                    code: "invalid_post_selection".into(),
                    message: "Choose between 1 and 50 ERLCX post drafts.".into(),
                });
            }
            let mut selected = std::collections::HashSet::new();
            let mut markdown = String::new();
            let mut codes = 0u32;
            let mut missing = 0u32;
            let mut livery_count = 0usize;
            for id in posts {
                if !selected.insert(id.clone()) {
                    continue;
                }
                let draft = app.post(&id).map_err(CliError::from)?;
                draft.data.validate(false).map_err(CliError::from)?;
                if !reviewed
                    && app
                        .post_sources(&id)
                        .map_err(CliError::from)?
                        .iter()
                        .any(|source| source.state != "current")
                {
                    return Err(CliError {
                        code: "source_review_required".into(),
                        message: format!(
                            "Draft '{}' has changed livery sources. Review the saved snapshot and pass --reviewed to export its codes.",
                            draft.data.title
                        ),
                    });
                }
                markdown.push_str(&format!("# {}\n\n", markdown_escape(&draft.data.title)));
                for livery in draft.data.liveries {
                    livery_count += 1;
                    markdown.push_str(&format!("## {}\n\n", markdown_escape(&livery.vehicle)));
                    for side in livery.sides {
                        let code = if side.decal_id.is_empty() {
                            missing += 1;
                            "Not uploaded".to_owned()
                        } else {
                            codes += 1;
                            format!("`{}`", side.decal_id)
                        };
                        markdown.push_str(&format!(
                            "- **{}:** {}\n",
                            markdown_escape(&side.side),
                            code
                        ));
                    }
                    markdown.push('\n');
                }
            }
            if destination.exists() {
                return Err(CliError {
                    code: "output_exists".into(),
                    message: format!(
                        "{} already exists; choose another output path.",
                        destination.display()
                    ),
                });
            }
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&destination)
                .map_err(|error| CliError {
                    code: "output_create_failed".into(),
                    message: format!("{}: {error}", destination.display()),
                })?;
            if let Err(error) = output
                .write_all(markdown.as_bytes())
                .and_then(|_| output.sync_all())
            {
                drop(output);
                let _ = fs::remove_file(&destination);
                return Err(CliError {
                    code: "output_write_failed".into(),
                    message: format!("{}: {error}", destination.display()),
                });
            }
            if json {
                print_json(&serde_json::json!({
                    "path": destination,
                    "posts": selected.len(),
                    "liveries": livery_count,
                    "codes": codes,
                    "missing": missing
                }))?;
            } else {
                println!(
                    "Exported {codes} decal codes from {livery_count} liveries; {missing} missing."
                );
            }
        }
    }
    Ok(())
}

fn run_roblox_command(
    app: &ApplicationServices,
    action: RobloxCommand,
    json: bool,
) -> Result<(), CliError> {
    match action {
        RobloxCommand::Liveries { account_id } => {
            if !valid_account_id(&account_id) {
                return Err(CliError {
                    code: "invalid_account_id".into(),
                    message: "Provide the Roblox account ID to inspect account-specific uploads."
                        .into(),
                });
            }
            let liveries = app.roblox_liveries(&account_id).map_err(CliError::from)?;
            if json {
                print_json(&liveries)?;
            } else if liveries.is_empty() {
                println!("No livery folders are available for Roblox uploads.");
            } else {
                for livery in liveries {
                    println!(
                        "{}\t{} / {}\t{} of {} texture(s) already uploaded",
                        livery.id,
                        livery.project_name,
                        livery.name,
                        livery.uploaded,
                        livery.textures
                    );
                }
            }
        }
        RobloxCommand::Queue => {
            let queue = app.upload_queue().map_err(CliError::from)?;
            if json {
                print_json(&queue)?;
            } else if queue.batches.is_empty() {
                println!("The Roblox upload queue is empty.");
            } else {
                for batch in &queue.batches {
                    let job_ids = queue
                        .items
                        .iter()
                        .filter(|item| item.batch_id == batch.id)
                        .map(|item| item.job_id.as_str())
                        .collect::<std::collections::HashSet<_>>();
                    let jobs = job_ids.len();
                    let failed = queue
                        .jobs
                        .iter()
                        .filter(|job| {
                            job_ids.contains(job.id.as_str())
                                && matches!(job.state.as_str(), "failed" | "needs_review")
                        })
                        .count();
                    println!(
                        "{}\t{}\taccount {}\t{} job(s), {} need attention",
                        batch.id, batch.state, batch.account_id, jobs, failed
                    );
                }
            }
        }
        RobloxCommand::Run { yes } => {
            run_roblox_worker(app, json, yes)?;
        }
        RobloxCommand::DoubleUpload { enable, disable } => {
            if enable || disable {
                app.set_roblox_double_upload(enable)
                    .map_err(CliError::from)?;
            }
            let enabled = app.roblox_double_upload().map_err(CliError::from)?;
            if json {
                print_json(&serde_json::json!({"doubleUpload": enabled}))?;
            } else {
                println!(
                    "Double upload is {} for newly created batches.",
                    if enabled { "enabled" } else { "disabled" }
                );
            }
        }
        RobloxCommand::Credentials { action } => match action {
            RobloxCredentialsCommand::Status => {
                let credentials =
                    apex_application::load_roblox_credentials().map_err(CliError::from)?;
                if json {
                    print_json(&serde_json::json!({
                        "configured": credentials.is_some(),
                        "accountId": credentials.as_ref().map(|value| &value.account_id)
                    }))?;
                } else if let Some(credentials) = credentials {
                    println!(
                        "Roblox credentials are configured for account {}.",
                        credentials.account_id
                    );
                } else {
                    println!("Roblox credentials are not configured.");
                }
            }
            RobloxCredentialsCommand::Set { account_id } => {
                if !valid_account_id(&account_id) {
                    return Err(CliError {
                        code: "invalid_account_id".into(),
                        message: "Provide a valid numeric Roblox account ID.".into(),
                    });
                }
                let api_key = read_hidden_secret("Roblox API key")?;
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|error| CliError {
                        code: "runtime_unavailable".into(),
                        message: error.to_string(),
                    })?;
                let credentials = runtime
                    .block_on(apex_application::prepare_roblox_credentials(
                        account_id.clone(),
                        api_key,
                    ))
                    .map_err(CliError::from)?;
                app.control_roblox_batch(None, "pause")
                    .map_err(CliError::from)?;
                apex_application::store_roblox_credentials(credentials).map_err(CliError::from)?;
                if json {
                    print_json(&serde_json::json!({"configured": true, "accountId": account_id}))?;
                } else {
                    println!("Roblox credentials validated and saved for account {account_id}. Upload batches remain paused; resume the ones you want to continue.");
                }
            }
            RobloxCredentialsCommand::Clear { yes } => {
                confirm_action("remove the saved Roblox API key", yes, json)?;
                app.control_roblox_batch(None, "pause")
                    .map_err(CliError::from)?;
                apex_application::clear_roblox_credentials().map_err(CliError::from)?;
                print_action(
                    "Removed Roblox credentials from Windows Credential Manager.",
                    json,
                );
            }
        },
        RobloxCommand::Batch { action } => match action {
            UploadBatchCommand::Start {
                account_id,
                liveries,
                reupload,
                yes,
            } => {
                if !valid_account_id(&account_id) {
                    return Err(CliError {
                        code: "invalid_account_id".into(),
                        message: "Provide the Roblox account ID for this batch.".into(),
                    });
                }
                if liveries.is_empty() || liveries.len() > 500 {
                    return Err(CliError {
                        code: "invalid_livery_selection".into(),
                        message: "Choose between 1 and 500 livery folders.".into(),
                    });
                }
                let double_upload = app.roblox_double_upload().map_err(CliError::from)?;
                let mut unique = std::collections::HashSet::new();
                let mut selected_ids = Vec::new();
                let mut textures = 0usize;
                let mut reusable = 0usize;
                for id in &liveries {
                    if !unique.insert(id) {
                        continue;
                    }
                    selected_ids.push(id.clone());
                    let livery = app
                        .livery_uploads(id, &account_id)
                        .map_err(CliError::from)?;
                    for texture in livery.textures {
                        textures += 1;
                        let usable = texture.upload.as_ref().is_some_and(|upload| {
                            upload.state == "uploaded"
                                && upload.account_id == account_id
                                && upload.hash == texture.hash
                                && upload.moderation != "rejected"
                                && upload.asset_id.as_deref().is_some_and(valid_decal_id)
                        });
                        if usable {
                            reusable += 1;
                        }
                    }
                }
                let selected = unique.len();
                let uploads = if reupload {
                    textures
                } else {
                    textures.saturating_sub(reusable)
                };
                if textures == 0 {
                    return Err(CliError {
                        code: "no_assigned_textures".into(),
                        message: "The selected livery folders have no assigned textures.".into(),
                    });
                }
                if uploads == 0 {
                    print_action(
                        "All selected textures already have reusable Roblox IDs.",
                        json,
                    );
                    return Ok(());
                }
                let passes = uploads * if double_upload { 2 } else { 1 };
                if !json {
                    println!(
                        "Review: {selected} livery/liveries, {textures} texture(s), {reusable} reusable IDs, {passes} upload pass(es)."
                    );
                    if reupload {
                        println!(
                            "Selected existing IDs will be replaced only after successful uploads."
                        );
                    }
                }
                confirm_action("start this Roblox upload batch", yes, json)?;
                let request_id = format!(
                    "cli-{}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos())
                        .unwrap_or_default()
                );
                let id = app
                    .create_roblox_batch(&request_id, &account_id, &selected_ids, reupload)
                    .map_err(CliError::from)?;
                if json {
                    print_json(&serde_json::json!({
                        "batchId": id,
                        "accountId": account_id,
                        "liveries": selected,
                        "textures": textures,
                        "reusedIds": if reupload { 0 } else { reusable },
                        "uploadPasses": passes,
                        "doubleUpload": double_upload
                    }))?;
                } else {
                    println!("Created batch {id}. Run `apex-studio-cli roblox run` to process it here, or keep Apex Studio open.");
                }
            }
            UploadBatchCommand::Pause { id } => {
                app.control_roblox_batch(id.as_deref(), "pause")
                    .map_err(CliError::from)?;
                print_action("Paused the selected Roblox batch(es).", json);
            }
            UploadBatchCommand::Resume { id } => {
                app.control_roblox_batch(id.as_deref(), "resume")
                    .map_err(CliError::from)?;
                print_action("Resumed the selected Roblox batch(es).", json);
            }
            UploadBatchCommand::Cancel { id, yes } => {
                confirm_action("cancel pending Roblox upload work", yes, json)?;
                app.control_roblox_batch(id.as_deref(), "cancel")
                    .map_err(CliError::from)?;
                print_action("Cancelled pending Roblox upload work.", json);
            }
            UploadBatchCommand::Retry { id } => {
                app.control_roblox_batch(id.as_deref(), "retry")
                    .map_err(CliError::from)?;
                print_action("Queued failed Roblox passes for retry.", json);
            }
        },
    }
    Ok(())
}

fn run_statistics_command(
    app: &ApplicationServices,
    action: StatisticsCommand,
    json: bool,
) -> Result<(), CliError> {
    match action {
        StatisticsCommand::Profile { set } => {
            if let Some(profile_id) = set {
                app.set_statistics_profile(&profile_id)
                    .map_err(CliError::from)?;
            }
            let profile_id = app.statistics_profile().map_err(CliError::from)?;
            if json {
                print_json(&serde_json::json!({"profileId": profile_id}))?;
            } else if let Some(profile_id) = profile_id {
                println!("ERLCX public profile: {profile_id}");
            } else {
                println!("No ERLCX profile is configured. Set one with `statistics profile --set <ROBLOX_USER_ID>`. ");
            }
        }
        StatisticsCommand::History => {
            let history = app.statistics_history().map_err(CliError::from)?;
            if json {
                print_json(&history)?;
            } else if history.is_empty() {
                println!("No ERLCX profile snapshots have been collected.");
            } else {
                for snapshot in history {
                    println!("{} · {} posts", snapshot.collected_at, snapshot.posts.len());
                }
            }
        }
        StatisticsCommand::Refresh => {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| CliError {
                    code: "runtime_unavailable".into(),
                    message: error.to_string(),
                })?;
            let history = runtime
                .block_on(app.refresh_statistics())
                .map_err(CliError::from)?;
            if json {
                print_json(&history)?;
            } else if let Some(snapshot) = history.last() {
                println!(
                    "Saved ERLCX public profile snapshot · {} posts · {}",
                    snapshot.posts.len(),
                    snapshot.collected_at
                );
            }
        }
        StatisticsCommand::Sales => {
            let sales = app.statistics_sales().map_err(CliError::from)?;
            if json {
                print_json(&sales)?;
            } else if let Some(snapshot) = sales {
                println!("Creator sales snapshot · {}", snapshot.collected_at);
                println!(
                    "{} sales · {} all-time · {} net Robux · {} gross Robux",
                    snapshot.data.sales,
                    snapshot.data.all_time_sales,
                    snapshot.data.net_robux,
                    snapshot.data.gross_robux
                );
            } else {
                println!("No authenticated creator-sales snapshot is available.");
            }
        }
        StatisticsCommand::Summary => {
            let profile_id = app.statistics_profile().map_err(CliError::from)?;
            let history = app.statistics_history().map_err(CliError::from)?;
            let latest = history.last();
            let posts = latest.map_or(&[][..], |snapshot| snapshot.posts.as_slice());
            let ratings = posts
                .iter()
                .filter_map(|post| post.rating_average)
                .collect::<Vec<_>>();
            let rating_average =
                (!ratings.is_empty()).then(|| ratings.iter().sum::<f64>() / ratings.len() as f64);
            let summary = serde_json::json!({
                "profileId": profile_id.clone(),
                "snapshotCount": history.len(),
                "collectedAt": latest.map(|snapshot| snapshot.collected_at.as_str()),
                "postCount": posts.len(),
                "views": statistics_metric(posts, |post| post.views),
                "likes": statistics_metric(posts, |post| post.likes),
                "copies": statistics_metric(posts, |post| post.copies),
                "favourites": statistics_metric(posts, |post| post.favourites),
                "ratingCount": statistics_metric(posts, |post| post.rating_count),
                "ratingAverage": rating_average
            });
            if json {
                print_json(&summary)?;
            } else if profile_id.is_none() {
                println!("No ERLCX profile is configured. Set one with `statistics profile --set <ROBLOX_USER_ID>`. ");
            } else if latest.is_none() {
                println!(
                    "No snapshot has been collected. Run `apex-studio-cli statistics refresh` to collect one."
                );
            } else {
                println!(
                    "ERLCX public statistics · {}",
                    summary["collectedAt"].as_str().unwrap_or("unknown time")
                );
                println!("Posts: {}", posts.len());
                for name in ["views", "likes", "copies", "favourites", "ratingCount"] {
                    let metric = &summary[name];
                    println!(
                        "{}: {} ({}/{} posts)",
                        name,
                        metric["total"]
                            .as_u64()
                            .map_or("—".into(), |value| value.to_string()),
                        metric["knownPosts"].as_u64().unwrap_or(0),
                        posts.len()
                    );
                }
                if let Some(rating) = summary["ratingAverage"].as_f64() {
                    println!("Average rating: {rating:.2} / 5");
                }
            }
        }
    }
    Ok(())
}

fn statistics_metric(
    posts: &[StatisticsPost],
    value: impl Fn(&StatisticsPost) -> Option<u64>,
) -> serde_json::Value {
    let values = posts.iter().filter_map(value).collect::<Vec<_>>();
    serde_json::json!({
        "total": (!values.is_empty()).then(|| values.iter().fold(0u64, |sum, value| sum.saturating_add(*value))),
        "knownPosts": values.len()
    })
}

fn run_revenue_command(
    app: &ApplicationServices,
    action: RevenueCommand,
    json: bool,
) -> Result<(), CliError> {
    let data = app.roblox_group_revenue().map_err(CliError::from)?;
    match action {
        RevenueCommand::Groups => {
            if json {
                let groups = data
                    .groups
                    .iter()
                    .map(|group| {
                        let sales = data
                            .sales
                            .iter()
                            .filter(|sale| sale.group_id == group.group_id)
                            .count();
                        serde_json::json!({"group": group, "sales": sales})
                    })
                    .collect::<Vec<_>>();
                print_json(&groups)?;
            } else if data.groups.is_empty() {
                println!("No Roblox groups are configured for revenue tracking.");
            } else {
                for group in &data.groups {
                    let sales = data
                        .sales
                        .iter()
                        .filter(|sale| sale.group_id == group.group_id)
                        .count();
                    println!(
                        "{} · {} · {} imported sales",
                        group.name, group.group_id, sales
                    );
                }
            }
        }
        RevenueCommand::Configure {
            group_id,
            name,
            experience_ids,
        } => {
            let existing = data.groups.iter().find(|group| group.group_id == group_id);
            let group = RobloxRevenueGroup {
                group_id,
                name,
                experience_ids: experience_ids
                    .into_iter()
                    .flat_map(|value| {
                        value
                            .split(',')
                            .map(str::trim)
                            .map(str::to_owned)
                            .collect::<Vec<_>>()
                    })
                    .filter(|value| !value.is_empty())
                    .collect(),
                pass_mappings: existing
                    .map_or_else(BTreeMap::new, |group| group.pass_mappings.clone()),
                selected_pass_ids: existing
                    .map_or_else(Vec::new, |group| group.selected_pass_ids.clone()),
                api_updated_at: existing.and_then(|group| group.api_updated_at.clone()),
                api_coverage_from: existing.and_then(|group| group.api_coverage_from.clone()),
                api_coverage_to: existing.and_then(|group| group.api_coverage_to.clone()),
                api_summary: existing.and_then(|group| group.api_summary.clone()),
                api_status: existing.and_then(|group| group.api_status.clone()),
                updated_at: existing.and_then(|group| group.updated_at.clone()),
                last_import_at: existing.and_then(|group| group.last_import_at.clone()),
            };
            let updated = app
                .save_roblox_revenue_group(&group)
                .map_err(CliError::from)?;
            let saved = updated
                .groups
                .iter()
                .find(|entry| entry.group_id == group.group_id)
                .ok_or_else(|| CliError {
                    code: "revenue_group_save_failed".into(),
                    message: "The group was saved but could not be read back.".into(),
                })?;
            if json {
                print_json(saved)?;
            } else {
                println!(
                    "Saved {} · group {} · {} experience(s)",
                    saved.name,
                    saved.group_id,
                    saved.experience_ids.len()
                );
            }
        }
        RevenueCommand::SelectPass { group_id, asset_id } => {
            update_revenue_pass(&data, app, &group_id, &asset_id, true, json)?;
        }
        RevenueCommand::DeselectPass { group_id, asset_id } => {
            update_revenue_pass(&data, app, &group_id, &asset_id, false, json)?;
        }
        RevenueCommand::Remove { group_id, yes } => {
            if !data.groups.iter().any(|group| group.group_id == group_id) {
                return Err(CliError {
                    code: "revenue_group_not_found".into(),
                    message: format!("No Group Revenue data exists for group {group_id}."),
                });
            }
            confirm_action(
                &format!("remove group {group_id} and its stored sales"),
                yes,
                json,
            )?;
            app.remove_roblox_group_revenue(&group_id)
                .map_err(CliError::from)?;
            if json {
                print_json(&serde_json::json!({"removedGroupId": group_id}))?;
            } else {
                println!("Removed group {group_id} and its stored sales.");
            }
        }
        RevenueCommand::Show { group_id } => {
            let group = data
                .groups
                .iter()
                .find(|group| group.group_id == group_id)
                .ok_or_else(|| CliError {
                    code: "revenue_group_not_found".into(),
                    message: format!("No Group Revenue data exists for group {group_id}."),
                })?;
            let sales = data
                .sales
                .into_iter()
                .filter(|sale| sale.group_id == group_id)
                .collect::<Vec<_>>();
            let passes = data
                .passes
                .into_iter()
                .filter(|pass| pass.group_id == group_id)
                .collect::<Vec<_>>();
            if json {
                print_json(&serde_json::json!({"group": group, "sales": sales, "passes": passes}))?;
            } else {
                let selected_passes = group
                    .selected_pass_ids
                    .iter()
                    .map(String::as_str)
                    .collect::<HashSet<_>>();
                let selected_sales = sales
                    .iter()
                    .filter(|sale| selected_passes.contains(sale.asset_id.as_str()))
                    .collect::<Vec<_>>();
                let gross = selected_sales
                    .iter()
                    .filter(|sale| sale.hold_status != "Cancelled")
                    .fold(0u64, |sum, sale| sum.saturating_add(sale.price));
                let revenue = selected_sales
                    .iter()
                    .filter(|sale| sale.hold_status != "Cancelled")
                    .fold(0u64, |sum, sale| sum.saturating_add(sale.revenue));
                println!("{} · group {}", group.name, group.group_id);
                println!(
                    "Experiences: {} · passes: {} · selected sales: {}",
                    group.experience_ids.len(),
                    passes.len(),
                    selected_sales.len()
                );
                println!("Selected pass price total: {} Robux", gross);
                println!("Selected pass revenue total: {} Robux", revenue);
                if let Some(updated) = &group.api_updated_at {
                    println!(
                        "Roblox API refresh: {updated} · status {}",
                        group.api_status.as_deref().unwrap_or("unknown")
                    );
                }
            }
        }
    }
    Ok(())
}

fn update_revenue_pass(
    data: &RobloxGroupRevenueData,
    app: &ApplicationServices,
    group_id: &str,
    asset_id: &str,
    selecting: bool,
    json: bool,
) -> Result<(), CliError> {
    let mut group = data
        .groups
        .iter()
        .find(|group| group.group_id == group_id)
        .cloned()
        .ok_or_else(|| CliError {
            code: "revenue_group_not_found".into(),
            message: format!("No Group Revenue data exists for group {group_id}."),
        })?;
    if selecting {
        let exists = data
            .passes
            .iter()
            .any(|pass| pass.group_id == group_id && pass.asset_id == asset_id)
            || data
                .sales
                .iter()
                .any(|sale| sale.group_id == group_id && sale.asset_id == asset_id)
            || group.pass_mappings.contains_key(asset_id);
        if !exists {
            return Err(CliError {
                code: "revenue_pass_not_found".into(),
                message: format!(
                    "Game Pass {asset_id} has not been discovered or imported for group {group_id}."
                ),
            });
        }
        if !group.selected_pass_ids.iter().any(|id| id == asset_id) {
            group.selected_pass_ids.push(asset_id.into());
            group.selected_pass_ids.sort();
        }
    } else {
        group.selected_pass_ids.retain(|id| id != asset_id);
    }
    let updated = app
        .save_roblox_revenue_group(&group)
        .map_err(CliError::from)?;
    let saved = updated
        .groups
        .iter()
        .find(|entry| entry.group_id == group_id)
        .ok_or_else(|| CliError {
            code: "revenue_group_save_failed".into(),
            message: "The group was saved but could not be read back.".into(),
        })?;
    if json {
        print_json(saved)?;
    } else {
        println!(
            "{} Game Pass {} for {}",
            if selecting { "Selected" } else { "Deselected" },
            asset_id,
            saved.name
        );
    }
    Ok(())
}

fn run_roblox_worker(app: &ApplicationServices, json: bool, yes: bool) -> Result<(), CliError> {
    let before = app.upload_queue().map_err(CliError::from)?;
    if !before.batches.iter().any(|batch| batch.state == "running") {
        if json {
            print_json(&before)?;
        } else {
            println!("No active Roblox upload batches.");
        }
        return Ok(());
    }
    confirm_action(
        "run the active Roblox upload queue (network uploads will begin)",
        yes,
        json,
    )?;

    let credentials = apex_application::load_roblox_credentials()
        .map_err(CliError::from)?
        .ok_or_else(|| CliError {
            code: "roblox_credentials_missing".into(),
            message: "Configure and validate Roblox credentials before running uploads.".into(),
        })?;
    let account_id = credentials.account_id.clone();
    drop(credentials);

    let mismatched = before
        .batches
        .iter()
        .filter(|batch| batch.state == "running" && batch.account_id != account_id)
        .map(|batch| batch.id.as_str())
        .collect::<Vec<_>>();
    if !mismatched.is_empty() {
        return Err(CliError {
            code: "roblox_queue_account_mismatch".into(),
            message: format!(
                "Running batches are pinned to another Roblox account ({}). Change the account or pause those batches before running this queue.",
                mismatched.join(", ")
            ),
        });
    }

    let stopping = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let signal_stop = stopping.clone();
    let last_progress =
        std::sync::Arc::new(std::sync::Mutex::new(None::<(usize, usize, usize, usize)>));
    let progress_state = last_progress.clone();
    let runner = app.run_roblox_upload_queue(stopping.clone(), move |snapshot, active| {
        let running = snapshot
            .batches
            .iter()
            .filter(|batch| batch.state == "running")
            .count();
        let complete = snapshot
            .jobs
            .iter()
            .filter(|job| job.state == "complete")
            .count();
        let attention = snapshot
            .jobs
            .iter()
            .filter(|job| matches!(job.state.as_str(), "failed" | "needs_review"))
            .count();
        let progress = (running, active, complete, attention);
        let changed = progress_state
            .lock()
            .map(|mut previous| {
                if *previous == Some(progress) {
                    false
                } else {
                    *previous = Some(progress);
                    true
                }
            })
            .unwrap_or(true);
        if changed {
            eprintln!(
                "Roblox queue: {running} active batch(es), {active} request(s), {complete} completed job(s), {attention} needing attention."
            );
        }
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| CliError {
            code: "runtime_unavailable".into(),
            message: error.to_string(),
        })?;
    let result = runtime
        .block_on(async move {
            let mut runner = Box::pin(runner);
            tokio::select! {
                result = &mut runner => result.map_err(CliError::from),
                signal = tokio::signal::ctrl_c() => {
                    signal_stop.store(true, std::sync::atomic::Ordering::Relaxed);
                    if signal.is_ok() {
                        eprintln!("Stopping after active requests finish; pausing pending batches...");
                    } else {
                        eprintln!("Ctrl+C handling is unavailable; stopping safely and pausing pending batches...");
                    }
                    runner.await.map_err(CliError::from)
                }
            }
        })
        ?;

    if json {
        print_json(&result)?;
    } else {
        let complete = result
            .jobs
            .iter()
            .filter(|job| job.state == "complete")
            .count();
        let attention = result
            .jobs
            .iter()
            .filter(|job| matches!(job.state.as_str(), "failed" | "needs_review"))
            .count();
        let running = result
            .batches
            .iter()
            .filter(|batch| batch.state == "running")
            .count();
        if complete == 0 && running == 0 && attention == 0 {
            println!("No active Roblox upload batches.");
        } else {
            println!(
                "Queue run finished: {complete} completed job(s), {attention} needing attention, {running} active batch(es)."
            );
        }
    }
    Ok(())
}

fn valid_decal_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 20
        && !value.starts_with('0')
        && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn valid_account_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 20
        && !value.starts_with('0')
        && value.bytes().all(|byte| byte.is_ascii_digit())
}

struct RawTerminalGuard;

impl Drop for RawTerminalGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

fn read_hidden_secret(prompt: &str) -> Result<String, CliError> {
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        return Err(CliError {
            code: "interactive_input_required".into(),
            message: "Run this command in an interactive terminal to enter the API key securely."
                .into(),
        });
    }
    eprint!("{prompt} (input hidden; Enter to submit, Esc to cancel): ");
    io::stderr().flush().map_err(|error| CliError {
        code: "hidden_input_failed".into(),
        message: error.to_string(),
    })?;
    crossterm::terminal::enable_raw_mode().map_err(|error| CliError {
        code: "hidden_input_failed".into(),
        message: error.to_string(),
    })?;
    let _guard = RawTerminalGuard;
    let mut secret = zeroize::Zeroizing::new(String::new());
    loop {
        match event::read().map_err(|error| CliError {
            code: "hidden_input_failed".into(),
            message: error.to_string(),
        })? {
            Event::Paste(value) => secret.push_str(&value),
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Enter => {
                    if secret.is_empty() || secret.len() > 2000 {
                        return Err(CliError {
                            code: "invalid_api_key_length".into(),
                            message: "Enter a non-empty API key of at most 2000 characters.".into(),
                        });
                    }
                    let value = std::mem::take(&mut *secret);
                    drop(_guard);
                    eprintln!();
                    return Ok(value);
                }
                KeyCode::Esc => {
                    return Err(CliError {
                        code: "cancelled".into(),
                        message: "No credentials were changed.".into(),
                    });
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Err(CliError {
                        code: "cancelled".into(),
                        message: "No credentials were changed.".into(),
                    });
                }
                KeyCode::Backspace => {
                    secret.pop();
                }
                KeyCode::Char(value)
                    if !key.modifiers.contains(KeyModifiers::CONTROL) && !value.is_control() =>
                {
                    secret.push(value);
                }
                _ => {}
            },
            _ => {}
        }
        if secret.len() > 2000 {
            return Err(CliError {
                code: "invalid_api_key_length".into(),
                message: "Enter an API key of at most 2000 characters.".into(),
            });
        }
    }
}

fn markdown_escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_control() {
            if character == '\n' || character == '\r' {
                escaped.push(' ');
            }
            continue;
        }
        if "\\`*_{}[]<>#+.!|~()-".contains(character) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

fn snapshot_archive(
    app: &ApplicationServices,
    path: &std::path::Path,
) -> Result<(tempfile::TempPath, apex_application::PackagePreview), CliError> {
    let mut source = fs::File::open(path).map_err(|error| CliError {
        code: "archive_read_failed".into(),
        message: format!("{}: {error}", path.display()),
    })?;
    let metadata = source.metadata().map_err(|error| CliError {
        code: "archive_read_failed".into(),
        message: format!("{}: {error}", path.display()),
    })?;
    if !metadata.is_file() {
        return Err(CliError {
            code: "invalid_archive_path".into(),
            message: "Choose a regular archive file.".into(),
        });
    }
    if metadata.len() > 2 * 1024 * 1024 * 1024 {
        return Err(CliError {
            code: "archive_too_large".into(),
            message: "Choose an archive smaller than 2 GB.".into(),
        });
    }
    let mut staged = tempfile::NamedTempFile::new().map_err(|error| CliError {
        code: "archive_staging_failed".into(),
        message: error.to_string(),
    })?;
    io::copy(&mut source, &mut staged).map_err(|error| CliError {
        code: "archive_staging_failed".into(),
        message: error.to_string(),
    })?;
    staged.as_file().sync_all().map_err(|error| CliError {
        code: "archive_staging_failed".into(),
        message: error.to_string(),
    })?;
    let staged = staged.into_temp_path();
    let preview = app.preview_archive(&staged).map_err(CliError::from)?;
    Ok((staged, preview))
}

fn print_archive_preview(
    preview: &apex_application::PackagePreview,
    json: bool,
) -> Result<(), CliError> {
    if json {
        print_json(preview)?;
    } else {
        println!("{}", preview.title);
        println!("Type: {}", preview.kind);
        println!("Assets: {}", preview.assets);
        println!("Notes: {}", preview.notes);
        println!("Stored bytes: {}", preview.bytes);
    }
    Ok(())
}

fn read_json_input<T: DeserializeOwned>(path: &PathBuf) -> Result<T, CliError> {
    let result = if path.as_os_str() == "-" {
        let mut content = String::new();
        io::stdin()
            .read_to_string(&mut content)
            .map_err(|error| CliError {
                code: "input_read_failed".into(),
                message: error.to_string(),
            })?;
        serde_json::from_str(&content).map_err(|error| CliError {
            code: "invalid_json_input".into(),
            message: error.to_string(),
        })
    } else {
        let bytes = fs::read(path).map_err(|error| CliError {
            code: "input_read_failed".into(),
            message: format!("{}: {error}", path.display()),
        })?;
        serde_json::from_slice(&bytes).map_err(|error| CliError {
            code: "invalid_json_input".into(),
            message: error.to_string(),
        })
    };
    result
}

fn read_markdown(path: &PathBuf) -> Result<(String, PathBuf), CliError> {
    let source = fs::canonicalize(path).map_err(|error| CliError {
        code: "input_read_failed".into(),
        message: format!("{}: {error}", path.display()),
    })?;
    let markdown = fs::read_to_string(&source).map_err(|error| CliError {
        code: "input_read_failed".into(),
        message: format!("{}: {error}", source.display()),
    })?;
    let directory = source.parent().unwrap_or_else(|| std::path::Path::new("."));
    Ok((markdown, directory.to_owned()))
}

fn print_json<T: Serialize>(value: &T) -> Result<(), CliError> {
    println!(
        "{}",
        serde_json::to_string(value).map_err(|error| CliError {
            code: "serialization_error".into(),
            message: error.to_string(),
        })?
    );
    Ok(())
}

fn print_action(message: &str, json: bool) {
    if json {
        println!("{{\"ok\":true,\"message\":{}}}", serde_json::json!(message));
    } else {
        println!("{message}");
    }
}

fn export_note_markdown(
    app: &ApplicationServices,
    id: &str,
    output: &PathBuf,
) -> Result<(), CliError> {
    let note = app.note(id).map_err(CliError::from)?;
    if output.exists() {
        return Err(CliError {
            code: "output_exists".into(),
            message: format!(
                "{} already exists; choose another output path.",
                output.display()
            ),
        });
    }
    let parent = output.parent().unwrap_or_else(|| std::path::Path::new("."));
    let image_dir_name = format!(".apex-note-{}.assets", note.id);
    let image_directory = parent.join(&image_dir_name);
    let hashes = apex_core::note_images(&note.document);
    let mut created_files = Vec::new();
    let mut created_directory = false;
    let mut output_created = false;
    let result = (|| -> Result<(), CliError> {
        let mut markdown = note.markdown.clone();
        if !hashes.is_empty() {
            fs::create_dir(&image_directory).map_err(|error| CliError {
                code: "output_create_failed".into(),
                message: format!("{}: {error}", image_directory.display()),
            })?;
            created_directory = true;
        }
        for hash in hashes {
            let data_uri = app.note_image(&note.id, &hash).map_err(CliError::from)?;
            let encoded = data_uri
                .strip_prefix("data:image/png;base64,")
                .ok_or_else(|| CliError {
                    code: "invalid_note_image".into(),
                    message: "The note contains an unsupported image format.".into(),
                })?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|error| CliError {
                    code: "invalid_note_image".into(),
                    message: error.to_string(),
                })?;
            let image_path = image_directory.join(format!("{hash}.png"));
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&image_path)
                .map_err(|error| CliError {
                    code: "output_create_failed".into(),
                    message: format!("{}: {error}", image_path.display()),
                })?;
            created_files.push(image_path);
            file.write_all(&bytes)
                .and_then(|_| file.sync_all())
                .map_err(|error| CliError {
                    code: "output_write_failed".into(),
                    message: error.to_string(),
                })?;
            markdown = markdown.replace(
                &format!("](apex-image:{hash}"),
                &format!("]({image_dir_name}/{hash}.png"),
            );
        }
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)
            .map_err(|error| CliError {
                code: "output_create_failed".into(),
                message: format!("{}: {error}", output.display()),
            })?;
        output_created = true;
        file.write_all(markdown.as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|error| CliError {
                code: "output_write_failed".into(),
                message: format!("{}: {error}", output.display()),
            })?;
        Ok(())
    })();
    if let Err(error) = result {
        for path in created_files {
            let _ = fs::remove_file(path);
        }
        if output_created {
            let _ = fs::remove_file(output);
        }
        if created_directory {
            let _ = fs::remove_dir(&image_directory);
        }
        return Err(error);
    }
    Ok(())
}

fn confirm_action(action: &str, yes: bool, json: bool) -> Result<(), CliError> {
    if yes {
        return Ok(());
    }
    if json || !io::stdin().is_terminal() {
        return Err(CliError {
            code: "confirmation_required".into(),
            message: format!("Pass --yes to {action}."),
        });
    }
    eprint!("Type YES to {action}: ");
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|error| CliError {
            code: "confirmation_failed".into(),
            message: error.to_string(),
        })?;
    if answer.trim() == "YES" {
        Ok(())
    } else {
        Err(CliError {
            code: "cancelled".into(),
            message: "No changes were made.".into(),
        })
    }
}

fn run_note_command(
    app: &ApplicationServices,
    action: NoteCommand,
    json: bool,
) -> Result<(), CliError> {
    match action {
        NoteCommand::List {
            project,
            general: _,
            search,
            archived,
        } => {
            let mut notes = app.notes(project.as_deref()).map_err(CliError::from)?;
            notes.retain(|note| note.archived == archived);
            if let Some(search) = search.filter(|query| !query.trim().is_empty()) {
                let search = search.trim().to_lowercase();
                notes.retain(|note| {
                    note.title.to_lowercase().contains(&search)
                        || note.markdown.to_lowercase().contains(&search)
                });
            }
            if json {
                print_json(&notes)?;
            } else if notes.is_empty() {
                println!("No notes found.");
            } else {
                for note in notes {
                    println!(
                        "{}\t{}\t{}",
                        note.id,
                        note.title,
                        if note.archived { "archived" } else { "active" }
                    );
                }
            }
        }
        NoteCommand::Show { id } => {
            let note = app.note(&id).map_err(CliError::from)?;
            if json {
                print_json(&note)?;
            } else {
                println!(
                    "{}\nID: {}\nRevision: {}\nStatus: {}\n",
                    note.title,
                    note.id,
                    note.revision,
                    if note.archived { "archived" } else { "active" }
                );
                println!("{}", note.markdown);
            }
        }
        NoteCommand::Export { id, output } => {
            export_note_markdown(app, &id, &output)?;
            print_action(&format!("Exported note to {}", output.display()), json);
        }
        NoteCommand::Create {
            project,
            general: _,
            title,
            input,
        } => {
            let markdown_source = input.as_ref().map(read_markdown).transpose()?;
            let note = app
                .create_note(project.as_deref())
                .map_err(CliError::from)?;
            let title = title.unwrap_or_else(|| "Untitled note".into());
            let saved = if let Some((markdown, base_directory)) = markdown_source {
                match app.save_note_markdown(
                    &note.id,
                    note.revision,
                    &title,
                    &markdown,
                    &base_directory,
                ) {
                    Ok(saved) => saved,
                    Err(error) => {
                        if let Ok(archived) = app.archive_note(&note.id, note.revision, true) {
                            let _ = app.delete_archived_note(&note.id, archived.revision);
                        }
                        return Err(CliError::from(error));
                    }
                }
            } else {
                let document = serde_json::json!({"type":"doc","content":[{"type":"paragraph"}]});
                app.save_note(&note.id, note.revision, &title, &document, "")
                    .map_err(CliError::from)?
            };
            if json {
                print_json(&saved)?;
            } else {
                println!("Created note {} ({})", saved.title, saved.id);
            }
        }
        NoteCommand::Edit { id, input, title } => {
            let current = app.note(&id).map_err(CliError::from)?;
            let (markdown, base_directory) = read_markdown(&input)?;
            let saved = app
                .save_note_markdown(
                    &id,
                    current.revision,
                    title.as_deref().unwrap_or(&current.title),
                    &markdown,
                    &base_directory,
                )
                .map_err(CliError::from)?;
            if json {
                print_json(&saved)?;
            } else {
                println!("Saved note {} at revision {}", saved.title, saved.revision);
            }
        }
        NoteCommand::Archive { id } => {
            let current = app.note(&id).map_err(CliError::from)?;
            let note = app
                .archive_note(&id, current.revision, true)
                .map_err(CliError::from)?;
            if json {
                print_json(&note)?;
            } else {
                println!("Archived note {}", note.title);
            }
        }
        NoteCommand::Restore { id } => {
            let current = app.note(&id).map_err(CliError::from)?;
            let note = app
                .archive_note(&id, current.revision, false)
                .map_err(CliError::from)?;
            if json {
                print_json(&note)?;
            } else {
                println!("Restored note {}", note.title);
            }
        }
        NoteCommand::Delete { id, yes } => {
            let note = app.note(&id).map_err(CliError::from)?;
            confirm_action(
                &format!("permanently delete archived note '{}'", note.title),
                yes,
                json,
            )?;
            app.delete_archived_note(&id, note.revision)
                .map_err(CliError::from)?;
            print_action("Note permanently deleted.", json);
        }
    }
    Ok(())
}

fn run_asset_command(
    app: &ApplicationServices,
    action: AssetCommand,
    json: bool,
) -> Result<(), CliError> {
    match action {
        AssetCommand::List { project } => {
            let board = app.asset_board(&project).map_err(CliError::from)?;
            if json {
                print_json(&board)?;
            } else {
                print_asset_board(&board);
            }
        }
        AssetCommand::Import {
            project,
            folder,
            paths,
        } => {
            let report = app
                .import_files(&project, folder.as_deref(), &paths)
                .map_err(CliError::from)?;
            if json {
                print_json(&report)?;
            } else {
                println!(
                    "Imported {} file(s); skipped {}; failed {}.",
                    report.imported.len(),
                    report.skipped.len(),
                    report.failed.len()
                );
                for item in report.skipped {
                    println!("Skipped: {item}");
                }
                for item in report.failed {
                    eprintln!("Failed: {item}");
                }
            }
        }
        AssetCommand::ImportLivery {
            project,
            path,
            vehicle_id,
            name,
            cover,
            assignments,
            unassigned,
            parent,
            existing,
        } => {
            let metadata = fs::symlink_metadata(&path).map_err(|error| CliError {
                code: "livery_folder_unavailable".into(),
                message: format!("{}: {error}", path.display()),
            })?;
            if !metadata.file_type().is_dir() {
                return Err(CliError {
                    code: "invalid_livery_folder".into(),
                    message: "Choose a regular directory containing PNG textures.".into(),
                });
            }
            let mut sources = Vec::new();
            let mut skipped = 0usize;
            for entry in fs::read_dir(&path).map_err(|error| CliError {
                code: "livery_folder_unavailable".into(),
                message: format!("{}: {error}", path.display()),
            })? {
                let entry = entry.map_err(|error| CliError {
                    code: "livery_folder_unavailable".into(),
                    message: error.to_string(),
                })?;
                let file_type = entry.file_type().map_err(|error| CliError {
                    code: "livery_folder_unavailable".into(),
                    message: error.to_string(),
                })?;
                if file_type.is_file()
                    && entry
                        .path()
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
                {
                    let filename = entry.file_name().to_string_lossy().into_owned();
                    let suggested = apex_core::suggest_slot(&filename);
                    sources.push((entry.path(), filename, suggested));
                } else {
                    skipped += 1;
                }
            }
            sources.sort_by(|left, right| left.1.to_lowercase().cmp(&right.1.to_lowercase()));
            if sources.is_empty() || sources.len() > 100 {
                return Err(CliError {
                    code: "invalid_livery_folder".into(),
                    message: "A livery import needs between 1 and 100 PNG files directly inside the selected folder.".into(),
                });
            }
            let mut overrides = std::collections::HashMap::new();
            for assignment in assignments {
                let (filename, slot) = assignment.split_once('=').ok_or_else(|| CliError {
                    code: "invalid_slot_assignment".into(),
                    message: "Use --assign \"FILE=Left 1\".".into(),
                })?;
                if filename.is_empty()
                    || slot.is_empty()
                    || overrides
                        .insert(filename.to_owned(), Some(slot.to_owned()))
                        .is_some()
                {
                    return Err(CliError {
                        code: "invalid_slot_assignment".into(),
                        message: "Each PNG can have only one slot override.".into(),
                    });
                }
            }
            for filename in unassigned {
                if overrides.insert(filename, None).is_some() {
                    return Err(CliError {
                        code: "invalid_slot_assignment".into(),
                        message: "Do not both assign and unassign the same PNG.".into(),
                    });
                }
            }
            if overrides
                .keys()
                .any(|filename| !sources.iter().any(|(_, source, _)| source == filename))
            {
                return Err(CliError {
                    code: "invalid_slot_assignment".into(),
                    message: "An assignment refers to a PNG that is not in the selected folder."
                        .into(),
                });
            }
            for (_, filename, slot) in &mut sources {
                if let Some(override_slot) = overrides.get(filename) {
                    *slot = override_slot.clone();
                }
            }
            let cover_index = cover
                .as_deref()
                .map(|filename| {
                    sources
                        .iter()
                        .position(|(_, name, _)| name == filename)
                        .ok_or_else(|| CliError {
                            code: "invalid_cover".into(),
                            message: "The cover filename must match a PNG in the selected folder."
                                .into(),
                        })
                })
                .transpose()?;
            let livery_name = name.unwrap_or_else(|| {
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
            let id = app
                .import_livery(
                    &project,
                    parent.as_deref(),
                    existing.as_deref(),
                    &livery_name,
                    &vehicle_id,
                    &sources,
                    cover_index,
                )
                .map_err(CliError::from)?;
            let board = app.asset_board(&project).map_err(CliError::from)?;
            let assets: Vec<_> = board
                .assets
                .iter()
                .filter(|asset| {
                    board.placements.iter().any(|placement| {
                        placement.asset_id == asset.id
                            && placement.folder_id.as_deref() == Some(id.as_str())
                    })
                })
                .collect();
            if json {
                print_json(
                    &serde_json::json!({"folderId":id,"name":livery_name,"vehicleId":vehicle_id,"skippedEntries":skipped,"assets":assets}),
                )?;
            } else {
                println!(
                    "Imported livery {} with {} texture(s); skipped {} non-PNG entries.",
                    livery_name,
                    assets.len(),
                    skipped
                );
                for asset in assets {
                    println!(
                        "{}\t{}\t{}",
                        asset.name,
                        asset.id,
                        board
                            .placements
                            .iter()
                            .find(|placement| placement.asset_id == asset.id)
                            .and_then(|placement| placement.slot.as_deref())
                            .unwrap_or("unassigned")
                    );
                }
            }
        }
        AssetCommand::NewFolder {
            project,
            name,
            parent,
        } => {
            let folder = app
                .save_folder(&project, None, parent.as_deref(), &name)
                .map_err(CliError::from)?;
            if json {
                print_json(&folder)?;
            } else {
                println!("Created folder {} ({})", folder.name, folder.id);
            }
        }
        AssetCommand::EditFolder {
            project,
            id,
            name,
            parent,
        } => {
            let folder = app
                .save_folder(&project, Some(&id), parent.as_deref(), &name)
                .map_err(CliError::from)?;
            if json {
                print_json(&folder)?;
            } else {
                println!("Updated folder {} ({})", folder.name, folder.id);
            }
        }
        AssetCommand::Move { id, folder } => {
            app.move_asset(&id, folder.as_deref())
                .map_err(CliError::from)?;
            print_action("Asset moved.", json);
        }
        AssetCommand::Copy {
            project,
            id,
            folder_item,
            folder,
        } => {
            app.copy_asset_item(&project, &id, folder_item, folder.as_deref())
                .map_err(CliError::from)?;
            print_action("Copied within the project.", json);
        }
        AssetCommand::Slot { id, slot, clear } => {
            app.assign_asset_slot(&id, if clear { None } else { slot.as_deref() })
                .map_err(CliError::from)?;
            print_action(
                if clear {
                    "Slot cleared."
                } else {
                    "Slot assigned."
                },
                json,
            );
        }
        AssetCommand::Versions { id } => {
            let versions = app.asset_versions(&id).map_err(CliError::from)?;
            if json {
                print_json(&versions)?;
            } else if versions.is_empty() {
                println!("No earlier versions.");
            } else {
                for version in versions {
                    println!(
                        "{}\t{}\t{}\t{}",
                        version.id, version.name, version.created_at, version.hash
                    );
                }
            }
        }
        AssetCommand::RestoreVersion { id, version } => {
            app.restore_asset_version(&id, &version)
                .map_err(CliError::from)?;
            print_action("Asset version restored.", json);
        }
        AssetCommand::Export { id, output } => {
            app.export_asset(&id, &output).map_err(CliError::from)?;
            print_action(&format!("Exported to {}", output.display()), json);
        }
        AssetCommand::Trash { id, folder, yes } => {
            let preview = app.trash_preview(&id, folder).map_err(CliError::from)?;
            confirm_action(
                &format!(
                    "move '{}' and {} files / {} folders to trash",
                    preview.name, preview.files, preview.folders
                ),
                yes,
                json,
            )?;
            let trash_id = app.trash_item(&id, folder).map_err(CliError::from)?;
            if json {
                print_json(&serde_json::json!({"ok":true,"trashId":trash_id,"preview":preview}))?;
            } else {
                println!(
                    "Moved {} to trash ({} files, {} folders).",
                    preview.name, preview.files, preview.folders
                );
            }
        }
        AssetCommand::SetCover { id, input } => {
            app.set_folder_cover(&id, &input).map_err(CliError::from)?;
            print_action("Cover image updated.", json);
        }
    }
    Ok(())
}

fn print_asset_board(board: &AssetBoard) {
    let mut folders: Vec<_> = board.folders.iter().collect();
    folders.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    let folder_path = |id: Option<&str>| -> String {
        let mut parts = Vec::new();
        let mut current = id;
        let mut steps = 0;
        while let Some(folder_id) = current {
            if let Some(folder) = board.folders.iter().find(|folder| folder.id == folder_id) {
                parts.push(folder.name.as_str());
                current = folder.parent_id.as_deref();
            } else {
                break;
            }
            steps += 1;
            if steps > board.folders.len() {
                break;
            }
        }
        parts.reverse();
        parts.join("/")
    };
    for folder in folders {
        let path = folder_path(Some(&folder.id));
        println!("{}/  [{}]", path, folder.kind);
    }
    let mut assets: Vec<_> = board.assets.iter().collect();
    assets.sort_by(|a, b| {
        let a_folder = board
            .placements
            .iter()
            .find(|p| p.asset_id == a.id)
            .and_then(|p| p.folder_id.as_deref());
        let b_folder = board
            .placements
            .iter()
            .find(|p| p.asset_id == b.id)
            .and_then(|p| p.folder_id.as_deref());
        folder_path(a_folder)
            .cmp(&folder_path(b_folder))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    for asset in assets {
        let placement = board.placements.iter().find(|p| p.asset_id == asset.id);
        let folder = folder_path(placement.and_then(|p| p.folder_id.as_deref()));
        let slot = placement.and_then(|p| p.slot.as_deref()).unwrap_or("—");
        println!(
            "{}/{}  ·  {}  ·  {} bytes  ·  {}",
            folder, asset.name, asset.id, asset.size, slot
        );
    }
}

fn run_trash_command(
    app: &ApplicationServices,
    action: TrashCommand,
    json: bool,
) -> Result<(), CliError> {
    match action {
        TrashCommand::List { project } => {
            let mut items = app.trash_items().map_err(CliError::from)?;
            if let Some(project) = project {
                items.retain(|item| item.project_id == project);
            }
            if json {
                print_json(&items)?;
            } else if items.is_empty() {
                println!("Trash is empty.");
            } else {
                for item in items {
                    println!(
                        "{}\t{}\t{}\t{} files · {} folders",
                        item.id, item.project_name, item.name, item.files, item.folders
                    );
                }
            }
        }
        TrashCommand::Restore { id } => {
            let name = app.restore_trash(&id).map_err(CliError::from)?;
            print_action(&format!("Restored {name}"), json);
        }
        TrashCommand::Purge { id, yes } => {
            let items = app.trash_items().map_err(CliError::from)?;
            let item = items
                .iter()
                .find(|item| item.id == id)
                .ok_or_else(|| CliError {
                    code: "not_found".into(),
                    message: "Trash item was not found.".into(),
                })?;
            confirm_action(
                &format!(
                    "permanently delete '{}' ({} files, {} folders)",
                    item.name, item.files, item.folders
                ),
                yes,
                json,
            )?;
            app.purge_trash(&id).map_err(CliError::from)?;
            print_action("Trash item permanently deleted.", json);
        }
    }
    Ok(())
}

fn run_vehicle_command(action: VehicleCommand, json: bool) -> Result<(), CliError> {
    match action {
        VehicleCommand::List {
            search,
            livery_support,
        } => {
            let catalogue = apex_core::vehicle_catalogue().map_err(CliError::from)?;
            let search = search.unwrap_or_default().trim().to_lowercase();
            let vehicles: Vec<&Vehicle> = catalogue
                .vehicles
                .iter()
                .filter(|vehicle| {
                    !livery_support
                        || vehicle.livery_support.eq_ignore_ascii_case("yes")
                        || vehicle.livery_support.eq_ignore_ascii_case("supported")
                })
                .filter(|vehicle| {
                    search.is_empty()
                        || vehicle.name.to_lowercase().contains(&search)
                        || vehicle.team.to_lowercase().contains(&search)
                })
                .collect();
            if json {
                print_json(&vehicles)?;
            } else if vehicles.is_empty() {
                println!("No vehicles found.");
            } else {
                for vehicle in vehicles {
                    println!(
                        "{}\t{}\t{}\t{}",
                        vehicle.id, vehicle.name, vehicle.team, vehicle.livery_support
                    );
                }
            }
        }
    }
    Ok(())
}

fn default_workspace_path() -> Option<PathBuf> {
    dirs::data_local_dir().map(|data| data.join(APP_IDENTIFIER).join("workspace"))
}

fn project_matches(project: &Project, query: &str) -> bool {
    project.fields.name.to_lowercase().contains(query)
        || project.fields.description.to_lowercase().contains(query)
        || project
            .fields
            .tags
            .iter()
            .any(|tag| tag.to_lowercase().contains(query))
        || project
            .fields
            .vehicles
            .iter()
            .any(|vehicle| vehicle.to_lowercase().contains(query))
}

fn print_projects(projects: &[Project], json: bool) {
    if json {
        match serde_json::to_string(projects) {
            Ok(value) => println!("{value}"),
            Err(error) => eprintln!("Could not encode project list as JSON: {error}"),
        }
        return;
    }
    if projects.is_empty() {
        println!("No projects found.");
        return;
    }
    for project in projects {
        let state = if project.archived {
            "archived"
        } else {
            "active"
        };
        println!(
            "{}\t{}\t{} assets",
            project.id, project.fields.name, project.asset_count
        );
        println!("  {state} · updated {}", project.updated_at);
    }
}

fn print_project(project: &Project, json: bool) {
    if json {
        match serde_json::to_string(project) {
            Ok(value) => println!("{value}"),
            Err(error) => eprintln!("Could not encode project as JSON: {error}"),
        }
        return;
    }
    println!("{}", project.fields.name);
    println!("ID: {}", project.id);
    println!("Created: {}", project.created_at);
    println!("Updated: {}", project.updated_at);
    println!(
        "Status: {}",
        if project.archived {
            "archived"
        } else {
            "active"
        }
    );
    println!("Assets: {}", project.asset_count);
    if !project.fields.description.is_empty() {
        println!("Description: {}", project.fields.description);
    }
    if !project.fields.tags.is_empty() {
        println!("Tags: {}", project.fields.tags.join(", "));
    }
    if !project.fields.vehicles.is_empty() {
        println!("Vehicles: {}", project.fields.vehicles.join(", "));
    }
    if !project.fields.reference_links.is_empty() {
        println!("Reference links:");
        for link in &project.fields.reference_links {
            println!("  {link}");
        }
    }
    if let Some(deadline) = &project.fields.deadline {
        println!("Deadline: {deadline}");
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    Home,
    Projects,
    Detail,
    Notes,
    NoteDetail,
    AssetProjects,
    Assets,
    Trash,
    Posts,
    PostDetail,
    PostEditor,
    PostMedia,
    PostLiveries,
    PostLiveryEditor,
    PostCodes,
    PostCodeEditor,
    PostRemoveReview,
    RobloxQueue,
    Statistics,
    GroupRevenue,
    RobloxCancelReview,
    RobloxLiveries,
    RobloxBatchReview,
}

#[derive(Clone, Debug, Default)]
struct RobloxBatchReviewState {
    livery_count: usize,
    texture_count: usize,
    reusable_ids: usize,
    upload_passes: usize,
    account_id: String,
    reupload: bool,
}

#[derive(Clone, Debug)]
struct PostEditorState {
    id: Option<String>,
    revision: u64,
    data: PublishingInput,
    tags_text: String,
    youtube_text: String,
    active_field: usize,
    cursor: usize,
    return_to_detail: bool,
    error: Option<String>,
}

#[derive(Clone, Debug, Default)]
struct PostMediaState {
    selected: usize,
    path_input: Option<String>,
    confirm_remove: bool,
    error: Option<String>,
}

#[derive(Clone, Debug)]
struct PostLiveryEditorState {
    livery_index: usize,
    is_new: bool,
    vehicle: String,
    primary_color: String,
    secondary_color: String,
    opacity: String,
    active_field: usize,
    cursor: usize,
    error: Option<String>,
}

#[derive(Clone, Debug)]
struct PostCodeEditorState {
    livery_index: usize,
    side_index: Option<usize>,
    side: String,
    decal_id: String,
    active_field: usize,
    cursor: usize,
    error: Option<String>,
}

#[derive(Clone, Copy, Debug)]
enum PostRemovalTarget {
    Livery(usize),
    Code {
        livery_index: usize,
        side_index: usize,
    },
}

impl PostEditorState {
    fn new(
        id: Option<String>,
        revision: u64,
        data: PublishingInput,
        return_to_detail: bool,
    ) -> Self {
        let tags_text = data.tags.join(", ");
        let youtube_text = data.youtube_url.clone().unwrap_or_default();
        let cursor = data.title.len();
        Self {
            id,
            revision,
            data,
            tags_text,
            youtube_text,
            active_field: 0,
            cursor,
            return_to_detail,
            error: None,
        }
    }
}

struct TuiState {
    screen: Screen,
    projects: Vec<Project>,
    selected: usize,
    search: String,
    searching: bool,
    selected_project: Option<String>,
    notes: Vec<Note>,
    selected_note: Option<String>,
    asset_board: Option<AssetBoard>,
    selected_asset_project: Option<String>,
    selected_asset: usize,
    trash: Vec<TrashItem>,
    posts: Vec<PostSummary>,
    post_import_path: Option<String>,
    post_import_error: Option<String>,
    selected_post: Option<String>,
    post_detail: Option<PublishingDraft>,
    post_handoff_message: Option<(String, bool)>,
    post_editor: Option<PostEditorState>,
    post_media: Option<PostMediaState>,
    post_livery_editor: Option<PostLiveryEditorState>,
    post_code_editor: Option<PostCodeEditorState>,
    selected_post_livery: usize,
    selected_post_code: usize,
    post_codes_livery: Option<usize>,
    pending_post_removal: Option<PostRemovalTarget>,
    post_removal_error: Option<String>,
    post_livery_error: Option<String>,
    upload_queue: Option<QueueSnapshot>,
    roblox_worker_active: bool,
    roblox_worker_status: String,
    statistics_profile: Option<String>,
    statistics_history: Vec<StatisticsSnapshot>,
    statistics_sales: Option<apex_application::SalesSnapshot>,
    statistics_refresh_active: bool,
    statistics_status: String,
    group_revenue: Option<RobloxGroupRevenueData>,
    selected_batch: usize,
    queue_batches_focused: bool,
    cancel_batch: Option<String>,
    roblox_account_id: Option<String>,
    roblox_for_post: bool,
    roblox_liveries: Vec<RobloxLiverySummary>,
    selected_roblox_liveries: HashSet<String>,
    roblox_reupload: bool,
    roblox_selection_error: Option<String>,
    roblox_batch_review: Option<RobloxBatchReviewState>,
    no_color: bool,
}

impl TuiState {
    fn new(projects: Vec<Project>) -> Self {
        Self {
            screen: Screen::Home,
            projects,
            selected: 0,
            search: String::new(),
            searching: false,
            selected_project: None,
            notes: Vec::new(),
            selected_note: None,
            asset_board: None,
            selected_asset_project: None,
            selected_asset: 0,
            trash: Vec::new(),
            posts: Vec::new(),
            post_import_path: None,
            post_import_error: None,
            selected_post: None,
            post_detail: None,
            post_handoff_message: None,
            post_editor: None,
            post_media: None,
            post_livery_editor: None,
            post_code_editor: None,
            selected_post_livery: 0,
            selected_post_code: 0,
            post_codes_livery: None,
            pending_post_removal: None,
            post_removal_error: None,
            post_livery_error: None,
            upload_queue: None,
            roblox_worker_active: false,
            roblox_worker_status: "Stopped".into(),
            statistics_profile: None,
            statistics_history: Vec::new(),
            statistics_sales: None,
            statistics_refresh_active: false,
            statistics_status: "Profile snapshot history".into(),
            group_revenue: None,
            selected_batch: 0,
            queue_batches_focused: true,
            cancel_batch: None,
            roblox_account_id: None,
            roblox_for_post: false,
            roblox_liveries: Vec::new(),
            selected_roblox_liveries: HashSet::new(),
            roblox_reupload: false,
            roblox_selection_error: None,
            roblox_batch_review: None,
            no_color: std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty()),
        }
    }

    fn visible_projects(&self) -> Vec<&Project> {
        let query = self.search.to_lowercase();
        self.projects
            .iter()
            .filter(|project| query.is_empty() || project_matches(project, &query))
            .collect()
    }

    fn visible_notes(&self) -> Vec<&Note> {
        let query = self.search.to_lowercase();
        self.notes
            .iter()
            .filter(|note| {
                query.is_empty()
                    || note.title.to_lowercase().contains(&query)
                    || note.markdown.to_lowercase().contains(&query)
            })
            .collect()
    }

    fn visible_trash(&self) -> Vec<&TrashItem> {
        let query = self.search.to_lowercase();
        self.trash
            .iter()
            .filter(|item| {
                query.is_empty()
                    || item.name.to_lowercase().contains(&query)
                    || item.project_name.to_lowercase().contains(&query)
            })
            .collect()
    }

    fn visible_posts(&self) -> Vec<&PostSummary> {
        let query = self.search.to_lowercase();
        self.posts
            .iter()
            .filter(|post| {
                query.is_empty()
                    || post.title.to_lowercase().contains(&query)
                    || post.id.to_lowercase().contains(&query)
            })
            .collect()
    }

    fn visible_upload_jobs(&self) -> Vec<&UploadJob> {
        let query = self.search.to_lowercase();
        self.upload_queue
            .iter()
            .flat_map(|queue| queue.jobs.iter())
            .filter(|job| {
                query.is_empty()
                    || job.name.to_lowercase().contains(&query)
                    || job.account_id.to_lowercase().contains(&query)
                    || job.state.to_lowercase().contains(&query)
                    || job.message.to_lowercase().contains(&query)
            })
            .collect()
    }

    fn visible_roblox_liveries(&self) -> Vec<&RobloxLiverySummary> {
        let query = self.search.to_lowercase();
        self.roblox_liveries
            .iter()
            .filter(|livery| {
                query.is_empty()
                    || livery.project_name.to_lowercase().contains(&query)
                    || livery.name.to_lowercase().contains(&query)
                    || livery.id.to_lowercase().contains(&query)
            })
            .collect()
    }

    fn visible_revenue_groups(&self) -> Vec<&apex_application::RobloxRevenueGroup> {
        let query = self.search.to_lowercase();
        self.group_revenue
            .iter()
            .flat_map(|data| data.groups.iter())
            .filter(|group| {
                query.is_empty()
                    || group.name.to_lowercase().contains(&query)
                    || group.group_id.to_lowercase().contains(&query)
            })
            .collect()
    }
}

enum TuiWorkerEvent {
    Snapshot(QueueSnapshot, usize),
    Finished(Result<QueueSnapshot, String>),
    StatisticsFinished(Result<Vec<StatisticsSnapshot>, String>),
}

fn start_tui_statistics_refresh(
    app: ApplicationServices,
    events: std::sync::mpsc::Sender<TuiWorkerEvent>,
) -> Result<std::thread::JoinHandle<()>, Error> {
    std::thread::Builder::new()
        .name("apex-erlcx-statistics-refresh".into())
        .spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| error.to_string())
                .and_then(|runtime| {
                    runtime
                        .block_on(app.refresh_statistics())
                        .map_err(|error| error.to_string())
                });
            let _ = events.send(TuiWorkerEvent::StatisticsFinished(result));
        })
        .map_err(|error| Error::Storage(format!("Could not start the statistics refresh: {error}")))
}

fn start_tui_roblox_worker(
    app: ApplicationServices,
    events: std::sync::mpsc::Sender<TuiWorkerEvent>,
) -> Result<
    (
        std::thread::JoinHandle<()>,
        std::sync::Arc<std::sync::atomic::AtomicBool>,
    ),
    Error,
> {
    let snapshot = app.upload_queue()?;
    let running = snapshot
        .batches
        .iter()
        .filter(|batch| batch.state == "running")
        .collect::<Vec<_>>();
    if running.is_empty() {
        return Err(Error::InvalidContent(
            "There are no running Roblox batches. Resume a batch or create a new one first.".into(),
        ));
    }
    let credentials = apex_application::load_roblox_credentials()?.ok_or_else(|| {
        Error::InvalidContent(
            "Configure and validate Roblox credentials before starting uploads.".into(),
        )
    })?;
    if let Some(batch) = running
        .iter()
        .find(|batch| batch.account_id != credentials.account_id)
    {
        return Err(Error::InvalidContent(format!(
            "Batch {} is pinned to another Roblox account. Change the account or pause that batch first.",
            batch.id
        )));
    }
    drop(credentials);

    let stopping = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let worker_stopping = stopping.clone();
    let worker = std::thread::Builder::new()
        .name("apex-roblox-cli-worker".into())
        .spawn(move || {
            let progress_events = events.clone();
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| error.to_string())
                .and_then(|runtime| {
                    runtime
                        .block_on(app.run_roblox_upload_queue(
                            worker_stopping,
                            move |snapshot, active| {
                                let _ = progress_events
                                    .send(TuiWorkerEvent::Snapshot(snapshot.clone(), active));
                            },
                        ))
                        .map_err(|error| error.to_string())
                });
            let _ = events.send(TuiWorkerEvent::Finished(result));
        })
        .map_err(|error| Error::Storage(format!("Could not start the Roblox worker: {error}")))?;
    Ok((worker, stopping))
}

fn run_tui(app: ApplicationServices) -> io::Result<()> {
    let projects = app.projects().map_err(io::Error::other)?;
    let mut state = TuiState::new(projects);
    let (worker_events_tx, worker_events_rx) = std::sync::mpsc::channel();
    let mut worker: Option<std::thread::JoinHandle<()>> = None;
    let mut worker_stop: Option<std::sync::Arc<std::sync::atomic::AtomicBool>> = None;
    let mut statistics_thread: Option<std::thread::JoinHandle<()>> = None;
    let mut quitting = false;
    ratatui::run(|terminal| 'tui: loop {
        while let Ok(event) = worker_events_rx.try_recv() {
            match event {
                TuiWorkerEvent::Snapshot(snapshot, active) => {
                    state.upload_queue = Some(snapshot);
                    state.roblox_worker_status = if worker_stop
                        .as_ref()
                        .is_some_and(|stopping| stopping.load(std::sync::atomic::Ordering::Relaxed))
                    {
                        "Stopping · finishing active requests".into()
                    } else {
                        format!("Running · {active} active requests")
                    };
                }
                TuiWorkerEvent::Finished(result) => {
                    let was_stopped = worker_stop.as_ref().is_some_and(|stopping| {
                        stopping.load(std::sync::atomic::Ordering::Relaxed)
                    });
                    state.roblox_worker_active = false;
                    worker_stop = None;
                    if let Some(handle) = worker.take() {
                        let _ = handle.join();
                    }
                    match result {
                        Ok(snapshot) => {
                            state.upload_queue = Some(snapshot);
                            state.roblox_worker_status = if quitting || was_stopped {
                                "Stopped · pending batches paused".into()
                            } else {
                                "Finished".into()
                            };
                        }
                        Err(error) => {
                            state.roblox_worker_status = format!("Worker error · {error}")
                        }
                    }
                }
                TuiWorkerEvent::StatisticsFinished(result) => {
                    state.statistics_refresh_active = false;
                    if let Some(handle) = statistics_thread.take() {
                        let _ = handle.join();
                    }
                    match result {
                        Ok(history) => {
                            state.statistics_history = history;
                            state.statistics_status = state.statistics_history.last().map_or_else(
                                || "No snapshots collected".into(),
                                |snapshot| format!("Updated · {}", snapshot.collected_at),
                            );
                        }
                        Err(error) => state.statistics_status = format!("Refresh failed · {error}"),
                    }
                }
            }
        }
        if quitting && !state.roblox_worker_active && !state.statistics_refresh_active {
            break 'tui Ok(());
        }
        terminal.draw(|frame| draw(frame, &state))?;
        if !event::poll(std::time::Duration::from_millis(200))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                    if let Some(stopping) = &worker_stop {
                        quitting = true;
                        state.roblox_worker_status = "Stopping · finishing active requests".into();
                        stopping.store(true, std::sync::atomic::Ordering::Relaxed);
                    } else if state.statistics_refresh_active {
                        quitting = true;
                        state.statistics_status = "Finishing profile refresh before exit".into();
                    } else {
                        break Ok(());
                    }
                    continue;
                }
                if quitting {
                    continue;
                }
                match state.screen {
                    Screen::Home => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc
                            if worker_stop.is_some() || state.statistics_refresh_active =>
                        {
                            quitting = true;
                            if let Some(stopping) = &worker_stop {
                                state.roblox_worker_status =
                                    "Stopping · finishing active requests".into();
                                stopping.store(true, std::sync::atomic::Ordering::Relaxed);
                            } else {
                                state.statistics_status =
                                    "Finishing profile refresh before exit".into();
                            }
                        }
                        KeyCode::Char('q') | KeyCode::Esc => break Ok(()),
                        KeyCode::Up | KeyCode::Char('k') => {
                            state.selected = state.selected.saturating_sub(1)
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            state.selected = (state.selected + 1).min(8)
                        }
                        KeyCode::Enter if state.selected == 0 => {
                            state.screen = Screen::Projects;
                            state.selected = 0;
                            refresh_projects(&mut state, &app).map_err(io::Error::other)?;
                        }
                        KeyCode::Enter if state.selected == 1 => {
                            state.screen = Screen::Notes;
                            state.selected = 0;
                            refresh_notes(&mut state, &app).map_err(io::Error::other)?;
                        }
                        KeyCode::Enter if state.selected == 2 => {
                            state.screen = Screen::AssetProjects;
                            state.selected = 0;
                            refresh_projects(&mut state, &app).map_err(io::Error::other)?;
                        }
                        KeyCode::Enter if state.selected == 3 => {
                            state.screen = Screen::Trash;
                            state.selected = 0;
                            refresh_trash(&mut state, &app).map_err(io::Error::other)?;
                        }
                        KeyCode::Enter if state.selected == 4 => {
                            state.screen = Screen::Posts;
                            state.selected = 0;
                            refresh_posts(&mut state, &app).map_err(io::Error::other)?;
                        }
                        KeyCode::Enter if state.selected == 5 => {
                            state.screen = Screen::RobloxQueue;
                            state.selected = 0;
                            refresh_upload_queue(&mut state, &app).map_err(io::Error::other)?;
                        }
                        KeyCode::Enter if state.selected == 6 => {
                            state.screen = Screen::Statistics;
                            state.selected = 0;
                            refresh_statistics_view(&mut state, &app).map_err(io::Error::other)?;
                        }
                        KeyCode::Enter if state.selected == 7 => {
                            state.screen = Screen::GroupRevenue;
                            state.selected = 0;
                            refresh_group_revenue_view(&mut state, &app)
                                .map_err(io::Error::other)?;
                        }
                        KeyCode::Enter => break Ok(()),
                        _ => {}
                    },
                    Screen::Projects if !state.searching && key.code == KeyCode::Char('r') => {
                        refresh_projects(&mut state, &app).map_err(io::Error::other)?;
                    }
                    Screen::Projects => handle_project_key(&mut state, key.code),
                    Screen::Notes if !state.searching && key.code == KeyCode::Char('r') => {
                        refresh_notes(&mut state, &app).map_err(io::Error::other)?;
                    }
                    Screen::Notes => handle_note_key(&mut state, key.code),
                    Screen::NoteDetail => match key.code {
                        KeyCode::Esc | KeyCode::Backspace => state.screen = Screen::Notes,
                        KeyCode::Char('q') => state.screen = Screen::Home,
                        _ => {}
                    },
                    Screen::AssetProjects => handle_asset_project_key(&mut state, key.code, &app)
                        .map_err(io::Error::other)?,
                    Screen::Assets => {
                        handle_asset_key(&mut state, key.code, &app).map_err(io::Error::other)?
                    }
                    Screen::Trash if !state.searching && key.code == KeyCode::Char('r') => {
                        refresh_trash(&mut state, &app).map_err(io::Error::other)?;
                    }
                    Screen::Trash => handle_trash_key(&mut state, key.code),
                    Screen::Posts if !state.searching && key.code == KeyCode::Char('r') => {
                        refresh_posts(&mut state, &app).map_err(io::Error::other)?;
                    }
                    Screen::Posts => {
                        handle_posts_key(&mut state, key.code, &app).map_err(io::Error::other)?
                    }
                    Screen::PostEditor
                        if key.code == KeyCode::Char('s')
                            && key.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        if let Err(error) = save_post_editor(&mut state, &app) {
                            if let Some(editor) = &mut state.post_editor {
                                editor.error = Some(error.to_string());
                            }
                        }
                    }
                    Screen::PostEditor => handle_post_editor_key(&mut state, key.code),
                    Screen::PostMedia => handle_post_media_key(&mut state, key.code, &app)
                        .map_err(io::Error::other)?,
                    Screen::PostLiveries => handle_post_liveries_key(&mut state, key.code, &app)
                        .map_err(io::Error::other)?,
                    Screen::PostLiveryEditor
                        if key.code == KeyCode::Char('s')
                            && key.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        if let Err(error) = save_post_livery_editor(&mut state, &app) {
                            if let Some(editor) = &mut state.post_livery_editor {
                                editor.error = Some(error.to_string());
                            }
                        }
                    }
                    Screen::PostLiveryEditor => handle_post_livery_editor_key(&mut state, key.code),
                    Screen::PostCodes => handle_post_codes_key(&mut state, key.code, &app)
                        .map_err(io::Error::other)?,
                    Screen::PostCodeEditor
                        if key.code == KeyCode::Char('s')
                            && key.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        if let Err(error) = save_post_code_editor(&mut state, &app) {
                            if let Some(editor) = &mut state.post_code_editor {
                                editor.error = Some(error.to_string());
                            }
                        }
                    }
                    Screen::PostCodeEditor => handle_post_code_editor_key(&mut state, key.code),
                    Screen::PostRemoveReview => {
                        handle_post_remove_review_key(&mut state, key.code, &app)
                            .map_err(io::Error::other)?
                    }
                    Screen::PostDetail => match key.code {
                        KeyCode::Esc | KeyCode::Backspace => state.screen = Screen::Posts,
                        KeyCode::Char('q') => state.screen = Screen::Home,
                        KeyCode::Char('m') => {
                            state.post_media = Some(PostMediaState::default());
                            state.screen = Screen::PostMedia;
                        }
                        KeyCode::Char('v') => {
                            state.selected_post_livery = 0;
                            state.screen = Screen::PostLiveries;
                        }
                        KeyCode::Char('l') => {
                            open_roblox_livery_selection(&mut state, &app, true)
                                .map_err(io::Error::other)?;
                        }
                        KeyCode::Char('h') | KeyCode::Char('H') => {
                            let reviewed = key.code == KeyCode::Char('H');
                            if let (Some(id), Some(draft)) =
                                (state.selected_post.clone(), state.post_detail.as_ref())
                            {
                                state.post_handoff_message = Some(match request_chrome_handoff(
                                    &id,
                                    draft.revision,
                                    reviewed,
                                ) {
                                    Ok(()) => ("Opened the ERLCX assistant in Chrome. Review the final page yourself; nothing will be submitted automatically.".into(), true),
                                    Err(error) => (error.to_string(), false),
                                });
                            }
                        }
                        KeyCode::Char('e') => {
                            if let Some(draft) = state.post_detail.clone() {
                                state.post_editor = Some(PostEditorState::new(
                                    state.selected_post.clone(),
                                    draft.revision,
                                    draft.data,
                                    true,
                                ));
                                state.screen = Screen::PostEditor;
                            }
                        }
                        _ => {}
                    },
                    Screen::RobloxQueue if !state.searching && key.code == KeyCode::Char('r') => {
                        refresh_upload_queue(&mut state, &app).map_err(io::Error::other)?;
                    }
                    Screen::RobloxQueue if !state.searching && key.code == KeyCode::Char('u') => {
                        open_roblox_livery_selection(&mut state, &app, false)
                            .map_err(io::Error::other)?;
                    }
                    Screen::RobloxQueue if !state.searching && key.code == KeyCode::Char('g') => {
                        if state.roblox_worker_active {
                            state.roblox_worker_status = "Worker is already running".into();
                        } else {
                            match start_tui_roblox_worker(app.clone(), worker_events_tx.clone()) {
                                Ok((handle, stopping)) => {
                                    worker = Some(handle);
                                    worker_stop = Some(stopping);
                                    state.roblox_worker_active = true;
                                    state.roblox_worker_status = "Starting".into();
                                }
                                Err(error) => {
                                    state.roblox_worker_status =
                                        format!("Could not start · {error}");
                                }
                            }
                        }
                    }
                    Screen::RobloxQueue if !state.searching && key.code == KeyCode::Char('x') => {
                        if let Some(stopping) = &worker_stop {
                            state.roblox_worker_status =
                                "Stopping · finishing active requests".into();
                            stopping.store(true, std::sync::atomic::Ordering::Relaxed);
                        } else {
                            state.roblox_worker_status = "Worker is not running".into();
                        }
                    }
                    Screen::RobloxQueue => {
                        let resume = key.code == KeyCode::Char('p')
                            && selected_upload_batch(&state)
                                .is_some_and(|(_, batch_state)| batch_state == "paused");
                        handle_queue_key(&mut state, key.code, &app).map_err(io::Error::other)?;
                        if resume && !state.roblox_worker_active {
                            match start_tui_roblox_worker(app.clone(), worker_events_tx.clone()) {
                                Ok((handle, stopping)) => {
                                    worker = Some(handle);
                                    worker_stop = Some(stopping);
                                    state.roblox_worker_active = true;
                                    state.roblox_worker_status = "Starting".into();
                                }
                                Err(error) => {
                                    state.roblox_worker_status =
                                        format!("Could not start · {error}");
                                }
                            }
                        }
                    }
                    Screen::Statistics if !state.searching && key.code == KeyCode::Char('r') => {
                        if !state.statistics_refresh_active {
                            match start_tui_statistics_refresh(
                                app.clone(),
                                worker_events_tx.clone(),
                            ) {
                                Ok(handle) => {
                                    statistics_thread = Some(handle);
                                    state.statistics_refresh_active = true;
                                    state.statistics_status = "Refreshing public profile…".into();
                                }
                                Err(error) => {
                                    state.statistics_status = format!("Refresh failed · {error}");
                                }
                            }
                        }
                    }
                    Screen::Statistics => match key.code {
                        KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('q') => {
                            state.screen = Screen::Home;
                        }
                        _ => {}
                    },
                    Screen::GroupRevenue if !state.searching && key.code == KeyCode::Char('r') => {
                        refresh_group_revenue_view(&mut state, &app).map_err(io::Error::other)?;
                    }
                    Screen::GroupRevenue => handle_group_revenue_key(&mut state, key.code),
                    Screen::RobloxCancelReview => {
                        handle_roblox_cancel_review_key(&mut state, key.code, &app)
                            .map_err(io::Error::other)?
                    }
                    Screen::RobloxLiveries => {
                        handle_roblox_liveries_key(&mut state, key.code, &app)
                            .map_err(io::Error::other)?
                    }
                    Screen::RobloxBatchReview => {
                        handle_roblox_batch_review_key(&mut state, key.code, &app)
                            .map_err(io::Error::other)?
                    }
                    Screen::Detail => match key.code {
                        KeyCode::Esc | KeyCode::Backspace => state.screen = Screen::Projects,
                        KeyCode::Char('q')
                            if worker_stop.is_some() || state.statistics_refresh_active =>
                        {
                            quitting = true;
                            if let Some(stopping) = &worker_stop {
                                state.roblox_worker_status =
                                    "Stopping · finishing active requests".into();
                                stopping.store(true, std::sync::atomic::Ordering::Relaxed);
                            } else {
                                state.statistics_status =
                                    "Finishing profile refresh before exit".into();
                            }
                        }
                        KeyCode::Char('q') => break Ok(()),
                        _ => {}
                    },
                }
            }
            Event::Resize(_, _) => {}
            _ => {}
        }
    })
}

fn refresh_projects(state: &mut TuiState, app: &ApplicationServices) -> Result<(), Error> {
    let selected_id = state
        .visible_projects()
        .get(state.selected)
        .map(|project| project.id.clone());
    state.projects = app.projects()?;
    let visible = state.visible_projects();
    state.selected = selected_id
        .and_then(|id| visible.iter().position(|project| project.id == id))
        .unwrap_or_else(|| state.selected.min(visible.len().saturating_sub(1)));
    Ok(())
}

fn refresh_notes(state: &mut TuiState, app: &ApplicationServices) -> Result<(), Error> {
    state.projects = app.projects()?;
    let mut notes = app.notes(None)?;
    for project in &state.projects {
        notes.extend(app.notes(Some(&project.id))?);
    }
    notes.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    state.notes = notes;
    state.selected = state
        .selected
        .min(state.visible_notes().len().saturating_sub(1));
    Ok(())
}

fn refresh_trash(state: &mut TuiState, app: &ApplicationServices) -> Result<(), Error> {
    state.trash = app.trash_items()?;
    state.selected = state
        .selected
        .min(state.visible_trash().len().saturating_sub(1));
    Ok(())
}

fn refresh_posts(state: &mut TuiState, app: &ApplicationServices) -> Result<(), Error> {
    let selected_id = state
        .visible_posts()
        .get(state.selected)
        .map(|post| post.id.clone());
    state.posts = app.posts()?;
    let visible = state.visible_posts();
    state.selected = selected_id
        .and_then(|id| visible.iter().position(|post| post.id == id))
        .unwrap_or_else(|| state.selected.min(visible.len().saturating_sub(1)));
    Ok(())
}

fn refresh_upload_queue(state: &mut TuiState, app: &ApplicationServices) -> Result<(), Error> {
    let selected_batch_id = selected_upload_batch(state).map(|(id, _)| id);
    let selected_job_id = state
        .visible_upload_jobs()
        .get(state.selected)
        .map(|job| job.id.clone());
    state.upload_queue = Some(app.upload_queue()?);
    let batch_count = state
        .upload_queue
        .as_ref()
        .map_or(0, |queue| queue.batches.len());
    state.selected_batch = selected_batch_id
        .and_then(|id| {
            state
                .upload_queue
                .as_ref()?
                .batches
                .iter()
                .position(|batch| batch.id == id)
        })
        .unwrap_or_else(|| state.selected_batch.min(batch_count.saturating_sub(1)));
    let visible_jobs = state.visible_upload_jobs();
    state.selected = selected_job_id
        .and_then(|id| visible_jobs.iter().position(|job| job.id == id))
        .unwrap_or_else(|| state.selected.min(visible_jobs.len().saturating_sub(1)));
    Ok(())
}

fn refresh_statistics_view(state: &mut TuiState, app: &ApplicationServices) -> Result<(), Error> {
    state.statistics_profile = app.statistics_profile()?;
    state.statistics_history = app.statistics_history()?;
    state.statistics_sales = app.statistics_sales()?;
    Ok(())
}

fn refresh_group_revenue_view(
    state: &mut TuiState,
    app: &ApplicationServices,
) -> Result<(), Error> {
    state.group_revenue = Some(app.roblox_group_revenue()?);
    state.selected = state
        .selected
        .min(state.visible_revenue_groups().len().saturating_sub(1));
    Ok(())
}

fn open_roblox_livery_selection(
    state: &mut TuiState,
    app: &ApplicationServices,
    for_post: bool,
) -> Result<(), Error> {
    let credentials = apex_application::load_roblox_credentials()?;
    state.roblox_account_id = credentials.as_ref().map(|value| value.account_id.clone());
    state.roblox_liveries = if let Some(account_id) = &state.roblox_account_id {
        app.roblox_liveries(account_id)?
    } else {
        Vec::new()
    };
    state.selected_roblox_liveries.clear();
    state.roblox_reupload = false;
    state.roblox_for_post = for_post;
    state.roblox_selection_error = None;
    state.roblox_batch_review = None;
    state.search.clear();
    state.selected = 0;
    state.searching = false;
    state.screen = Screen::RobloxLiveries;
    Ok(())
}

fn make_roblox_batch_review(
    state: &TuiState,
    app: &ApplicationServices,
) -> Result<RobloxBatchReviewState, Error> {
    let account_id = state
        .roblox_account_id
        .as_deref()
        .ok_or_else(|| Error::InvalidContent("Configure Roblox credentials first.".into()))?;
    let selected = state
        .roblox_liveries
        .iter()
        .filter(|livery| state.selected_roblox_liveries.contains(&livery.id))
        .collect::<Vec<_>>();
    let mut texture_count = 0usize;
    let mut reusable_ids = 0usize;
    for livery in &selected {
        let uploads = app.livery_uploads(&livery.id, account_id)?;
        for texture in uploads.textures {
            texture_count += 1;
            if texture.upload.as_ref().is_some_and(|upload| {
                upload.state == "uploaded"
                    && upload.asset_id.is_some()
                    && upload.moderation != "rejected"
            }) {
                reusable_ids += 1;
            }
        }
    }
    let uploads = if state.roblox_reupload {
        texture_count
    } else {
        texture_count.saturating_sub(reusable_ids)
    };
    let double_upload = app.roblox_double_upload()?;
    Ok(RobloxBatchReviewState {
        livery_count: selected.len(),
        texture_count,
        reusable_ids,
        upload_passes: uploads * if double_upload { 2 } else { 1 },
        account_id: account_id.to_owned(),
        reupload: state.roblox_reupload,
    })
}

fn handle_roblox_liveries_key(
    state: &mut TuiState,
    key: KeyCode,
    app: &ApplicationServices,
) -> Result<(), Error> {
    if state.searching {
        match key {
            KeyCode::Esc | KeyCode::Enter => state.searching = false,
            KeyCode::Backspace => {
                state.search.pop();
                state.selected = 0;
            }
            KeyCode::Char(value) if !value.is_control() => {
                state.search.push(value);
                state.selected = 0;
            }
            _ => {}
        }
        return Ok(());
    }
    match key {
        KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('q') => {
            state.screen = if state.roblox_for_post {
                Screen::PostDetail
            } else {
                Screen::RobloxQueue
            };
            state.search.clear();
            state.selected = 0;
            state.roblox_for_post = false;
        }
        KeyCode::Char('/') => state.searching = true,
        KeyCode::Up | KeyCode::Char('k') => state.selected = state.selected.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => {
            state.selected =
                (state.selected + 1).min(state.visible_roblox_liveries().len().saturating_sub(1))
        }
        KeyCode::Char(' ') => {
            if let Some(id) = state
                .visible_roblox_liveries()
                .get(state.selected)
                .map(|livery| livery.id.clone())
            {
                if !state.selected_roblox_liveries.insert(id.clone()) {
                    state.selected_roblox_liveries.remove(&id);
                }
                state.roblox_selection_error = None;
            }
        }
        KeyCode::Char('a') => {
            let visible_ids = state
                .visible_roblox_liveries()
                .into_iter()
                .map(|livery| livery.id.clone())
                .collect::<Vec<_>>();
            state.selected_roblox_liveries.extend(visible_ids);
            state.roblox_selection_error = None;
        }
        KeyCode::Char('c') => {
            state.selected_roblox_liveries.clear();
            state.roblox_selection_error = None;
        }
        KeyCode::Char('r') if !state.roblox_for_post => {
            state.roblox_reupload = !state.roblox_reupload
        }
        KeyCode::Enter if !state.selected_roblox_liveries.is_empty() => {
            if state.roblox_for_post {
                match append_selected_liveries_to_post(state, app) {
                    Ok(()) => state.roblox_selection_error = None,
                    Err(error) => state.roblox_selection_error = Some(error.to_string()),
                }
            } else {
                match make_roblox_batch_review(state, app) {
                    Ok(review) => {
                        state.roblox_batch_review = Some(review);
                        state.roblox_selection_error = None;
                        state.screen = Screen::RobloxBatchReview;
                    }
                    Err(error) => state.roblox_selection_error = Some(error.to_string()),
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn append_selected_liveries_to_post(
    state: &mut TuiState,
    app: &ApplicationServices,
) -> Result<(), Error> {
    let account_id = state
        .roblox_account_id
        .as_deref()
        .ok_or_else(|| Error::InvalidContent("Configure Roblox credentials first.".into()))?;
    let post_id = state
        .selected_post
        .as_deref()
        .ok_or_else(|| Error::InvalidContent("Open an ERLCX draft first.".into()))?;
    let mut draft = state
        .post_detail
        .clone()
        .ok_or_else(|| Error::InvalidContent("Open an ERLCX draft first.".into()))?;
    let mut added = false;
    for livery in state
        .roblox_liveries
        .iter()
        .filter(|livery| state.selected_roblox_liveries.contains(&livery.id))
    {
        if draft.data.liveries.iter().any(|existing| {
            existing.source.as_ref().is_some_and(|source| {
                source.folder_id == livery.id && source.account_id == account_id
            })
        }) {
            continue;
        }
        let uploads = app.livery_uploads(&livery.id, account_id)?;
        let source = PublishingSource {
            folder_id: livery.id.clone(),
            account_id: account_id.to_owned(),
            hashes: uploads
                .textures
                .iter()
                .map(|texture| texture.hash.clone())
                .collect(),
        };
        let sides = uploads
            .textures
            .iter()
            .map(|texture| PublishingSide {
                side: texture.side.clone(),
                decal_id: texture
                    .upload
                    .as_ref()
                    .filter(|upload| upload.state == "uploaded" && upload.moderation != "rejected")
                    .and_then(|upload| upload.asset_id.clone())
                    .unwrap_or_default(),
            })
            .collect();
        draft.data.liveries.push(PublishingLivery {
            source: Some(source),
            vehicle: uploads.vehicle,
            primary_color: "#FFFFFF".into(),
            secondary_color: None,
            opacity: 100,
            sides,
        });
        added = true;
    }
    if !added {
        state.roblox_for_post = false;
        state.screen = Screen::PostDetail;
        return Ok(());
    }
    draft.data.validate(false)?;
    let saved = app.save_post(post_id, draft.revision, draft.data)?;
    let _ = app.release_post_image_pins(post_id);
    state.post_detail = Some(saved);
    state.roblox_for_post = false;
    state.screen = Screen::PostDetail;
    refresh_posts(state, app)?;
    Ok(())
}

fn handle_roblox_batch_review_key(
    state: &mut TuiState,
    key: KeyCode,
    app: &ApplicationServices,
) -> Result<(), Error> {
    match key {
        KeyCode::Esc | KeyCode::Backspace => state.screen = Screen::RobloxLiveries,
        KeyCode::Char('r') => {
            state.roblox_reupload = !state.roblox_reupload;
            state.roblox_batch_review = Some(make_roblox_batch_review(state, app)?);
        }
        KeyCode::Char('y')
            if state
                .roblox_batch_review
                .as_ref()
                .is_some_and(|review| review.upload_passes > 0) =>
        {
            let review = state
                .roblox_batch_review
                .as_ref()
                .ok_or_else(|| Error::InvalidContent("Review the selected batch first.".into()))?;
            let livery_ids = state
                .roblox_liveries
                .iter()
                .filter(|livery| state.selected_roblox_liveries.contains(&livery.id))
                .map(|livery| livery.id.clone())
                .collect::<Vec<_>>();
            let request_id = format!(
                "tui-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|duration| duration.as_nanos())
                    .unwrap_or_default()
            );
            app.create_roblox_batch(
                &request_id,
                &review.account_id,
                &livery_ids,
                review.reupload,
            )?;
            state.screen = Screen::RobloxQueue;
            state.search.clear();
            state.selected = 0;
            refresh_upload_queue(state, app)?;
        }
        _ => {}
    }
    Ok(())
}

fn handle_posts_key(
    state: &mut TuiState,
    key: KeyCode,
    app: &ApplicationServices,
) -> Result<(), Error> {
    if let Some(path) = &mut state.post_import_path {
        match key {
            KeyCode::Esc => {
                state.post_import_path = None;
                state.post_import_error = None;
            }
            KeyCode::Enter => {
                let source = PathBuf::from(path.trim());
                match app.import_post_bundle(&source) {
                    Ok(id) => {
                        state.post_import_path = None;
                        state.post_import_error = None;
                        state.selected_post = Some(id.clone());
                        refresh_posts(state, app)?;
                        state.selected = state
                            .visible_posts()
                            .iter()
                            .position(|post| post.id == id)
                            .unwrap_or(state.selected);
                        state.post_detail = Some(app.post(&id)?);
                        state.selected_post_livery = 0;
                        state.screen = Screen::PostDetail;
                    }
                    Err(error) => state.post_import_error = Some(error.to_string()),
                }
            }
            KeyCode::Backspace => {
                path.pop();
                state.post_import_error = None;
            }
            KeyCode::Char(value) if !value.is_control() => {
                path.push(value);
                state.post_import_error = None;
            }
            _ => {}
        }
        return Ok(());
    }
    if state.searching {
        match key {
            KeyCode::Esc | KeyCode::Enter => state.searching = false,
            KeyCode::Backspace => {
                state.search.pop();
                state.selected = 0;
            }
            KeyCode::Char(value) if !value.is_control() => {
                state.search.push(value);
                state.selected = 0;
            }
            _ => {}
        }
        return Ok(());
    }
    match key {
        KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('q') => {
            state.screen = Screen::Home;
            state.search.clear();
            state.selected = 0;
        }
        KeyCode::Char('/') => state.searching = true,
        KeyCode::Char('n') => {
            state.post_editor = Some(PostEditorState::new(
                None,
                0,
                PublishingInput::default(),
                false,
            ));
            state.screen = Screen::PostEditor;
        }
        KeyCode::Char('i') => {
            state.post_import_path = Some(String::new());
            state.post_import_error = None;
        }
        KeyCode::Up | KeyCode::Char('k') => state.selected = state.selected.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => {
            state.selected = (state.selected + 1).min(state.visible_posts().len().saturating_sub(1))
        }
        KeyCode::Enter => {
            state.selected_post = state
                .visible_posts()
                .get(state.selected)
                .map(|post| post.id.clone());
            if let Some(id) = state.selected_post.as_deref() {
                state.post_detail = Some(app.post(id)?);
                state.screen = Screen::PostDetail;
            }
        }
        _ => {}
    }
    Ok(())
}

fn post_editor_value(editor: &PostEditorState) -> &str {
    match editor.active_field {
        0 => &editor.data.title,
        1 => &editor.data.description,
        2 => &editor.tags_text,
        _ => &editor.youtube_text,
    }
}

fn post_editor_value_mut(editor: &mut PostEditorState) -> &mut String {
    match editor.active_field {
        0 => &mut editor.data.title,
        1 => &mut editor.data.description,
        2 => &mut editor.tags_text,
        _ => &mut editor.youtube_text,
    }
}

fn handle_post_editor_key(state: &mut TuiState, key: KeyCode) {
    let Some(editor) = &mut state.post_editor else {
        state.screen = Screen::Posts;
        return;
    };
    match key {
        KeyCode::Esc => {
            let return_to_detail = editor.return_to_detail;
            state.post_editor = None;
            state.screen = if return_to_detail {
                Screen::PostDetail
            } else {
                Screen::Posts
            };
        }
        KeyCode::Tab => {
            editor.active_field = (editor.active_field + 1) % 4;
            editor.cursor = post_editor_value(editor).len();
            editor.error = None;
        }
        KeyCode::Enter if editor.active_field == 1 => {
            let cursor = editor.cursor;
            post_editor_value_mut(editor).insert(cursor, '\n');
            editor.cursor += 1;
            editor.error = None;
        }
        KeyCode::Enter => {
            editor.active_field = (editor.active_field + 1) % 4;
            editor.cursor = post_editor_value(editor).len();
            editor.error = None;
        }
        KeyCode::Left => {
            let value = post_editor_value(editor);
            editor.cursor = value[..editor.cursor]
                .char_indices()
                .next_back()
                .map_or(0, |(index, _)| index);
        }
        KeyCode::Right => {
            let value = post_editor_value(editor);
            if let Some(character) = value[editor.cursor..].chars().next() {
                editor.cursor += character.len_utf8();
            }
        }
        KeyCode::Home => editor.cursor = 0,
        KeyCode::End => editor.cursor = post_editor_value(editor).len(),
        KeyCode::Backspace if editor.cursor > 0 => {
            let value = post_editor_value(editor);
            let start = value[..editor.cursor]
                .char_indices()
                .next_back()
                .map_or(0, |(index, _)| index);
            let end = editor.cursor;
            post_editor_value_mut(editor).replace_range(start..end, "");
            editor.cursor = start;
            editor.error = None;
        }
        KeyCode::Char(character) if !character.is_control() => {
            let cursor = editor.cursor;
            post_editor_value_mut(editor).insert(cursor, character);
            editor.cursor += character.len_utf8();
            editor.error = None;
        }
        _ => {}
    }
}

fn save_post_editor(state: &mut TuiState, app: &ApplicationServices) -> Result<(), Error> {
    let editor = state
        .post_editor
        .as_ref()
        .ok_or_else(|| Error::InvalidContent("No ERLCX draft is open for editing.".into()))?;
    let mut data = editor.data.clone();
    data.tags = editor
        .tags_text
        .split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(str::to_owned)
        .collect();
    let youtube = editor.youtube_text.trim();
    data.youtube_url = (!youtube.is_empty()).then(|| youtube.to_owned());
    data.validate(false)?;

    let (id, revision) = if let Some(id) = editor.id.as_deref() {
        (id.to_owned(), editor.revision)
    } else {
        let id = app.create_post()?;
        let revision = app.post(&id)?.revision;
        (id, revision)
    };
    let saved = app.save_post(&id, revision, data)?;
    let _ = app.release_post_image_pins(&id);
    state.selected_post = Some(id);
    state.post_detail = Some(saved);
    state.post_editor = None;
    state.screen = Screen::PostDetail;
    refresh_posts(state, app)?;
    Ok(())
}

fn save_post_media_snapshot(
    state: &mut TuiState,
    app: &ApplicationServices,
    data: PublishingInput,
) -> Result<(), Error> {
    data.validate(false)?;
    let id = state
        .selected_post
        .as_deref()
        .ok_or_else(|| Error::InvalidContent("Open an ERLCX draft first.".into()))?;
    let revision = state
        .post_detail
        .as_ref()
        .ok_or_else(|| Error::InvalidContent("Open an ERLCX draft first.".into()))?
        .revision;
    let saved = app.save_post(id, revision, data)?;
    let _ = app.release_post_image_pins(id);
    state.post_detail = Some(saved);
    refresh_posts(state, app)?;
    Ok(())
}

fn import_post_media(
    state: &mut TuiState,
    app: &ApplicationServices,
    path: PathBuf,
) -> Result<(), Error> {
    let id = state
        .selected_post
        .clone()
        .ok_or_else(|| Error::InvalidContent("Open an ERLCX draft first.".into()))?;
    let draft = state
        .post_detail
        .clone()
        .ok_or_else(|| Error::InvalidContent("Open an ERLCX draft first.".into()))?;
    if draft.data.media.len() >= 10 {
        return Err(Error::InvalidContent(
            "This draft already has the maximum of ten preview images.".into(),
        ));
    }
    let previews = app.add_post_images(&id, &[path])?;
    let mut data = draft.data;
    data.media
        .extend(previews.into_iter().map(|preview| PublishingImage {
            hash: preview.hash,
            name: preview.name,
        }));
    if let Err(error) = save_post_media_snapshot(state, app, data) {
        let _ = app.release_post_image_pins(&id);
        return Err(error);
    }
    if let Some(media) = &mut state.post_media {
        media.selected = state
            .post_detail
            .as_ref()
            .map_or(0, |saved| saved.data.media.len().saturating_sub(1));
        media.error = None;
    }
    Ok(())
}

fn handle_post_media_key(
    state: &mut TuiState,
    key: KeyCode,
    app: &ApplicationServices,
) -> Result<(), Error> {
    if state.post_media.is_none() {
        state.post_media = Some(PostMediaState::default());
    }
    if state
        .post_media
        .as_ref()
        .is_some_and(|media| media.path_input.is_some())
    {
        match key {
            KeyCode::Esc => {
                if let Some(media) = &mut state.post_media {
                    media.path_input = None;
                    media.error = None;
                }
            }
            KeyCode::Enter => {
                let path = state
                    .post_media
                    .as_mut()
                    .and_then(|media| media.path_input.take())
                    .unwrap_or_default();
                let path = path.trim().trim_matches('"').trim().to_owned();
                if path.is_empty() {
                    if let Some(media) = &mut state.post_media {
                        media.error = Some("Enter a PNG file path.".into());
                    }
                } else if let Err(error) = import_post_media(state, app, PathBuf::from(path)) {
                    if let Some(media) = &mut state.post_media {
                        media.error = Some(error.to_string());
                    }
                }
            }
            KeyCode::Backspace => {
                if let Some(path) = state
                    .post_media
                    .as_mut()
                    .and_then(|media| media.path_input.as_mut())
                {
                    path.pop();
                }
            }
            KeyCode::Char(value) if !value.is_control() => {
                if let Some(path) = state
                    .post_media
                    .as_mut()
                    .and_then(|media| media.path_input.as_mut())
                {
                    path.push(value);
                }
            }
            _ => {}
        }
        return Ok(());
    }
    if state
        .post_media
        .as_ref()
        .is_some_and(|media| media.confirm_remove)
    {
        match key {
            KeyCode::Esc | KeyCode::Char('n') => {
                if let Some(media) = &mut state.post_media {
                    media.confirm_remove = false;
                }
            }
            KeyCode::Char('y') => {
                let selected = state.post_media.as_ref().map_or(0, |media| media.selected);
                if let Some(mut draft) = state.post_detail.clone() {
                    if selected < draft.data.media.len() {
                        draft.data.media.remove(selected);
                        match save_post_media_snapshot(state, app, draft.data) {
                            Ok(()) => {
                                let new_count = state
                                    .post_detail
                                    .as_ref()
                                    .map_or(0, |saved| saved.data.media.len());
                                if let Some(media) = &mut state.post_media {
                                    media.selected = selected.min(new_count.saturating_sub(1));
                                    media.confirm_remove = false;
                                    media.error = None;
                                }
                            }
                            Err(error) => {
                                if let Some(media) = &mut state.post_media {
                                    media.error = Some(error.to_string());
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        return Ok(());
    }

    let count = state
        .post_detail
        .as_ref()
        .map_or(0, |draft| draft.data.media.len());
    match key {
        KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('q') => {
            state.post_media = None;
            state.screen = Screen::PostDetail;
        }
        KeyCode::Char('a') => {
            if let Some(media) = &mut state.post_media {
                media.path_input = Some(String::new());
                media.error = None;
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(media) = &mut state.post_media {
                media.selected = media.selected.saturating_sub(1);
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(media) = &mut state.post_media {
                media.selected = (media.selected + 1).min(count.saturating_sub(1));
            }
        }
        KeyCode::Char('d') if count > 0 => {
            if let Some(media) = &mut state.post_media {
                media.confirm_remove = true;
            }
        }
        KeyCode::Char('[') | KeyCode::Char(']') if count > 1 => {
            let selected = state.post_media.as_ref().map_or(0, |media| media.selected);
            let target = if key == KeyCode::Char('[') {
                selected.checked_sub(1)
            } else if selected + 1 < count {
                Some(selected + 1)
            } else {
                None
            };
            if let (Some(target), Some(mut draft)) = (target, state.post_detail.clone()) {
                draft.data.media.swap(selected, target);
                match save_post_media_snapshot(state, app, draft.data) {
                    Ok(()) => {
                        if let Some(media) = &mut state.post_media {
                            media.selected = target;
                            media.error = None;
                        }
                    }
                    Err(error) => {
                        if let Some(media) = &mut state.post_media {
                            media.error = Some(error.to_string());
                        }
                    }
                }
            }
        }
        KeyCode::Char('r') => {
            if let Some(id) = state.selected_post.as_deref() {
                match app.post(id) {
                    Ok(draft) => {
                        let count = draft.data.media.len();
                        state.post_detail = Some(draft);
                        if let Some(media) = &mut state.post_media {
                            media.selected = media.selected.min(count.saturating_sub(1));
                            media.error = None;
                        }
                    }
                    Err(error) => {
                        if let Some(media) = &mut state.post_media {
                            media.error = Some(error.to_string());
                        }
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn save_post_snapshot_data(
    state: &mut TuiState,
    app: &ApplicationServices,
    data: PublishingInput,
) -> Result<(), Error> {
    data.validate(false)?;
    let id = state
        .selected_post
        .as_deref()
        .ok_or_else(|| Error::InvalidContent("Open an ERLCX draft first.".into()))?;
    let revision = state
        .post_detail
        .as_ref()
        .ok_or_else(|| Error::InvalidContent("Open an ERLCX draft first.".into()))?
        .revision;
    let saved = app.save_post(id, revision, data)?;
    let _ = app.release_post_image_pins(id);
    state.post_detail = Some(saved);
    refresh_posts(state, app)?;
    Ok(())
}

fn handle_post_liveries_key(
    state: &mut TuiState,
    key: KeyCode,
    app: &ApplicationServices,
) -> Result<(), Error> {
    let count = state
        .post_detail
        .as_ref()
        .map_or(0, |draft| draft.data.liveries.len());
    match key {
        KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('q') => {
            state.screen = Screen::PostDetail;
            state.post_livery_error = None;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.selected_post_livery = state.selected_post_livery.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.selected_post_livery =
                (state.selected_post_livery + 1).min(count.saturating_sub(1));
        }
        KeyCode::Char('a') if count < 20 => {
            state.post_livery_editor = Some(PostLiveryEditorState {
                livery_index: count,
                is_new: true,
                vehicle: String::new(),
                primary_color: "#FFFFFF".into(),
                secondary_color: String::new(),
                opacity: "100".into(),
                active_field: 0,
                cursor: 0,
                error: None,
            });
            state.screen = Screen::PostLiveryEditor;
        }
        KeyCode::Enter | KeyCode::Char('c') if count > 0 => {
            state.post_codes_livery = Some(state.selected_post_livery);
            state.selected_post_code = 0;
            state.post_livery_error = None;
            state.screen = Screen::PostCodes;
        }
        KeyCode::Char('e') if count > 0 => {
            if let Some(livery) = state
                .post_detail
                .as_ref()
                .and_then(|draft| draft.data.liveries.get(state.selected_post_livery))
            {
                state.post_livery_editor = Some(PostLiveryEditorState {
                    livery_index: state.selected_post_livery,
                    is_new: false,
                    vehicle: livery.vehicle.clone(),
                    primary_color: livery.primary_color.clone(),
                    secondary_color: livery.secondary_color.clone().unwrap_or_default(),
                    opacity: livery.opacity.to_string(),
                    active_field: 0,
                    cursor: livery.vehicle.len(),
                    error: None,
                });
                state.screen = Screen::PostLiveryEditor;
            }
        }
        KeyCode::Char('d') if count > 0 => {
            state.pending_post_removal =
                Some(PostRemovalTarget::Livery(state.selected_post_livery));
            state.post_removal_error = None;
            state.screen = Screen::PostRemoveReview;
        }
        KeyCode::Char('[') | KeyCode::Char(']') if count > 1 => {
            let selected = state.selected_post_livery;
            let target = if key == KeyCode::Char('[') {
                selected.checked_sub(1)
            } else if selected + 1 < count {
                Some(selected + 1)
            } else {
                None
            };
            if let (Some(target), Some(mut draft)) = (target, state.post_detail.clone()) {
                draft.data.liveries.swap(selected, target);
                match save_post_snapshot_data(state, app, draft.data) {
                    Ok(()) => {
                        state.selected_post_livery = target;
                        state.post_livery_error = None;
                    }
                    Err(error) => state.post_livery_error = Some(error.to_string()),
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn handle_post_codes_key(
    state: &mut TuiState,
    key: KeyCode,
    app: &ApplicationServices,
) -> Result<(), Error> {
    let livery_index = state.post_codes_livery.unwrap_or_default();
    let sides = state
        .post_detail
        .as_ref()
        .and_then(|draft| draft.data.liveries.get(livery_index))
        .map_or(0, |livery| livery.sides.len());
    match key {
        KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('q') => {
            state.screen = Screen::PostLiveries;
            state.post_livery_error = None;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.selected_post_code = state.selected_post_code.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.selected_post_code = (state.selected_post_code + 1).min(sides.saturating_sub(1));
        }
        KeyCode::Char('a') => {
            state.post_code_editor = Some(PostCodeEditorState {
                livery_index,
                side_index: None,
                side: String::new(),
                decal_id: String::new(),
                active_field: 0,
                cursor: 0,
                error: None,
            });
            state.screen = Screen::PostCodeEditor;
        }
        KeyCode::Char('e') if sides > 0 => {
            if let Some(side) = state
                .post_detail
                .as_ref()
                .and_then(|draft| draft.data.liveries.get(livery_index))
                .and_then(|livery| livery.sides.get(state.selected_post_code))
            {
                state.post_code_editor = Some(PostCodeEditorState {
                    livery_index,
                    side_index: Some(state.selected_post_code),
                    side: side.side.clone(),
                    decal_id: side.decal_id.clone(),
                    active_field: 0,
                    cursor: side.side.len(),
                    error: None,
                });
                state.screen = Screen::PostCodeEditor;
            }
        }
        KeyCode::Char('d') if sides > 0 => {
            state.pending_post_removal = Some(PostRemovalTarget::Code {
                livery_index,
                side_index: state.selected_post_code,
            });
            state.post_removal_error = None;
            state.screen = Screen::PostRemoveReview;
        }
        KeyCode::Char('[') | KeyCode::Char(']') if sides > 1 => {
            let selected = state.selected_post_code;
            let target = if key == KeyCode::Char('[') {
                selected.checked_sub(1)
            } else if selected + 1 < sides {
                Some(selected + 1)
            } else {
                None
            };
            if let (Some(target), Some(mut draft)) = (target, state.post_detail.clone()) {
                let livery = &mut draft.data.liveries[livery_index];
                livery.sides.swap(selected, target);
                if let Some(source) = &mut livery.source {
                    if selected < source.hashes.len() && target < source.hashes.len() {
                        source.hashes.swap(selected, target);
                    }
                }
                match save_post_snapshot_data(state, app, draft.data) {
                    Ok(()) => {
                        state.selected_post_code = target;
                        state.post_livery_error = None;
                    }
                    Err(error) => state.post_livery_error = Some(error.to_string()),
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn post_code_editor_value(editor: &PostCodeEditorState) -> &str {
    if editor.active_field == 0 {
        &editor.side
    } else {
        &editor.decal_id
    }
}

fn post_livery_editor_value(editor: &PostLiveryEditorState) -> &str {
    match editor.active_field {
        0 => &editor.vehicle,
        1 => &editor.primary_color,
        2 => &editor.secondary_color,
        _ => &editor.opacity,
    }
}

fn edit_text_at_cursor(value: &mut String, cursor: &mut usize, key: KeyCode) -> bool {
    match key {
        KeyCode::Left => {
            *cursor = value[..*cursor]
                .char_indices()
                .next_back()
                .map_or(0, |(index, _)| index);
            true
        }
        KeyCode::Right => {
            if let Some(character) = value[*cursor..].chars().next() {
                *cursor += character.len_utf8();
            }
            true
        }
        KeyCode::Home => {
            *cursor = 0;
            true
        }
        KeyCode::End => {
            *cursor = value.len();
            true
        }
        KeyCode::Backspace if *cursor > 0 => {
            let start = value[..*cursor]
                .char_indices()
                .next_back()
                .map_or(0, |(index, _)| index);
            value.replace_range(start..*cursor, "");
            *cursor = start;
            true
        }
        KeyCode::Char(character) if !character.is_control() => {
            value.insert(*cursor, character);
            *cursor += character.len_utf8();
            true
        }
        _ => false,
    }
}

fn handle_post_code_editor_key(state: &mut TuiState, key: KeyCode) {
    let Some(editor) = &mut state.post_code_editor else {
        state.screen = Screen::PostCodes;
        return;
    };
    match key {
        KeyCode::Esc => {
            state.post_code_editor = None;
            state.screen = Screen::PostCodes;
        }
        KeyCode::Tab | KeyCode::Enter => {
            editor.active_field = (editor.active_field + 1) % 2;
            editor.cursor = post_code_editor_value(editor).len();
            editor.error = None;
        }
        _ => {
            let cursor = &mut editor.cursor;
            let value = if editor.active_field == 0 {
                &mut editor.side
            } else {
                &mut editor.decal_id
            };
            if edit_text_at_cursor(value, cursor, key) {
                editor.error = None;
            }
        }
    }
}

fn save_post_code_editor(state: &mut TuiState, app: &ApplicationServices) -> Result<(), Error> {
    let editor = state
        .post_code_editor
        .clone()
        .ok_or_else(|| Error::InvalidContent("No side code is open for editing.".into()))?;
    let mut draft = state
        .post_detail
        .clone()
        .ok_or_else(|| Error::InvalidContent("Open an ERLCX draft first.".into()))?;
    let livery = draft
        .data
        .liveries
        .get_mut(editor.livery_index)
        .ok_or(Error::NotFound)?;
    let side = PublishingSide {
        side: editor.side,
        decal_id: editor.decal_id,
    };
    let selected = if let Some(index) = editor.side_index {
        let target = livery.sides.get_mut(index).ok_or(Error::NotFound)?;
        *target = side;
        index
    } else {
        livery.sides.push(side);
        livery.sides.len() - 1
    };
    save_post_snapshot_data(state, app, draft.data)?;
    state.selected_post_code = selected;
    state.post_code_editor = None;
    state.screen = Screen::PostCodes;
    state.post_livery_error = None;
    Ok(())
}

fn handle_post_livery_editor_key(state: &mut TuiState, key: KeyCode) {
    let Some(editor) = &mut state.post_livery_editor else {
        state.screen = Screen::PostLiveries;
        return;
    };
    match key {
        KeyCode::Esc => {
            state.post_livery_editor = None;
            state.screen = Screen::PostLiveries;
        }
        KeyCode::Tab | KeyCode::Enter => {
            editor.active_field = (editor.active_field + 1) % 4;
            editor.cursor = post_livery_editor_value(editor).len();
            editor.error = None;
        }
        _ => {
            let cursor = &mut editor.cursor;
            let value = match editor.active_field {
                0 => &mut editor.vehicle,
                1 => &mut editor.primary_color,
                2 => &mut editor.secondary_color,
                _ => &mut editor.opacity,
            };
            if edit_text_at_cursor(value, cursor, key) {
                editor.error = None;
            }
        }
    }
}

fn save_post_livery_editor(state: &mut TuiState, app: &ApplicationServices) -> Result<(), Error> {
    let editor = state
        .post_livery_editor
        .clone()
        .ok_or_else(|| Error::InvalidContent("No livery is open for editing.".into()))?;
    let mut draft = state
        .post_detail
        .clone()
        .ok_or_else(|| Error::InvalidContent("Open an ERLCX draft first.".into()))?;
    let opacity = editor
        .opacity
        .parse::<u8>()
        .map_err(|_| Error::InvalidContent("Opacity must be a number from 0 to 100.".into()))?;
    let secondary_color = (!editor.secondary_color.trim().is_empty())
        .then(|| editor.secondary_color.trim().to_owned());
    if editor.is_new {
        if editor.livery_index != draft.data.liveries.len() || draft.data.liveries.len() >= 20 {
            return Err(Error::Conflict);
        }
        draft.data.liveries.push(PublishingLivery {
            source: None,
            vehicle: editor.vehicle,
            primary_color: editor.primary_color,
            secondary_color,
            opacity,
            sides: Vec::new(),
        });
    } else {
        let livery = draft
            .data
            .liveries
            .get_mut(editor.livery_index)
            .ok_or(Error::NotFound)?;
        livery.vehicle = editor.vehicle;
        livery.primary_color = editor.primary_color;
        livery.secondary_color = secondary_color;
        livery.opacity = opacity;
    }
    save_post_snapshot_data(state, app, draft.data)?;
    if editor.is_new {
        state.selected_post_livery = editor.livery_index;
    }
    state.post_livery_editor = None;
    state.screen = Screen::PostLiveries;
    state.post_livery_error = None;
    Ok(())
}

fn handle_post_remove_review_key(
    state: &mut TuiState,
    key: KeyCode,
    app: &ApplicationServices,
) -> Result<(), Error> {
    match key {
        KeyCode::Esc | KeyCode::Char('n') => {
            let target = state.pending_post_removal.take();
            state.post_removal_error = None;
            state.screen = match target {
                Some(PostRemovalTarget::Code { .. }) => Screen::PostCodes,
                Some(PostRemovalTarget::Livery(_)) => Screen::PostLiveries,
                None => Screen::PostDetail,
            };
        }
        KeyCode::Char('y') => {
            let Some(target) = state.pending_post_removal else {
                state.screen = Screen::PostDetail;
                return Ok(());
            };
            let mut draft = state
                .post_detail
                .clone()
                .ok_or_else(|| Error::InvalidContent("Open an ERLCX draft first.".into()))?;
            let destination = match target {
                PostRemovalTarget::Livery(index) => {
                    if index >= draft.data.liveries.len() {
                        return Err(Error::NotFound);
                    }
                    draft.data.liveries.remove(index);
                    state.selected_post_livery =
                        index.min(draft.data.liveries.len().saturating_sub(1));
                    Screen::PostLiveries
                }
                PostRemovalTarget::Code {
                    livery_index,
                    side_index,
                } => {
                    let livery = draft
                        .data
                        .liveries
                        .get_mut(livery_index)
                        .ok_or(Error::NotFound)?;
                    if side_index >= livery.sides.len() {
                        return Err(Error::NotFound);
                    }
                    livery.sides.remove(side_index);
                    state.selected_post_code = side_index.min(livery.sides.len().saturating_sub(1));
                    Screen::PostCodes
                }
            };
            match save_post_snapshot_data(state, app, draft.data) {
                Ok(()) => {
                    state.pending_post_removal = None;
                    state.post_removal_error = None;
                    state.screen = destination;
                }
                Err(error) => state.post_removal_error = Some(error.to_string()),
            }
        }
        _ => {}
    }
    Ok(())
}

fn handle_queue_key(
    state: &mut TuiState,
    key: KeyCode,
    app: &ApplicationServices,
) -> Result<(), Error> {
    if state.searching {
        match key {
            KeyCode::Esc | KeyCode::Enter => state.searching = false,
            KeyCode::Backspace => {
                state.search.pop();
                state.selected = 0;
            }
            KeyCode::Char(value) if !value.is_control() => {
                state.search.push(value);
                state.selected = 0;
            }
            _ => {}
        }
        return Ok(());
    }
    match key {
        KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('q') => {
            state.screen = Screen::Home;
            state.search.clear();
            state.selected = 0;
        }
        KeyCode::Char('/') => state.searching = true,
        KeyCode::Tab => state.queue_batches_focused = !state.queue_batches_focused,
        KeyCode::Up | KeyCode::Char('k') if state.queue_batches_focused => {
            state.selected_batch = state.selected_batch.saturating_sub(1)
        }
        KeyCode::Down | KeyCode::Char('j') if state.queue_batches_focused => {
            let count = state
                .upload_queue
                .as_ref()
                .map_or(0, |queue| queue.batches.len());
            state.selected_batch = (state.selected_batch + 1).min(count.saturating_sub(1));
        }
        KeyCode::Up | KeyCode::Char('k') => state.selected = state.selected.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => {
            state.selected =
                (state.selected + 1).min(state.visible_upload_jobs().len().saturating_sub(1))
        }
        KeyCode::Char('p') => {
            if let Some((batch_id, batch_state)) = selected_upload_batch(state) {
                let action = if batch_state == "paused" {
                    "resume"
                } else {
                    "pause"
                };
                app.control_roblox_batch(Some(&batch_id), action)?;
                refresh_upload_queue(state, app)?;
            }
        }
        KeyCode::Char('t') => {
            if let Some((batch_id, _)) = selected_upload_batch(state) {
                app.control_roblox_batch(Some(&batch_id), "retry")?;
                refresh_upload_queue(state, app)?;
            }
        }
        KeyCode::Char('c') => {
            state.cancel_batch = selected_upload_batch(state).map(|(batch_id, _)| batch_id);
            if state.cancel_batch.is_some() {
                state.screen = Screen::RobloxCancelReview;
            }
        }
        _ => {}
    }
    Ok(())
}

fn handle_group_revenue_key(state: &mut TuiState, key: KeyCode) {
    if state.searching {
        match key {
            KeyCode::Esc | KeyCode::Enter => state.searching = false,
            KeyCode::Backspace => {
                state.search.pop();
                state.selected = 0;
            }
            KeyCode::Char(value) if !value.is_control() => {
                state.search.push(value);
                state.selected = 0;
            }
            _ => {}
        }
        return;
    }
    match key {
        KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('q') => {
            state.screen = Screen::Home;
            state.search.clear();
            state.selected = 0;
        }
        KeyCode::Char('/') => state.searching = true,
        KeyCode::Up | KeyCode::Char('k') => state.selected = state.selected.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => {
            state.selected =
                (state.selected + 1).min(state.visible_revenue_groups().len().saturating_sub(1));
        }
        _ => {}
    }
}

fn selected_upload_batch(state: &TuiState) -> Option<(String, String)> {
    let batch = state
        .upload_queue
        .as_ref()?
        .batches
        .get(state.selected_batch)?;
    Some((batch.id.clone(), batch.state.clone()))
}

fn handle_roblox_cancel_review_key(
    state: &mut TuiState,
    key: KeyCode,
    app: &ApplicationServices,
) -> Result<(), Error> {
    match key {
        KeyCode::Esc | KeyCode::Char('n') => {
            state.cancel_batch = None;
            state.screen = Screen::RobloxQueue;
        }
        KeyCode::Char('y') => {
            if let Some(batch_id) = state.cancel_batch.take() {
                app.control_roblox_batch(Some(&batch_id), "cancel")?;
                refresh_upload_queue(state, app)?;
            }
            state.screen = Screen::RobloxQueue;
        }
        _ => {}
    }
    Ok(())
}

fn handle_project_key(state: &mut TuiState, key: KeyCode) {
    if state.searching {
        match key {
            KeyCode::Esc | KeyCode::Enter => state.searching = false,
            KeyCode::Backspace => {
                state.search.pop();
                state.selected = 0;
            }
            KeyCode::Char(value) if !value.is_control() => {
                state.search.push(value);
                state.selected = 0;
            }
            _ => {}
        }
        return;
    }
    match key {
        KeyCode::Esc | KeyCode::Backspace => {
            state.screen = Screen::Home;
            state.search.clear();
            state.selected = 0;
        }
        KeyCode::Char('q') => state.screen = Screen::Home,
        KeyCode::Char('/') => state.searching = true,
        KeyCode::Up | KeyCode::Char('k') => {
            state.selected = state.selected.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let max = state.visible_projects().len().saturating_sub(1);
            state.selected = (state.selected + 1).min(max);
        }
        KeyCode::Enter => {
            state.selected_project = state
                .visible_projects()
                .get(state.selected)
                .map(|project| project.id.clone());
            if state.selected_project.is_some() {
                state.screen = Screen::Detail;
            }
        }
        _ => {}
    }
}

fn handle_note_key(state: &mut TuiState, key: KeyCode) {
    if state.searching {
        match key {
            KeyCode::Esc | KeyCode::Enter => state.searching = false,
            KeyCode::Backspace => {
                state.search.pop();
                state.selected = 0;
            }
            KeyCode::Char(value) if !value.is_control() => {
                state.search.push(value);
                state.selected = 0;
            }
            _ => {}
        }
        return;
    }
    match key {
        KeyCode::Esc | KeyCode::Backspace => {
            state.screen = Screen::Home;
            state.search.clear();
            state.selected = 0;
        }
        KeyCode::Char('q') => state.screen = Screen::Home,
        KeyCode::Char('/') => state.searching = true,
        KeyCode::Char('r') => state.selected = 0,
        KeyCode::Up | KeyCode::Char('k') => state.selected = state.selected.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => {
            state.selected =
                (state.selected + 1).min(state.visible_notes().len().saturating_sub(1));
        }
        KeyCode::Enter => {
            state.selected_note = state
                .visible_notes()
                .get(state.selected)
                .map(|note| note.id.clone());
            if state.selected_note.is_some() {
                state.screen = Screen::NoteDetail;
            }
        }
        _ => {}
    }
}

fn handle_asset_project_key(
    state: &mut TuiState,
    key: KeyCode,
    app: &ApplicationServices,
) -> Result<(), Error> {
    if state.searching {
        match key {
            KeyCode::Esc | KeyCode::Enter => state.searching = false,
            KeyCode::Backspace => {
                state.search.pop();
                state.selected = 0;
            }
            KeyCode::Char(value) if !value.is_control() => {
                state.search.push(value);
                state.selected = 0;
            }
            _ => {}
        }
        return Ok(());
    }
    match key {
        KeyCode::Esc | KeyCode::Backspace => {
            state.screen = Screen::Home;
            state.search.clear();
            state.selected = 0;
        }
        KeyCode::Char('q') => state.screen = Screen::Home,
        KeyCode::Char('/') => state.searching = true,
        KeyCode::Up | KeyCode::Char('k') => state.selected = state.selected.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => {
            state.selected =
                (state.selected + 1).min(state.visible_projects().len().saturating_sub(1))
        }
        KeyCode::Enter => {
            let project_id = state
                .visible_projects()
                .get(state.selected)
                .map(|project| project.id.clone());
            if let Some(project_id) = project_id {
                state.asset_board = Some(app.asset_board(&project_id)?);
                state.selected_asset_project = Some(project_id);
                state.selected_asset = 0;
                state.search.clear();
                state.screen = Screen::Assets;
            }
        }
        _ => {}
    }
    Ok(())
}

fn handle_asset_key(
    state: &mut TuiState,
    key: KeyCode,
    app: &ApplicationServices,
) -> Result<(), Error> {
    if state.searching {
        match key {
            KeyCode::Esc | KeyCode::Enter => state.searching = false,
            KeyCode::Backspace => {
                state.search.pop();
                state.selected_asset = 0;
            }
            KeyCode::Char(value) if !value.is_control() => {
                state.search.push(value);
                state.selected_asset = 0;
            }
            _ => {}
        }
        return Ok(());
    }
    match key {
        KeyCode::Esc | KeyCode::Backspace => {
            state.screen = Screen::AssetProjects;
            state.search.clear();
            state.selected = 0;
        }
        KeyCode::Char('q') => state.screen = Screen::Home,
        KeyCode::Char('/') => state.searching = true,
        KeyCode::Char('r') => {
            if let Some(project_id) = state.selected_asset_project.as_deref() {
                state.asset_board = Some(app.asset_board(project_id)?);
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.selected_asset = state.selected_asset.saturating_sub(1)
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.selected_asset = (state.selected_asset + 1).min(
                asset_rows(state.asset_board.as_ref())
                    .len()
                    .saturating_sub(1),
            );
        }
        _ => {}
    }
    Ok(())
}

fn handle_trash_key(state: &mut TuiState, key: KeyCode) {
    if state.searching {
        match key {
            KeyCode::Esc | KeyCode::Enter => state.searching = false,
            KeyCode::Backspace => {
                state.search.pop();
                state.selected = 0;
            }
            KeyCode::Char(value) if !value.is_control() => {
                state.search.push(value);
                state.selected = 0;
            }
            _ => {}
        }
        return;
    }
    match key {
        KeyCode::Esc | KeyCode::Backspace => {
            state.screen = Screen::Home;
            state.search.clear();
            state.selected = 0;
        }
        KeyCode::Char('q') => state.screen = Screen::Home,
        KeyCode::Char('/') => state.searching = true,
        KeyCode::Up | KeyCode::Char('k') => state.selected = state.selected.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => {
            state.selected = (state.selected + 1).min(state.visible_trash().len().saturating_sub(1))
        }
        _ => {}
    }
}

fn asset_rows(board: Option<&AssetBoard>) -> Vec<(String, String)> {
    let Some(board) = board else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    let folder_path = |folder_id: Option<&str>| -> String {
        let mut parts = Vec::new();
        let mut current = folder_id;
        let mut visited = 0usize;
        while let Some(id) = current {
            if let Some(folder) = board.folders.iter().find(|folder| folder.id == id) {
                parts.push(folder.name.as_str());
                current = folder.parent_id.as_deref();
            } else {
                break;
            }
            visited += 1;
            if visited > board.folders.len() {
                break;
            }
        }
        parts.reverse();
        parts.join("/")
    };
    for folder in &board.folders {
        rows.push((
            format!("{}/", folder_path(Some(&folder.id))),
            format!(
                "{} · {}",
                folder.kind,
                folder.vehicle_id.as_deref().unwrap_or("folder")
            ),
        ));
    }
    for asset in &board.assets {
        let placement = board
            .placements
            .iter()
            .find(|placement| placement.asset_id == asset.id);
        let path = folder_path(placement.and_then(|placement| placement.folder_id.as_deref()));
        rows.push((
            format!("{}/{}", path, asset.name),
            format!(
                "{} bytes · {}",
                asset.size,
                placement
                    .and_then(|placement| placement.slot.as_deref())
                    .unwrap_or("unassigned")
            ),
        ));
    }
    rows.sort_by(|left, right| left.0.to_lowercase().cmp(&right.0.to_lowercase()));
    rows
}

fn draw(frame: &mut Frame<'_>, state: &TuiState) {
    let area = frame.area();
    let regions = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(
                if area.width < 64
                    && matches!(
                        state.screen,
                        Screen::RobloxQueue
                            | Screen::RobloxLiveries
                            | Screen::PostMedia
                            | Screen::PostLiveries
                            | Screen::PostCodes
                    )
                {
                    3
                } else {
                    2
                },
            ),
        ])
        .split(area);
    let accent = if state.no_color {
        Color::Reset
    } else {
        Color::Rgb(112, 104, 255)
    };
    let muted = if state.no_color {
        Color::Reset
    } else {
        Color::DarkGray
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " APEX ",
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled("STUDIO", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled("  ·  Workspace CLI", Style::default().fg(muted)),
        ]))
        .block(Block::default().borders(Borders::ALL)),
        regions[0],
    );
    match state.screen {
        Screen::Home => draw_home(frame, state, regions[1], accent, muted),
        Screen::Projects => draw_projects(frame, state, regions[1], accent, muted),
        Screen::Detail => draw_detail(frame, state, regions[1], accent, muted),
        Screen::Notes => draw_notes(frame, state, regions[1], accent, muted),
        Screen::NoteDetail => draw_note_detail(frame, state, regions[1], accent, muted),
        Screen::AssetProjects => draw_asset_projects(frame, state, regions[1], accent, muted),
        Screen::Assets => draw_assets(frame, state, regions[1], accent, muted),
        Screen::Trash => draw_trash(frame, state, regions[1], accent, muted),
        Screen::Posts => draw_posts(frame, state, regions[1], accent, muted),
        Screen::PostDetail => draw_post_detail(frame, state, regions[1], accent, muted),
        Screen::PostEditor => draw_post_editor(frame, state, regions[1], accent, muted),
        Screen::PostMedia => draw_post_media(frame, state, regions[1], accent, muted),
        Screen::PostLiveries => draw_post_liveries(frame, state, regions[1], accent, muted),
        Screen::PostLiveryEditor => {
            draw_post_livery_editor(frame, state, regions[1], accent, muted)
        }
        Screen::PostCodes => draw_post_codes(frame, state, regions[1], accent, muted),
        Screen::PostCodeEditor => draw_post_code_editor(frame, state, regions[1], accent, muted),
        Screen::PostRemoveReview => draw_post_remove_review(frame, state, regions[1], muted),
        Screen::RobloxQueue => draw_roblox_queue(frame, state, regions[1], accent, muted),
        Screen::Statistics => draw_statistics(frame, state, regions[1], accent, muted),
        Screen::GroupRevenue => draw_group_revenue(frame, state, regions[1], accent, muted),
        Screen::RobloxCancelReview => {
            draw_roblox_cancel_review(frame, state, regions[1], accent, muted)
        }
        Screen::RobloxLiveries => draw_roblox_liveries(frame, state, regions[1], accent, muted),
        Screen::RobloxBatchReview => {
            draw_roblox_batch_review(frame, state, regions[1], accent, muted)
        }
    }
    let footer = match state.screen {
        Screen::Home => " ↑/↓ Select   Enter Open   q Quit ",
        Screen::Projects if state.searching => " Type to filter   Enter/Esc Finish search ",
        Screen::Projects => {
            " ↑/↓ Select   Enter Details   / Search   r Refresh   Esc Back   q Back "
        }
        Screen::Detail => " Esc Back   q Home ",
        Screen::Notes if state.searching => " Type to filter   Enter/Esc Finish search ",
        Screen::Notes => " ↑/↓ Select   Enter Open   / Search   r Refresh   Esc Back ",
        Screen::NoteDetail => " Esc Back   q Home ",
        Screen::AssetProjects if state.searching => " Type to filter   Enter/Esc Finish search ",
        Screen::AssetProjects => " ↑/↓ Select   Enter Browse assets   / Search   Esc Back ",
        Screen::Assets if state.searching => " Type to filter   Enter/Esc Finish search ",
        Screen::Assets => " ↑/↓ Select   / Search   r Refresh   Esc Projects ",
        Screen::Trash if state.searching => " Type to filter   Enter/Esc Finish search ",
        Screen::Trash => " ↑/↓ Select   / Search   r Refresh   Esc Back ",
        Screen::Posts if state.searching => " Type to filter   Enter/Esc Finish search ",
        Screen::Posts if state.post_import_path.is_some() => {
            "Type/paste .apexpublish path   Enter Import   Esc Cancel"
        }
        Screen::Posts => " ↑/↓ Select   Enter Open   n New draft   i Import bundle   / Search   r Refresh   Esc Back ",
        Screen::PostDetail => " e Metadata   m Images   v Liveries/codes   l Add project livery   h Prepare   H Review saved codes & prepare   Esc Back ",
        Screen::PostEditor => " Tab Next field   ←/→ Edit   Enter Next/new line   Ctrl+S Save   Esc Discard ",
        Screen::PostMedia if state.post_media.as_ref().is_some_and(|media| media.path_input.is_some()) => {
            "Type/paste path   Enter Import PNG   Esc Cancel path"
        }
        Screen::PostMedia if state.post_media.as_ref().is_some_and(|media| media.confirm_remove) => {
            " y Remove from this draft   n/Esc Keep image "
        }
        Screen::PostMedia if regions[2].width < 64 => {
            "↑↓ Select · a Import · d Remove\n[ ] Reorder · r Refresh · Esc Back"
        }
        Screen::PostMedia => {
            " ↑/↓ Select   a Import PNG   d Remove (confirm)   [ / ] Reorder   r Refresh   Esc Back "
        }
        Screen::PostLiveries if regions[2].width < 64 => {
            "↑↓ Select · a Add · e Edit · c Codes\n[ ] Reorder · d Remove · Esc Back"
        }
        Screen::PostLiveries => {
            " ↑/↓ Select   a Add manual livery   e Edit vehicle/colors   c Codes   [ / ] Reorder   d Remove   Esc Back "
        }
        Screen::PostLiveryEditor => {
            " Tab/Enter Next field   ←/→ Edit   Ctrl+S Save   Esc Discard "
        }
        Screen::PostCodes if regions[2].width < 64 => {
            "↑↓ Select · a Add · e Edit · d Remove\n[ ] Reorder · Esc Back"
        }
        Screen::PostCodes => {
            " ↑/↓ Select   a Add code   e Edit   [ / ] Reorder   d Remove   Esc Back "
        }
        Screen::PostCodeEditor => {
            " Tab/Enter Next field   ←/→ Edit   Ctrl+S Save   Esc Discard "
        }
        Screen::PostRemoveReview => " y Remove from draft   n/Esc Keep ",
        Screen::Statistics => " r Refresh public ERLCX profile   Esc/q Back ",
        Screen::GroupRevenue if state.searching => " Type to filter   Enter/Esc Finish search ",
        Screen::GroupRevenue => " ↑/↓ Select group   / Search   r Refresh   Esc/q Back ",
        Screen::RobloxQueue if state.searching => " Type to filter   Enter/Esc Finish search ",
        Screen::RobloxQueue if regions[2].width < 64 => {
            "Tab Focus · ↑↓ Select · g Run · x Stop\np Pause · t Retry · c Cancel · u Upload\n/ Search · r Refresh · Esc Back"
        }
        Screen::RobloxQueue if regions[2].width < 96 => {
            "Tab Lists · ↑↓ Select · g Run · x Stop · p Pause\nt Retry · c Cancel · u Upload · r Refresh · / Search"
        }
        Screen::RobloxQueue => {
            " Tab Focus list   g Run worker   x Stop worker   p Pause/Resume batch   t Retry   c Cancel   u New   / Search   r Refresh "
        }
        Screen::RobloxCancelReview if regions[2].width < 64 => {
            "y Confirm cancel · n/Esc Keep"
        }
        Screen::RobloxCancelReview => " y Confirm cancel   n/Esc Keep batch ",
        Screen::RobloxLiveries if state.searching => " Type to filter   Enter/Esc Finish search ",
        Screen::RobloxLiveries if state.roblox_for_post && regions[2].width < 64 => {
            "↑↓ Move · Space Select · a All\nEnter Add to draft · Esc Back"
        }
        Screen::RobloxLiveries if state.roblox_for_post && regions[2].width < 96 => {
            "↑↓ Select · Space Toggle · a Filtered · c Clear\nEnter Add to draft · / Search · Esc Back"
        }
        Screen::RobloxLiveries if state.roblox_for_post => {
            " ↑/↓ Select   Space Toggle   a Select filtered   c Clear   Enter Add to draft   / Search   Esc Back "
        }
        Screen::RobloxLiveries if regions[2].width < 64 => {
            "↑↓ Move · Space Select · a All\nc Clear · r Re-upload · Enter Review\n/ Search · Esc Back"
        }
        Screen::RobloxLiveries if regions[2].width < 96 => {
            "↑↓ Select · Space Toggle · a Filtered · c Clear · r Re-upload\nEnter Review · / Search · Esc Back"
        }
        Screen::RobloxLiveries => {
            " ↑/↓ Select   Space Toggle   a Select filtered   c Clear   r Re-upload   Enter Review "
        }
        Screen::RobloxBatchReview if regions[2].width < 64 => "y Start batch · r Re-upload · Esc Back",
        Screen::RobloxBatchReview => " y Start confirmed batch   r Toggle re-upload   Esc Back ",
    };
    frame.render_widget(
        Paragraph::new(footer)
            .style(Style::default().fg(muted))
            .alignment(ratatui::layout::Alignment::Center)
            .wrap(Wrap { trim: true }),
        regions[2],
    );
}

fn draw_home(
    frame: &mut Frame<'_>,
    state: &TuiState,
    area: ratatui::layout::Rect,
    accent: Color,
    muted: Color,
) {
    let sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);
    let items = vec![
        ListItem::new("Projects"),
        ListItem::new("Notes"),
        ListItem::new("Assets"),
        ListItem::new("Trash"),
        ListItem::new("ERLCX drafts"),
        ListItem::new("Roblox uploads"),
        ListItem::new("Statistics"),
        ListItem::new("Group Revenue"),
        ListItem::new("Quit"),
    ];
    let mut list_state = ListState::default().with_selected(Some(state.selected.min(8)));
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::default().title("Menu").borders(Borders::ALL))
            .highlight_symbol("▶ ")
            .highlight_style(Style::default().fg(accent).add_modifier(Modifier::BOLD)),
        sections[0],
        &mut list_state,
    );
    frame.render_widget(
        Paragraph::new(match state.selected.min(8) {
            0 => "Browse, search, and inspect project records.",
            1 => "Browse and read project or general notes.",
            2 => "Inspect project folders, liveries, and files.",
            3 => "Browse deleted project assets.",
            4 => "Browse global ERLCX post drafts and their saved snapshots.",
            5 => "Inspect Roblox upload batches and texture processing status.",
            6 => "Review public ERLCX post metrics and collected snapshots.",
            7 => "Review imported Roblox group sales and refresh coverage.",
            _ => "Close the terminal workspace.",
        })
        .block(Block::default().title("Apex Studio").borders(Borders::ALL))
        .style(Style::default().fg(muted))
        .wrap(Wrap { trim: true }),
        sections[1],
    );
}

fn draw_projects(
    frame: &mut Frame<'_>,
    state: &TuiState,
    area: ratatui::layout::Rect,
    accent: Color,
    muted: Color,
) {
    let (list_area, detail_area) = if area.width < 88 {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
            .split(area);
        (rows[0], rows[1])
    } else {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(area);
        (columns[0], columns[1])
    };
    let visible = state.visible_projects();
    let query_label = if state.searching {
        "Search (editing)"
    } else {
        "Search"
    };
    let search = if state.search.is_empty() {
        "Type / to search"
    } else {
        state.search.as_str()
    };
    let title = format!("Projects · {}", visible.len());
    let items = visible
        .iter()
        .map(|project| {
            let status = if project.archived { " [archived]" } else { "" };
            ListItem::new(format!("{}{}", project.fields.name, status))
        })
        .collect::<Vec<_>>();
    let mut list_state = ListState::default()
        .with_selected((!visible.is_empty()).then(|| state.selected.min(visible.len() - 1)));
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::default().title(title).borders(Borders::ALL))
            .highlight_symbol("▶ ")
            .highlight_style(Style::default().fg(accent).add_modifier(Modifier::BOLD)),
        list_area,
        &mut list_state,
    );
    let detail = visible.get(state.selected.min(visible.len().saturating_sub(1)));
    let detail_text = if let Some(project) = detail {
        format!(
            "{}\n\n{}\n\n{} assets\nCreated {}\nUpdated {}\n\nTags: {}\nVehicles: {}",
            project.fields.name,
            if project.fields.description.is_empty() {
                "No description"
            } else {
                &project.fields.description
            },
            project.asset_count,
            project.created_at,
            project.updated_at,
            if project.fields.tags.is_empty() {
                "—".into()
            } else {
                project.fields.tags.join(", ")
            },
            if project.fields.vehicles.is_empty() {
                "—".into()
            } else {
                project.fields.vehicles.join(", ")
            },
        )
    } else if state.projects.is_empty() {
        "No projects yet.".to_owned()
    } else {
        "No projects match this search.".to_owned()
    };
    let detail_lines = vec![
        Line::styled(query_label, Style::default().fg(muted)),
        Line::raw(search),
        Line::raw(""),
        Line::raw(detail_text),
    ];
    frame.render_widget(
        Paragraph::new(detail_lines)
            .block(
                Block::default()
                    .title("Project details")
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: true }),
        detail_area,
    );
}

fn draw_detail(
    frame: &mut Frame<'_>,
    state: &TuiState,
    area: ratatui::layout::Rect,
    accent: Color,
    muted: Color,
) {
    let project = state
        .selected_project
        .as_deref()
        .and_then(|id| state.projects.iter().find(|project| project.id == id));
    let contents = if let Some(project) = project {
        vec![
            Line::styled(
                &project.fields.name,
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
            Line::raw(format!("ID: {}", project.id)),
            Line::raw(format!(
                "Status: {}",
                if project.archived {
                    "Archived"
                } else {
                    "Active"
                }
            )),
            Line::raw(format!("Created: {}", project.created_at)),
            Line::raw(format!("Updated: {}", project.updated_at)),
            Line::raw(format!("Assets: {}", project.asset_count)),
            Line::raw(format!(
                "Deadline: {}",
                project.fields.deadline.as_deref().unwrap_or("—")
            )),
            Line::raw(""),
            Line::raw(if project.fields.description.is_empty() {
                "No description"
            } else {
                &project.fields.description
            }),
            Line::raw(""),
            Line::raw(format!(
                "Tags: {}",
                if project.fields.tags.is_empty() {
                    "—".into()
                } else {
                    project.fields.tags.join(", ")
                }
            )),
            Line::raw(format!(
                "Vehicles: {}",
                if project.fields.vehicles.is_empty() {
                    "—".into()
                } else {
                    project.fields.vehicles.join(", ")
                }
            )),
            Line::raw(format!(
                "Reference links: {}",
                project.fields.reference_links.len()
            )),
        ]
    } else {
        vec![Line::raw("Project is no longer available.")]
    };
    frame.render_widget(
        Paragraph::new(contents)
            .block(Block::default().title("Project").borders(Borders::ALL))
            .style(Style::default().fg(muted))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn draw_notes(
    frame: &mut Frame<'_>,
    state: &TuiState,
    area: ratatui::layout::Rect,
    accent: Color,
    muted: Color,
) {
    let (list_area, detail_area) = if area.width < 88 {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
            .split(area);
        (rows[0], rows[1])
    } else {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);
        (columns[0], columns[1])
    };
    let visible = state.visible_notes();
    let items = visible
        .iter()
        .map(|note| {
            ListItem::new(format!(
                "{}{}",
                note.title,
                if note.archived { " [archived]" } else { "" }
            ))
        })
        .collect::<Vec<_>>();
    let mut list_state = ListState::default()
        .with_selected((!visible.is_empty()).then(|| state.selected.min(visible.len() - 1)));
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(format!("Notes · {}", visible.len()))
                    .borders(Borders::ALL),
            )
            .highlight_symbol("▶ ")
            .highlight_style(Style::default().fg(accent).add_modifier(Modifier::BOLD)),
        list_area,
        &mut list_state,
    );
    let detail = visible.get(state.selected.min(visible.len().saturating_sub(1)));
    let lines = if let Some(note) = detail {
        let scope = note
            .project_id
            .as_deref()
            .and_then(|id| state.projects.iter().find(|project| project.id == id))
            .map(|project| project.fields.name.as_str())
            .unwrap_or("General notebook");
        vec![
            Line::styled(
                &note.title,
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
            Line::raw(scope),
            Line::raw(format!(
                "Updated {} · revision {}",
                note.updated_at, note.revision
            )),
            Line::raw(""),
            Line::raw(note.markdown.lines().take(8).collect::<Vec<_>>().join("\n")),
        ]
    } else {
        vec![Line::raw(if state.notes.is_empty() {
            "No notes yet."
        } else {
            "No notes match this search."
        })]
    };
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title("Note preview").borders(Borders::ALL))
            .style(Style::default().fg(muted))
            .wrap(Wrap { trim: true }),
        detail_area,
    );
}

fn draw_note_detail(
    frame: &mut Frame<'_>,
    state: &TuiState,
    area: ratatui::layout::Rect,
    accent: Color,
    muted: Color,
) {
    let note = state
        .selected_note
        .as_deref()
        .and_then(|id| state.notes.iter().find(|note| note.id == id));
    if let Some(note) = note {
        let scope = note
            .project_id
            .as_deref()
            .and_then(|id| state.projects.iter().find(|project| project.id == id))
            .map(|project| project.fields.name.as_str())
            .unwrap_or("General notebook");
        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(4), Constraint::Min(1)])
            .split(area);
        frame.render_widget(
            Paragraph::new(vec![
                Line::styled(
                    &note.title,
                    Style::default().fg(accent).add_modifier(Modifier::BOLD),
                ),
                Line::raw(format!(
                    "{scope} · revision {} · {}",
                    note.revision,
                    if note.archived { "archived" } else { "active" }
                )),
            ])
            .block(Block::default().title("Note").borders(Borders::ALL)),
            sections[0],
        );
        frame.render_widget(
            Paragraph::new(note.markdown.as_str())
                .block(Block::default().title("Markdown").borders(Borders::ALL))
                .style(Style::default().fg(muted))
                .wrap(Wrap { trim: false }),
            sections[1],
        );
    } else {
        frame.render_widget(
            Paragraph::new("This note is no longer available.")
                .block(Block::default().borders(Borders::ALL)),
            area,
        );
    }
}

fn draw_asset_projects(
    frame: &mut Frame<'_>,
    state: &TuiState,
    area: ratatui::layout::Rect,
    accent: Color,
    muted: Color,
) {
    let visible = state.visible_projects();
    let items = visible
        .iter()
        .map(|project| {
            ListItem::new(format!(
                "{} · {} assets",
                project.fields.name, project.asset_count
            ))
        })
        .collect::<Vec<_>>();
    let mut list_state = ListState::default()
        .with_selected((!visible.is_empty()).then(|| state.selected.min(visible.len() - 1)));
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .title("Choose a project · Assets")
                    .borders(Borders::ALL),
            )
            .highlight_symbol("▶ ")
            .highlight_style(Style::default().fg(accent).add_modifier(Modifier::BOLD)),
        area,
        &mut list_state,
    );
    if visible.is_empty() {
        let message = if state.projects.is_empty() {
            "Create a project before importing assets."
        } else {
            "No projects match this search."
        };
        frame.render_widget(
            Paragraph::new(message).style(Style::default().fg(muted)),
            area,
        );
    }
}

fn draw_assets(
    frame: &mut Frame<'_>,
    state: &TuiState,
    area: ratatui::layout::Rect,
    accent: Color,
    muted: Color,
) {
    let rows = asset_rows(state.asset_board.as_ref());
    let query = state.search.to_lowercase();
    let visible: Vec<_> = rows
        .into_iter()
        .filter(|(label, detail)| {
            query.is_empty()
                || label.to_lowercase().contains(&query)
                || detail.to_lowercase().contains(&query)
        })
        .collect();
    let (list_area, detail_area) = if area.width < 88 {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
            .split(area);
        (split[0], split[1])
    } else {
        let split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area);
        (split[0], split[1])
    };
    let items = visible
        .iter()
        .map(|(label, detail)| ListItem::new(format!("{label}  ·  {detail}")))
        .collect::<Vec<_>>();
    let mut list_state = ListState::default()
        .with_selected((!visible.is_empty()).then(|| state.selected_asset.min(visible.len() - 1)));
    let project_name = state
        .selected_asset_project
        .as_deref()
        .and_then(|id| state.projects.iter().find(|project| project.id == id))
        .map(|project| project.fields.name.as_str())
        .unwrap_or("Assets");
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(format!("{project_name} · {} items", visible.len()))
                    .borders(Borders::ALL),
            )
            .highlight_symbol("▶ ")
            .highlight_style(Style::default().fg(accent).add_modifier(Modifier::BOLD)),
        list_area,
        &mut list_state,
    );
    let selected = visible.get(state.selected_asset.min(visible.len().saturating_sub(1)));
    let contents = selected
        .map(|(label, detail)| {
            vec![
                Line::styled(
                    label.as_str(),
                    Style::default().fg(accent).add_modifier(Modifier::BOLD),
                ),
                Line::raw(""),
                Line::raw(detail.as_str()),
            ]
        })
        .unwrap_or_else(|| {
            vec![Line::raw(
                if state
                    .asset_board
                    .as_ref()
                    .is_some_and(|board| board.assets.is_empty() && board.folders.is_empty())
                {
                    "No assets yet."
                } else {
                    "No assets match this search."
                },
            )]
        });
    frame.render_widget(
        Paragraph::new(contents)
            .block(Block::default().title("Selection").borders(Borders::ALL))
            .style(Style::default().fg(muted))
            .wrap(Wrap { trim: true }),
        detail_area,
    );
}

fn draw_trash(
    frame: &mut Frame<'_>,
    state: &TuiState,
    area: ratatui::layout::Rect,
    accent: Color,
    muted: Color,
) {
    let visible = state.visible_trash();
    let items = visible
        .iter()
        .map(|item| ListItem::new(format!("{} · {}", item.name, item.project_name)))
        .collect::<Vec<_>>();
    let mut list_state = ListState::default()
        .with_selected((!visible.is_empty()).then(|| state.selected.min(visible.len() - 1)));
    let (list_area, detail_area) = if area.width < 88 {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area);
        (split[0], split[1])
    } else {
        let split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(52), Constraint::Percentage(48)])
            .split(area);
        (split[0], split[1])
    };
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(format!("Trash · {}", visible.len()))
                    .borders(Borders::ALL),
            )
            .highlight_symbol("▶ ")
            .highlight_style(Style::default().fg(accent).add_modifier(Modifier::BOLD)),
        list_area,
        &mut list_state,
    );
    let lines = if let Some(item) = visible.get(state.selected.min(visible.len().saturating_sub(1)))
    {
        vec![
            Line::styled(
                &item.name,
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
            Line::raw(&item.project_name),
            Line::raw(format!("{} files · {} folders", item.files, item.folders)),
            Line::raw(format!("Deleted {}", item.deleted_at)),
        ]
    } else {
        vec![Line::raw("Trash is empty.")]
    };
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title("Item details").borders(Borders::ALL))
            .style(Style::default().fg(muted))
            .wrap(Wrap { trim: true }),
        detail_area,
    );
}

fn draw_posts(
    frame: &mut Frame<'_>,
    state: &TuiState,
    area: ratatui::layout::Rect,
    accent: Color,
    muted: Color,
) {
    let (list_area, detail_area) = if area.width < 88 {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
            .split(area);
        (split[0], split[1])
    } else {
        let split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(area);
        (split[0], split[1])
    };
    let visible = state.visible_posts();
    let items = visible
        .iter()
        .map(|post| {
            ListItem::new(format!(
                "{} · {} livery/liveries · rev {}",
                post.title, post.liveries, post.revision
            ))
        })
        .collect::<Vec<_>>();
    let mut list_state = ListState::default()
        .with_selected((!visible.is_empty()).then(|| state.selected.min(visible.len() - 1)));
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(format!("ERLCX drafts · {}", visible.len()))
                    .borders(Borders::ALL),
            )
            .highlight_symbol("▶ ")
            .highlight_style(Style::default().fg(accent).add_modifier(Modifier::BOLD)),
        list_area,
        &mut list_state,
    );
    let detail = visible.get(state.selected.min(visible.len().saturating_sub(1)));
    let lines = if let Some(path) = &state.post_import_path {
        let mut lines = vec![
            Line::styled(
                "Import an ERLCX publishing bundle",
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
            Line::raw("Version 1 bundles only · source projects stay unchanged"),
            Line::raw(""),
            Line::raw("Bundle path:"),
            Line::raw(format!("{path}▏")),
        ];
        if let Some(error) = &state.post_import_error {
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                error,
                Style::default().fg(if state.no_color {
                    Color::Reset
                } else {
                    Color::Red
                }),
            ));
        }
        lines
    } else if let Some(post) = detail {
        vec![
            Line::styled(
                &post.title,
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
            Line::raw(format!("{} livery/liveries", post.liveries)),
            Line::raw(format!(
                "Revision {} · updated {}",
                post.revision, post.updated_at
            )),
            Line::raw(format!("ID: {}", post.id)),
            Line::raw(""),
            Line::raw("Press Enter to inspect the saved post snapshot."),
        ]
    } else {
        vec![Line::raw(if state.posts.is_empty() {
            "No ERLCX drafts."
        } else {
            "No drafts match this search."
        })]
    };
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title("Selection").borders(Borders::ALL))
            .style(Style::default().fg(muted))
            .wrap(Wrap { trim: true }),
        detail_area,
    );
}

fn draw_post_detail(
    frame: &mut Frame<'_>,
    state: &TuiState,
    area: ratatui::layout::Rect,
    accent: Color,
    muted: Color,
) {
    let Some(draft) = &state.post_detail else {
        frame.render_widget(
            Paragraph::new("This draft is no longer available.")
                .block(Block::default().borders(Borders::ALL)),
            area,
        );
        return;
    };
    let mut lines = vec![
        Line::styled(
            draft.data.title.as_str(),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ),
        Line::raw(format!("Revision {}", draft.revision)),
        Line::raw(""),
        Line::raw(draft.data.description.as_str()),
        Line::raw(""),
        Line::raw(format!(
            "Tags: {}",
            if draft.data.tags.is_empty() {
                "—".to_owned()
            } else {
                draft.data.tags.join(", ")
            }
        )),
        Line::raw(format!(
            "Preview images: {} · YouTube: {}",
            draft.data.media.len(),
            draft.data.youtube_url.as_deref().unwrap_or("—")
        )),
        Line::raw(""),
    ];
    for (index, livery) in draft.data.liveries.iter().enumerate() {
        lines.push(Line::styled(
            format!("{}. {}", index + 1, livery.vehicle),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ));
        for side in &livery.sides {
            lines.push(Line::raw(format!(
                "   {}: {}",
                side.side,
                if side.decal_id.is_empty() {
                    "Not uploaded"
                } else {
                    side.decal_id.as_str()
                }
            )));
        }
    }
    if let Some((message, succeeded)) = &state.post_handoff_message {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            message,
            Style::default().fg(if state.no_color {
                Color::Reset
            } else if *succeeded {
                accent
            } else {
                Color::Red
            }),
        ));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title("Saved ERLCX snapshot")
                    .borders(Borders::ALL),
            )
            .style(Style::default().fg(muted))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn draw_post_editor(
    frame: &mut Frame<'_>,
    state: &TuiState,
    area: ratatui::layout::Rect,
    accent: Color,
    muted: Color,
) {
    let Some(editor) = &state.post_editor else {
        frame.render_widget(
            Paragraph::new("No ERLCX draft is open for editing.")
                .block(Block::default().borders(Borders::ALL)),
            area,
        );
        return;
    };
    let fields = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(4),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(area);
    let values = [
        editor.data.title.as_str(),
        editor.data.description.as_str(),
        editor.tags_text.as_str(),
        editor.youtube_text.as_str(),
    ];
    let labels = [
        format!("Title · {}/50", editor.data.title.chars().count()),
        format!(
            "Description · {}/2000",
            editor.data.description.chars().count()
        ),
        "Tags · comma-separated, up to 10".to_owned(),
        format!(
            "YouTube URL · {} preview image(s) · {} livery/liveries",
            editor.data.media.len(),
            editor.data.liveries.len()
        ),
    ];
    for index in 0..4 {
        let active = editor.active_field == index;
        let shown = if active {
            let cursor = editor.cursor.min(values[index].len());
            format!("{}▏{}", &values[index][..cursor], &values[index][cursor..])
        } else if values[index].is_empty() {
            "(empty)".to_owned()
        } else {
            values[index].to_owned()
        };
        let title = if active {
            format!("▶ {}", labels[index])
        } else {
            labels[index].clone()
        };
        frame.render_widget(
            Paragraph::new(shown)
                .block(
                    Block::default()
                        .title(title)
                        .borders(Borders::ALL)
                        .border_style(if active {
                            Style::default().fg(accent)
                        } else {
                            Style::default().fg(muted)
                        }),
                )
                .style(Style::default().fg(muted))
                .wrap(Wrap { trim: true }),
            fields[index],
        );
    }
    let notice = editor
        .error
        .as_deref()
        .unwrap_or("Ctrl+S saves this snapshot.");
    frame.render_widget(
        Paragraph::new(notice)
            .block(Block::default().title("Validation").borders(Borders::ALL))
            .style(
                Style::default().fg(if editor.error.is_some() && !state.no_color {
                    Color::Red
                } else {
                    muted
                }),
            )
            .wrap(Wrap { trim: true }),
        fields[4],
    );
}

fn draw_post_media(
    frame: &mut Frame<'_>,
    state: &TuiState,
    area: ratatui::layout::Rect,
    accent: Color,
    muted: Color,
) {
    let (list_area, detail_area) = if area.width < 88 {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area);
        (split[0], split[1])
    } else {
        let split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
            .split(area);
        (split[0], split[1])
    };
    let draft = state.post_detail.as_ref();
    let images = draft.map_or(&[][..], |draft| draft.data.media.as_slice());
    let selected = state.post_media.as_ref().map_or(0, |media| media.selected);
    let image_items = images
        .iter()
        .enumerate()
        .map(|(index, image)| {
            ListItem::new(format!(
                "{} {} · {}",
                if index == 0 { "[COVER]" } else { "        " },
                image.name,
                &image.hash[..image.hash.len().min(12)]
            ))
        })
        .collect::<Vec<_>>();
    let mut list_state = ListState::default()
        .with_selected((!image_items.is_empty()).then(|| selected.min(image_items.len() - 1)));
    frame.render_stateful_widget(
        List::new(image_items)
            .block(
                Block::default()
                    .title(format!("Preview images · {}/10", images.len()))
                    .borders(Borders::ALL),
            )
            .highlight_symbol("▶ ")
            .highlight_style(Style::default().fg(accent).add_modifier(Modifier::BOLD)),
        list_area,
        &mut list_state,
    );

    let mut details = if let Some(image) = images.get(selected.min(images.len().saturating_sub(1)))
    {
        vec![
            Line::styled(
                image.name.as_str(),
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
            Line::raw(if selected == 0 {
                "This is the post cover image."
            } else {
                "Move this image to the first position to use it as cover."
            }),
            Line::raw(""),
            Line::raw(format!("Content hash: {}", image.hash)),
        ]
    } else {
        vec![Line::raw("No preview images in this draft.")]
    };
    if let Some(path) = state
        .post_media
        .as_ref()
        .and_then(|media| media.path_input.as_deref())
    {
        details.push(Line::raw(""));
        details.push(Line::styled(
            "PNG path to import:",
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ));
        details.push(Line::raw(format!("{path}▏")));
    }
    if let Some(media) = &state.post_media {
        if media.confirm_remove {
            details.push(Line::raw(""));
            if let Some(image) = images.get(selected) {
                details.push(Line::styled(
                    format!("Remove {} from this draft?", image.name),
                    Style::default().fg(accent).add_modifier(Modifier::BOLD),
                ));
                details.push(Line::raw(
                    "The managed file stays in the workspace; only this post reference is removed.",
                ));
            }
        }
        if let Some(error) = &media.error {
            details.push(Line::raw(""));
            details.push(Line::styled(
                error,
                Style::default().fg(if state.no_color {
                    Color::Reset
                } else {
                    Color::Red
                }),
            ));
        }
    }
    frame.render_widget(
        Paragraph::new(std::mem::take(&mut details))
            .block(
                Block::default()
                    .title("Image details")
                    .borders(Borders::ALL),
            )
            .style(Style::default().fg(muted))
            .wrap(Wrap { trim: true }),
        detail_area,
    );
}

fn draw_post_liveries(
    frame: &mut Frame<'_>,
    state: &TuiState,
    area: ratatui::layout::Rect,
    accent: Color,
    muted: Color,
) {
    let liveries = state
        .post_detail
        .as_ref()
        .map_or(&[][..], |draft| draft.data.liveries.as_slice());
    let (list_area, detail_area) = if area.width < 88 {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area);
        (split[0], split[1])
    } else {
        let split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(area);
        (split[0], split[1])
    };
    let rows = liveries
        .iter()
        .map(|livery| {
            let missing = livery
                .sides
                .iter()
                .filter(|side| side.decal_id.is_empty())
                .count();
            ListItem::new(format!(
                "{} · {} codes · {} missing",
                livery.vehicle,
                livery.sides.len(),
                missing
            ))
        })
        .collect::<Vec<_>>();
    let mut list_state = ListState::default()
        .with_selected((!rows.is_empty()).then(|| state.selected_post_livery.min(rows.len() - 1)));
    frame.render_stateful_widget(
        List::new(rows)
            .block(
                Block::default()
                    .title(format!("Post liveries · {}", liveries.len()))
                    .borders(Borders::ALL),
            )
            .highlight_symbol("▶ ")
            .highlight_style(Style::default().fg(accent).add_modifier(Modifier::BOLD)),
        list_area,
        &mut list_state,
    );
    let details = if let Some(livery) = liveries.get(state.selected_post_livery) {
        let missing = livery
            .sides
            .iter()
            .filter(|side| side.decal_id.is_empty())
            .count();
        vec![
            Line::styled(
                livery.vehicle.as_str(),
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
            Line::raw(format!("Primary color: {}", livery.primary_color)),
            Line::raw(format!(
                "Secondary color: {}",
                livery.secondary_color.as_deref().unwrap_or("—")
            )),
            Line::raw(format!("Opacity: {}%", livery.opacity)),
            Line::raw(format!(
                "{} side code(s) · {missing} missing",
                livery.sides.len()
            )),
            Line::raw(if livery.source.is_some() {
                "Source linked to a project livery"
            } else {
                "Manual livery snapshot"
            }),
            Line::raw(""),
            Line::raw("Enter/C opens side codes. Edit changes vehicle and colors."),
        ]
    } else {
        vec![Line::raw("This post has no livery entries.")]
    };
    let mut details = details;
    if let Some(error) = &state.post_livery_error {
        details.push(Line::raw(""));
        details.push(Line::styled(
            error,
            Style::default().fg(if state.no_color {
                Color::Reset
            } else {
                Color::Red
            }),
        ));
    }
    frame.render_widget(
        Paragraph::new(details)
            .block(
                Block::default()
                    .title("Livery details")
                    .borders(Borders::ALL),
            )
            .style(Style::default().fg(muted))
            .wrap(Wrap { trim: true }),
        detail_area,
    );
}

fn draw_post_livery_editor(
    frame: &mut Frame<'_>,
    state: &TuiState,
    area: ratatui::layout::Rect,
    accent: Color,
    muted: Color,
) {
    let Some(editor) = &state.post_livery_editor else {
        frame.render_widget(Paragraph::new("No livery is open for editing."), area);
        return;
    };
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(3),
        ])
        .split(area);
    let values = [
        editor.vehicle.as_str(),
        editor.primary_color.as_str(),
        editor.secondary_color.as_str(),
        editor.opacity.as_str(),
    ];
    let labels = [
        "Vehicle",
        "Primary color (#RRGGBB)",
        "Secondary color (optional)",
        "Opacity (0–100)",
    ];
    for index in 0..4 {
        let shown = if editor.active_field == index {
            let cursor = editor.cursor.min(values[index].len());
            format!("{}▏{}", &values[index][..cursor], &values[index][cursor..])
        } else if values[index].is_empty() {
            "(none)".to_owned()
        } else {
            values[index].to_owned()
        };
        frame.render_widget(
            Paragraph::new(shown)
                .block(
                    Block::default()
                        .title(if editor.active_field == index {
                            format!("▶ {}", labels[index])
                        } else {
                            labels[index].to_owned()
                        })
                        .borders(Borders::ALL)
                        .border_style(if editor.active_field == index {
                            Style::default().fg(accent)
                        } else {
                            Style::default().fg(muted)
                        }),
                )
                .wrap(Wrap { trim: true }),
            sections[index],
        );
    }
    let notice = editor
        .error
        .as_deref()
        .unwrap_or("Ctrl+S saves this livery snapshot.");
    frame.render_widget(
        Paragraph::new(notice)
            .block(Block::default().title("Validation").borders(Borders::ALL))
            .style(
                Style::default().fg(if editor.error.is_some() && !state.no_color {
                    Color::Red
                } else {
                    muted
                }),
            )
            .wrap(Wrap { trim: true }),
        sections[4],
    );
}

fn draw_post_codes(
    frame: &mut Frame<'_>,
    state: &TuiState,
    area: ratatui::layout::Rect,
    accent: Color,
    muted: Color,
) {
    let livery = state
        .post_codes_livery
        .and_then(|index| state.post_detail.as_ref()?.data.liveries.get(index));
    let Some(livery) = livery else {
        frame.render_widget(
            Paragraph::new("The selected livery is no longer available."),
            area,
        );
        return;
    };
    let (list_area, detail_area) = if area.width < 88 {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area);
        (split[0], split[1])
    } else {
        let split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(area);
        (split[0], split[1])
    };
    let rows = livery
        .sides
        .iter()
        .map(|side| {
            ListItem::new(format!(
                "{} · {}",
                if side.side.is_empty() {
                    "(unnamed side)"
                } else {
                    &side.side
                },
                if side.decal_id.is_empty() {
                    "Not uploaded"
                } else {
                    &side.decal_id
                }
            ))
        })
        .collect::<Vec<_>>();
    let mut list_state = ListState::default()
        .with_selected((!rows.is_empty()).then(|| state.selected_post_code.min(rows.len() - 1)));
    frame.render_stateful_widget(
        List::new(rows)
            .block(
                Block::default()
                    .title(format!("{} · side codes", livery.vehicle))
                    .borders(Borders::ALL),
            )
            .highlight_symbol("▶ ")
            .highlight_style(Style::default().fg(accent).add_modifier(Modifier::BOLD)),
        list_area,
        &mut list_state,
    );
    let mut details = if let Some(side) = livery.sides.get(state.selected_post_code) {
        vec![
            Line::styled(
                if side.side.is_empty() {
                    "Unnamed side"
                } else {
                    &side.side
                },
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
            Line::raw(format!(
                "Decal asset ID: {}",
                if side.decal_id.is_empty() {
                    "Not uploaded"
                } else {
                    &side.decal_id
                }
            )),
            Line::raw(""),
            Line::raw("Edit this row or add another side. Empty codes remain incomplete."),
        ]
    } else {
        vec![Line::raw("No side codes yet. Press A to add one.")]
    };
    if let Some(error) = &state.post_livery_error {
        details.push(Line::raw(""));
        details.push(Line::styled(
            error,
            Style::default().fg(if state.no_color {
                Color::Reset
            } else {
                Color::Red
            }),
        ));
    }
    frame.render_widget(
        Paragraph::new(details)
            .block(Block::default().title("Code details").borders(Borders::ALL))
            .style(Style::default().fg(muted))
            .wrap(Wrap { trim: true }),
        detail_area,
    );
}

fn draw_post_code_editor(
    frame: &mut Frame<'_>,
    state: &TuiState,
    area: ratatui::layout::Rect,
    accent: Color,
    muted: Color,
) {
    let Some(editor) = &state.post_code_editor else {
        frame.render_widget(Paragraph::new("No side code is open for editing."), area);
        return;
    };
    let fields = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(3),
        ])
        .split(area);
    let values = [editor.side.as_str(), editor.decal_id.as_str()];
    let labels = ["Side", "Decal asset ID"];
    for index in 0..2 {
        let shown = if editor.active_field == index {
            let cursor = editor.cursor.min(values[index].len());
            format!("{}▏{}", &values[index][..cursor], &values[index][cursor..])
        } else if values[index].is_empty() {
            "(empty)".to_owned()
        } else {
            values[index].to_owned()
        };
        frame.render_widget(
            Paragraph::new(shown)
                .block(
                    Block::default()
                        .title(if editor.active_field == index {
                            format!("▶ {}", labels[index])
                        } else {
                            labels[index].to_owned()
                        })
                        .borders(Borders::ALL)
                        .border_style(if editor.active_field == index {
                            Style::default().fg(accent)
                        } else {
                            Style::default().fg(muted)
                        }),
                )
                .wrap(Wrap { trim: true }),
            fields[index],
        );
    }
    let notice = editor
        .error
        .as_deref()
        .unwrap_or("Ctrl+S saves this code row.");
    frame.render_widget(
        Paragraph::new(notice)
            .block(Block::default().title("Validation").borders(Borders::ALL))
            .style(
                Style::default().fg(if editor.error.is_some() && !state.no_color {
                    Color::Red
                } else {
                    muted
                }),
            )
            .wrap(Wrap { trim: true }),
        fields[2],
    );
}

fn draw_post_remove_review(
    frame: &mut Frame<'_>,
    state: &TuiState,
    area: ratatui::layout::Rect,
    muted: Color,
) {
    let target = state.pending_post_removal;
    let label = match (target, state.post_detail.as_ref()) {
        (Some(PostRemovalTarget::Livery(index)), Some(draft)) => draft
            .data
            .liveries
            .get(index)
            .map(|livery| format!("livery {}", livery.vehicle))
            .unwrap_or_else(|| "selected livery".into()),
        (
            Some(PostRemovalTarget::Code {
                livery_index,
                side_index,
            }),
            Some(draft),
        ) => draft
            .data
            .liveries
            .get(livery_index)
            .and_then(|livery| livery.sides.get(side_index))
            .map(|side| {
                format!(
                    "{} code for {}",
                    side.side, draft.data.liveries[livery_index].vehicle
                )
            })
            .unwrap_or_else(|| "selected code row".into()),
        _ => "selected post item".into(),
    };
    let mut lines = vec![
        Line::styled(
            format!("Remove {label} from this post?"),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Line::raw("This edits the saved snapshot; linked project assets are not deleted."),
        Line::raw(""),
        Line::raw("Press Y to confirm, or Esc/N to keep it."),
    ];
    if let Some(error) = &state.post_removal_error {
        lines.push(Line::raw(""));
        lines.push(Line::raw(error));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title("Confirm removal")
                    .borders(Borders::ALL),
            )
            .style(Style::default().fg(muted))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn draw_statistics(
    frame: &mut Frame<'_>,
    state: &TuiState,
    area: ratatui::layout::Rect,
    accent: Color,
    muted: Color,
) {
    let sections = if area.width < 88 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(52), Constraint::Percentage(48)])
            .split(area)
    };
    let latest = state.statistics_history.last();
    let posts = latest.map_or(&[][..], |snapshot| snapshot.posts.as_slice());
    let total = |select: fn(&StatisticsPost) -> Option<u64>| {
        let values = posts.iter().filter_map(select).collect::<Vec<_>>();
        (
            values.len(),
            (!values.is_empty()).then(|| {
                values
                    .iter()
                    .fold(0u64, |sum, value| sum.saturating_add(*value))
            }),
        )
    };
    let metrics = [
        ("Views", total(|post| post.views)),
        ("Likes", total(|post| post.likes)),
        ("Copies", total(|post| post.copies)),
        ("Favourites", total(|post| post.favourites)),
        ("Ratings", total(|post| post.rating_count)),
    ];
    let mut summary = vec![
        Line::styled(
            "ERLCX public profile",
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ),
        Line::raw(format!(
            "Profile: {}",
            state
                .statistics_profile
                .as_deref()
                .unwrap_or("not configured")
        )),
        Line::raw(format!("Snapshots: {}", state.statistics_history.len())),
        Line::raw(format!("Refresh: {}", state.statistics_status)),
        Line::raw(format!(
            "Latest collection: {}",
            latest.map_or("—", |snapshot| snapshot.collected_at.as_str())
        )),
        Line::raw(format!("Posts: {}", posts.len())),
        Line::raw(""),
    ];
    for (name, (measured, value)) in metrics {
        summary.push(Line::raw(format!(
            "{name}: {} · {measured}/{} posts",
            value.map_or("unknown".into(), |number| number.to_string()),
            posts.len()
        )));
    }
    if let Some(sales) = &state.statistics_sales {
        summary.push(Line::raw(""));
        summary.push(Line::styled(
            "Creator dashboard sales",
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ));
        summary.push(Line::raw(format!(
            "{} · {} sales · {} net Robux",
            sales.collected_at, sales.data.sales, sales.data.net_robux
        )));
    }
    if state.statistics_profile.is_none() {
        summary.push(Line::raw(""));
        summary.push(Line::raw(
            "Set a profile with `apex-studio-cli statistics profile --set <ID>`.",
        ));
    } else if latest.is_none() {
        summary.push(Line::raw(
            "Press r to refresh, or run `apex-studio-cli statistics refresh`.",
        ));
    }
    frame.render_widget(
        Paragraph::new(summary)
            .block(Block::default().title("Overview").borders(Borders::ALL))
            .style(Style::default().fg(muted))
            .wrap(Wrap { trim: true }),
        sections[0],
    );

    let mut history = state
        .statistics_history
        .iter()
        .rev()
        .take(12)
        .collect::<Vec<_>>();
    history.reverse();
    let view_totals = history
        .iter()
        .map(|snapshot| {
            snapshot
                .posts
                .iter()
                .filter_map(|post| post.views)
                .fold(None, |sum, value| {
                    Some(sum.unwrap_or(0u64).saturating_add(value))
                })
        })
        .collect::<Vec<_>>();
    let max_value = view_totals
        .iter()
        .flatten()
        .copied()
        .max()
        .unwrap_or(0)
        .max(1);
    let bar_width = sections[1].width.saturating_sub(24).max(1) as usize;
    let chart_lines = history
        .iter()
        .zip(view_totals.iter())
        .map(|(snapshot, total)| {
            let date = snapshot
                .collected_at
                .get(0..10)
                .unwrap_or(&snapshot.collected_at);
            let bar_len = total.map_or(0, |value| {
                ((value as f64 / max_value as f64) * bar_width as f64).round() as usize
            });
            Line::raw(format!(
                "{date} {:<bar_width$} {}",
                "█".repeat(bar_len.min(bar_width)),
                total.map_or("unknown".into(), |value| value.to_string()),
                bar_width = bar_width
            ))
        })
        .collect::<Vec<_>>();
    let chart = if chart_lines.is_empty() {
        vec![Line::raw("No snapshots available yet.")]
    } else {
        chart_lines
    };
    frame.render_widget(
        Paragraph::new(chart)
            .block(
                Block::default()
                    .title("Views over time · total views")
                    .borders(Borders::ALL),
            )
            .style(Style::default().fg(muted))
            .wrap(Wrap { trim: false }),
        sections[1],
    );
}

fn draw_group_revenue(
    frame: &mut Frame<'_>,
    state: &TuiState,
    area: ratatui::layout::Rect,
    accent: Color,
    muted: Color,
) {
    let sections = if area.width < 88 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
            .split(area)
    };
    let groups = state.visible_revenue_groups();
    let items = groups
        .iter()
        .map(|group| {
            let count = state.group_revenue.as_ref().map_or(0, |data| {
                let selected_passes = group
                    .selected_pass_ids
                    .iter()
                    .map(String::as_str)
                    .collect::<HashSet<_>>();
                data.sales
                    .iter()
                    .filter(|sale| {
                        sale.group_id == group.group_id
                            && selected_passes.contains(sale.asset_id.as_str())
                    })
                    .count()
            });
            ListItem::new(format!("{} · {} sales", group.name, count))
        })
        .collect::<Vec<_>>();
    let mut list_state = ListState::default()
        .with_selected((!items.is_empty()).then(|| state.selected.min(items.len() - 1)));
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(format!("Groups · {}", groups.len()))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(accent)),
            )
            .highlight_symbol("▶ ")
            .highlight_style(Style::default().fg(accent).add_modifier(Modifier::BOLD)),
        sections[0],
        &mut list_state,
    );
    let details =
        if let Some(group) = groups.get(state.selected.min(groups.len().saturating_sub(1))) {
            let sales = state
                .group_revenue
                .as_ref()
                .map(|data| {
                    let selected_passes = group
                        .selected_pass_ids
                        .iter()
                        .map(String::as_str)
                        .collect::<HashSet<_>>();
                    data.sales
                        .iter()
                        .filter(|sale| {
                            sale.group_id == group.group_id
                                && selected_passes.contains(sale.asset_id.as_str())
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let price = sales
                .iter()
                .filter(|sale| sale.hold_status != "Cancelled")
                .fold(0u64, |sum, sale| sum.saturating_add(sale.price));
            let revenue = sales
                .iter()
                .filter(|sale| sale.hold_status != "Cancelled")
                .fold(0u64, |sum, sale| sum.saturating_add(sale.revenue));
            let released = sales
                .iter()
                .filter(|sale| sale.hold_status == "Released")
                .count();
            let held = sales
                .iter()
                .filter(|sale| sale.hold_status == "Held")
                .count();
            let cancelled = sales
                .iter()
                .filter(|sale| sale.hold_status == "Cancelled")
                .count();
            let mut lines = vec![
                Line::styled(
                    &group.name,
                    Style::default().fg(accent).add_modifier(Modifier::BOLD),
                ),
                Line::raw(format!("Group ID: {}", group.group_id)),
                Line::raw(format!(
                    "Experience IDs: {}",
                    group.experience_ids.join(", ")
                )),
                Line::raw(format!("Imported sales: {}", sales.len())),
                Line::raw(format!("Selected pass price: {price} Robux")),
                Line::raw(format!("Selected pass revenue: {revenue} Robux")),
                Line::raw(format!(
                    "Released {released} · held {held} · cancelled {cancelled}"
                )),
            ];
            if let Some(updated) = &group.api_updated_at {
                lines.push(Line::raw(format!(
                    "Roblox API: {} · coverage {} to {}",
                    group.api_status.as_deref().unwrap_or("unknown"),
                    group.api_coverage_from.as_deref().unwrap_or("unknown"),
                    group.api_coverage_to.as_deref().unwrap_or("unknown")
                )));
                lines.push(Line::raw(format!("Last API refresh: {updated}")));
            }
            if let Some(imported) = &group.last_import_at {
                lines.push(Line::raw(format!("Last CSV import: {imported}")));
            }
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                "Price and revenue are kept as separate source fields.",
                Style::default().fg(muted),
            ));
            lines
        } else if state.search.is_empty() {
            vec![Line::raw("No Group Revenue groups are configured yet.")]
        } else {
            vec![Line::raw("No groups match this search.")]
        };
    frame.render_widget(
        Paragraph::new(details)
            .block(
                Block::default()
                    .title("Group details")
                    .borders(Borders::ALL),
            )
            .style(Style::default().fg(muted))
            .wrap(Wrap { trim: true }),
        sections[1],
    );
}

fn draw_roblox_queue(
    frame: &mut Frame<'_>,
    state: &TuiState,
    area: ratatui::layout::Rect,
    accent: Color,
    muted: Color,
) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(36), Constraint::Percentage(64)])
        .split(area);
    let job_sections = if sections[1].width < 88 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
            .split(sections[1])
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(sections[1])
    };
    let batches = state
        .upload_queue
        .as_ref()
        .map(|queue| {
            queue
                .batches
                .iter()
                .map(|batch| {
                    ListItem::new(format!(
                        "{} · {} · account {}",
                        batch.id, batch.state, batch.account_id
                    ))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut batch_list_state = ListState::default()
        .with_selected((!batches.is_empty()).then(|| state.selected_batch.min(batches.len() - 1)));
    frame.render_stateful_widget(
        List::new(batches)
            .block(
                Block::default()
                    .title(format!(
                        "Upload batches · worker {} · Tab to focus",
                        state.roblox_worker_status
                    ))
                    .borders(Borders::ALL)
                    .border_style(if state.queue_batches_focused {
                        Style::default().fg(accent)
                    } else {
                        Style::default().fg(muted)
                    }),
            )
            .highlight_symbol("▶ ")
            .highlight_style(Style::default().fg(accent).add_modifier(Modifier::BOLD)),
        sections[0],
        &mut batch_list_state,
    );

    let visible = state.visible_upload_jobs();
    let items = visible
        .iter()
        .map(|job| {
            ListItem::new(format!(
                "{} · {} · pass {}/{}",
                job.name, job.state, job.pass, job.passes_required
            ))
        })
        .collect::<Vec<_>>();
    let mut list_state = ListState::default()
        .with_selected((!visible.is_empty()).then(|| state.selected.min(visible.len() - 1)));
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(format!("Texture jobs · {}", visible.len()))
                    .borders(Borders::ALL)
                    .border_style(if state.queue_batches_focused {
                        Style::default().fg(muted)
                    } else {
                        Style::default().fg(accent)
                    }),
            )
            .highlight_symbol("▶ ")
            .highlight_style(Style::default().fg(accent).add_modifier(Modifier::BOLD)),
        job_sections[0],
        &mut list_state,
    );
    if let Some(job) = visible.get(state.selected.min(visible.len().saturating_sub(1))) {
        let details = vec![
            Line::styled(
                &job.name,
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
            Line::raw(format!(
                "State: {} · pass {}/{}",
                job.state, job.pass, job.passes_required
            )),
            Line::raw(format!("Account: {}", job.account_id)),
            Line::raw(format!(
                "First decal ID: {}",
                job.first_id.as_deref().unwrap_or("—")
            )),
            Line::raw(format!(
                "Second decal ID: {}",
                job.second_id.as_deref().unwrap_or("—")
            )),
            Line::raw(job.message.as_str()),
        ];
        frame.render_widget(
            Paragraph::new(details)
                .block(Block::default().title("Selected job").borders(Borders::ALL))
                .style(Style::default().fg(muted))
                .wrap(Wrap { trim: true }),
            job_sections[1],
        );
    } else {
        let message = if state.visible_upload_jobs().is_empty() {
            if state
                .upload_queue
                .as_ref()
                .is_some_and(|queue| queue.jobs.is_empty())
            {
                "No texture jobs yet."
            } else {
                "No texture jobs match this search."
            }
        } else {
            "Select a texture job to inspect its status."
        };
        frame.render_widget(
            Paragraph::new(message)
                .block(Block::default().title("Selected job").borders(Borders::ALL))
                .style(Style::default().fg(muted)),
            job_sections[1],
        );
    }
}

fn draw_roblox_cancel_review(
    frame: &mut Frame<'_>,
    state: &TuiState,
    area: ratatui::layout::Rect,
    accent: Color,
    muted: Color,
) {
    let Some(batch_id) = state.cancel_batch.as_deref() else {
        frame.render_widget(
            Paragraph::new("No Roblox batch is selected.")
                .block(Block::default().borders(Borders::ALL)),
            area,
        );
        return;
    };
    let batch = state
        .upload_queue
        .as_ref()
        .and_then(|queue| queue.batches.iter().find(|batch| batch.id == batch_id));
    let (account, status) = batch
        .map(|batch| (batch.account_id.as_str(), batch.state.as_str()))
        .unwrap_or(("unknown", "missing"));
    let selected_items = state
        .upload_queue
        .as_ref()
        .map(|queue| {
            queue
                .items
                .iter()
                .filter(|item| item.batch_id == batch_id)
                .count()
        })
        .unwrap_or_default();
    let lines = vec![
        Line::styled(
            "Cancel this Roblox upload batch?",
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ),
        Line::raw(format!("Batch: {batch_id}")),
        Line::raw(format!("Account: {account}")),
        Line::raw(format!("Current state: {status}")),
        Line::raw(format!("Texture entries: {selected_items}")),
        Line::raw(""),
        Line::raw("Queued work will be cancelled where it is not shared by another active batch."),
        Line::raw("Requests already accepted by Roblox cannot be recalled."),
        Line::raw(""),
        Line::raw("Press Y to confirm, or Esc/N to keep this batch."),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title("Confirm cancellation")
                    .borders(Borders::ALL),
            )
            .style(Style::default().fg(muted))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn draw_roblox_liveries(
    frame: &mut Frame<'_>,
    state: &TuiState,
    area: ratatui::layout::Rect,
    accent: Color,
    muted: Color,
) {
    if state.roblox_account_id.is_none() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::styled(
                    "Roblox credentials are not configured.",
                    Style::default().fg(accent).add_modifier(Modifier::BOLD),
                ),
                Line::raw(""),
                Line::raw("Set and validate them with: apex-studio-cli roblox credentials set --account-id <ID>"),
                Line::raw("Then reopen Roblox uploads."),
            ])
            .block(Block::default().title("Upload liveries").borders(Borders::ALL))
            .style(Style::default().fg(muted))
            .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }
    let (list_area, detail_area) = if area.width < 88 {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(66), Constraint::Percentage(34)])
            .split(area);
        (split[0], split[1])
    } else {
        let split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
            .split(area);
        (split[0], split[1])
    };
    let visible = state.visible_roblox_liveries();
    let items = visible
        .iter()
        .map(|livery| {
            let marker = if state.selected_roblox_liveries.contains(&livery.id) {
                "[x]"
            } else {
                "[ ]"
            };
            ListItem::new(format!(
                "{marker} {} · {} · {} of {} uploaded",
                livery.project_name, livery.name, livery.uploaded, livery.textures
            ))
        })
        .collect::<Vec<_>>();
    let mut list_state = ListState::default()
        .with_selected((!visible.is_empty()).then(|| state.selected.min(visible.len() - 1)));
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(if state.roblox_for_post {
                        format!(
                            "Add liveries to ERLCX draft · {} selected",
                            state.selected_roblox_liveries.len()
                        )
                    } else {
                        format!(
                            "Liveries for account {} · {} selected",
                            state.roblox_account_id.as_deref().unwrap_or("—"),
                            state.selected_roblox_liveries.len()
                        )
                    })
                    .borders(Borders::ALL),
            )
            .highlight_symbol("▶ ")
            .highlight_style(Style::default().fg(accent).add_modifier(Modifier::BOLD)),
        list_area,
        &mut list_state,
    );
    let details =
        if let Some(livery) = visible.get(state.selected.min(visible.len().saturating_sub(1))) {
            vec![
                Line::styled(
                    livery.name.as_str(),
                    Style::default().fg(accent).add_modifier(Modifier::BOLD),
                ),
                Line::raw(&livery.project_name),
                Line::raw(format!(
                    "{} of {} textures already uploaded",
                    livery.uploaded, livery.textures
                )),
                Line::raw(format!("Folder ID: {}", livery.id)),
                Line::raw(""),
                Line::raw(if state.roblox_for_post {
                    "Action: add codes from this livery to the draft"
                } else if state.roblox_reupload {
                    "Mode: re-upload selected textures"
                } else {
                    "Mode: upload missing or changed textures"
                }),
                Line::raw(""),
                Line::raw(if state.roblox_for_post {
                    "Existing usable decal IDs are copied; missing IDs stay blank."
                } else {
                    "Select several liveries, then press Enter to review."
                }),
            ]
        } else if state.roblox_liveries.is_empty() {
            vec![Line::raw(
                "No livery folders are available for this account.",
            )]
        } else {
            vec![Line::raw("No liveries match this search.")]
        };
    let details = if let Some(error) = &state.roblox_selection_error {
        let mut details = details;
        details.push(Line::raw(""));
        details.push(Line::styled(
            error,
            Style::default().fg(if state.no_color {
                Color::Reset
            } else {
                Color::Red
            }),
        ));
        details
    } else {
        details
    };
    frame.render_widget(
        Paragraph::new(details)
            .block(
                Block::default()
                    .title("Selection details")
                    .borders(Borders::ALL),
            )
            .style(Style::default().fg(muted))
            .wrap(Wrap { trim: true }),
        detail_area,
    );
}

fn draw_roblox_batch_review(
    frame: &mut Frame<'_>,
    state: &TuiState,
    area: ratatui::layout::Rect,
    accent: Color,
    muted: Color,
) {
    let Some(review) = &state.roblox_batch_review else {
        frame.render_widget(
            Paragraph::new("Select liveries before reviewing a batch.")
                .block(Block::default().borders(Borders::ALL)),
            area,
        );
        return;
    };
    let mut lines = vec![
        Line::styled(
            "Review Roblox upload batch",
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ),
        Line::raw(format!("Account: {}", review.account_id)),
        Line::raw(format!("Liveries: {}", review.livery_count)),
        Line::raw(format!("Textures: {}", review.texture_count)),
        Line::raw(format!("Reusable existing IDs: {}", review.reusable_ids)),
        Line::raw(format!("Required upload passes: {}", review.upload_passes)),
        Line::raw(format!(
            "Mode: {}",
            if review.reupload {
                "re-upload selected textures"
            } else {
                "upload missing or changed textures"
            }
        )),
        Line::raw(""),
    ];
    if review.upload_passes == 0 {
        lines.push(Line::raw(
            "Every selected texture already has a reusable ID. Nothing will be queued.",
        ));
        lines.push(Line::raw(
            "Press Esc to return and choose Re-upload if you intend to upload them again.",
        ));
    } else {
        lines.push(Line::raw(
            "Press Y to create the batch, then press g in the queue to run it.",
        ));
        lines.push(Line::raw(
            "Apex Studio can also process this queue from its tray worker.",
        ));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title("Final review").borders(Borders::ALL))
            .style(Style::default().fg(muted))
            .wrap(Wrap { trim: true }),
        area,
    );
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use apex_core::ProjectInput;
    use ratatui::backend::TestBackend;

    #[test]
    fn project_filter_matches_name_description_tags_and_vehicle() {
        let project = Project {
            id: "p1".into(),
            fields: ProjectInput {
                name: "Rodenauer Land".into(),
                description: "EMS fleet".into(),
                tags: vec!["rescue".into()],
                vehicles: vec!["Falcon Interceptor Utility".into()],
                ..Default::default()
            },
            created_at: "2026-11-08".into(),
            updated_at: "2026-11-08".into(),
            archived: false,
            asset_count: 0,
        };
        for query in ["rodenauer", "EMS", "RESCUE", "falcon"] {
            assert!(project_matches(&project, &query.to_lowercase()));
        }
        assert!(!project_matches(&project, "fire"));
    }

    #[test]
    fn project_screen_filters_typing_and_opens_selected_project() {
        let project = Project {
            id: "p1".into(),
            fields: ProjectInput {
                name: "EMS Fleet".into(),
                ..Default::default()
            },
            created_at: "2026-11-08".into(),
            updated_at: "2026-11-08".into(),
            archived: false,
            asset_count: 0,
        };
        let mut state = TuiState::new(vec![project]);
        state.screen = Screen::Projects;
        handle_project_key(&mut state, KeyCode::Char('/'));
        handle_project_key(&mut state, KeyCode::Char('e'));
        handle_project_key(&mut state, KeyCode::Char('m'));
        handle_project_key(&mut state, KeyCode::Char('s'));
        assert_eq!(state.visible_projects().len(), 1);
        handle_project_key(&mut state, KeyCode::Enter);
        handle_project_key(&mut state, KeyCode::Enter);
        assert_eq!(state.screen, Screen::Detail);
        assert_eq!(state.selected_project.as_deref(), Some("p1"));
    }

    #[test]
    fn tui_draws_in_narrow_and_standard_terminal_sizes() {
        for (width, height) in [(40, 12), (80, 24), (120, 36)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = ratatui::Terminal::new(backend).expect("test terminal");
            let state = TuiState::new(Vec::new());
            terminal.draw(|frame| draw(frame, &state)).expect("render");
            assert_eq!(terminal.backend().buffer().area.width, width);
            assert_eq!(terminal.backend().buffer().area.height, height);
        }
    }

    #[test]
    fn tui_queue_worker_refuses_to_start_when_no_batch_is_running() {
        let directory = tempfile::tempdir().expect("temporary workspace");
        let app = ApplicationServices::open(directory.path()).expect("dummy workspace");
        let (events, received) = std::sync::mpsc::channel();
        let error = start_tui_roblox_worker(app, events).expect_err("empty queue should not run");
        assert!(error.to_string().contains("no running Roblox batches"));
        assert!(received.try_recv().is_err());
    }

    #[test]
    fn phase_four_views_filter_posts_and_upload_jobs() {
        let mut state = TuiState::new(Vec::new());
        state.posts = vec![
            PostSummary {
                id: "post-1".into(),
                title: "Highway Patrol".into(),
                updated_at: String::new(),
                revision: 1,
                liveries: 1,
            },
            PostSummary {
                id: "post-2".into(),
                title: "Fire Rescue".into(),
                updated_at: String::new(),
                revision: 1,
                liveries: 2,
            },
        ];
        state.upload_queue = Some(QueueSnapshot {
            jobs: vec![UploadJob {
                id: "job-1".into(),
                account_id: "123456".into(),
                hash: "a".repeat(64),
                name: "Patrol Left".into(),
                state: "processing".into(),
                pass: 2,
                passes_required: 2,
                next_at: 0,
                message: "Waiting for Roblox".into(),
                operation: Some("operations/demo".into()),
                first_id: Some("111".into()),
                second_id: None,
                moderation: "unknown".into(),
                generation: 1,
                poll_attempts: 1,
            }],
            ..Default::default()
        });

        state.search = "patrol".into();
        assert_eq!(state.visible_posts().len(), 1);
        assert_eq!(state.visible_upload_jobs().len(), 1);
        state.search = "fire".into();
        assert_eq!(state.visible_posts().len(), 1);
        assert!(state.visible_upload_jobs().is_empty());
    }

    #[test]
    fn roblox_livery_picker_filters_project_name_livery_name_and_id() {
        let mut state = TuiState::new(Vec::new());
        state.roblox_liveries = vec![
            RobloxLiverySummary {
                id: "livery-1".into(),
                project_id: "project-1".into(),
                project_name: "EMS Fleet".into(),
                name: "Highway Patrol".into(),
                vehicle_id: None,
                textures: 3,
                uploaded: 1,
            },
            RobloxLiverySummary {
                id: "livery-2".into(),
                project_id: "project-2".into(),
                project_name: "Fire Rescue".into(),
                name: "Engine Company".into(),
                vehicle_id: None,
                textures: 5,
                uploaded: 0,
            },
        ];

        for query in ["ems", "HIGHWAY", "livery-1"] {
            state.search = query.into();
            assert_eq!(state.visible_roblox_liveries().len(), 1, "query: {query}");
            assert_eq!(state.visible_roblox_liveries()[0].id, "livery-1");
        }
        state.search = "rescue".into();
        assert_eq!(state.visible_roblox_liveries().len(), 1);
        assert_eq!(state.visible_roblox_liveries()[0].id, "livery-2");
        state.search = "missing".into();
        assert!(state.visible_roblox_liveries().is_empty());
    }

    #[test]
    fn roblox_picker_supports_filtered_multi_selection() {
        let directory = tempfile::tempdir().expect("temporary workspace directory");
        let app = ApplicationServices::open(directory.path()).expect("open dummy workspace");
        let mut state = TuiState::new(Vec::new());
        state.screen = Screen::RobloxLiveries;
        state.roblox_liveries = vec![
            RobloxLiverySummary {
                id: "livery-1".into(),
                project_id: "project-1".into(),
                project_name: "EMS Fleet".into(),
                name: "Highway Patrol".into(),
                vehicle_id: None,
                textures: 3,
                uploaded: 1,
            },
            RobloxLiverySummary {
                id: "livery-2".into(),
                project_id: "project-2".into(),
                project_name: "Fire Rescue".into(),
                name: "Engine Company".into(),
                vehicle_id: None,
                textures: 5,
                uploaded: 0,
            },
        ];

        handle_roblox_liveries_key(&mut state, KeyCode::Char(' '), &app)
            .expect("select first livery");
        handle_roblox_liveries_key(&mut state, KeyCode::Char('/'), &app)
            .expect("start livery search");
        for value in "rescue".chars() {
            handle_roblox_liveries_key(&mut state, KeyCode::Char(value), &app)
                .expect("type search");
        }
        handle_roblox_liveries_key(&mut state, KeyCode::Enter, &app).expect("finish livery search");
        handle_roblox_liveries_key(&mut state, KeyCode::Char('a'), &app)
            .expect("select filtered liveries");

        assert_eq!(state.selected_roblox_liveries.len(), 2);
        assert!(state.selected_roblox_liveries.contains("livery-1"));
        assert!(state.selected_roblox_liveries.contains("livery-2"));
        assert_eq!(state.search, "rescue");
    }

    #[test]
    fn canceling_a_batch_requires_a_second_confirmation_step() {
        let directory = tempfile::tempdir().expect("temporary workspace directory");
        let app = ApplicationServices::open(directory.path()).expect("open dummy workspace");
        let mut state = TuiState::new(Vec::new());
        state.screen = Screen::RobloxQueue;
        state.upload_queue = Some(QueueSnapshot {
            batches: vec![apex_storage::UploadBatch {
                id: "batch-1".into(),
                account_id: "123456".into(),
                state: "running".into(),
                created_at: String::new(),
            }],
            ..Default::default()
        });

        handle_queue_key(&mut state, KeyCode::Char('c'), &app).expect("open cancellation review");
        assert_eq!(state.screen, Screen::RobloxCancelReview);
        assert_eq!(state.cancel_batch.as_deref(), Some("batch-1"));
        handle_roblox_cancel_review_key(&mut state, KeyCode::Esc, &app)
            .expect("dismiss cancellation review");
        assert_eq!(state.screen, Screen::RobloxQueue);
        assert!(state.cancel_batch.is_none());
        assert_eq!(app.upload_queue().expect("read queue").batches.len(), 0);
    }

    #[test]
    fn post_editor_creates_a_validated_snapshot_without_losing_multiline_fields() {
        let directory = tempfile::tempdir().expect("temporary workspace directory");
        let app = ApplicationServices::open(directory.path()).expect("open dummy workspace");
        let mut state = TuiState::new(Vec::new());
        state.screen = Screen::Posts;
        handle_posts_key(&mut state, KeyCode::Char('n'), &app).expect("open new draft editor");
        assert!(app.posts().expect("list drafts").is_empty());

        for character in "Demo Livery Pack".chars() {
            handle_post_editor_key(&mut state, KeyCode::Char(character));
        }
        handle_post_editor_key(&mut state, KeyCode::Tab);
        for character in "First line".chars() {
            handle_post_editor_key(&mut state, KeyCode::Char(character));
        }
        handle_post_editor_key(&mut state, KeyCode::Enter);
        for character in "Second line".chars() {
            handle_post_editor_key(&mut state, KeyCode::Char(character));
        }
        handle_post_editor_key(&mut state, KeyCode::Tab);
        for character in "ems, highway patrol".chars() {
            handle_post_editor_key(&mut state, KeyCode::Char(character));
        }
        handle_post_editor_key(&mut state, KeyCode::Tab);
        for character in "https://youtu.be/example".chars() {
            handle_post_editor_key(&mut state, KeyCode::Char(character));
        }

        save_post_editor(&mut state, &app).expect("save new draft");
        let id = state
            .selected_post
            .as_deref()
            .expect("selected saved draft");
        let saved = app.post(id).expect("read saved draft");
        assert_eq!(saved.data.title, "Demo Livery Pack");
        assert_eq!(saved.data.description, "First line\nSecond line");
        assert_eq!(saved.data.tags, ["ems", "highway patrol"]);
        assert_eq!(
            saved.data.youtube_url.as_deref(),
            Some("https://youtu.be/example")
        );
        assert_eq!(state.screen, Screen::PostDetail);
    }

    #[test]
    fn post_editor_keeps_invalid_content_open_for_correction() {
        let directory = tempfile::tempdir().expect("temporary workspace directory");
        let app = ApplicationServices::open(directory.path()).expect("open dummy workspace");
        let mut state = TuiState::new(Vec::new());
        state.post_editor = Some(PostEditorState::new(
            None,
            0,
            PublishingInput {
                title: "x".repeat(51),
                ..Default::default()
            },
            false,
        ));
        state.screen = Screen::PostEditor;

        assert!(save_post_editor(&mut state, &app).is_err());
        assert_eq!(state.screen, Screen::PostEditor);
        assert!(app.posts().expect("list drafts").is_empty());
    }

    #[test]
    fn posts_list_imports_a_bundle_into_the_interactive_draft_flow() {
        let directory = tempfile::tempdir().expect("temporary workspace directory");
        let app = ApplicationServices::open(directory.path()).expect("open dummy workspace");
        let source_id = app.create_post().expect("create source post");
        let image_paths = [
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../design/Logo_Main.png"),
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../design/Banner_Small.png"),
        ];
        let imported_images = app
            .add_post_images(&source_id, &image_paths)
            .expect("import dummy preview images");
        let initial = app.post(&source_id).expect("read source post");
        let saved = app
            .save_post(
                &source_id,
                initial.revision,
                PublishingInput {
                    title: "TUI import test".into(),
                    description: "A disposable publishing bundle fixture".into(),
                    media: imported_images
                        .into_iter()
                        .map(|image| PublishingImage {
                            hash: image.hash,
                            name: image.name,
                        })
                        .collect(),
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
            .expect("save source snapshot");
        let bundle = directory.path().join("tui-import.apexpublish");
        app.export_post(&source_id, saved.revision, false, &bundle)
            .expect("export source fixture");

        let mut state = TuiState::new(Vec::new());
        state.screen = Screen::Posts;
        handle_posts_key(&mut state, KeyCode::Char('i'), &app).expect("open import prompt");
        for character in directory
            .path()
            .join("missing.apexpublish")
            .to_string_lossy()
            .chars()
        {
            handle_posts_key(&mut state, KeyCode::Char(character), &app)
                .expect("type invalid bundle path");
        }
        handle_posts_key(&mut state, KeyCode::Enter, &app).expect("show import error");
        assert!(state.post_import_error.is_some());
        assert_eq!(state.screen, Screen::Posts);
        handle_posts_key(&mut state, KeyCode::Esc, &app).expect("cancel invalid import");

        handle_posts_key(&mut state, KeyCode::Char('i'), &app).expect("reopen import prompt");
        for character in bundle.to_string_lossy().chars() {
            handle_posts_key(&mut state, KeyCode::Char(character), &app)
                .expect("type valid bundle path");
        }
        handle_posts_key(&mut state, KeyCode::Enter, &app).expect("import valid bundle");
        assert_eq!(state.screen, Screen::PostDetail);
        let imported_id = state
            .selected_post
            .as_deref()
            .expect("selected imported post");
        assert_ne!(imported_id, source_id);
        let imported = app.post(imported_id).expect("read imported snapshot");
        assert_eq!(imported.data.title, "TUI import test");
        assert_eq!(imported.data.media.len(), 2);
        assert_eq!(imported.data.liveries[0].sides[0].decal_id, "123456");
    }

    #[test]
    fn post_metadata_edit_preserves_existing_livery_codes() {
        let directory = tempfile::tempdir().expect("temporary workspace directory");
        let app = ApplicationServices::open(directory.path()).expect("open dummy workspace");
        let id = app.create_post().expect("create source draft");
        let empty = app.post(&id).expect("read source revision");
        let initial = app
            .save_post(
                &id,
                empty.revision,
                PublishingInput {
                    liveries: vec![apex_core::PublishingLivery {
                        source: None,
                        vehicle: "Falcon Interceptor Utility".into(),
                        primary_color: "#FFFFFF".into(),
                        secondary_color: None,
                        opacity: 100,
                        sides: vec![apex_core::PublishingSide {
                            side: "Frontplate 1".into(),
                            decal_id: "123456789".into(),
                        }],
                    }],
                    ..Default::default()
                },
            )
            .expect("save source livery code");
        let mut state = TuiState::new(Vec::new());
        state.screen = Screen::PostDetail;
        state.selected_post = Some(id.clone());
        state.post_detail = Some(initial.clone());
        state.post_editor = Some(PostEditorState::new(
            Some(id.clone()),
            initial.revision,
            initial.data,
            true,
        ));

        for character in "Updated post title".chars() {
            handle_post_editor_key(&mut state, KeyCode::Char(character));
        }
        save_post_editor(&mut state, &app).expect("save metadata change");
        let saved = app.post(&id).expect("read edited draft");
        assert_eq!(saved.data.title, "Updated post title");
        assert_eq!(saved.data.liveries.len(), 1);
        assert_eq!(saved.data.liveries[0].vehicle, "Falcon Interceptor Utility");
        assert_eq!(saved.data.liveries[0].sides[0].decal_id, "123456789");
    }

    #[test]
    fn adding_a_project_livery_copies_only_usable_decal_ids_and_tracks_its_source() {
        let directory = tempfile::tempdir().expect("temporary workspace directory");
        let app = ApplicationServices::open(directory.path()).expect("open dummy workspace");
        let project = app
            .save_project(
                None,
                ProjectInput {
                    name: "Dummy livery project".into(),
                    ..Default::default()
                },
            )
            .expect("create dummy project");
        let image = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../design/Logo_Main.png");
        let vehicle = apex_core::vehicle_catalogue()
            .expect("load official vehicle catalogue")
            .vehicles[0]
            .id
            .clone();
        let folder = app
            .workspace()
            .import_livery(
                &project.id,
                None,
                None,
                "Patrol livery",
                &vehicle,
                &[(image, "Left1.png".into(), Some("Left 1".into()))],
                None,
            )
            .expect("import dummy livery");
        let livery = app
            .roblox_liveries("123456789")
            .expect("list dummy account livery candidates")
            .into_iter()
            .find(|livery| livery.id == folder)
            .expect("find imported livery");
        let post_id = app.create_post().expect("create draft");
        let initial = app.post(&post_id).expect("read initial draft");
        let mut state = TuiState::new(Vec::new());
        state.screen = Screen::RobloxLiveries;
        state.selected_post = Some(post_id);
        state.post_detail = Some(initial);
        state.roblox_account_id = Some("123456789".into());
        state.roblox_for_post = true;
        state.roblox_liveries = vec![livery];
        state.selected_roblox_liveries.insert(folder.clone());

        handle_roblox_liveries_key(&mut state, KeyCode::Enter, &app)
            .expect("attach selected livery from picker");
        let saved = state.post_detail.expect("updated post snapshot");
        assert_eq!(state.screen, Screen::PostDetail);
        assert_eq!(saved.data.liveries.len(), 1);
        let added = &saved.data.liveries[0];
        assert_eq!(
            added.vehicle,
            app.livery_uploads(&folder, "123456789")
                .expect("read linked livery")
                .vehicle
        );
        assert_eq!(added.sides.len(), 1);
        assert_eq!(added.sides[0].side, "Left 1");
        assert!(added.sides[0].decal_id.is_empty());
        let source = added.source.as_ref().expect("linked source snapshot");
        assert_eq!(source.folder_id, folder);
        assert_eq!(source.account_id, "123456789");
        assert_eq!(source.hashes.len(), 1);
    }

    #[test]
    fn post_media_manager_imports_reorders_and_confirms_removal_in_a_dummy_workspace() {
        let directory = tempfile::tempdir().expect("temporary workspace directory");
        let app = ApplicationServices::open(directory.path()).expect("open dummy workspace");
        let post_id = app.create_post().expect("create dummy post");
        let initial = app.post(&post_id).expect("read new draft");
        let mut state = TuiState::new(Vec::new());
        state.screen = Screen::PostMedia;
        state.selected_post = Some(post_id.clone());
        state.post_detail = Some(initial);
        state.post_media = Some(PostMediaState::default());

        let image_paths = [
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../design/Logo_Main.png"),
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../design/Banner_Small.png"),
        ];
        let originals = image_paths
            .iter()
            .map(|path| fs::read(path).expect("read preview fixture"))
            .collect::<Vec<_>>();
        for path in &image_paths {
            handle_post_media_key(&mut state, KeyCode::Char('a'), &app)
                .expect("start preview import");
            for character in path.to_string_lossy().chars() {
                handle_post_media_key(&mut state, KeyCode::Char(character), &app)
                    .expect("type preview path");
            }
            handle_post_media_key(&mut state, KeyCode::Enter, &app)
                .expect("import and save preview");
        }

        let saved = app.post(&post_id).expect("read imported previews");
        assert_eq!(saved.data.media.len(), 2);
        assert_eq!(saved.data.media[0].name, "Logo_Main.png");
        assert_eq!(saved.data.media[1].name, "Banner_Small.png");
        assert_eq!(
            image_paths
                .iter()
                .map(|path| fs::read(path).expect("verify source unchanged"))
                .collect::<Vec<_>>(),
            originals
        );

        handle_post_media_key(&mut state, KeyCode::Char('['), &app)
            .expect("move second image into cover position");
        let reordered = app.post(&post_id).expect("read reordered previews");
        assert_eq!(reordered.data.media[0].name, "Banner_Small.png");
        assert_eq!(reordered.data.media[1].name, "Logo_Main.png");

        handle_post_media_key(&mut state, KeyCode::Char('d'), &app)
            .expect("open remove confirmation");
        assert!(state
            .post_media
            .as_ref()
            .is_some_and(|media| media.confirm_remove));
        handle_post_media_key(&mut state, KeyCode::Char('n'), &app).expect("keep preview image");
        assert_eq!(
            app.post(&post_id)
                .expect("read kept preview")
                .data
                .media
                .len(),
            2
        );
        handle_post_media_key(&mut state, KeyCode::Char('d'), &app)
            .expect("reopen remove confirmation");
        handle_post_media_key(&mut state, KeyCode::Char('y'), &app)
            .expect("remove preview from draft");
        let removed = app.post(&post_id).expect("read final post snapshot");
        assert_eq!(removed.data.media.len(), 1);
        assert_eq!(removed.data.media[0].name, "Logo_Main.png");
    }

    #[test]
    fn post_livery_manager_edits_vehicle_and_codes_and_confirms_removal() {
        let directory = tempfile::tempdir().expect("temporary workspace directory");
        let app = ApplicationServices::open(directory.path()).expect("open dummy workspace");
        let post_id = app.create_post().expect("create dummy post");
        let empty = app.post(&post_id).expect("read new post");
        let initial = app
            .save_post(
                &post_id,
                empty.revision,
                PublishingInput {
                    liveries: vec![apex_core::PublishingLivery {
                        source: Some(PublishingSource {
                            folder_id: "source-folder".into(),
                            account_id: "123456789".into(),
                            hashes: vec!["a".repeat(64), "b".repeat(64)],
                        }),
                        vehicle: "Falcon Interceptor".into(),
                        primary_color: "#FFFFFF".into(),
                        secondary_color: None,
                        opacity: 100,
                        sides: vec![
                            apex_core::PublishingSide {
                                side: "Left 1".into(),
                                decal_id: "111".into(),
                            },
                            apex_core::PublishingSide {
                                side: "Right 1".into(),
                                decal_id: "444".into(),
                            },
                        ],
                    }],
                    ..Default::default()
                },
            )
            .expect("save source livery");
        let mut state = TuiState::new(Vec::new());
        state.screen = Screen::PostLiveries;
        state.selected_post = Some(post_id.clone());
        state.post_detail = Some(initial);

        handle_post_liveries_key(&mut state, KeyCode::Char('e'), &app).expect("open livery editor");
        for character in " 2024".chars() {
            handle_post_livery_editor_key(&mut state, KeyCode::Char(character));
        }
        handle_post_livery_editor_key(&mut state, KeyCode::Tab);
        for _ in 0..6 {
            handle_post_livery_editor_key(&mut state, KeyCode::Backspace);
        }
        for character in "112233".chars() {
            handle_post_livery_editor_key(&mut state, KeyCode::Char(character));
        }
        save_post_livery_editor(&mut state, &app).expect("save livery properties");
        let saved = app.post(&post_id).expect("read edited livery");
        assert_eq!(saved.data.liveries[0].vehicle, "Falcon Interceptor 2024");
        assert_eq!(saved.data.liveries[0].primary_color, "#112233");

        handle_post_liveries_key(&mut state, KeyCode::Char('c'), &app).expect("open code list");
        handle_post_codes_key(&mut state, KeyCode::Char('e'), &app).expect("open code editor");
        handle_post_code_editor_key(&mut state, KeyCode::Tab);
        for _ in 0..3 {
            handle_post_code_editor_key(&mut state, KeyCode::Backspace);
        }
        for character in "222".chars() {
            handle_post_code_editor_key(&mut state, KeyCode::Char(character));
        }
        save_post_code_editor(&mut state, &app).expect("save edited decal ID");
        assert_eq!(
            app.post(&post_id).expect("read edited code").data.liveries[0].sides[0].decal_id,
            "222"
        );

        handle_post_codes_key(&mut state, KeyCode::Char(']'), &app)
            .expect("reorder linked side codes");
        let reordered = app.post(&post_id).expect("read reordered side codes");
        assert_eq!(reordered.data.liveries[0].sides[0].side, "Right 1");
        assert_eq!(
            reordered.data.liveries[0]
                .source
                .as_ref()
                .expect("source link remains")
                .hashes[0],
            "b".repeat(64)
        );

        handle_post_codes_key(&mut state, KeyCode::Char('a'), &app).expect("add side code");
        for character in "Back 1".chars() {
            handle_post_code_editor_key(&mut state, KeyCode::Char(character));
        }
        handle_post_code_editor_key(&mut state, KeyCode::Tab);
        for character in "333".chars() {
            handle_post_code_editor_key(&mut state, KeyCode::Char(character));
        }
        save_post_code_editor(&mut state, &app).expect("save new side code");
        assert_eq!(
            app.post(&post_id).expect("read added code").data.liveries[0]
                .sides
                .len(),
            3
        );

        handle_post_codes_key(&mut state, KeyCode::Char('d'), &app)
            .expect("request side-code removal");
        handle_post_remove_review_key(&mut state, KeyCode::Char('n'), &app)
            .expect("cancel removal");
        assert_eq!(state.screen, Screen::PostCodes);
        assert_eq!(
            app.post(&post_id)
                .expect("check cancelled removal")
                .data
                .liveries[0]
                .sides
                .len(),
            3
        );
        handle_post_codes_key(&mut state, KeyCode::Char('d'), &app)
            .expect("request side-code removal again");
        handle_post_remove_review_key(&mut state, KeyCode::Char('y'), &app)
            .expect("confirm side-code removal");
        let final_post = app.post(&post_id).expect("read final post");
        assert_eq!(final_post.data.liveries[0].sides.len(), 2);
        assert_eq!(final_post.data.liveries[0].sides[0].side, "Right 1");
        assert_eq!(final_post.data.liveries[0].sides[1].side, "Left 1");

        handle_post_codes_key(&mut state, KeyCode::Esc, &app).expect("return to livery list");
        handle_post_liveries_key(&mut state, KeyCode::Char('d'), &app)
            .expect("request livery removal");
        handle_post_remove_review_key(&mut state, KeyCode::Char('n'), &app)
            .expect("cancel livery removal");
        assert_eq!(state.screen, Screen::PostLiveries);
        handle_post_liveries_key(&mut state, KeyCode::Char('d'), &app)
            .expect("request livery removal again");
        handle_post_remove_review_key(&mut state, KeyCode::Char('y'), &app)
            .expect("confirm livery removal");
        assert!(app
            .post(&post_id)
            .expect("read post after livery removal")
            .data
            .liveries
            .is_empty());

        handle_post_liveries_key(&mut state, KeyCode::Char('a'), &app)
            .expect("open manual livery editor");
        for character in "Manual vehicle".chars() {
            handle_post_livery_editor_key(&mut state, KeyCode::Char(character));
        }
        save_post_livery_editor(&mut state, &app).expect("save manual livery");
        let manual = app.post(&post_id).expect("read manual livery");
        assert_eq!(manual.data.liveries.len(), 1);
        assert_eq!(manual.data.liveries[0].vehicle, "Manual vehicle");
        assert!(manual.data.liveries[0].source.is_none());
        assert!(manual.data.liveries[0].sides.is_empty());

        handle_post_liveries_key(&mut state, KeyCode::Char('c'), &app)
            .expect("open manual livery codes");
        handle_post_codes_key(&mut state, KeyCode::Char('a'), &app)
            .expect("open manual side-code editor");
        for character in "Left 1".chars() {
            handle_post_code_editor_key(&mut state, KeyCode::Char(character));
        }
        handle_post_code_editor_key(&mut state, KeyCode::Tab);
        for character in "987654321".chars() {
            handle_post_code_editor_key(&mut state, KeyCode::Char(character));
        }
        save_post_code_editor(&mut state, &app).expect("save manual decal code");
        let completed = app.post(&post_id).expect("read completed manual livery");
        assert_eq!(completed.data.liveries[0].sides[0].side, "Left 1");
        assert_eq!(completed.data.liveries[0].sides[0].decal_id, "987654321");
    }

    #[test]
    fn roblox_api_key_is_not_an_accepted_command_line_argument() {
        let error = Args::try_parse_from([
            "apex-studio-cli",
            "roblox",
            "credentials",
            "set",
            "--account-id",
            "123456",
            "test-key-must-not-be-an-argument",
        ])
        .expect_err("credential command must reject key arguments");
        assert_eq!(error.kind(), clap::error::ErrorKind::UnknownArgument);
    }

    #[test]
    fn posts_handoff_command_accepts_explicit_stale_snapshot_review() {
        let args = Args::try_parse_from([
            "apex-studio-cli",
            "posts",
            "handoff",
            "draft-123",
            "--reviewed",
        ])
        .expect("parse ERLCX handoff command");
        assert!(matches!(
            args.command,
            Some(Command::Posts {
                action: PostCommand::Handoff {
                    id,
                    reviewed: true
                }
            }) if id == "draft-123"
        ));
    }

    #[cfg(windows)]
    #[test]
    fn chrome_handoff_client_uses_the_native_pipe_frame_protocol() {
        use tokio::{
            io::{AsyncReadExt, AsyncWriteExt},
            net::windows::named_pipe::ServerOptions,
        };

        let pipe_name = format!(
            r"\\.\pipe\ApexStudio-cli-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock after epoch")
                .as_nanos()
        );
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(0);
        let server_name = pipe_name.clone();
        let server_thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("create test pipe runtime");
            runtime.block_on(async move {
                let mut server = ServerOptions::new()
                    .first_pipe_instance(true)
                    .reject_remote_clients(true)
                    .create(&server_name)
                    .expect("create test named pipe");
                ready_tx.send(()).expect("signal pipe readiness");
                server.connect().await.expect("accept CLI request");
                let size = server.read_u32_le().await.expect("read request length") as usize;
                let mut payload = vec![0; size];
                server
                    .read_exact(&mut payload)
                    .await
                    .expect("read request payload");
                let request: serde_json::Value =
                    serde_json::from_slice(&payload).expect("decode request");
                assert_eq!(request["origin"], "apex-cli");
                assert_eq!(request["kind"], "prepare_post");
                assert_eq!(request["id"], "draft-123");
                assert_eq!(request["revision"], 7);
                assert_eq!(request["reviewed"], true);
                let response = serde_json::to_vec(&serde_json::json!({
                    "ok": true,
                    "postId": "draft-123"
                }))
                .expect("encode response");
                server
                    .write_u32_le(response.len() as u32)
                    .await
                    .expect("write response length");
                server
                    .write_all(&response)
                    .await
                    .expect("write response payload");
                server.flush().await.expect("flush response");
            });
        });
        ready_rx.recv().expect("test pipe available");
        request_chrome_handoff_on_pipe(&pipe_name, "draft-123", 7, true)
            .expect("complete framed bridge request");
        server_thread.join().expect("join test pipe server");
    }

    #[test]
    fn phase_two_screens_render_in_compact_and_wide_terminals() {
        let project = Project {
            id: "project-1".into(),
            fields: ProjectInput {
                name: "Dummy project".into(),
                ..Default::default()
            },
            created_at: "2026-11-08".into(),
            updated_at: "2026-11-08".into(),
            archived: false,
            asset_count: 1,
        };
        let note = Note {
            id: "note-1".into(),
            project_id: Some(project.id.clone()),
            title: "Dummy note".into(),
            document: serde_json::json!({"type":"doc","content":[{"type":"paragraph"}]}),
            markdown: "# Dummy heading\n\nMarkdown body".into(),
            revision: 1,
            archived: false,
            created_at: "2026-11-08".into(),
            updated_at: "2026-11-08".into(),
        };
        for (width, height) in [(40, 12), (80, 24), (120, 36)] {
            for screen in [
                Screen::Home,
                Screen::Projects,
                Screen::Detail,
                Screen::Notes,
                Screen::NoteDetail,
                Screen::AssetProjects,
                Screen::Assets,
                Screen::Trash,
                Screen::Posts,
                Screen::PostDetail,
                Screen::PostEditor,
                Screen::PostMedia,
                Screen::PostLiveries,
                Screen::PostLiveryEditor,
                Screen::PostCodes,
                Screen::PostCodeEditor,
                Screen::PostRemoveReview,
                Screen::RobloxQueue,
                Screen::Statistics,
                Screen::GroupRevenue,
                Screen::RobloxCancelReview,
                Screen::RobloxLiveries,
                Screen::RobloxBatchReview,
            ] {
                let backend = TestBackend::new(width, height);
                let mut terminal = ratatui::Terminal::new(backend).expect("test terminal");
                let mut state = TuiState::new(vec![project.clone()]);
                state.screen = screen;
                state.notes = vec![note.clone()];
                state.selected_project = Some(project.id.clone());
                state.selected_note = Some(note.id.clone());
                state.selected_asset_project = Some(project.id.clone());
                state.posts = vec![PostSummary {
                    id: "post-1".into(),
                    title: "Dummy ERLCX post".into(),
                    updated_at: "2026-11-08".into(),
                    revision: 1,
                    liveries: 1,
                }];
                state.post_detail = Some(PublishingDraft {
                    data: apex_core::PublishingInput {
                        title: "Dummy ERLCX post".into(),
                        description: "Fixture description".into(),
                        liveries: vec![apex_core::PublishingLivery {
                            source: None,
                            vehicle: "Fixture vehicle".into(),
                            primary_color: "#FFFFFF".into(),
                            secondary_color: None,
                            opacity: 100,
                            sides: vec![apex_core::PublishingSide {
                                side: "Left 1".into(),
                                decal_id: "123456".into(),
                            }],
                        }],
                        ..Default::default()
                    },
                    revision: 1,
                });
                state.post_editor = Some(PostEditorState::new(
                    Some("post-1".into()),
                    1,
                    PublishingInput {
                        title: "Dummy ERLCX post".into(),
                        description: "Editor fixture".into(),
                        ..Default::default()
                    },
                    true,
                ));
                state.post_media = Some(PostMediaState::default());
                state.post_livery_editor = Some(PostLiveryEditorState {
                    livery_index: 0,
                    is_new: false,
                    vehicle: "Fixture vehicle".into(),
                    primary_color: "#FFFFFF".into(),
                    secondary_color: String::new(),
                    opacity: "100".into(),
                    active_field: 0,
                    cursor: 0,
                    error: None,
                });
                state.post_code_editor = Some(PostCodeEditorState {
                    livery_index: 0,
                    side_index: Some(0),
                    side: "Left 1".into(),
                    decal_id: "123456".into(),
                    active_field: 0,
                    cursor: 0,
                    error: None,
                });
                state.post_codes_livery = Some(0);
                state.pending_post_removal = Some(PostRemovalTarget::Code {
                    livery_index: 0,
                    side_index: 0,
                });
                state.upload_queue = Some(QueueSnapshot::default());
                state.roblox_account_id = Some("123456789".into());
                state.roblox_liveries = vec![RobloxLiverySummary {
                    id: "livery-1".into(),
                    project_id: project.id.clone(),
                    project_name: project.fields.name.clone(),
                    name: "Dummy livery".into(),
                    vehicle_id: None,
                    textures: 12,
                    uploaded: 4,
                }];
                state.roblox_batch_review = Some(RobloxBatchReviewState {
                    livery_count: 1,
                    texture_count: 12,
                    reusable_ids: 4,
                    upload_passes: 16,
                    account_id: "123456789".into(),
                    reupload: false,
                });
                state.asset_board = Some(AssetBoard {
                    assets: Vec::new(),
                    folders: Vec::new(),
                    placements: Vec::new(),
                });
                terminal
                    .draw(|frame| draw(frame, &state))
                    .expect("render TUI screen");
                assert_eq!(terminal.backend().buffer().area.width, width);
                assert_eq!(terminal.backend().buffer().area.height, height);
            }
        }
    }

    #[test]
    fn group_revenue_tui_search_filters_the_group_list() {
        let mut state = TuiState::new(Vec::new());
        state.screen = Screen::GroupRevenue;
        state.group_revenue = Some(RobloxGroupRevenueData {
            groups: vec![
                apex_application::RobloxRevenueGroup {
                    group_id: "123".into(),
                    name: "Apex Designs".into(),
                    experience_ids: vec![],
                    pass_mappings: Default::default(),
                    selected_pass_ids: vec![],
                    api_updated_at: None,
                    api_coverage_from: None,
                    api_coverage_to: None,
                    api_summary: None,
                    api_status: None,
                    updated_at: None,
                    last_import_at: None,
                },
                apex_application::RobloxRevenueGroup {
                    group_id: "456".into(),
                    name: "Rescue Fleet".into(),
                    experience_ids: vec![],
                    pass_mappings: Default::default(),
                    selected_pass_ids: vec![],
                    api_updated_at: None,
                    api_coverage_from: None,
                    api_coverage_to: None,
                    api_summary: None,
                    api_status: None,
                    updated_at: None,
                    last_import_at: None,
                },
            ],
            sales: vec![],
            passes: vec![],
        });
        handle_group_revenue_key(&mut state, KeyCode::Char('/'));
        for character in "rescue".chars() {
            handle_group_revenue_key(&mut state, KeyCode::Char(character));
        }
        handle_group_revenue_key(&mut state, KeyCode::Enter);
        let groups = state.visible_revenue_groups();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].group_id, "456");
    }
}
