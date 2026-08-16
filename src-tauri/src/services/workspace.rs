use crate::models::workspace::{Document, DocumentId, Folder, FolderId, ProjectModel};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fmt, fs, io,
    path::{Component, Path, PathBuf},
};

const SYSTEM_DIRECTORY_NAMES: [&str; 3] = ["$RECYCLE.BIN", "System Volume Information", "__MACOSX"];

#[cfg(any(windows, test))]
const WINDOWS_FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;

#[cfg(any(windows, test))]
const WINDOWS_FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;

#[derive(Debug)]
pub enum WorkspaceError {
    InvalidRootPath(String),
    RootUnavailable { path: String, source: io::Error },
    NotDirectory(String),
    RootUnreadable { path: String, source: io::Error },
}

impl fmt::Display for WorkspaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRootPath(path) => {
                write!(formatter, "The workspace path is malformed: {path}")
            }
            Self::RootUnavailable { path, source } => {
                write!(formatter, "Unable to access workspace '{path}': {source}")
            }
            Self::NotDirectory(path) => {
                write!(formatter, "The selected workspace is not a folder: {path}")
            }
            Self::RootUnreadable { path, source } => {
                write!(formatter, "Unable to read workspace '{path}': {source}")
            }
        }
    }
}

impl std::error::Error for WorkspaceError {}

pub fn scan_workspace(root: &Path) -> Result<ProjectModel, WorkspaceError> {
    let supplied_root = root
        .to_str()
        .filter(|path| !path.is_empty())
        .ok_or_else(|| WorkspaceError::InvalidRootPath(root.to_string_lossy().into_owned()))?;
    let metadata = fs::metadata(root).map_err(|source| WorkspaceError::RootUnavailable {
        path: supplied_root.to_owned(),
        source,
    })?;

    if !metadata.is_dir() {
        return Err(WorkspaceError::NotDirectory(supplied_root.to_owned()));
    }

    let root = fs::canonicalize(root).map_err(|source| WorkspaceError::RootUnavailable {
        path: supplied_root.to_owned(),
        source,
    })?;
    let root_path = normalize_root_path(&root);
    let root_name = root
        .file_name()
        .and_then(OsStr::to_str)
        .filter(|name| !name.is_empty())
        .unwrap_or(&root_path)
        .to_owned();
    let root_id = folder_id("");
    let mut scanner = WorkspaceScanner {
        root: root.clone(),
        folders: BTreeMap::from([(
            root_id.clone(),
            Folder {
                id: root_id.clone(),
                name: root_name,
                path: String::new(),
                parent_id: None,
                child_folder_ids: Vec::new(),
                document_ids: Vec::new(),
            },
        )]),
        documents: BTreeMap::new(),
    };

    scanner.scan_directory(&root, &root_id, true)?;

    Ok(ProjectModel {
        root_path,
        folders: scanner.folders,
        documents: scanner.documents,
        links: Vec::new(),
    })
}

struct WorkspaceScanner {
    root: PathBuf,
    folders: BTreeMap<FolderId, Folder>,
    documents: BTreeMap<DocumentId, Document>,
}

impl WorkspaceScanner {
    fn scan_directory(
        &mut self,
        directory: &Path,
        parent_id: &str,
        is_root: bool,
    ) -> Result<(), WorkspaceError> {
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(source) if is_root => {
                return Err(WorkspaceError::RootUnreadable {
                    path: normalize_root_path(&self.root),
                    source,
                });
            }
            Err(_) => return Ok(()),
        };
        let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
        entries.sort_by_key(|entry| entry.file_name());

        for entry in entries {
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(_) => continue,
            };

            if file_type.is_symlink() {
                continue;
            }

            let name = match entry.file_name().into_string() {
                Ok(name) => name,
                Err(_) => continue,
            };
            let entry_path = entry.path();
            let relative_path = match entry_path
                .strip_prefix(&self.root)
                .map_err(|_| ())
                .and_then(|path| normalize_relative_path(path).map_err(|_| ()))
            {
                Ok(path) => path,
                Err(()) => continue,
            };

            if file_type.is_dir() {
                if is_ignored_directory(&entry, &name) {
                    continue;
                }

                let id = folder_id(&relative_path);
                self.folders.insert(
                    id.clone(),
                    Folder {
                        id: id.clone(),
                        name,
                        path: relative_path.clone(),
                        parent_id: Some(parent_id.to_owned()),
                        child_folder_ids: Vec::new(),
                        document_ids: Vec::new(),
                    },
                );
                if let Some(parent) = self.folders.get_mut(parent_id) {
                    parent.child_folder_ids.push(id.clone());
                }
                self.scan_directory(&entry_path, &id, false)?;
            } else if file_type.is_file() && is_markdown_file(&entry_path) {
                let id = document_id(&relative_path);
                self.documents.insert(
                    id.clone(),
                    Document {
                        id: id.clone(),
                        name,
                        title: None,
                        path: relative_path,
                        parent_id: parent_id.to_owned(),
                    },
                );
                if let Some(parent) = self.folders.get_mut(parent_id) {
                    parent.document_ids.push(id);
                }
            }
        }

        Ok(())
    }
}

fn is_ignored_directory(entry: &fs::DirEntry, name: &str) -> bool {
    is_ignored_directory_name(name) || has_ignored_directory_attributes(entry)
}

fn is_ignored_directory_name(name: &str) -> bool {
    name.starts_with('.')
        || SYSTEM_DIRECTORY_NAMES
            .iter()
            .any(|system_name| name.eq_ignore_ascii_case(system_name))
}

#[cfg(windows)]
fn has_ignored_directory_attributes(entry: &fs::DirEntry) -> bool {
    use std::os::windows::fs::MetadataExt;

    entry
        .metadata()
        .is_ok_and(|metadata| has_windows_hidden_or_system_attributes(metadata.file_attributes()))
}

#[cfg(not(windows))]
fn has_ignored_directory_attributes(_entry: &fs::DirEntry) -> bool {
    false
}

#[cfg(any(windows, test))]
fn has_windows_hidden_or_system_attributes(attributes: u32) -> bool {
    attributes & (WINDOWS_FILE_ATTRIBUTE_HIDDEN | WINDOWS_FILE_ATTRIBUTE_SYSTEM) != 0
}

fn is_markdown_file(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

fn folder_id(relative_path: &str) -> FolderId {
    if relative_path.is_empty() {
        "folder:.".to_owned()
    } else {
        format!("folder:{relative_path}")
    }
}

fn document_id(relative_path: &str) -> String {
    format!("document:{relative_path}")
}

fn normalize_root_path(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");

    if let Some(path) = normalized.strip_prefix("//?/UNC/") {
        format!("//{path}")
    } else {
        normalized
            .strip_prefix("//?/")
            .unwrap_or(&normalized)
            .to_owned()
    }
}

pub(crate) fn normalize_relative_path(path: &Path) -> Result<String, WorkspaceError> {
    let mut segments = Vec::new();

    for component in path.components() {
        match component {
            Component::Normal(segment) => {
                let segment = segment.to_str().ok_or_else(|| {
                    WorkspaceError::InvalidRootPath(path.to_string_lossy().into_owned())
                })?;
                segments.push(segment);
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(WorkspaceError::InvalidRootPath(
                    path.to_string_lossy().into_owned(),
                ));
            }
        }
    }

    Ok(segments.join("/"))
}

#[cfg(test)]
mod tests {
    use super::{
        has_windows_hidden_or_system_attributes, is_ignored_directory_name,
        normalize_relative_path, scan_workspace, WorkspaceError, WINDOWS_FILE_ATTRIBUTE_HIDDEN,
        WINDOWS_FILE_ATTRIBUTE_SYSTEM,
    };
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT_FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

    struct TestWorkspace {
        path: PathBuf,
    }

    impl TestWorkspace {
        fn new() -> Self {
            let fixture_id = NEXT_FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "tracedoc-workspace-test-{}-{fixture_id}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("test workspace should be created");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn normalizes_workspace_relative_paths() {
        assert_eq!(
            normalize_relative_path(Path::new("./backend/api.md"))
                .expect("relative path should normalize"),
            "backend/api.md"
        );
        assert!(normalize_relative_path(Path::new("../outside.md")).is_err());
        assert!(normalize_relative_path(Path::new("/absolute.md")).is_err());
    }

    #[test]
    fn scans_nested_markdown_tree_and_ignores_other_files() {
        let workspace = TestWorkspace::new();
        fs::create_dir_all(workspace.path().join("backend"))
            .expect("nested folder should be created");
        fs::create_dir_all(workspace.path().join("assets"))
            .expect("assets folder should be created");
        fs::create_dir_all(workspace.path().join(".git")).expect("git folder should be created");
        fs::create_dir_all(workspace.path().join(".hidden"))
            .expect("hidden folder should be created");
        fs::create_dir_all(workspace.path().join("$Recycle.Bin"))
            .expect("recycle bin folder should be created");
        fs::create_dir_all(workspace.path().join("sYsTeM vOlUmE iNfOrMaTiOn"))
            .expect("system volume folder should be created");
        fs::create_dir_all(workspace.path().join("__macosx"))
            .expect("macOS metadata folder should be created");
        fs::write(workspace.path().join("overview.md"), "# Overview")
            .expect("overview should be written");
        fs::write(workspace.path().join("backend/api.md"), "# API").expect("api should be written");
        fs::write(workspace.path().join("backend/auth.MD"), "# Auth")
            .expect("auth should be written");
        fs::write(workspace.path().join("assets/logo.png"), [0_u8, 1, 2])
            .expect("asset should be written");
        fs::write(workspace.path().join(".git/ignored.md"), "# Ignored")
            .expect("git document should be written");
        fs::write(workspace.path().join(".hidden/ignored.md"), "# Ignored")
            .expect("hidden document should be written");
        fs::write(
            workspace.path().join("$Recycle.Bin/ignored.md"),
            "# Ignored",
        )
        .expect("recycle bin document should be written");
        fs::write(
            workspace
                .path()
                .join("sYsTeM vOlUmE iNfOrMaTiOn/ignored.md"),
            "# Ignored",
        )
        .expect("system volume document should be written");
        fs::write(workspace.path().join("__macosx/ignored.md"), "# Ignored")
            .expect("macOS metadata document should be written");

        let project = scan_workspace(workspace.path()).expect("workspace should scan");
        let folder_paths: Vec<_> = project
            .folders
            .values()
            .map(|folder| folder.path.as_str())
            .collect();
        let document_paths: Vec<_> = project
            .documents
            .values()
            .map(|document| document.path.as_str())
            .collect();

        assert_eq!(folder_paths, ["", "assets", "backend"]);
        assert_eq!(
            document_paths,
            ["backend/api.md", "backend/auth.MD", "overview.md"]
        );
        assert!(project.links.is_empty());

        let root = project
            .folders
            .get("folder:.")
            .expect("root folder should exist");
        assert_eq!(root.child_folder_ids, ["folder:assets", "folder:backend"]);
        assert_eq!(root.document_ids, ["document:overview.md"]);

        let backend = project
            .folders
            .get("folder:backend")
            .expect("backend folder should exist");
        assert_eq!(
            backend.document_ids,
            ["document:backend/api.md", "document:backend/auth.MD"]
        );
    }

    #[test]
    fn produces_stable_relative_paths_and_ids() {
        let workspace = TestWorkspace::new();
        fs::create_dir_all(workspace.path().join("nested/deeper"))
            .expect("nested folders should be created");
        fs::write(workspace.path().join("nested/deeper/page.md"), "# Page")
            .expect("document should be written");

        let first = scan_workspace(workspace.path()).expect("first scan should succeed");
        let second = scan_workspace(workspace.path()).expect("second scan should succeed");

        assert_eq!(first, second);
        assert!(first.folders.contains_key("folder:nested/deeper"));
        assert!(first
            .documents
            .contains_key("document:nested/deeper/page.md"));
        assert_eq!(
            first.documents["document:nested/deeper/page.md"].path,
            "nested/deeper/page.md"
        );
    }

    #[test]
    fn reports_invalid_unavailable_and_non_directory_roots() {
        let workspace = TestWorkspace::new();
        let file_path = workspace.path().join("page.md");
        let missing_path = workspace.path().join("missing");
        fs::write(&file_path, "# Page").expect("document should be written");

        assert!(matches!(
            scan_workspace(Path::new("")).expect_err("empty root should fail"),
            WorkspaceError::InvalidRootPath(_)
        ));
        assert!(matches!(
            scan_workspace(&missing_path).expect_err("missing root should fail"),
            WorkspaceError::RootUnavailable { .. }
        ));
        assert!(matches!(
            scan_workspace(&file_path).expect_err("file root should fail"),
            WorkspaceError::NotDirectory(_)
        ));
    }

    #[test]
    fn recognizes_common_system_directory_names_case_insensitively() {
        assert!(is_ignored_directory_name("$Recycle.Bin"));
        assert!(is_ignored_directory_name("system volume information"));
        assert!(is_ignored_directory_name("__macosx"));
        assert!(!is_ignored_directory_name("system"));
    }

    #[test]
    fn recognizes_windows_hidden_and_system_attribute_bits() {
        assert!(has_windows_hidden_or_system_attributes(
            WINDOWS_FILE_ATTRIBUTE_HIDDEN
        ));
        assert!(has_windows_hidden_or_system_attributes(
            WINDOWS_FILE_ATTRIBUTE_SYSTEM
        ));
        assert!(has_windows_hidden_or_system_attributes(
            WINDOWS_FILE_ATTRIBUTE_HIDDEN | WINDOWS_FILE_ATTRIBUTE_SYSTEM
        ));
        assert!(!has_windows_hidden_or_system_attributes(0));
        assert!(!has_windows_hidden_or_system_attributes(0x20));
    }
}
