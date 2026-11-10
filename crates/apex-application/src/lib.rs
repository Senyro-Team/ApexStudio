//! Shared application services for desktop, terminal, and future MCP adapters.
//! Presentation layers should call these services instead of reimplementing
//! validation or storage rules.

use apex_core::{
    Asset, AssetBoard, AssetFolder, AssetVersion, Error, ImportReport, Note, Project, ProjectInput,
    PublishingDraft, PublishingInput, PublishingPreview, WorkspaceInfo, WorkspaceRepository,
};
use apex_storage::Workspace;
use std::path::{Path, PathBuf};

mod roblox_credentials;
pub use roblox_credentials::{
    clear_roblox_credentials, load_roblox_credentials, prepare_roblox_credentials,
    save_roblox_credentials, store_roblox_credentials, validate_roblox_credentials,
    RobloxCredentials, ValidatedRobloxCredentials,
};

pub use apex_storage::{
    LiveryUploads, PackagePreview, PostSourceStatus, PostSummary, QueueSnapshot,
    RobloxLiverySummary, TrashItem, TrashPreview, UploadJob,
};

#[derive(Clone)]
pub struct ApplicationServices {
    workspace: Workspace,
}

impl ApplicationServices {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, Error> {
        Workspace::open(root).map(Self::new)
    }

    pub fn new(workspace: Workspace) -> Self {
        Self { workspace }
    }

    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    pub fn workspace_info(&self) -> Result<WorkspaceInfo, Error> {
        self.workspace.info()
    }

    pub fn posts(&self) -> Result<Vec<PostSummary>, Error> {
        self.workspace.posts()
    }

    pub fn post(&self, id: &str) -> Result<PublishingDraft, Error> {
        self.workspace.post(id)
    }

    pub fn create_post(&self) -> Result<String, Error> {
        self.workspace.create_post()
    }

    pub fn import_post_bundle(&self, source: &Path) -> Result<String, Error> {
        self.workspace.import_post_bundle(source)
    }

    pub fn save_post(
        &self,
        id: &str,
        revision: u64,
        data: PublishingInput,
    ) -> Result<PublishingDraft, Error> {
        self.workspace.save_post(id, revision, data)
    }

    pub fn post_sources(&self, id: &str) -> Result<Vec<PostSourceStatus>, Error> {
        self.workspace.post_sources(id)
    }

    pub fn add_post_images(
        &self,
        id: &str,
        paths: &[PathBuf],
    ) -> Result<Vec<PublishingPreview>, Error> {
        self.workspace
            .add_post_images(&format!("editor:{id}"), paths)
    }

    pub fn release_post_image_pins(&self, id: &str) -> Result<(), Error> {
        self.workspace.release_content(&format!("editor:{id}"))
    }

    pub fn export_post(
        &self,
        id: &str,
        revision: u64,
        reviewed: bool,
        destination: &Path,
    ) -> Result<(), Error> {
        self.workspace
            .export_post(id, revision, reviewed, destination)
    }

    pub fn roblox_liveries(&self, account_id: &str) -> Result<Vec<RobloxLiverySummary>, Error> {
        self.workspace.roblox_liveries(account_id)
    }

    pub fn livery_uploads(
        &self,
        folder_id: &str,
        account_id: &str,
    ) -> Result<LiveryUploads, Error> {
        self.workspace.livery_uploads(folder_id, account_id)
    }

    pub fn upload_queue(&self) -> Result<QueueSnapshot, Error> {
        self.workspace.upload_snapshot()
    }

    pub fn set_roblox_double_upload(&self, enabled: bool) -> Result<(), Error> {
        self.workspace.set_roblox_double_upload(enabled)
    }

    pub fn roblox_double_upload(&self) -> Result<bool, Error> {
        self.workspace.roblox_double_upload()
    }

    pub fn create_roblox_batch(
        &self,
        request_id: &str,
        account_id: &str,
        livery_ids: &[String],
        force: bool,
    ) -> Result<String, Error> {
        self.workspace
            .create_upload_batch(request_id, account_id, livery_ids, force)
    }

    pub fn control_roblox_batch(&self, batch_id: Option<&str>, action: &str) -> Result<(), Error> {
        self.workspace.control_upload_batch(batch_id, action)
    }

    pub fn rename_workspace(&self, name: &str) -> Result<WorkspaceInfo, Error> {
        apex_core::rename_workspace(&self.workspace, name)
    }

    pub fn preview_archive(&self, path: &Path) -> Result<PackagePreview, Error> {
        self.workspace.preview_package(path)
    }

    pub fn export_project_package(
        &self,
        id: &str,
        include_assets: bool,
        include_notes: bool,
        destination: &Path,
    ) -> Result<(), Error> {
        self.workspace
            .export_project(id, include_assets, include_notes, destination)
    }

    pub fn import_project_package(&self, path: &Path) -> Result<String, Error> {
        self.workspace.import_package(path)
    }

    pub fn create_workspace_backup(&self, destination: &Path) -> Result<(), Error> {
        self.workspace.backup(destination)
    }

    pub fn restore_workspace_backup(&self, path: &Path) -> Result<PathBuf, Error> {
        self.workspace.restore_backup(path)
    }

    pub fn projects(&self) -> Result<Vec<Project>, Error> {
        self.workspace.projects()
    }

    pub fn project(&self, id: &str) -> Result<Project, Error> {
        self.workspace.project(id)
    }

    pub fn save_project(&self, id: Option<&str>, fields: ProjectInput) -> Result<Project, Error> {
        self.workspace.save_project(id, fields)
    }

    pub fn archive_project(&self, id: &str, archived: bool) -> Result<Project, Error> {
        self.workspace.archive_project(id, archived)
    }

    pub fn delete_archived_project(&self, id: &str) -> Result<(), Error> {
        self.workspace.delete_archived_project(id)
    }

    pub fn notes(&self, project_id: Option<&str>) -> Result<Vec<Note>, Error> {
        self.workspace.notes(project_id)
    }

    pub fn note(&self, id: &str) -> Result<Note, Error> {
        self.workspace.note(id)
    }

    pub fn create_note(&self, project_id: Option<&str>) -> Result<Note, Error> {
        self.workspace.create_note(project_id)
    }

    pub fn save_note(
        &self,
        id: &str,
        revision: u32,
        title: &str,
        document: &serde_json::Value,
        markdown: &str,
    ) -> Result<Note, Error> {
        self.workspace
            .save_note(id, revision, title, document, markdown)
    }

    pub fn save_note_markdown(
        &self,
        id: &str,
        revision: u32,
        title: &str,
        markdown: &str,
        base_directory: &Path,
    ) -> Result<Note, Error> {
        let mut image_rewrites = Vec::new();
        let document = markdown_to_document(markdown, |source| {
            if let Some(hash) = source.strip_prefix("apex-image:") {
                return Ok(format!("apex-image:{hash}"));
            }
            let path = Path::new(source);
            let path = if path.is_absolute() {
                path.to_owned()
            } else {
                base_directory.join(path)
            };
            let hash = self.workspace.attach_note_image(id, &path)?;
            let managed = format!("apex-image:{hash}");
            image_rewrites.push((source.to_owned(), managed.clone()));
            Ok(managed)
        })?;
        let markdown =
            image_rewrites
                .into_iter()
                .fold(markdown.to_owned(), |content, (source, managed)| {
                    content.replace(&format!("]({source}"), &format!("]({managed}"))
                });
        self.workspace
            .save_note(id, revision, title, &document, &markdown)
    }

    pub fn archive_note(&self, id: &str, revision: u32, archived: bool) -> Result<Note, Error> {
        self.workspace.archive_note(id, revision, archived)
    }

    pub fn delete_archived_note(&self, id: &str, revision: u32) -> Result<(), Error> {
        self.workspace.delete_archived_note(id, revision)
    }

    pub fn attach_note_image(&self, id: &str, path: &Path) -> Result<String, Error> {
        self.workspace.attach_note_image(id, path)
    }

    pub fn note_image(&self, id: &str, hash: &str) -> Result<String, Error> {
        self.workspace.note_image(id, hash)
    }

    pub fn asset_board(&self, project_id: &str) -> Result<AssetBoard, Error> {
        self.workspace.board(project_id)
    }

    pub fn save_folder(
        &self,
        project_id: &str,
        id: Option<&str>,
        parent_id: Option<&str>,
        name: &str,
    ) -> Result<AssetFolder, Error> {
        self.workspace.save_folder(project_id, id, parent_id, name)
    }

    pub fn move_asset(&self, asset_id: &str, folder_id: Option<&str>) -> Result<(), Error> {
        self.workspace.move_asset(asset_id, folder_id)
    }

    pub fn assign_asset_slot(&self, asset_id: &str, slot: Option<&str>) -> Result<(), Error> {
        self.workspace.assign_slot(asset_id, slot)
    }

    pub fn copy_asset_item(
        &self,
        project_id: &str,
        id: &str,
        is_folder: bool,
        folder_id: Option<&str>,
    ) -> Result<(), Error> {
        self.workspace
            .copy_item(project_id, id, is_folder, folder_id)
    }

    pub fn import_files(
        &self,
        project_id: &str,
        folder_id: Option<&str>,
        paths: &[PathBuf],
    ) -> Result<ImportReport, Error> {
        let prior_ids: std::collections::HashSet<String> = self
            .workspace
            .assets(Some(project_id))?
            .into_iter()
            .map(|asset| asset.id)
            .collect();
        let mut report = self
            .workspace
            .import_files_into(project_id, folder_id, paths)?;
        report
            .imported
            .retain(|asset| !prior_ids.contains(&asset.id));
        Ok(report)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Adapter forwards the shared import_livery service's typed fields."
    )]
    pub fn import_livery(
        &self,
        project_id: &str,
        parent_id: Option<&str>,
        existing_id: Option<&str>,
        name: &str,
        vehicle_id: &str,
        files: &[(PathBuf, String, Option<String>)],
        cover_index: Option<usize>,
    ) -> Result<String, Error> {
        self.workspace.import_livery(
            project_id,
            parent_id,
            existing_id,
            name,
            vehicle_id,
            files,
            cover_index,
        )
    }

    pub fn set_folder_cover(&self, folder_id: &str, path: &Path) -> Result<(), Error> {
        self.workspace.set_cover(folder_id, path)
    }

    pub fn asset_versions(&self, asset_id: &str) -> Result<Vec<AssetVersion>, Error> {
        self.workspace.versions(asset_id)
    }

    pub fn restore_asset_version(&self, asset_id: &str, version_id: &str) -> Result<(), Error> {
        self.workspace.restore_version(asset_id, version_id)
    }

    pub fn export_asset(&self, asset_id: &str, destination: &Path) -> Result<(), Error> {
        self.workspace.save_asset_copy(asset_id, destination)
    }

    pub fn trash_preview(&self, id: &str, is_folder: bool) -> Result<TrashPreview, Error> {
        self.workspace.trash_preview(id, is_folder)
    }

    pub fn trash_item(&self, id: &str, is_folder: bool) -> Result<String, Error> {
        self.workspace.trash_item(id, is_folder)
    }

    pub fn trash_items(&self) -> Result<Vec<TrashItem>, Error> {
        self.workspace.trash_items()
    }

    pub fn restore_trash(&self, id: &str) -> Result<String, Error> {
        self.workspace.restore_trash(id)
    }

    pub fn purge_trash(&self, id: &str) -> Result<(), Error> {
        self.workspace.purge_trash(id)
    }

    pub fn asset(&self, id: &str) -> Result<Asset, Error> {
        self.workspace
            .assets(None)?
            .into_iter()
            .find(|asset| asset.id == id)
            .ok_or(Error::NotFound)
    }
}

fn markdown_to_document(
    markdown: &str,
    mut resolve_image: impl FnMut(&str) -> Result<String, Error>,
) -> Result<serde_json::Value, Error> {
    use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
    use serde_json::{json, Value};

    #[derive(Clone)]
    enum Mark {
        Bold,
        Italic,
        Strike,
        Code,
        Link(String),
    }

    let mut options = Options::all();
    options.remove(Options::ENABLE_GFM);
    let mut root = json!({"type": "doc", "content": []});
    let mut blocks: Vec<Value> = Vec::new();
    let mut marks: Vec<Mark> = Vec::new();
    let mut image_stack: Vec<Option<String>> = Vec::new();
    let mut table_header = false;

    fn append_child(parent: &mut Value, node: Value) {
        let Some(object) = parent.as_object_mut() else {
            return;
        };
        let content = object.entry("content").or_insert_with(|| json!([]));
        if let Some(content) = content.as_array_mut() {
            content.push(node);
        }
    }

    fn push_block(blocks: &mut Vec<Value>, node: Value) {
        if let Some(parent) = blocks.last_mut() {
            append_child(parent, node);
        } else {
            blocks.push(node);
        }
    }

    fn flush_block(blocks: &mut Vec<Value>, root: &mut Value) {
        if let Some(node) = blocks.pop() {
            let parent = blocks.last_mut().unwrap_or(root);
            append_child(parent, node);
        }
    }

    fn push_text(blocks: &mut [Value], text: &str, marks: &[Mark]) {
        if text.is_empty() {
            return;
        }
        let mut node = json!({"type": "text", "text": text});
        let resolved_marks: Vec<Value> = marks
            .iter()
            .map(|mark| match mark {
                Mark::Bold => json!({"type": "bold"}),
                Mark::Italic => json!({"type": "italic"}),
                Mark::Strike => json!({"type": "strike"}),
                Mark::Code => json!({"type": "code"}),
                Mark::Link(href) => json!({"type": "link", "attrs": {"href": href}}),
            })
            .collect();
        if !resolved_marks.is_empty() {
            node["marks"] = json!(resolved_marks);
        }
        if let Some(parent) = blocks.last_mut() {
            append_child(parent, node);
        }
    }

    fn push_marked_text(blocks: &mut [Value], text: &str, marks: &[Mark], code: bool) {
        if !code {
            push_text(blocks, text, marks);
            return;
        }
        let mut inline_marks = marks.to_vec();
        inline_marks.push(Mark::Code);
        push_text(blocks, text, &inline_marks);
    }

    for event in Parser::new_ext(markdown, options) {
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => blocks.push(json!({"type": "paragraph", "content": []})),
                Tag::Heading { level, .. } => {
                    let level = match level {
                        HeadingLevel::H1 => 1,
                        HeadingLevel::H2 => 2,
                        HeadingLevel::H3 => 3,
                        HeadingLevel::H4 => 4,
                        HeadingLevel::H5 => 5,
                        HeadingLevel::H6 => 6,
                    };
                    blocks
                        .push(json!({"type": "heading", "attrs": {"level": level}, "content": []}));
                }
                Tag::BlockQuote(_) => blocks.push(json!({"type": "blockquote", "content": []})),
                Tag::CodeBlock(kind) => {
                    let language = match kind {
                        CodeBlockKind::Fenced(language) if !language.is_empty() => {
                            Some(language.to_string())
                        }
                        _ => None,
                    };
                    let mut block = json!({"type": "codeBlock", "content": []});
                    if let Some(language) = language {
                        block["attrs"] = json!({"language": language});
                    }
                    blocks.push(block);
                }
                Tag::List(start) => {
                    if let Some(start) = start {
                        blocks.push(json!({"type": "orderedList", "attrs": {"start": start}, "content": []}));
                    } else {
                        blocks.push(json!({"type": "bulletList", "content": []}));
                    }
                }
                Tag::Item => blocks.push(json!({"type": "listItem", "content": []})),
                Tag::Table(_) => blocks.push(json!({"type": "table", "content": []})),
                Tag::TableHead => {
                    table_header = true;
                    blocks.push(json!({"type": "tableRow", "content": []}));
                }
                Tag::TableRow => blocks.push(json!({"type": "tableRow", "content": []})),
                Tag::TableCell => {
                    let cell_type = if table_header {
                        "tableHeader"
                    } else {
                        "tableCell"
                    };
                    blocks.push(json!({"type": cell_type, "content": [{"type": "paragraph"}]}))
                }
                Tag::Emphasis => marks.push(Mark::Italic),
                Tag::Strong => marks.push(Mark::Bold),
                Tag::Strikethrough => marks.push(Mark::Strike),
                Tag::Link { dest_url, .. } => marks.push(Mark::Link(dest_url.to_string())),
                Tag::Image {
                    dest_url, title, ..
                } => {
                    let src = resolve_image(&dest_url)?;
                    let mut image = json!({"type": "image", "attrs": {"src": src}});
                    if !title.is_empty() {
                        image["attrs"]["title"] = json!(title.to_string());
                    }
                    push_block(&mut blocks, image);
                    image_stack.push(Some(String::new()));
                }
                Tag::HtmlBlock
                | Tag::FootnoteDefinition(_)
                | Tag::DefinitionList
                | Tag::DefinitionListTitle
                | Tag::DefinitionListDefinition
                | Tag::MetadataBlock(_) => {
                    return Err(Error::InvalidContent(
                        "This Markdown block is not supported in Apex Studio notes.".into(),
                    ));
                }
                _ => image_stack.push(None),
            },
            Event::End(tag) => {
                let mark_index = match tag {
                    TagEnd::Emphasis => marks.iter().rposition(|mark| matches!(mark, Mark::Italic)),
                    TagEnd::Strong => marks.iter().rposition(|mark| matches!(mark, Mark::Bold)),
                    TagEnd::Strikethrough => {
                        marks.iter().rposition(|mark| matches!(mark, Mark::Strike))
                    }
                    TagEnd::Link => marks.iter().rposition(|mark| matches!(mark, Mark::Link(_))),
                    _ => None,
                };
                if let Some(index) = mark_index {
                    marks.remove(index);
                    continue;
                }
                match tag {
                    TagEnd::Paragraph
                    | TagEnd::Heading(_)
                    | TagEnd::BlockQuote(_)
                    | TagEnd::CodeBlock
                    | TagEnd::List(_)
                    | TagEnd::Item
                    | TagEnd::Table
                    | TagEnd::TableRow
                    | TagEnd::TableCell => flush_block(&mut blocks, &mut root),
                    TagEnd::TableHead => {
                        table_header = false;
                        flush_block(&mut blocks, &mut root);
                    }
                    TagEnd::Image => {
                        let alt = image_stack.pop().flatten();
                        if let (Some(alt), Some(parent)) = (alt, blocks.last_mut()) {
                            if let Some(image) = parent
                                .get_mut("content")
                                .and_then(Value::as_array_mut)
                                .and_then(|children| children.last_mut())
                            {
                                image["attrs"]["alt"] = json!(alt);
                            }
                        }
                    }
                    _ => {
                        image_stack.pop();
                    }
                }
            }
            Event::Text(text) => {
                if let Some(Some(alt)) = image_stack.last_mut() {
                    alt.push_str(&text);
                    continue;
                }
                push_text(&mut blocks, &text, &marks);
            }
            Event::Code(text) => {
                if !matches!(image_stack.last(), Some(Some(_))) {
                    push_marked_text(&mut blocks, &text, &marks, true);
                }
            }
            Event::Html(_) | Event::InlineHtml(_) => {
                return Err(Error::InvalidContent(
                    "HTML is not supported in Apex Studio notes.".into(),
                ));
            }
            Event::SoftBreak => push_text(&mut blocks, " ", &marks),
            Event::HardBreak => push_block(&mut blocks, json!({"type": "hardBreak"})),
            Event::Rule => push_block(&mut blocks, json!({"type": "horizontalRule"})),
            Event::TaskListMarker(checked) => {
                if let Some(item) = blocks.last_mut() {
                    item["type"] = json!("taskItem");
                    item["attrs"] = json!({"checked": checked});
                    let parent_index = blocks.len().checked_sub(2);
                    if let Some(parent) = parent_index.and_then(|index| blocks.get_mut(index)) {
                        parent["type"] = json!("taskList");
                    }
                }
            }
            Event::InlineMath(_) | Event::DisplayMath(_) | Event::FootnoteReference(_) => {
                return Err(Error::InvalidContent(
                    "Math and footnotes are not supported in Apex Studio notes.".into(),
                ));
            }
        }
    }
    while !blocks.is_empty() {
        flush_block(&mut blocks, &mut root);
    }
    if root["content"].as_array().is_some_and(Vec::is_empty) {
        root["content"] = json!([{"type": "paragraph"}]);
    }
    Ok(root)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::sync::{Arc, Barrier};

    #[test]
    fn independently_opened_clients_share_workspace_reads_and_writes() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let root = temp.path().join("workspace");
        drop(Workspace::open(&root).expect("initialize dummy workspace"));

        let first = ApplicationServices::open(&root).expect("first client");
        let second = ApplicationServices::open(&root).expect("second client");
        let barrier = Arc::new(Barrier::new(3));
        let first_barrier = barrier.clone();
        let first_writer = std::thread::spawn(move || {
            first_barrier.wait();
            first.save_project(
                None,
                ProjectInput {
                    name: "CLI dummy project".into(),
                    ..Default::default()
                },
            )
        });
        let second_barrier = barrier.clone();
        let second_writer = std::thread::spawn(move || {
            second_barrier.wait();
            second.save_project(
                None,
                ProjectInput {
                    name: "Desktop dummy project".into(),
                    ..Default::default()
                },
            )
        });
        barrier.wait();

        first_writer
            .join()
            .expect("first writer thread")
            .expect("first write");
        second_writer
            .join()
            .expect("second writer thread")
            .expect("second write");

        let verifier = ApplicationServices::open(&root).expect("verify workspace");
        let projects = verifier.projects().expect("list shared projects");
        assert_eq!(projects.len(), 2);
        assert!(projects
            .iter()
            .any(|project| project.fields.name == "CLI dummy project"));
        assert!(projects
            .iter()
            .any(|project| project.fields.name == "Desktop dummy project"));
    }

    #[test]
    fn markdown_parser_keeps_common_blocks_marks_links_tables_and_images() {
        let markdown = "# Title\n\nText **bold**, _italic_, ~~gone~~ and `code` with [link](https://example.com).\n\n- first\n- second\n\n| A | B |\n| - | - |\n| 1 | 2 |\n\n![preview](asset.png)";
        let document =
            markdown_to_document(markdown, |_| Ok(format!("apex-image:{}", "a".repeat(64))))
                .expect("convert Markdown");
        apex_core::validate_note("Dummy", &document, markdown).expect("valid note");
        let encoded = document.to_string();
        for node in [
            "heading",
            "paragraph",
            "bulletList",
            "listItem",
            "table",
            "tableHeader",
            "image",
        ] {
            assert!(encoded.contains(node), "missing {node} in {encoded}");
        }
        assert!(encoded.contains("https://example.com"));
        assert!(encoded.contains("apex-image:"));
    }

    #[test]
    fn markdown_note_import_embeds_png_from_adjacent_path_and_preserves_original() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let root = temp.path().join("workspace");
        let app = ApplicationServices::open(&root).expect("workspace");
        let note = app.create_note(None).expect("new note");
        let png = temp.path().join("preview.png");
        let bytes = include_bytes!("../../../apps/erlcx-extension/icon32.png");
        std::fs::write(&png, bytes).expect("dummy png");

        let saved = app
            .save_note_markdown(
                &note.id,
                note.revision,
                "Markdown image test",
                "# Preview\n\n![Apex](preview.png)",
                temp.path(),
            )
            .expect("save Markdown note");

        let src = saved
            .document
            .pointer("/content/1/content/0/attrs/src")
            .and_then(Value::as_str)
            .expect("managed image reference");
        assert_eq!(src.len(), 75);
        assert_eq!(
            saved
                .document
                .pointer("/content/1/content/0/attrs/alt")
                .and_then(Value::as_str),
            Some("Apex")
        );
        assert!(saved.markdown.contains("apex-image:"));
        assert_eq!(std::fs::read(&png).expect("original remains"), bytes);
        assert_eq!(
            std::fs::read(
                root.join("files")
                    .join(src.trim_start_matches("apex-image:"))
            )
            .expect("managed image"),
            bytes
        );
    }
}
