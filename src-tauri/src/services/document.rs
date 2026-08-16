use crate::services::workspace::{normalize_relative_path, scan_workspace, WorkspaceError};
use std::{
    fmt, fs,
    path::{Path, PathBuf},
    sync::RwLock,
};

#[derive(Default)]
pub struct WorkspaceSession {
    root: RwLock<Option<PathBuf>>,
}

impl WorkspaceSession {
    pub fn activate(&self, root: PathBuf) -> Result<(), DocumentError> {
        let mut active_root = self
            .root
            .write()
            .map_err(|_| DocumentError::new("The workspace session is unavailable"))?;
        *active_root = Some(root);
        Ok(())
    }

    pub fn root(&self) -> Result<PathBuf, DocumentError> {
        self.root
            .read()
            .map_err(|_| DocumentError::new("The workspace session is unavailable"))?
            .clone()
            .ok_or_else(|| DocumentError::new("No workspace is currently open"))
    }
}

#[derive(Debug)]
pub struct DocumentError(String);

impl DocumentError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for DocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for DocumentError {}

pub fn open_workspace(
    root: &Path,
) -> Result<(crate::models::workspace::ProjectModel, PathBuf), WorkspaceError> {
    let project = scan_workspace(root)?;
    let canonical_root =
        fs::canonicalize(root).map_err(|source| WorkspaceError::RootUnavailable {
            path: root.to_string_lossy().into_owned(),
            source,
        })?;
    Ok((project, canonical_root))
}

pub fn read_document(root: &Path, document_path: &str) -> Result<String, DocumentError> {
    let path = resolve_document_path(root, document_path)?;
    fs::read_to_string(&path).map_err(|source| {
        DocumentError::new(format!(
            "Unable to read document '{document_path}': {source}"
        ))
    })
}

pub fn write_document(
    root: &Path,
    document_path: &str,
    content: &str,
) -> Result<(), DocumentError> {
    let path = resolve_document_path(root, document_path)?;
    let existing_content = fs::read_to_string(&path).map_err(|source| {
        DocumentError::new(format!(
            "Unable to read document '{document_path}' before saving: {source}"
        ))
    })?;
    let content = preserve_line_endings(&existing_content, content);
    fs::write(&path, content).map_err(|source| {
        DocumentError::new(format!(
            "Unable to save document '{document_path}': {source}"
        ))
    })
}

fn resolve_document_path(root: &Path, document_path: &str) -> Result<PathBuf, DocumentError> {
    if document_path.is_empty() || has_forbidden_backslash(document_path, cfg!(windows)) {
        return Err(DocumentError::new(format!(
            "Invalid document path: {document_path}"
        )));
    }

    let relative_path = Path::new(document_path);
    let normalized = normalize_relative_path(relative_path)
        .map_err(|_| DocumentError::new(format!("Invalid document path: {document_path}")))?;

    if normalized != document_path {
        return Err(DocumentError::new(format!(
            "Invalid document path: {document_path}"
        )));
    }

    if !relative_path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
    {
        return Err(DocumentError::new(format!(
            "The document is not Markdown: {document_path}"
        )));
    }

    let path = fs::canonicalize(root.join(relative_path)).map_err(|source| {
        DocumentError::new(format!(
            "Unable to access document '{document_path}': {source}"
        ))
    })?;

    if !path.starts_with(root) {
        return Err(DocumentError::new(format!(
            "The document is outside the workspace: {document_path}"
        )));
    }

    if !path.is_file() {
        return Err(DocumentError::new(format!(
            "The document is not a file: {document_path}"
        )));
    }

    Ok(path)
}

fn has_forbidden_backslash(document_path: &str, is_windows: bool) -> bool {
    is_windows && document_path.contains('\\')
}

fn preserve_line_endings(existing_content: &str, content: &str) -> String {
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");

    if existing_content.contains("\r\n") {
        normalized.replace('\n', "\r\n")
    } else if existing_content.contains('\r') {
        normalized.replace('\n', "\r")
    } else {
        normalized
    }
}

#[cfg(test)]
mod tests {
    use super::{has_forbidden_backslash, open_workspace, read_document, write_document};
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT_FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let fixture_id = NEXT_FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "tracedoc-document-test-{label}-{}-{fixture_id}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("test directory should be created");
            let path = fs::canonicalize(path).expect("test directory should canonicalize");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn reads_and_writes_unicode_markdown() {
        let workspace = TestDirectory::new("round-trip");
        fs::create_dir_all(workspace.path().join("guides"))
            .expect("nested folder should be created");
        fs::write(workspace.path().join("guides/start.md"), "# Start")
            .expect("document should be written");

        write_document(workspace.path(), "guides/start.md", "# Привет\n\nZażółć")
            .expect("document should save");

        assert_eq!(
            read_document(workspace.path(), "guides/start.md")
                .expect("document should be readable"),
            "# Привет\n\nZażółć"
        );
    }

    #[test]
    fn rejects_invalid_and_non_markdown_paths() {
        let workspace = TestDirectory::new("invalid-paths");
        fs::write(workspace.path().join("page.md"), "# Page").expect("document should be written");
        fs::write(workspace.path().join("notes.txt"), "Notes")
            .expect("text file should be written");

        for invalid_path in ["../page.md", "/page.md"] {
            let error = read_document(workspace.path(), invalid_path)
                .expect_err("invalid document path should fail");
            assert!(error.to_string().contains("Invalid document path"));
        }

        let error = read_document(workspace.path(), "notes.txt")
            .expect_err("non-Markdown document should fail");
        assert!(error.to_string().contains("not Markdown"));
    }

    #[test]
    fn treats_backslash_as_a_separator_only_on_windows() {
        assert!(has_forbidden_backslash("folder\\page.md", true));
        assert!(!has_forbidden_backslash("folder\\page.md", false));
    }

    #[test]
    fn preserves_crlf_line_endings_when_saving() {
        let workspace = TestDirectory::new("crlf");
        let path = workspace.path().join("windows.md");
        fs::write(&path, "# Heading\r\n\r\nOriginal\r\n").expect("CRLF document should be written");

        write_document(
            workspace.path(),
            "windows.md",
            "# Heading\n\nChanged Zażółć\n",
        )
        .expect("CRLF document should save");

        assert_eq!(
            fs::read_to_string(path).expect("CRLF document should be readable"),
            "# Heading\r\n\r\nChanged Zażółć\r\n"
        );
    }

    #[test]
    fn preserves_lf_line_endings_when_saving() {
        let workspace = TestDirectory::new("lf");
        let path = workspace.path().join("unix.md");
        fs::write(&path, "# Heading\n\nOriginal\n").expect("LF document should be written");

        write_document(workspace.path(), "unix.md", "# Heading\n\nChanged\n")
            .expect("LF document should save");

        assert_eq!(
            fs::read_to_string(path).expect("LF document should be readable"),
            "# Heading\n\nChanged\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn opens_scanned_unix_markdown_filename_with_backslash() {
        let workspace = TestDirectory::new("unix-backslash");
        let document_path = r"part\name.md";
        fs::write(workspace.path().join(document_path), "# Original")
            .expect("Unix document should be written");

        let (project, root) = open_workspace(workspace.path()).expect("workspace should scan");
        assert!(project
            .documents
            .contains_key(&format!("document:{document_path}")));
        assert_eq!(
            read_document(&root, document_path).expect("Unix document should open"),
            "# Original"
        );

        write_document(&root, document_path, "# Changed").expect("Unix document should save");
        assert_eq!(
            fs::read_to_string(workspace.path().join(document_path))
                .expect("Unix document should remain readable"),
            "# Changed"
        );
    }

    #[test]
    fn handles_a_moderately_large_markdown_document() {
        let workspace = TestDirectory::new("large-document");
        let content = format!(
            "# Large\n\n{}",
            "A searchable Markdown line.\n".repeat(20_000)
        );
        fs::write(workspace.path().join("large.md"), &content)
            .expect("large document should be written");

        assert_eq!(
            read_document(workspace.path(), "large.md").expect("large document should be readable"),
            content
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_that_escape_the_workspace() {
        use std::os::unix::fs::symlink;

        let workspace = TestDirectory::new("workspace");
        let outside = TestDirectory::new("outside");
        fs::write(outside.path().join("secret.md"), "Secret")
            .expect("outside document should be written");
        symlink(
            outside.path().join("secret.md"),
            workspace.path().join("escape.md"),
        )
        .expect("symlink should be created");

        let read_error =
            read_document(workspace.path(), "escape.md").expect_err("outside read should fail");
        let write_error = write_document(workspace.path(), "escape.md", "Changed")
            .expect_err("outside write should fail");
        assert!(read_error.to_string().contains("outside the workspace"));
        assert!(write_error.to_string().contains("outside the workspace"));
        assert_eq!(
            fs::read_to_string(outside.path().join("secret.md"))
                .expect("outside document should remain readable"),
            "Secret"
        );
    }
}
