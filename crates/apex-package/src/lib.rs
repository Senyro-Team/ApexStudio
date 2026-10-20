//! Versioned, local-only `.apexstd` project packages.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

use apex_domain::{AppError, Commission, Project};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

pub const FORMAT_VERSION: u32 = 1;
pub const MANIFEST_FILE: &str = "manifest.json";
pub const PROJECT_FILE: &str = "project.json";
pub const CHECKSUMS_FILE: &str = "checksums.sha256";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportProfile {
    BackupPrivate,
    TeamTransferPrivate,
    ClientShare,
}

impl ExportProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BackupPrivate => "backup_private",
            Self::TeamTransferPrivate => "team_transfer_private",
            Self::ClientShare => "client_share",
        }
    }

    fn is_private(self) -> bool {
        self != Self::ClientShare
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PackageFileRecord {
    pub path: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub media_type: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PackageManifest {
    pub format_version: u32,
    pub package_id: String,
    pub project_id: String,
    pub project_revision: u64,
    pub app_version: String,
    pub exported_at: DateTime<Utc>,
    pub export_profile: ExportProfile,
    pub files: Vec<PackageFileRecord>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ExportSnapshot {
    pub project: Project,
    pub commissions: Vec<Commission>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ExportFile {
    pub bytes: Vec<u8>,
    pub media_type: String,
    pub classification: PackageContentClass,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PackageContentClass {
    FinalAsset,
    DesignSource,
    Reference,
    Note,
    History,
    Other,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ExportRequest {
    pub snapshot: ExportSnapshot,
    pub profile: ExportProfile,
    /// Files are an explicit per-export selection, keyed by package-relative path.
    pub files: BTreeMap<String, ExportFile>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ExportPreview {
    pub profile: ExportProfile,
    pub project_name: String,
    pub commission_count: usize,
    pub file_count: usize,
    pub total_bytes: u64,
    pub files: Vec<ExportFilePreview>,
    pub unresolved_file_references: Vec<String>,
    pub external_references: Vec<String>,
    pub included_metadata: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ExportFilePreview {
    pub path: String,
    pub size_bytes: u64,
    pub media_type: String,
    pub classification: PackageContentClass,
    pub included: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct ProjectDocument {
    format_version: u32,
    export_profile: ExportProfile,
    project: Value,
    commissions: Vec<Value>,
    provenance: Provenance,
    #[serde(default)]
    extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct Provenance {
    source_project_id: String,
    source_project_revision: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PackagePreview {
    pub manifest: PackageManifest,
    pub project_name: String,
    pub commission_count: usize,
    pub file_count: usize,
    pub total_bytes: u64,
    pub warnings: Vec<String>,
}

#[derive(Clone, Copy, Debug)]
pub struct PackageLimits {
    pub max_files: usize,
    pub max_total_bytes: u64,
    pub max_file_bytes: u64,
    pub max_manifest_bytes: u64,
    pub max_archive_bytes: u64,
}

impl Default for PackageLimits {
    fn default() -> Self {
        Self {
            max_files: 10_000,
            max_total_bytes: 2 * 1024 * 1024 * 1024,
            max_file_bytes: 100 * 1024 * 1024,
            max_manifest_bytes: 4 * 1024 * 1024,
            max_archive_bytes: 2_250 * 1024 * 1024,
        }
    }
}

pub struct StagedPackage {
    directory: tempfile::TempDir,
    preview: PackagePreview,
    project_document: Value,
}

impl StagedPackage {
    pub fn preview(&self) -> &PackagePreview {
        &self.preview
    }

    pub fn project_document(&self) -> &Value {
        &self.project_document
    }

    pub fn staged_path(&self, relative_path: &str) -> Result<PathBuf, AppError> {
        validate_package_path(relative_path, false)?;
        Ok(self.directory.path().join(relative_path))
    }
}

pub fn preview_export(request: &ExportRequest) -> Result<ExportPreview, AppError> {
    validate_request(request)?;
    let (document, included_metadata, mut warnings) = project_document(request)?;
    let files = selected_files(request);
    let document_bytes = serde_json::to_vec_pretty(&document).map_err(serialization_error)?;
    if document_bytes.len() as u64 > PackageLimits::default().max_file_bytes
        || files
            .values()
            .any(|file| file.bytes.len() as u64 > PackageLimits::default().max_file_bytes)
    {
        return Err(package_error(
            "package.file.limit",
            "Eine ausgewählte Paketdatei überschreitet die zulässige Größe.",
        ));
    }
    let total_bytes = files
        .values()
        .try_fold(document_bytes.len() as u64, |sum, file| {
            sum.checked_add(file.bytes.len() as u64)
        })
        .ok_or_else(|| package_error("package.size.overflow", "Das Paket ist zu groß."))?;
    if total_bytes > PackageLimits::default().max_total_bytes {
        return Err(package_error(
            "package.total_size.limit",
            "Die Auswahl überschreitet die zulässige Paketgröße.",
        ));
    }
    if request.profile.is_private() {
        warnings.push(
            "Dieses Paket enthält private Projektmetadaten. Prüfe die Dateiauswahl vor dem Export."
                .into(),
        );
    } else {
        warnings.push("Clientpakete enthalten nur die Client-Allowlist und als finale Assets klassifizierte Dateien.".into());
        let excluded = request.files.len() - files.len();
        if excluded > 0 {
            warnings.push(format!("{excluded} ausgewählte Datei(en) wurden ausgeschlossen, weil sie keine finalen Assets sind."));
        }
    }
    let files_for_preview = request
        .files
        .iter()
        .map(|(path, file)| ExportFilePreview {
            path: path.clone(),
            size_bytes: file.bytes.len() as u64,
            media_type: file.media_type.clone(),
            classification: file.classification,
            included: request.profile.is_private()
                || file.classification == PackageContentClass::FinalAsset,
        })
        .collect();
    let known_files: BTreeSet<_> = request.files.keys().cloned().collect();
    let unresolved_file_references: Vec<String> = request
        .snapshot
        .commissions
        .iter()
        .flat_map(|commission| commission.export_files.iter())
        .map(|path| path.as_str().to_owned())
        .filter(|path| !known_files.contains(path))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let external_references = request
        .snapshot
        .commissions
        .iter()
        .flat_map(|commission| commission.reference_materials.iter())
        .filter(|reference| {
            reference.uri.starts_with("https://") || reference.uri.starts_with("http://")
        })
        .map(|reference| reference.uri.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if !unresolved_file_references.is_empty() {
        warnings.push("Einige lokale Dateireferenzen sind in dieser Auswahl nicht enthalten; ihre Verfügbarkeit wurde nicht geprüft.".into());
    }
    if !external_references.is_empty() {
        warnings.push(
            "Externe Referenzen werden als Links aufgeführt und nicht automatisch geladen.".into(),
        );
    }
    Ok(ExportPreview {
        profile: request.profile,
        project_name: request.snapshot.project.name.as_str().into(),
        commission_count: request.snapshot.commissions.len(),
        file_count: files.len(),
        total_bytes,
        files: files_for_preview,
        unresolved_file_references,
        external_references,
        included_metadata,
        warnings,
    })
}

pub fn export_package(
    request: &ExportRequest,
    destination: impl AsRef<Path>,
) -> Result<PackageManifest, AppError> {
    let preview = preview_export(request)?;
    let destination = destination.as_ref();
    if destination.extension().and_then(|value| value.to_str()) != Some("apexstd") {
        return Err(package_error(
            "package.extension.invalid",
            "Projektpakete müssen die Endung .apexstd verwenden.",
        ));
    }
    if destination.exists() {
        return Err(package_error(
            "package.destination_exists",
            "Die Zieldatei existiert bereits; sie wurde nicht überschrieben.",
        ));
    }
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(io_error)?;
    let (document, _, _) = project_document(request)?;
    let files = selected_files(request);
    let project_bytes = serde_json::to_vec_pretty(&document).map_err(serialization_error)?;

    let mut file_records = Vec::with_capacity(files.len() + 1);
    file_records.push(PackageFileRecord {
        path: PROJECT_FILE.into(),
        size_bytes: project_bytes.len() as u64,
        sha256: sha256_hex(&project_bytes),
        media_type: "application/json".into(),
    });
    for (path, file) in &files {
        file_records.push(PackageFileRecord {
            path: path.clone(),
            size_bytes: file.bytes.len() as u64,
            sha256: sha256_hex(&file.bytes),
            media_type: file.media_type.clone(),
        });
    }
    file_records.sort_by(|a, b| a.path.cmp(&b.path));
    let manifest = PackageManifest {
        format_version: FORMAT_VERSION,
        package_id: Uuid::now_v7().to_string(),
        project_id: request.snapshot.project.id.0.to_string(),
        project_revision: request.snapshot.project.revision,
        app_version: env!("CARGO_PKG_VERSION").into(),
        exported_at: Utc::now(),
        export_profile: request.profile,
        files: file_records,
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(serialization_error)?;
    let mut checksum_lines = Vec::with_capacity(manifest.files.len() + 1);
    for record in &manifest.files {
        let hash = if record.path == PROJECT_FILE {
            sha256_hex(&project_bytes)
        } else {
            sha256_hex(&files[&record.path].bytes)
        };
        checksum_lines.push(format!("{hash}  {}", record.path));
    }
    checksum_lines.push(format!("{}  {MANIFEST_FILE}", sha256_hex(&manifest_bytes)));
    checksum_lines.sort();
    let checksum_bytes = checksum_lines.join("\n").into_bytes();

    let partial_path = unique_partial_path(parent)?;
    let result = write_zip(
        &partial_path,
        &manifest_bytes,
        &project_bytes,
        &checksum_bytes,
        &files,
    )
    .and_then(|()| publish_new_file(&partial_path, destination));
    if result.is_err() {
        let _ = fs::remove_file(&partial_path);
    }
    result?;
    let _ = preview;
    Ok(manifest)
}

pub fn stage_package(
    package_path: impl AsRef<Path>,
    staging_root: impl AsRef<Path>,
    limits: PackageLimits,
) -> Result<StagedPackage, AppError> {
    let package_path = package_path.as_ref();
    if fs::metadata(package_path).map_err(io_error)?.len() > limits.max_archive_bytes {
        return Err(package_error(
            "package.archive.limit",
            "Das komprimierte Paket überschreitet die zulässige Größe.",
        ));
    }
    let package_file = File::open(package_path).map_err(io_error)?;
    let mut archive = ZipArchive::new(package_file).map_err(zip_error)?;
    if archive.len() > limits.max_files.saturating_add(2) {
        return Err(package_error(
            "package.files.limit",
            "Das Paket überschreitet die Dateianzahlgrenze.",
        ));
    }
    fs::create_dir_all(staging_root.as_ref()).map_err(io_error)?;
    let staging = tempfile::Builder::new()
        .prefix("apexstd-")
        .tempdir_in(staging_root.as_ref())
        .map_err(io_error)?;
    let mut seen = BTreeSet::new();
    let mut records = BTreeMap::<String, (u64, String)>::new();
    let mut manifest_bytes = None;
    let mut project_bytes = None;
    let mut checksum_bytes = None;
    let mut actual_total = 0_u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(zip_error)?;
        let name = entry.name().map_err(zip_error)?.into_owned();
        validate_zip_entry(&entry, &name)?;
        let path = name.trim_end_matches('/');
        if path.is_empty() {
            return Err(package_error(
                "package.path.invalid",
                "Das Archiv enthält einen leeren Eintrag.",
            ));
        }
        validate_package_path(path, true)?;
        let folded = path.to_lowercase();
        if !seen.insert(folded) {
            return Err(package_error(
                "package.path.duplicate",
                "Das Archiv enthält doppelte oder case-insensitive kollidierende Pfade.",
            ));
        }
        if entry.is_dir() {
            continue;
        }
        let advertised_size = entry.size();
        let per_file_limit = if path == MANIFEST_FILE {
            limits.max_manifest_bytes
        } else {
            limits.max_file_bytes
        };
        if advertised_size > per_file_limit {
            return Err(package_error(
                "package.file.limit",
                "Eine Paketdatei überschreitet die zulässige Größe.",
            ));
        }
        actual_total = actual_total
            .checked_add(advertised_size)
            .ok_or_else(|| package_error("package.size.overflow", "Das Paket ist zu groß."))?;
        if actual_total > limits.max_total_bytes {
            return Err(package_error(
                "package.total_size.limit",
                "Das Paket überschreitet die entpackte Gesamtgröße.",
            ));
        }
        let is_metadata = matches!(path, MANIFEST_FILE | PROJECT_FILE | CHECKSUMS_FILE);
        let mut metadata_bytes = is_metadata.then(Vec::new);
        let output_path = if is_metadata {
            None
        } else {
            let output = staging.path().join(path);
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent).map_err(io_error)?;
            }
            Some(output)
        };
        let mut output_file = output_path
            .as_ref()
            .map(|output| {
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(output)
                    .map_err(io_error)
            })
            .transpose()?;
        let mut hasher = Sha256::new();
        let mut written = 0_u64;
        let mut limited = entry.take(per_file_limit + 1);
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let count = limited.read(&mut buffer).map_err(io_error)?;
            if count == 0 {
                break;
            }
            written = written
                .checked_add(count as u64)
                .ok_or_else(|| package_error("package.size.overflow", "Das Paket ist zu groß."))?;
            if written > per_file_limit {
                return Err(package_error(
                    "package.file.limit",
                    "Eine Paketdatei überschreitet die zulässige Größe.",
                ));
            }
            hasher.update(&buffer[..count]);
            if let Some(bytes) = &mut metadata_bytes {
                bytes.extend_from_slice(&buffer[..count]);
            }
            if let Some(output) = &mut output_file {
                output.write_all(&buffer[..count]).map_err(io_error)?;
            }
        }
        if written != advertised_size {
            return Err(package_error(
                "package.file.size_mismatch",
                "Eine Paketdatei entspricht nicht ihrer deklarierten Größe.",
            ));
        }
        if let Some(output) = &mut output_file {
            output.sync_all().map_err(io_error)?;
        }
        let hash: String = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if path == MANIFEST_FILE {
            manifest_bytes = metadata_bytes;
        } else if path == PROJECT_FILE {
            project_bytes = metadata_bytes;
        } else if path == CHECKSUMS_FILE {
            checksum_bytes = metadata_bytes;
        }
        records.insert(path.to_owned(), (written, hash));
    }

    let manifest_bytes = manifest_bytes
        .as_deref()
        .ok_or_else(|| package_error("package.manifest.missing", "Das Manifest fehlt."))?;
    let manifest: PackageManifest =
        serde_json::from_slice(manifest_bytes).map_err(serialization_error)?;
    if manifest.format_version != FORMAT_VERSION {
        return Err(package_error(
            "package.format.unsupported",
            "Diese .apexstd-Formatversion wird nicht unterstützt.",
        ));
    }
    if manifest.files.len() > limits.max_files {
        return Err(package_error(
            "package.files.limit",
            "Das Manifest überschreitet die Dateianzahlgrenze.",
        ));
    }
    let project_bytes = project_bytes
        .as_deref()
        .ok_or_else(|| package_error("package.project.missing", "project.json fehlt."))?;
    let project_document: Value =
        serde_json::from_slice(project_bytes).map_err(serialization_error)?;
    if project_document
        .get("format_version")
        .and_then(Value::as_u64)
        != Some(FORMAT_VERSION as u64)
    {
        return Err(package_error(
            "package.project.version",
            "Die project.json-Formatversion wird nicht unterstützt.",
        ));
    }
    let project_name = project_document
        .pointer("/project/name")
        .and_then(Value::as_str)
        .unwrap_or("Unbenanntes Projekt")
        .to_owned();
    let commission_count = project_document
        .get("commissions")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    if project_document
        .get("extensions")
        .is_some_and(|v| !v.is_object())
    {
        return Err(package_error(
            "package.extensions.invalid",
            "Der Namespaced-Erweiterungsbereich muss ein Objekt sein.",
        ));
    }
    let project_id = project_document
        .pointer("/project/id")
        .and_then(Value::as_str);
    let provenance_id = project_document
        .pointer("/provenance/source_project_id")
        .and_then(Value::as_str);
    let provenance_revision = project_document
        .pointer("/provenance/source_project_revision")
        .and_then(Value::as_u64);
    let document_profile = project_document
        .get("export_profile")
        .and_then(Value::as_str);
    if project_id != Some(manifest.project_id.as_str())
        || provenance_id != Some(manifest.project_id.as_str())
        || provenance_revision != Some(manifest.project_revision)
        || document_profile != Some(manifest.export_profile.as_str())
    {
        return Err(package_error(
            "package.manifest.inconsistent",
            "Manifest und project.json beschreiben unterschiedliche Projektstände oder Exportprofile.",
        ));
    }
    let manifest_paths: BTreeSet<_> = manifest
        .files
        .iter()
        .map(|record| record.path.clone())
        .collect();
    if manifest_paths.len() != manifest.files.len() {
        return Err(package_error(
            "package.manifest.duplicate",
            "Das Manifest enthält doppelte Dateipfade.",
        ));
    }
    let actual_payload_paths: BTreeSet<_> = records
        .keys()
        .filter(|path| path.as_str() != MANIFEST_FILE && path.as_str() != CHECKSUMS_FILE)
        .cloned()
        .collect();
    if actual_payload_paths != manifest_paths {
        return Err(package_error(
            "package.manifest.inconsistent",
            "Manifest und ZIP-Dateiliste stimmen nicht überein.",
        ));
    }
    for record in &manifest.files {
        validate_package_path(&record.path, true)?;
        if record.media_type.is_empty()
            || record.media_type.len() > 128
            || record.media_type.chars().any(char::is_control)
        {
            return Err(package_error(
                "package.media_type.invalid",
                "Ein Medientyp ist ungültig.",
            ));
        }
        let (size, hash) = records.get(&record.path).ok_or_else(|| {
            package_error(
                "package.file.missing",
                "Eine im Manifest genannte Datei fehlt.",
            )
        })?;
        if *size != record.size_bytes || hash != &record.sha256 {
            return Err(package_error(
                "package.file.integrity",
                "Größe oder SHA-256 einer Paketdatei stimmt nicht.",
            ));
        }
    }
    let checksum_bytes = checksum_bytes
        .as_deref()
        .ok_or_else(|| package_error("package.checksums.missing", "checksums.sha256 fehlt."))?;
    let checksums = parse_checksums(checksum_bytes)?;
    let mut expected_checksums = BTreeMap::new();
    expected_checksums.insert(MANIFEST_FILE.to_owned(), records[MANIFEST_FILE].1.clone());
    for record in &manifest.files {
        expected_checksums.insert(record.path.clone(), record.sha256.clone());
    }
    if checksums != expected_checksums {
        return Err(package_error(
            "package.checksums.invalid",
            "checksums.sha256 ist unvollständig oder inkonsistent.",
        ));
    }

    let included = manifest
        .files
        .iter()
        .filter(|record| record.path != PROJECT_FILE)
        .count();
    let payload_bytes = manifest
        .files
        .iter()
        .try_fold(0_u64, |sum, record| sum.checked_add(record.size_bytes))
        .ok_or_else(|| package_error("package.size.overflow", "Das Paket ist zu groß."))?;
    let preview = PackagePreview {
        manifest,
        project_name,
        commission_count,
        file_count: included,
        total_bytes: payload_bytes,
        warnings: vec!["Import wurde nur in isoliertes Staging entpackt. Datenbank und Projektdateien wurden nicht verändert.".into()],
    };
    Ok(StagedPackage {
        directory: staging,
        preview,
        project_document,
    })
}

fn project_document(
    request: &ExportRequest,
) -> Result<(ProjectDocument, Vec<String>, Vec<String>), AppError> {
    let project = serde_json::to_value(&request.snapshot.project).map_err(serialization_error)?;
    if request.profile.is_private() {
        let commissions = request
            .snapshot
            .commissions
            .iter()
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()
            .map_err(serialization_error)?;
        Ok((
            ProjectDocument {
                format_version: FORMAT_VERSION,
                export_profile: request.profile,
                project,
                commissions,
                provenance: Provenance {
                    source_project_id: request.snapshot.project.id.0.to_string(),
                    source_project_revision: request.snapshot.project.revision,
                },
                extensions: BTreeMap::new(),
            },
            vec![
                "project metadata".into(),
                "commission workflow and private metadata".into(),
            ],
            Vec::new(),
        ))
    } else {
        let commissions = request
            .snapshot
            .commissions
            .iter()
            .map(|commission| serde_json::to_value(commission.client_share()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(serialization_error)?;
        let safe_project = serde_json::json!({
            "id": request.snapshot.project.id,
            "name": request.snapshot.project.name,
            "description": request.snapshot.project.description,
        });
        Ok((ProjectDocument {
            format_version: FORMAT_VERSION,
            export_profile: request.profile,
            project: safe_project,
            commissions,
            provenance: Provenance { source_project_id: request.snapshot.project.id.0.to_string(), source_project_revision: request.snapshot.project.revision },
            extensions: BTreeMap::new(),
        }, vec!["client-approved project metadata".into(), "client-allowlisted commission fields".into()], vec!["Client-Share schließt interne, finanzielle und nicht freigegebene Commission-Felder aus.".into()]))
    }
}

fn validate_request(request: &ExportRequest) -> Result<(), AppError> {
    if request
        .snapshot
        .commissions
        .iter()
        .any(|commission| commission.project_id != request.snapshot.project.id)
    {
        return Err(package_error(
            "package.snapshot.inconsistent",
            "Die Auswahl enthält eine Commission aus einem anderen Projekt.",
        ));
    }
    let mut folded_paths = BTreeSet::new();
    for (path, file) in &request.files {
        validate_package_path(path, false)?;
        if [MANIFEST_FILE, PROJECT_FILE, CHECKSUMS_FILE].contains(&path.as_str()) {
            return Err(package_error(
                "package.path.reserved",
                "Ein Dateipfad ist für Paketmetadaten reserviert.",
            ));
        }
        if !folded_paths.insert(path.to_lowercase()) {
            return Err(package_error(
                "package.path.duplicate",
                "Die Dateiauswahl enthält case-insensitive kollidierende Pfade.",
            ));
        }
        if file.media_type.is_empty()
            || file.media_type.len() > 128
            || file.media_type.chars().any(char::is_control)
        {
            return Err(package_error(
                "package.media_type.invalid",
                "Ein Medientyp ist ungültig.",
            ));
        }
    }
    if request.files.len() > PackageLimits::default().max_files {
        return Err(package_error(
            "package.files.limit",
            "Die Dateiauswahl überschreitet die Dateianzahlgrenze.",
        ));
    }
    Ok(())
}

fn selected_files(request: &ExportRequest) -> BTreeMap<String, ExportFile> {
    request
        .files
        .iter()
        .filter(|(_, file)| {
            request.profile.is_private() || file.classification == PackageContentClass::FinalAsset
        })
        .map(|(path, file)| (path.clone(), file.clone()))
        .collect()
}

fn write_zip(
    path: &Path,
    manifest: &[u8],
    project: &[u8],
    checksums: &[u8],
    files: &BTreeMap<String, ExportFile>,
) -> Result<(), AppError> {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io_error)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    zip.start_file(MANIFEST_FILE, options).map_err(zip_error)?;
    zip.write_all(manifest).map_err(io_error)?;
    zip.start_file(PROJECT_FILE, options).map_err(zip_error)?;
    zip.write_all(project).map_err(io_error)?;
    for (name, entry) in files {
        zip.start_file(name, options).map_err(zip_error)?;
        zip.write_all(&entry.bytes).map_err(io_error)?;
    }
    zip.start_file(CHECKSUMS_FILE, options).map_err(zip_error)?;
    zip.write_all(checksums).map_err(io_error)?;
    let file = zip.finish().map_err(zip_error)?;
    file.sync_all().map_err(io_error)
}

fn validate_zip_entry<R: Read>(
    entry: &zip::read::ZipFile<'_, R>,
    name: &str,
) -> Result<(), AppError> {
    if let Some(mode) = entry.unix_mode() {
        let file_type = mode & 0o170000;
        if file_type != 0 && file_type != 0o100000 && file_type != 0o040000 {
            return Err(package_error(
                "package.entry.special_file",
                "Symlinks und spezielle Dateitypen sind im Paket nicht zulässig.",
            ));
        }
    }
    if name.contains('\\') || name.starts_with('/') || name.starts_with("//") {
        return Err(package_error(
            "package.path.invalid",
            "Paketpfade müssen relative POSIX-Pfade sein.",
        ));
    }
    Ok(())
}

fn validate_package_path(value: &str, allow_directory: bool) -> Result<(), AppError> {
    if value.is_empty()
        || value.len() > 1024
        || value.contains(['\\', ':', '\0', '\n', '\r'])
        || value.starts_with('/')
        || value.starts_with("//")
    {
        return Err(package_error(
            "package.path.invalid",
            "Ein Paketpfad ist ungültig.",
        ));
    }
    let path = Path::new(value);
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(package_error(
            "package.path.invalid",
            "Absolute und übergeordnete Paketpfade sind nicht zulässig.",
        ));
    }
    for component in value.trim_end_matches('/').split('/') {
        if component.is_empty()
            || component.ends_with([' ', '.'])
            || component.chars().any(char::is_control)
        {
            return Err(package_error(
                "package.path.invalid",
                "Ein Pfadbestandteil ist für Windows nicht zulässig.",
            ));
        }
        let stem = component
            .split('.')
            .next()
            .unwrap_or("")
            .to_ascii_uppercase();
        let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9'));
        if reserved {
            return Err(package_error(
                "package.path.reserved_name",
                "Der Paketpfad enthält einen reservierten Windows-Dateinamen.",
            ));
        }
    }
    if !allow_directory && value.ends_with('/') {
        return Err(package_error(
            "package.path.invalid",
            "Verzeichnisse sind hier nicht als Nutzdatei zulässig.",
        ));
    }
    Ok(())
}

fn parse_checksums(bytes: &[u8]) -> Result<BTreeMap<String, String>, AppError> {
    let text = std::str::from_utf8(bytes).map_err(|_| {
        package_error(
            "package.checksums.encoding",
            "checksums.sha256 muss UTF-8 sein.",
        )
    })?;
    let mut result = BTreeMap::new();
    for line in text.lines() {
        let raw = line.as_bytes();
        if raw.len() < 67 || &raw[64..66] != b"  " {
            return Err(package_error(
                "package.checksums.invalid",
                "Eine Checksum-Zeile hat ein ungültiges Format.",
            ));
        }
        let hash = &raw[..64];
        if !hash.iter().all(u8::is_ascii_hexdigit) {
            return Err(package_error(
                "package.checksums.invalid",
                "Eine Checksum-Zeile enthält keinen SHA-256-Hash.",
            ));
        }
        let path = std::str::from_utf8(&raw[66..]).map_err(|_| {
            package_error(
                "package.checksums.encoding",
                "Ein Checksum-Pfad ist kein gültiges UTF-8.",
            )
        })?;
        let hash = std::str::from_utf8(hash).map_err(|_| {
            package_error(
                "package.checksums.invalid",
                "Eine Checksum-Zeile enthält keinen gültigen SHA-256-Hash.",
            )
        })?;
        validate_package_path(path, false)?;
        if result
            .insert(path.to_owned(), hash.to_ascii_lowercase())
            .is_some()
        {
            return Err(package_error(
                "package.checksums.duplicate",
                "checksums.sha256 enthält doppelte Pfade.",
            ));
        }
    }
    Ok(result)
}

fn unique_partial_path(parent: &Path) -> Result<PathBuf, AppError> {
    for _ in 0..10 {
        let path = parent.join(format!(".apexstd-{}.partial", Uuid::now_v7()));
        if !path.exists() {
            return Ok(path);
        }
    }
    Err(package_error(
        "package.temp.unavailable",
        "Es konnte keine temporäre Exportdatei angelegt werden.",
    ))
}

fn publish_new_file(partial: &Path, destination: &Path) -> Result<(), AppError> {
    if destination.exists() {
        return Err(package_error(
            "package.destination_exists",
            "Die Zieldatei existiert bereits; sie wurde nicht überschrieben.",
        ));
    }
    fs::hard_link(partial, destination).map_err(io_error)?;
    fs::remove_file(partial).map_err(io_error)
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn serialization_error(error: serde_json::Error) -> AppError {
    package_error("package.serialization", error.to_string())
}
fn io_error(error: std::io::Error) -> AppError {
    package_error("package.io", error.to_string())
}
fn zip_error(error: zip::result::ZipError) -> AppError {
    package_error("package.zip", error.to_string())
}
fn package_error(code: &str, message: impl Into<String>) -> AppError {
    AppError {
        code: code.into(),
        message: message.into(),
        retryable: false,
        details: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apex_domain::{CommissionId, ProjectId, ProjectName};
    use chrono::Utc;

    fn sample_request(profile: ExportProfile) -> ExportRequest {
        let project = Project::with_id(
            ProjectId::new_v7(),
            ProjectName::parse("Wärmeinsatz").unwrap(),
            Utc::now(),
        );
        let commission = Commission {
            id: CommissionId::new_v7(),
            project_id: project.id.clone(),
            display_id: "COM-2026-0001".into(),
            name: "Fleet".into(),
            client_id: None,
            client_name: Some("Client".into()),
            organisation_id: None,
            organisation_name: None,
            commission_type: Some("Livery".into()),
            description: Some("Shareable".into()),
            requirements: vec!["left".into()],
            reference_materials: vec![],
            created_at: Utc::now(),
            due_date: None,
            status: apex_domain::CommissionStatus::Draft,
            priority: None,
            assigned_designer_id: None,
            assigned_designer_handle: None,
            started_at: None,
            completed_at: None,
            delivered_at: None,
            revision_count: 0,
            internal_notes: Some("secret".into()),
            design_type: None,
            vehicle_model: None,
            template_reference: None,
            figma_file_url: None,
            export_files: vec![],
            roblox_identifiers: vec![],
            file_hashes: vec![],
            version: None,
            total_price_minor: Some(1250),
            currency: Some(apex_domain::CurrencyCode::parse("EUR").unwrap()),
            currency_precision: Some(2),
            payment_status: Some(apex_domain::PaymentStatus::Unpaid),
            payment_method: Some("private".into()),
            usage_rights: None,
            exclusivity: None,
            license_notes: None,
            revision: 1,
        };
        ExportRequest {
            snapshot: ExportSnapshot {
                project,
                commissions: vec![commission],
            },
            profile,
            files: BTreeMap::from([(
                "assets/final.png".into(),
                ExportFile {
                    bytes: b"png bytes".to_vec(),
                    media_type: "image/png".into(),
                    classification: PackageContentClass::FinalAsset,
                },
            )]),
        }
    }

    #[test]
    fn client_share_uses_allowlist_and_private_profile_warns() {
        let mut request = sample_request(ExportProfile::ClientShare);
        request.files.insert(
            "sources/design.psd".into(),
            ExportFile {
                bytes: b"private source".to_vec(),
                media_type: "image/vnd.adobe.photoshop".into(),
                classification: PackageContentClass::DesignSource,
            },
        );
        let preview = preview_export(&request).unwrap();
        assert_eq!(preview.file_count, 1);
        assert!(
            preview
                .warnings
                .iter()
                .any(|warning| warning.contains("ausgeschlossen"))
        );
        let (document, _, _) = project_document(&request).unwrap();
        let serialized = serde_json::to_string(&document).unwrap();
        assert!(!serialized.contains("internal_notes"));
        assert!(!serialized.contains("payment_method"));
        assert!(serialized.contains("Shareable"));
        assert!(
            preview_export(&sample_request(ExportProfile::BackupPrivate))
                .unwrap()
                .warnings
                .iter()
                .any(|warning| warning.contains("private"))
        );
    }

    #[test]
    fn package_roundtrip_verifies_manifest_checksums_and_extracts_to_staging() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("project.apexstd");
        let request = sample_request(ExportProfile::TeamTransferPrivate);
        let manifest = export_package(&request, &destination).unwrap();
        assert_eq!(manifest.format_version, 1);
        let staged = stage_package(
            &destination,
            directory.path().join("staging"),
            PackageLimits::default(),
        )
        .unwrap();
        assert_eq!(staged.preview().project_name, "Wärmeinsatz");
        assert_eq!(staged.preview().commission_count, 1);
        assert_eq!(
            fs::read(staged.staged_path("assets/final.png").unwrap()).unwrap(),
            b"png bytes"
        );
        assert!(staged.project_document().to_string().contains("secret"));
    }

    #[test]
    fn client_share_archive_omits_selected_design_sources() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("client.apexstd");
        let mut request = sample_request(ExportProfile::ClientShare);
        request.files.insert(
            "sources/private.psd".into(),
            ExportFile {
                bytes: b"private source".to_vec(),
                media_type: "image/vnd.adobe.photoshop".into(),
                classification: PackageContentClass::DesignSource,
            },
        );
        export_package(&request, &destination).unwrap();
        let staged = stage_package(
            &destination,
            directory.path().join("staging"),
            PackageLimits::default(),
        )
        .unwrap();
        assert!(staged.staged_path("assets/final.png").unwrap().exists());
        assert!(!staged.staged_path("sources/private.psd").unwrap().exists());
        let document = staged.project_document().to_string();
        assert!(!document.contains("internal_notes"));
        assert!(!document.contains("payment_method"));
    }

    #[test]
    fn manifest_hash_mismatch_is_rejected_before_staged_import_is_returned() {
        let directory = tempfile::tempdir().unwrap();
        let package_path = directory.path().join("tampered.apexstd");
        let request = sample_request(ExportProfile::BackupPrivate);
        let (document, _, _) = project_document(&request).unwrap();
        let project_bytes = serde_json::to_vec_pretty(&document).unwrap();
        let manifest = PackageManifest {
            format_version: FORMAT_VERSION,
            package_id: Uuid::now_v7().to_string(),
            project_id: request.snapshot.project.id.0.to_string(),
            project_revision: request.snapshot.project.revision,
            app_version: "0.1.0".into(),
            exported_at: Utc::now(),
            export_profile: ExportProfile::BackupPrivate,
            files: vec![PackageFileRecord {
                path: PROJECT_FILE.into(),
                size_bytes: project_bytes.len() as u64,
                sha256: "0".repeat(64),
                media_type: "application/json".into(),
            }],
        };
        let manifest_bytes = serde_json::to_vec_pretty(&manifest).unwrap();
        let checksums = format!(
            "{}  {PROJECT_FILE}\n{}  {MANIFEST_FILE}",
            "0".repeat(64),
            sha256_hex(&manifest_bytes)
        );
        let mut zip = ZipWriter::new(File::create(&package_path).unwrap());
        for (name, bytes) in [
            (MANIFEST_FILE, manifest_bytes.as_slice()),
            (PROJECT_FILE, project_bytes.as_slice()),
            (CHECKSUMS_FILE, checksums.as_bytes()),
        ] {
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
        let error = stage_package(
            &package_path,
            directory.path().join("staging"),
            PackageLimits::default(),
        )
        .err()
        .unwrap();
        assert_eq!(error.code, "package.file.integrity");
    }

    #[test]
    fn malicious_paths_and_duplicate_casefolded_paths_are_rejected() {
        for path in [
            "../secret",
            "C:/secret",
            "\\\\server\\share",
            "assets/CON.txt",
            "assets/file:stream",
            "assets/trailing. ",
        ] {
            assert!(
                validate_package_path(path, false).is_err(),
                "accepted {path:?}"
            );
        }
        assert!(validate_package_path("assets/Left.png", false).is_ok());
        let malformed = format!("{}é  assets/x", "a".repeat(63));
        assert!(parse_checksums(malformed.as_bytes()).is_err());
    }

    #[test]
    fn zip_slip_entry_is_rejected_before_writing_outside_staging() {
        let directory = tempfile::tempdir().unwrap();
        let archive_path = directory.path().join("hostile.apexstd");
        let file = File::create(&archive_path).unwrap();
        let mut zip = ZipWriter::new(file);
        zip.start_file("../outside.txt", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"must not escape").unwrap();
        zip.finish().unwrap();
        let staging_root = directory.path().join("staging");
        let error = stage_package(&archive_path, &staging_root, PackageLimits::default())
            .err()
            .unwrap();
        assert_eq!(error.code, "package.path.invalid");
        assert!(!directory.path().join("outside.txt").exists());
    }

    #[test]
    fn existing_export_destination_is_not_overwritten() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("project.apexstd");
        fs::write(&destination, b"keep").unwrap();
        assert_eq!(
            export_package(&sample_request(ExportProfile::BackupPrivate), &destination)
                .unwrap_err()
                .code,
            "package.destination_exists"
        );
        assert_eq!(fs::read(destination).unwrap(), b"keep");
    }
}
