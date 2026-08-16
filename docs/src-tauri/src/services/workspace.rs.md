# Workspace Scanner

Source: `src-tauri/src/services/workspace.rs`.

Deterministic recursive full scan. Produces normalized folders/documents, applies one ignore policy, never follows symlink directories, then invokes the [`Markdown indexer`](markdown.rs.md).

## API and scanner

- `WorkspaceError` — invalid root, unavailable root, non-directory root, or I/O error; `fmt` implements the stable user-facing message and `Error` exposes the standard trait.
- `scan_workspace(root)` — canonicalizes/validates root, recursively scans, constructs `ProjectModel`, indexes readable Markdown.
- `WorkspaceScanner` — root-relative accumulator for folder/document `BTreeMap`s.
- `WorkspaceScanner::scan_directory(directory,parentId,isRoot)` — sorted directory traversal; creates hierarchy records and Markdown documents.

## Ignore and path policy

- `is_ignored_entry(entry,name,isDir)` — platform-specific metadata adapter; Windows includes hidden/system attributes.
- `is_ignored_directory_name(name)` — common system directory names.
- `is_ignored_entry_policy(name,isDir,attributes)` — dot-directory/system/attribute policy; Unix dot Markdown files remain valid.
- `ignored_relative_ancestor(root,relative,isDir)` — returns first ignored or unsafe ancestor for incremental events.
- `ignored_relative_ancestor_with(...)` — injectable policy implementation.
- `path_has_ignored_attributes(path)` — Windows attribute query; false elsewhere.
- `has_windows_hidden_or_system_attributes(attributes)` — bitmask predicate.
- `is_markdown_file(path)` — case-insensitive `.md` extension.
- `folder_id(relativePath)` / `document_id(relativePath)` — deterministic path-based IDs.
- `normalize_root_path(path)` — slash-normalized root string.
- `normalize_relative_path(path)` — validates relative components and joins with `/`.

## Tests

- `TestWorkspace::new/path/drop` — isolated fixture lifecycle.
- Path normalization and nested Markdown-only scan.
- Initial title/heading/link indexing and unreadable UTF-8 fallback.
- Stable IDs/paths; invalid root errors.
- Case-insensitive system names and Windows attribute bits.
- Unix dotfile/Windows attribute policy convergence.
- First ignored ancestor for direct watcher events.

Test functions: `normalizes_workspace_relative_paths`, `scans_nested_markdown_tree_and_ignores_other_files`, `indexes_document_titles_headings_and_nested_links_during_scan`, `keeps_invalid_utf8_markdown_in_the_tree_with_a_filename_title`, `produces_stable_relative_paths_and_ids`, `reports_invalid_unavailable_and_non_directory_roots`, `recognizes_common_system_directory_names_case_insensitively`, `recognizes_windows_hidden_and_system_attribute_bits`, `applies_the_same_ignore_policy_to_unix_dotfiles_and_windows_attributes`, `returns_the_first_ignored_ancestor_for_direct_events`.

Consumers: [workspace commands](../commands/workspace.rs.md), [watcher](watcher.rs.md).
