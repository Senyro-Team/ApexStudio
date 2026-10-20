use std::{io::Read, path::PathBuf, sync::Arc};

use apex_core::{
    ArchiveProject, CommissionRepository, CreateCommission, CreateProject, ListCommissions,
    ListProjects, ProjectRepository, SystemClock, UpdateProject, UuidV7Generator,
};
use apex_domain::{ArchiveProjectInput, CreateCommissionInput, ProjectDto, UpdateProjectInput};
use apex_images::{BlobStore, DEFAULT_INDEX_LIMIT, inspect_png, parse_filename};
use apex_package::{
    ExportProfile, ExportRequest, ExportSnapshot, PackageLimits, export_package, preview_export,
    stage_package,
};
use apex_storage::sqlite::{SqliteCommissionRepository, SqliteDatabase, SqliteProjectRepository};
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "apex",
    version,
    about = "Apex Studio – lokaler Livery-Workflow"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Project {
        #[command(subcommand)]
        command: ProjectCommand,
    },
    Commission {
        #[command(subcommand)]
        command: CommissionCommand,
    },
    Image {
        #[command(subcommand)]
        command: ImageCommand,
    },
}

#[derive(Subcommand)]
enum ImageCommand {
    Inspect {
        path: PathBuf,
        #[arg(long)]
        json: bool,
    },
    Import {
        path: PathBuf,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum ProjectCommand {
    Create {
        name: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long = "tag", value_delimiter = ',')]
        tags: Vec<String>,
    },
    List {
        #[arg(long)]
        json: bool,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        include_archived: bool,
    },
    Update {
        id: String,
        #[arg(long)]
        expected_revision: u64,
        name: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long = "tag", value_delimiter = ',')]
        tags: Vec<String>,
    },
    Archive {
        id: String,
        #[arg(long)]
        expected_revision: u64,
    },
    Export {
        id: String,
        destination: PathBuf,
        #[arg(long, value_enum, default_value = "client-share")]
        profile: CliExportProfile,
        #[arg(long)]
        confirm_private: bool,
    },
    Import {
        source: PathBuf,
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CliExportProfile {
    BackupPrivate,
    TeamTransferPrivate,
    ClientShare,
}

impl From<CliExportProfile> for ExportProfile {
    fn from(value: CliExportProfile) -> Self {
        match value {
            CliExportProfile::BackupPrivate => Self::BackupPrivate,
            CliExportProfile::TeamTransferPrivate => Self::TeamTransferPrivate,
            CliExportProfile::ClientShare => Self::ClientShare,
        }
    }
}

#[derive(Subcommand)]
enum CommissionCommand {
    Create {
        project_id: String,
        name: String,
        #[arg(long)]
        client: Option<String>,
        #[arg(long)]
        organisation: Option<String>,
        #[arg(long = "type")]
        commission_type: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        due_date: Option<String>,
        #[arg(long = "requirement", value_delimiter = ';')]
        requirements: Vec<String>,
    },
    List {
        project_id: String,
        #[arg(long)]
        json: bool,
    },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{}: {}", error.code, error.message);
        let code = if error.code.starts_with("project.name.") {
            2
        } else {
            1
        };
        std::process::exit(code);
    }
}

fn run() -> Result<(), apex_domain::AppError> {
    let cli = Cli::parse();
    if let Command::Image { command } = &cli.command {
        return run_image_command(command);
    }
    if let Command::Project {
        command: ProjectCommand::Import { source, dry_run },
    } = &cli.command
    {
        if !dry_run {
            return Err(apex_domain::AppError::validation(
                "package.import.dry_run_required",
                "Der Import ist in dieser Phase nur mit --dry-run freigeschaltet; es wurden keine Daten geändert.",
            ));
        }
        let staging_root = std::env::temp_dir()
            .join("Apex Studio")
            .join("import-staging");
        let staged = stage_package(source, staging_root, PackageLimits::default())?;
        let preview = staged.preview();
        println!(
            "Dry Run gültig: {} · {} Commission(s) · {} Datei(en) · {} Bytes · Profil {}",
            preview.project_name,
            preview.commission_count,
            preview.file_count,
            preview.total_bytes,
            preview.manifest.export_profile.as_str()
        );
        for warning in &preview.warnings {
            println!("Hinweis: {warning}");
        }
        println!("Keine Projektdaten oder Uploadjobs wurden geändert.");
        return Ok(());
    }
    let database = SqliteDatabase::open_default()?;
    let repository = Arc::new(SqliteProjectRepository::new(database.clone()));
    let commission_repository = Arc::new(SqliteCommissionRepository::new(database));
    let clock = Arc::new(SystemClock);
    match cli.command {
        Command::Project {
            command:
                ProjectCommand::Create {
                    name,
                    description,
                    tags,
                },
        } => {
            let project = CreateProject::new(repository, clock, Arc::new(UuidV7Generator))
                .execute_input(apex_domain::CreateProjectInput {
                    name,
                    description,
                    tags,
                })?;
            println!(
                "Projekt angelegt: {} ({})",
                project.name.as_str(),
                project.id.0
            );
        }
        Command::Commission {
            command:
                CommissionCommand::Create {
                    project_id,
                    name,
                    client,
                    organisation,
                    commission_type,
                    description,
                    due_date,
                    requirements,
                },
        } => {
            let commission = CreateCommission::new(commission_repository, clock).execute(
                CreateCommissionInput {
                    project_id,
                    name,
                    client_name: client,
                    organisation_name: organisation,
                    commission_type,
                    description,
                    requirements,
                    due_date,
                },
            )?;
            println!(
                "Commission angelegt: {} ({})",
                commission.display_id, commission.id.0
            );
        }
        Command::Commission {
            command: CommissionCommand::List { project_id, json },
        } => {
            let commissions = ListCommissions::new(commission_repository).execute(&project_id)?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string(&commissions).map_err(|_| {
                        apex_domain::AppError::configuration(
                            "output.json",
                            "Commissiondaten konnten nicht serialisiert werden.",
                        )
                    })?
                );
            } else {
                for commission in commissions {
                    println!(
                        "{}  {}  {:?}",
                        commission.display_id, commission.name, commission.status
                    );
                }
            }
        }
        Command::Project {
            command:
                ProjectCommand::List {
                    json,
                    search,
                    include_archived,
                },
        } => {
            let projects = ListProjects::new(repository)
                .search(search.as_deref(), include_archived)?
                .into_iter()
                .map(ProjectDto::from)
                .collect::<Vec<_>>();
            if json {
                println!(
                    "{}",
                    serde_json::to_string(&projects).map_err(|_| {
                        apex_domain::AppError::configuration(
                            "output.json",
                            "Projektdaten konnten nicht serialisiert werden.",
                        )
                    })?
                );
            } else if projects.is_empty() {
                println!("Keine passenden Projekte gefunden.");
            } else {
                for project in projects {
                    println!("{}  {}", project.id, project.name);
                }
            }
        }
        Command::Project {
            command:
                ProjectCommand::Update {
                    id,
                    expected_revision,
                    name,
                    description,
                    tags,
                },
        } => {
            let project = UpdateProject::new(repository).execute(UpdateProjectInput {
                id,
                expected_revision,
                name,
                description,
                tags,
            })?;
            println!(
                "Projekt aktualisiert: {} (Revision {})",
                project.name.as_str(),
                project.revision
            );
        }
        Command::Project {
            command:
                ProjectCommand::Archive {
                    id,
                    expected_revision,
                },
        } => {
            let project = ArchiveProject::new(repository, clock).execute(ArchiveProjectInput {
                id,
                expected_revision,
            })?;
            println!(
                "Projekt archiviert: {} (Revision {})",
                project.name.as_str(),
                project.revision
            );
        }
        Command::Project {
            command:
                ProjectCommand::Export {
                    id,
                    destination,
                    profile,
                    confirm_private,
                },
        } => {
            let id = apex_domain::ProjectId::parse(&id)?;
            let project = repository.get(&id)?.ok_or_else(|| {
                apex_domain::AppError::validation(
                    "project.not_found",
                    "Das Projekt wurde nicht gefunden.",
                )
            })?;
            let commissions = commission_repository.list_for_project(&id)?;
            let profile: ExportProfile = profile.into();
            let request = ExportRequest {
                snapshot: ExportSnapshot {
                    project,
                    commissions,
                },
                profile,
                files: Default::default(),
            };
            let preview = preview_export(&request)?;
            println!(
                "Exportvorschau: {} · {} Commission(s) · {} Datei(en) · {} Bytes · Profil {}",
                preview.project_name,
                preview.commission_count,
                preview.file_count,
                preview.total_bytes,
                profile.as_str()
            );
            for metadata in &preview.included_metadata {
                println!("Enthalten: {metadata}");
            }
            for file in &preview.files {
                println!(
                    "Datei: {} · {} Bytes · {} · {}",
                    file.path,
                    file.size_bytes,
                    file.media_type,
                    if file.included {
                        "enthalten"
                    } else {
                        "ausgeschlossen"
                    }
                );
            }
            for path in &preview.unresolved_file_references {
                println!("Lokale Referenz nicht ausgewählt/verifiziert: {path}");
            }
            for reference in &preview.external_references {
                println!("Externe Referenz, nicht geladen: {reference}");
            }
            for warning in &preview.warnings {
                println!("Hinweis: {warning}");
            }
            if profile != ExportProfile::ClientShare && !confirm_private {
                return Err(apex_domain::AppError::validation(
                    "package.confirmation.required",
                    "Privater Export wurde nicht geschrieben. Prüfe die Vorschau und wiederhole den Befehl mit --confirm-private.",
                ));
            }
            let manifest = export_package(&request, &destination)?;
            println!(
                "Projektpaket erstellt: {} · Paket-ID {}",
                destination.display(),
                manifest.package_id
            );
        }
        Command::Project {
            command: ProjectCommand::Import { source, dry_run },
        } => {
            if !dry_run {
                return Err(apex_domain::AppError::validation(
                    "package.import.dry_run_required",
                    "Der Import ist in dieser Phase nur mit --dry-run freigeschaltet; es wurden keine Daten geändert.",
                ));
            }
            let staging_root = std::env::temp_dir()
                .join("Apex Studio")
                .join("import-staging");
            let staged = stage_package(source, staging_root, PackageLimits::default())?;
            let preview = staged.preview();
            println!(
                "Dry Run gültig: {} · {} Commission(s) · {} Datei(en) · {} Bytes · Profil {}",
                preview.project_name,
                preview.commission_count,
                preview.file_count,
                preview.total_bytes,
                preview.manifest.export_profile.as_str()
            );
            for warning in &preview.warnings {
                println!("Hinweis: {warning}");
            }
            println!("Keine Projektdaten oder Uploadjobs wurden geändert.");
        }
        Command::Image { command } => run_image_command(&command)?,
    }
    Ok(())
}

fn run_image_command(command: &ImageCommand) -> Result<(), apex_domain::AppError> {
    let path = match command {
        ImageCommand::Inspect { path, .. } | ImageCommand::Import { path, .. } => path,
    };
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let suggestion = parse_filename(filename, DEFAULT_INDEX_LIMIT);
    match command {
        ImageCommand::Inspect { json, .. } => {
            let entry = std::fs::symlink_metadata(path).map_err(|error| {
                apex_domain::AppError::validation(
                    "image.source.read",
                    &format!("Bilddatei konnte nicht gelesen werden: {error}"),
                )
            })?;
            if entry.file_type().is_symlink() || !entry.is_file() {
                return Err(apex_domain::AppError::validation(
                    "image.source.path",
                    "Bildpfad muss eine reguläre Datei und darf kein Symlink sein.",
                ));
            }
            if entry.len() > apex_images::MAX_FILE_BYTES as u64 {
                return Err(apex_domain::AppError::validation(
                    "image.file.too_large",
                    &format!(
                        "Bilddatei ist größer als {} Bytes.",
                        apex_images::MAX_FILE_BYTES
                    ),
                ));
            }
            let mut file = std::fs::File::open(path).map_err(|error| {
                apex_domain::AppError::validation("image.source.read", &error.to_string())
            })?;
            let mut bytes = Vec::with_capacity(entry.len() as usize);
            std::io::Read::by_ref(&mut file)
                .take((apex_images::MAX_FILE_BYTES + 1) as u64)
                .read_to_end(&mut bytes)
                .map_err(|error| {
                    apex_domain::AppError::validation("image.source.read", &error.to_string())
                })?;
            let metadata = inspect_png(&bytes).map_err(|error| {
                apex_domain::AppError::validation("image.png.invalid", &error.to_string())
            })?;
            if *json {
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({"image":metadata,"slot":suggestion}))
                        .map_err(|_| {
                            apex_domain::AppError::configuration(
                                "output.json",
                                "Bildprüfung konnte nicht serialisiert werden.",
                            )
                        })?
                );
            } else {
                println!(
                    "PNG gültig: {}×{} · {} Bytes · SHA-256 {}",
                    metadata.width, metadata.height, metadata.byte_len, metadata.file_sha256
                );
                println!(
                    "Zuordnung: {:?} {:?} · {:?} · {}",
                    suggestion.side, suggestion.index, suggestion.confidence, suggestion.reason
                );
                if suggestion.conflict {
                    println!(
                        "Manuelle Zuordnung erforderlich; kein Upload darf automatisch vorbereitet werden."
                    );
                }
            }
        }
        ImageCommand::Import { json, .. } => {
            let root = dirs::data_local_dir()
                .ok_or_else(|| {
                    apex_domain::AppError::configuration(
                        "image.store.path",
                        "Der lokale Anwendungsdatenordner ist nicht verfügbar.",
                    )
                })?
                .join("Apex Studio");
            let blob = BlobStore::new(root).import_path(path).map_err(|error| {
                apex_domain::AppError::validation("image.import.failed", &error.to_string())
            })?;
            if *json {
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({"blob":blob,"slot":suggestion}))
                        .map_err(|_| {
                            apex_domain::AppError::configuration(
                                "output.json",
                                "Bildimport konnte nicht serialisiert werden.",
                            )
                        })?
                );
            } else {
                println!(
                    "Unveränderlicher Blob importiert: {} · {}×{} · SHA-256 {}",
                    blob.relative_path,
                    blob.metadata.width,
                    blob.metadata.height,
                    blob.metadata.file_sha256
                );
                println!(
                    "Zuordnungsvorschlag: {:?} {:?} · {:?}",
                    suggestion.side, suggestion.index, suggestion.confidence
                );
                if suggestion.conflict {
                    println!(
                        "Manuelle Zuordnung erforderlich; kein Upload darf automatisch vorbereitet werden."
                    );
                }
            }
        }
    }
    Ok(())
}
