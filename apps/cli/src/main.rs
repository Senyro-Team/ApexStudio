use apex_application::{ApplicationServices, PostSummary, QueueSnapshot, TrashItem, UploadJob};
use apex_core::{AssetBoard, Error, Note, Project, ProjectInput, PublishingDraft, Vehicle};
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
    fmt, fs,
    io::{self, IsTerminal, Read, Write},
    path::PathBuf,
    process::ExitCode,
};

const APP_IDENTIFIER: &str = "studio.apexdesigns.apexstudio";

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
    /// Create a reviewed batch; the desktop upload worker can process it while Apex Studio is running.
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
                    println!("Created batch {id}. Configure Roblox credentials in Apex Studio Settings and keep the desktop app running to process it.");
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
    RobloxQueue,
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
    selected_post: Option<String>,
    post_detail: Option<PublishingDraft>,
    upload_queue: Option<QueueSnapshot>,
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
            selected_post: None,
            post_detail: None,
            upload_queue: None,
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
}

fn run_tui(app: ApplicationServices) -> io::Result<()> {
    let projects = app.projects().map_err(io::Error::other)?;
    let mut state = TuiState::new(projects);
    ratatui::run(|terminal| loop {
        terminal.draw(|frame| draw(frame, &state))?;
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                    break Ok(());
                }
                match state.screen {
                    Screen::Home => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break Ok(()),
                        KeyCode::Up | KeyCode::Char('k') => {
                            state.selected = state.selected.saturating_sub(1)
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            state.selected = (state.selected + 1).min(6)
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
                    Screen::PostDetail => match key.code {
                        KeyCode::Esc | KeyCode::Backspace => state.screen = Screen::Posts,
                        KeyCode::Char('q') => state.screen = Screen::Home,
                        _ => {}
                    },
                    Screen::RobloxQueue if !state.searching && key.code == KeyCode::Char('r') => {
                        refresh_upload_queue(&mut state, &app).map_err(io::Error::other)?;
                    }
                    Screen::RobloxQueue => handle_queue_key(&mut state, key.code),
                    Screen::Detail => match key.code {
                        KeyCode::Esc | KeyCode::Backspace => state.screen = Screen::Projects,
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
    state.posts = app.posts()?;
    state.selected = state
        .selected
        .min(state.visible_posts().len().saturating_sub(1));
    Ok(())
}

fn refresh_upload_queue(state: &mut TuiState, app: &ApplicationServices) -> Result<(), Error> {
    state.upload_queue = Some(app.upload_queue()?);
    state.selected = state
        .selected
        .min(state.visible_upload_jobs().len().saturating_sub(1));
    Ok(())
}

fn handle_posts_key(
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

fn handle_queue_key(state: &mut TuiState, key: KeyCode) {
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
                (state.selected + 1).min(state.visible_upload_jobs().len().saturating_sub(1))
        }
        _ => {}
    }
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
            Constraint::Length(2),
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
        Screen::RobloxQueue => draw_roblox_queue(frame, state, regions[1], accent, muted),
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
        Screen::Posts => " ↑/↓ Select   Enter Open   / Search   r Refresh   Esc Back ",
        Screen::PostDetail => " Esc Back   q Home ",
        Screen::RobloxQueue if state.searching => " Type to filter   Enter/Esc Finish search ",
        Screen::RobloxQueue => " ↑/↓ Select   / Search   r Refresh   Esc Back ",
    };
    frame.render_widget(
        Paragraph::new(footer)
            .style(Style::default().fg(muted))
            .alignment(ratatui::layout::Alignment::Center),
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
        ListItem::new("Quit"),
    ];
    let mut list_state = ListState::default().with_selected(Some(state.selected.min(6)));
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::default().title("Menu").borders(Borders::ALL))
            .highlight_symbol("▶ ")
            .highlight_style(Style::default().fg(accent).add_modifier(Modifier::BOLD)),
        sections[0],
        &mut list_state,
    );
    frame.render_widget(
        Paragraph::new(match state.selected.min(6) {
            0 => "Browse, search, and inspect project records.",
            1 => "Browse and read project or general notes.",
            2 => "Inspect project folders, liveries, and files.",
            3 => "Browse deleted project assets.",
            4 => "Browse global ERLCX post drafts and their saved snapshots.",
            5 => "Inspect Roblox upload batches and texture processing status.",
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
    let lines = if let Some(post) = detail {
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
    frame.render_widget(
        List::new(batches).block(
            Block::default()
                .title("Upload batches")
                .borders(Borders::ALL),
        ),
        sections[0],
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
                    .borders(Borders::ALL),
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
                Screen::RobloxQueue,
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
                state.upload_queue = Some(QueueSnapshot::default());
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
}
