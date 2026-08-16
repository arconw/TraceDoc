# Markdown Indexer

Source: `src-tauri/src/services/markdown.rs`.

Parses headings, ordinary Markdown links, and supported wiki links; resolves internal targets against the normalized project; emits deterministic document metadata and graph edges.

## Parsing and indexing

- `LinkSyntax` — Markdown or wiki identity component.
- `RawLink` — raw target plus syntax/source offsets.
- `ParsedMarkdown` — visible title/headings/raw links.
- `index_workspace_documents(root,project)` — one full parse/read pass plus one shared resolution index.
- `refresh_document_index(project,documentPath,content,workspaceGeneration)` — reparses one document from already-loaded content and recomputes affected resolution; returns a `Result<DocumentIndexUpdate, String>`.
- `replace_document_index(...)` — installs metadata/source links and re-resolves canonically.
- `filename_title(filename)` — Unicode-safe `.md` stem fallback.
- `parse_markdown(content)` — two-pass pulldown-cmark parse: headings/ordinary ranges, then wiki events; suppresses nested/code/image/unsupported syntax.
- `unsupported_wiki_ranges(content,codeRanges)` / `scan_unsupported_wiki_segment(...)` — raw tokenizer for piped/malformed wiki visibility and suppression.
- `is_escaped(bytes,index)` — odd-backslash check.
- `merge_ranges`, `is_contained_by_any`, `is_strictly_contained_by_any` — parser range normalization.

## Resolution

- `ResolutionIndex::new(project)` — path/stem/title lookup maps built once per refresh/pass.
- `resolve_link(...)` — dispatches syntax, preserves raw target/reason, builds stable edge.
- `resolve_markdown_target(...)` — relative path/anchor resolution; ignores external targets.
- `resolve_wiki_target(...)` — explicit path or same-folder-first/global-unique bare name resolution.
- `choose_wiki_candidate(candidates)` — unique/ambiguous/missing result.
- `link_id(source,rawLink,occurrence)` — stable source+syntax+raw-target+duplicate identity.
- `sort_links(links)` — canonical source/ID order.
- `without_markdown_extension`, `has_markdown_extension` — ASCII-safe suffix handling.
- `source_directory`, `normalize_target_path` — lexical relative target normalization.
- `is_external_target` — URI/protocol-relative classification.
- `heading_level` — pulldown heading enum to `u8`.

## Tests

- `TestWorkspace::new/path/drop`; `project`, `project_owned`, `link_with_raw_target` fixtures.
- Heading levels/text, wiki heading labels, mixed markup, piped/malformed/code wiki behavior.
- Relative Markdown/wiki/anchor resolution; Unicode targets and ASCII-only case-insensitive `.md` suffix.
- External/image/code/nested suppression and unsupported delimiters across code ranges.
- Missing/ambiguous diagnostics, same-folder priority, nested explicit paths, filename title fallback.
- Single-document replacement; stable IDs; incremental/full canonical equivalence; incoming resolution after title change.
- Initial readable-document pass, moderate document, 10k document/link scale, and missing-model error.

Test functions: `parses_heading_text_and_levels`, `extracts_a_wiki_links_visible_text_as_the_title`, `extracts_mixed_heading_markup_and_multiple_wiki_labels`, `renders_unsupported_piped_wiki_labels_without_indexing_them`, `keeps_wiki_syntax_literal_inside_heading_code`, `resolves_relative_links_wiki_links_and_anchors`, `resolves_unicode_targets_without_utf8_boundary_panics`, `treats_only_an_ascii_case_insensitive_md_suffix_as_markdown`, `ignores_external_links_images_and_wiki_syntax_in_code`, `suppresses_nested_edges_inside_unsupported_wiki_links_and_images`, `suppresses_wiki_edges_inside_ordinary_markdown_link_labels`, `unsupported_wiki_delimiters_do_not_cross_code_ranges`, `keeps_unresolved_targets_with_diagnostic_reasons`, `leaves_ambiguous_bare_wiki_links_unresolved`, `prefers_a_same_folder_wiki_target_over_global_duplicates`, `resolves_explicit_nested_wiki_paths_without_an_extension`, `uses_filename_when_there_is_no_nonempty_h1`, `replaces_only_the_changed_documents_metadata_and_links`, `keeps_ids_stable_when_a_different_link_is_inserted_earlier`, `incremental_reindex_matches_full_index_and_canonical_order`, `changing_a_target_title_preserves_incoming_resolution`, `indexes_all_readable_documents_during_the_initial_pass`, `handles_a_moderately_large_document_in_one_document_pass`, `resolves_many_links_in_a_large_workspace`, `reports_a_document_missing_from_the_model`.

Consumers: [`workspace scan`](workspace.rs.md), [`document save`](document.rs.md), [`watcher`](watcher.rs.md).
