use serde::Serialize;
use std::collections::BTreeMap;

pub type FolderId = String;
pub type DocumentId = String;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Heading {
    pub level: u8,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub id: FolderId,
    pub name: String,
    pub path: String,
    pub parent_id: Option<FolderId>,
    pub child_folder_ids: Vec<FolderId>,
    pub document_ids: Vec<DocumentId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub id: DocumentId,
    pub name: String,
    pub title: Option<String>,
    pub headings: Vec<Heading>,
    pub path: String,
    pub parent_id: FolderId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentLink {
    pub id: String,
    pub source_document_id: DocumentId,
    pub target_document_id: Option<DocumentId>,
    pub raw_target: String,
    pub resolved: bool,
    pub unresolved_reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentIndexUpdate {
    pub workspace_generation: u64,
    pub workspace_revision: u64,
    pub content_token: String,
    pub save_warning: Option<String>,
    pub document: Document,
    pub links: Vec<DocumentLink>,
    pub patches: Vec<WorkspacePatch>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentReadResult {
    pub workspace_generation: u64,
    pub workspace_revision: u64,
    pub content: String,
    pub content_token: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSnapshot {
    pub workspace_generation: u64,
    pub workspace_revision: u64,
    pub project: ProjectModel,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePatch {
    pub workspace_generation: u64,
    pub workspace_revision: u64,
    pub upserted_folders: Vec<Folder>,
    pub removed_folder_ids: Vec<FolderId>,
    pub upserted_documents: Vec<Document>,
    pub removed_document_ids: Vec<DocumentId>,
    pub upserted_links: Vec<DocumentLink>,
    pub removed_link_ids: Vec<String>,
    pub externally_changed_document_ids: Vec<DocumentId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEventError {
    pub workspace_generation: u64,
    pub workspace_revision: u64,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectModel {
    pub root_path: String,
    pub folders: BTreeMap<FolderId, Folder>,
    pub documents: BTreeMap<DocumentId, Document>,
    pub links: Vec<DocumentLink>,
}
