use crate::models::workspace::{
    Document, DocumentIndexUpdate, DocumentLink, Heading, ProjectModel,
};
use pulldown_cmark::{Event, HeadingLevel, LinkType, Options, Parser, Tag, TagEnd};
use std::{collections::HashMap, fs, ops::Range, path::Path};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum LinkSyntax {
    Markdown,
    Wiki,
}

#[derive(Debug, Eq, PartialEq)]
struct RawLink {
    target: String,
    syntax: LinkSyntax,
    source_range: Range<usize>,
}

#[derive(Debug, Eq, PartialEq)]
struct ParsedMarkdown {
    headings: Vec<Heading>,
    links: Vec<RawLink>,
}

pub fn index_workspace_documents(root: &Path, project: &mut ProjectModel) {
    let resolution_index = ResolutionIndex::new(project);
    let document_paths: Vec<_> = project
        .documents
        .values()
        .map(|document| document.path.clone())
        .collect();
    project.links.clear();

    for document_path in document_paths {
        if let Ok(content) = fs::read_to_string(root.join(&document_path)) {
            if let Ok(update) = replace_document_index(
                project,
                &resolution_index,
                &document_path,
                &content,
                0,
                false,
            ) {
                project.links.extend(update.links);
            }
        }
    }
    sort_links(&mut project.links);
}

pub fn refresh_document_index(
    project: &mut ProjectModel,
    document_path: &str,
    content: &str,
    workspace_generation: u64,
) -> Result<DocumentIndexUpdate, String> {
    let resolution_index = ResolutionIndex::new(project);
    replace_document_index(
        project,
        &resolution_index,
        document_path,
        content,
        workspace_generation,
        true,
    )
}

fn replace_document_index(
    project: &mut ProjectModel,
    resolution_index: &ResolutionIndex,
    document_path: &str,
    content: &str,
    workspace_generation: u64,
    merge_project_links: bool,
) -> Result<DocumentIndexUpdate, String> {
    let document_id = format!("document:{document_path}");
    let parsed = parse_markdown(content);
    let document = project
        .documents
        .get_mut(&document_id)
        .ok_or_else(|| format!("The document is not part of the workspace: {document_path}"))?;
    document.headings = parsed.headings;
    document.title = document
        .headings
        .iter()
        .find(|heading| heading.level == 1 && !heading.text.is_empty())
        .map(|heading| heading.text.clone())
        .or_else(|| filename_title(&document.name));
    let document = document.clone();
    let mut duplicate_counts = HashMap::new();
    let mut links: Vec<_> = parsed
        .links
        .iter()
        .filter_map(|link| {
            let target = link.target.trim().to_owned();
            let occurrence = duplicate_counts
                .entry((link.syntax, target))
                .and_modify(|count| *count += 1)
                .or_insert(0);
            resolve_link(resolution_index, &document, link).map(|mut resolved_link| {
                resolved_link.id = link_id(&document.id, link, *occurrence);
                resolved_link
            })
        })
        .collect();
    links.sort_by(|left, right| left.id.cmp(&right.id));

    if merge_project_links {
        project
            .links
            .retain(|link| link.source_document_id != document.id);
        project.links.extend(links.iter().cloned());
        sort_links(&mut project.links);
    }

    Ok(DocumentIndexUpdate {
        workspace_generation,
        document,
        links,
    })
}

pub fn filename_title(filename: &str) -> Option<String> {
    Path::new(filename)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .map(str::to_owned)
}

fn parse_markdown(content: &str) -> ParsedMarkdown {
    let mut markdown_links = Vec::new();
    let mut markdown_link_ranges = Vec::new();
    let mut image_ranges = Vec::new();
    let mut code_ranges = Vec::new();

    for (event, source_range) in Parser::new_ext(content, Options::empty()).into_offset_iter() {
        match event {
            Event::Start(Tag::Link { dest_url, .. }) => {
                markdown_link_ranges.push(source_range.clone());
                markdown_links.push(RawLink {
                    target: dest_url.into_string(),
                    syntax: LinkSyntax::Markdown,
                    source_range,
                });
            }
            Event::Start(Tag::Image { .. }) => {
                image_ranges.push(source_range);
            }
            Event::Start(Tag::CodeBlock(_)) => code_ranges.push(source_range),
            Event::Code(_) => code_ranges.push(source_range),
            _ => {}
        }
    }

    merge_ranges(&mut code_ranges);
    let unsupported_wiki_ranges = unsupported_wiki_ranges(content, &code_ranges);
    markdown_links.retain(|link| {
        !is_contained_by_any(&link.source_range, &unsupported_wiki_ranges)
            && !is_contained_by_any(&link.source_range, &image_ranges)
    });

    let mut links = markdown_links;
    let mut headings = Vec::new();
    let mut heading: Option<(u8, String)> = None;
    for (event, source_range) in
        Parser::new_ext(content, Options::ENABLE_WIKILINKS).into_offset_iter()
    {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                heading = Some((heading_level(level), String::new()));
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, text)) = heading.take() {
                    headings.push(Heading {
                        level,
                        text: text.trim().to_owned(),
                    });
                }
            }
            Event::Start(Tag::Link {
                link_type: LinkType::WikiLink { has_pothole: false },
                dest_url,
                ..
            }) => {
                if is_contained_by_any(&source_range, &unsupported_wiki_ranges)
                    || is_contained_by_any(&source_range, &image_ranges)
                    || is_strictly_contained_by_any(&source_range, &markdown_link_ranges)
                {
                    continue;
                }

                links.push(RawLink {
                    target: dest_url.into_string(),
                    syntax: LinkSyntax::Wiki,
                    source_range,
                });
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some((_, heading_text)) = heading.as_mut() {
                    heading_text.push_str(&text);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some((_, heading_text)) = heading.as_mut() {
                    heading_text.push(' ');
                }
            }
            _ => {}
        }
    }

    links.sort_by_key(|link| link.source_range.start);
    ParsedMarkdown { headings, links }
}

fn unsupported_wiki_ranges(content: &str, code_ranges: &[Range<usize>]) -> Vec<Range<usize>> {
    let bytes = content.as_bytes();
    let mut ranges = Vec::new();
    let mut segment_start = 0;

    for code_range in code_ranges {
        scan_unsupported_wiki_segment(bytes, segment_start..code_range.start, &mut ranges);
        segment_start = code_range.end;
    }
    scan_unsupported_wiki_segment(bytes, segment_start..bytes.len(), &mut ranges);
    ranges
}

fn scan_unsupported_wiki_segment(
    bytes: &[u8],
    segment: Range<usize>,
    ranges: &mut Vec<Range<usize>>,
) {
    let mut start = segment.start;

    while start + 1 < segment.end {
        if bytes[start] != b'[' || bytes[start + 1] != b'[' || is_escaped(bytes, start) {
            start += 1;
            continue;
        }

        let mut cursor = start + 2;
        let mut depth = 1;
        let mut has_pipe = false;
        let mut has_nested_wiki = false;
        let mut has_nested_markup = false;
        let mut end = None;

        while cursor + 1 < segment.end {
            if bytes[cursor] == b'[' && bytes[cursor + 1] == b'[' {
                depth += 1;
                has_nested_wiki = true;
                cursor += 2;
            } else if bytes[cursor] == b']' && bytes[cursor + 1] == b']' {
                depth -= 1;
                cursor += 2;
                if depth == 0 {
                    end = Some(cursor);
                    break;
                }
            } else {
                has_pipe |= bytes[cursor] == b'|';
                has_nested_markup |= matches!(bytes[cursor], b'[' | b']');
                cursor += 1;
            }
        }

        if let Some(end) = end {
            if has_pipe || has_nested_wiki || has_nested_markup {
                ranges.push(start..end);
            }
            start = end;
        } else {
            start += 2;
        }
    }
}

fn is_escaped(bytes: &[u8], index: usize) -> bool {
    let preceding_backslashes = bytes[..index]
        .iter()
        .rev()
        .take_while(|byte| **byte == b'\\')
        .count();
    preceding_backslashes % 2 == 1
}

fn merge_ranges(ranges: &mut Vec<Range<usize>>) {
    ranges.sort_by_key(|range| range.start);
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());

    for range in ranges.drain(..) {
        if let Some(previous) = merged.last_mut() {
            if range.start <= previous.end {
                previous.end = previous.end.max(range.end);
                continue;
            }
        }
        merged.push(range);
    }

    *ranges = merged;
}

fn is_contained_by_any(candidate: &Range<usize>, containers: &[Range<usize>]) -> bool {
    containers
        .iter()
        .any(|container| container.start <= candidate.start && candidate.end <= container.end)
}

fn is_strictly_contained_by_any(candidate: &Range<usize>, containers: &[Range<usize>]) -> bool {
    containers.iter().any(|container| {
        container.start <= candidate.start
            && candidate.end <= container.end
            && (container.start < candidate.start || candidate.end < container.end)
    })
}

struct ResolutionIndex {
    by_path: HashMap<String, String>,
    by_extensionless_path: HashMap<String, Vec<String>>,
    by_filename: HashMap<String, Vec<String>>,
    by_stem: HashMap<String, Vec<String>>,
}

impl ResolutionIndex {
    fn new(project: &ProjectModel) -> Self {
        let mut index = Self {
            by_path: HashMap::with_capacity(project.documents.len()),
            by_extensionless_path: HashMap::with_capacity(project.documents.len()),
            by_filename: HashMap::with_capacity(project.documents.len()),
            by_stem: HashMap::with_capacity(project.documents.len()),
        };

        for document in project.documents.values() {
            index
                .by_path
                .insert(document.path.clone(), document.id.clone());
            index
                .by_extensionless_path
                .entry(without_markdown_extension(&document.path).to_owned())
                .or_default()
                .push(document.id.clone());
            index
                .by_filename
                .entry(document.name.clone())
                .or_default()
                .push(document.id.clone());
            index
                .by_stem
                .entry(without_markdown_extension(&document.name).to_owned())
                .or_default()
                .push(document.id.clone());
        }

        index
    }
}

fn resolve_link(
    resolution_index: &ResolutionIndex,
    source: &Document,
    link: &RawLink,
) -> Option<DocumentLink> {
    let raw_target = link.target.trim();

    if raw_target.is_empty() || is_external_target(raw_target) {
        return None;
    }

    let path_target = raw_target
        .split_once('#')
        .map_or(raw_target, |(path, _)| path);
    let resolution = if path_target.is_empty() {
        Ok(source.id.clone())
    } else {
        match link.syntax {
            LinkSyntax::Markdown => resolve_markdown_target(resolution_index, source, path_target),
            LinkSyntax::Wiki => resolve_wiki_target(resolution_index, source, path_target),
        }
    };
    let (target_document_id, unresolved_reason) = match resolution {
        Ok(target_id) => (Some(target_id), None),
        Err(reason) => (None, Some(reason)),
    };

    Some(DocumentLink {
        id: String::new(),
        source_document_id: source.id.clone(),
        resolved: target_document_id.is_some(),
        target_document_id,
        raw_target: raw_target.to_owned(),
        unresolved_reason,
    })
}

fn resolve_markdown_target(
    resolution_index: &ResolutionIndex,
    source: &Document,
    target: &str,
) -> Result<String, String> {
    if !has_markdown_extension(target) {
        return Err("Markdown targets must name a .md file".to_owned());
    }

    let path = normalize_target_path(source_directory(&source.path), target)
        .ok_or_else(|| "Target path escapes the workspace".to_owned())?;
    resolution_index
        .by_path
        .get(&path)
        .cloned()
        .ok_or_else(|| "Target document was not found".to_owned())
}

fn resolve_wiki_target(
    resolution_index: &ResolutionIndex,
    source: &Document,
    target: &str,
) -> Result<String, String> {
    if target.contains('/') || target.starts_with('.') {
        let path = normalize_target_path(source_directory(&source.path), target)
            .ok_or_else(|| "Target path escapes the workspace".to_owned())?;
        return if has_markdown_extension(&path) {
            resolution_index
                .by_path
                .get(&path)
                .cloned()
                .ok_or_else(|| "Target document was not found".to_owned())
        } else {
            choose_wiki_candidate(
                resolution_index
                    .by_extensionless_path
                    .get(&path)
                    .map(Vec::as_slice),
            )
        };
    }

    let local_path = if source_directory(&source.path).is_empty() {
        target.to_owned()
    } else {
        format!("{}/{target}", source_directory(&source.path))
    };
    let local_candidates = if has_markdown_extension(target) {
        resolution_index
            .by_path
            .get(&local_path)
            .map(std::slice::from_ref)
    } else {
        resolution_index
            .by_extensionless_path
            .get(&local_path)
            .map(Vec::as_slice)
    };

    if local_candidates.is_some_and(|items| !items.is_empty()) {
        return choose_wiki_candidate(local_candidates);
    }

    if has_markdown_extension(target) {
        choose_wiki_candidate(resolution_index.by_filename.get(target).map(Vec::as_slice))
    } else {
        choose_wiki_candidate(resolution_index.by_stem.get(target).map(Vec::as_slice))
    }
}

fn choose_wiki_candidate(candidates: Option<&[String]>) -> Result<String, String> {
    match candidates {
        Some([document_id]) => Ok(document_id.clone()),
        Some([]) | None => Err("Target document was not found".to_owned()),
        Some(_) => Err("Wiki target is ambiguous".to_owned()),
    }
}

fn link_id(source_document_id: &str, link: &RawLink, occurrence: usize) -> String {
    let syntax = match link.syntax {
        LinkSyntax::Markdown => "markdown",
        LinkSyntax::Wiki => "wiki",
    };
    let target = link.target.trim();
    format!(
        "link:{}:{source_document_id}:{syntax}:{}:{target}:{occurrence:020}",
        source_document_id.len(),
        target.len()
    )
}

fn sort_links(links: &mut [DocumentLink]) {
    links.sort_by(|left, right| {
        left.source_document_id
            .cmp(&right.source_document_id)
            .then_with(|| left.id.cmp(&right.id))
    });
}

fn without_markdown_extension(path: &str) -> &str {
    if has_markdown_extension(path) {
        &path[..path.len() - 3]
    } else {
        path
    }
}

fn has_markdown_extension(path: &str) -> bool {
    path.as_bytes()
        .get(path.len().saturating_sub(3)..)
        .is_some_and(|suffix| suffix.eq_ignore_ascii_case(b".md"))
}

fn source_directory(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(directory, _)| directory)
}

fn normalize_target_path(source_directory: &str, target: &str) -> Option<String> {
    if target.starts_with('/') || target.contains('\\') {
        return None;
    }

    let mut segments: Vec<_> = source_directory
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();

    for segment in target.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            segment => segments.push(segment),
        }
    }

    (!segments.is_empty()).then(|| segments.join("/"))
}

fn is_external_target(target: &str) -> bool {
    if target.starts_with("//") {
        return true;
    }

    let Some((scheme, _)) = target.split_once(':') else {
        return false;
    };
    let mut characters = scheme.chars();

    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && characters.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.')
        })
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::{index_workspace_documents, parse_markdown, refresh_document_index};
    use crate::models::workspace::{Document, Folder, Heading, ProjectModel};
    use std::{
        collections::BTreeMap,
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
                "tracedoc-markdown-test-{}-{fixture_id}",
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
    fn parses_heading_text_and_levels() {
        let parsed = parse_markdown("# Main *title*\n\n## `Code` heading\n\n###### Last");

        assert_eq!(parsed.headings.len(), 3);
        assert_eq!(parsed.headings[0].level, 1);
        assert_eq!(parsed.headings[0].text, "Main title");
        assert_eq!(parsed.headings[1].level, 2);
        assert_eq!(parsed.headings[1].text, "Code heading");
        assert_eq!(parsed.headings[2].level, 6);
    }

    #[test]
    fn extracts_a_wiki_links_visible_text_as_the_title() {
        let mut project = project(&["page.md", "target.md"]);
        let update = refresh_document_index(&mut project, "page.md", "# [[target]]", 0)
            .expect("wiki heading should index");

        assert_eq!(update.document.title.as_deref(), Some("target"));
        assert_eq!(
            update.document.headings,
            [Heading {
                level: 1,
                text: "target".to_owned(),
            }]
        );
        assert_eq!(update.links.len(), 1);
        assert_eq!(update.links[0].raw_target, "target");
    }

    #[test]
    fn extracts_mixed_heading_markup_and_multiple_wiki_labels() {
        let mut project = project(&["page.md", "target.md", "one.md", "two.md"]);
        let update = refresh_document_index(
            &mut project,
            "page.md",
            "# *Build* [Target](target.md) with `code` and [[one]] plus [[two]]",
            0,
        )
        .expect("mixed heading should index");

        assert_eq!(
            update.document.title.as_deref(),
            Some("Build Target with code and one plus two")
        );
        assert_eq!(update.document.headings[0].level, 1);
        assert_eq!(
            update.document.headings[0].text,
            "Build Target with code and one plus two"
        );
        assert_eq!(update.links.len(), 3);
        assert_eq!(
            update
                .links
                .iter()
                .filter(|link| link.raw_target == "one")
                .count(),
            1
        );
        assert_eq!(
            update
                .links
                .iter()
                .filter(|link| link.raw_target == "two")
                .count(),
            1
        );
    }

    #[test]
    fn renders_unsupported_piped_wiki_labels_without_indexing_them() {
        let mut project = project(&["page.md", "target.md"]);
        let piped =
            refresh_document_index(&mut project, "page.md", "# [[target|Visible label]]", 0)
                .expect("piped wiki heading should parse as visible text");

        assert_eq!(piped.document.title.as_deref(), Some("Visible label"));
        assert_eq!(piped.document.headings[0].text, "Visible label");
        assert!(piped.links.is_empty());

        let invalid = refresh_document_index(&mut project, "page.md", "# Broken [[target", 0)
            .expect("invalid wiki heading should remain literal");
        assert_eq!(invalid.document.title.as_deref(), Some("Broken [[target"));
        assert!(invalid.links.is_empty());
    }

    #[test]
    fn keeps_wiki_syntax_literal_inside_heading_code() {
        let mut project = project(&["page.md", "target.md"]);
        let update = refresh_document_index(
            &mut project,
            "page.md",
            "# Use `[[target]]` with **emphasis**",
            0,
        )
        .expect("heading code should parse");

        assert_eq!(
            update.document.title.as_deref(),
            Some("Use [[target]] with emphasis")
        );
        assert!(update.links.is_empty());
    }

    #[test]
    fn resolves_relative_links_wiki_links_and_anchors() {
        let mut project = project(&["frontend/app.md", "frontend/database.md", "backend/api.md"]);
        let update = refresh_document_index(
            &mut project,
            "frontend/app.md",
            "# App\n\n[API](../backend/api.md#routes)\n[Database](./database.md)\n[[database]]\n[[../backend/api#usage]]\n[#local](#details)",
            7,
        )
        .expect("document should index");

        assert_eq!(update.document.title.as_deref(), Some("App"));
        assert_eq!(update.workspace_generation, 7);
        assert_eq!(update.links.len(), 5);
        assert!(update.links.iter().all(|link| link.resolved));
        assert_eq!(
            link_with_raw_target(&update, "../backend/api.md#routes")
                .target_document_id
                .as_deref(),
            Some("document:backend/api.md")
        );
        assert_eq!(
            link_with_raw_target(&update, "./database.md")
                .target_document_id
                .as_deref(),
            Some("document:frontend/database.md")
        );
        assert_eq!(
            link_with_raw_target(&update, "database")
                .target_document_id
                .as_deref(),
            Some("document:frontend/database.md")
        );
        assert_eq!(
            link_with_raw_target(&update, "../backend/api#usage")
                .target_document_id
                .as_deref(),
            Some("document:backend/api.md")
        );
        assert_eq!(
            link_with_raw_target(&update, "#details")
                .target_document_id
                .as_deref(),
            Some("document:frontend/app.md")
        );
    }

    #[test]
    fn resolves_unicode_targets_without_utf8_boundary_panics() {
        let mut wiki_project = project(&["page.md", "éé.md"]);
        let wiki = refresh_document_index(&mut wiki_project, "page.md", "[[éé]]", 0)
            .expect("unicode wiki targets should index without panicking");
        assert_eq!(wiki.links.len(), 1);
        assert_eq!(
            wiki.links[0].target_document_id.as_deref(),
            Some("document:éé.md")
        );

        let mut markdown_project = project(&["page.md", "éé.MD"]);
        let markdown =
            refresh_document_index(&mut markdown_project, "page.md", "[Markdown](éé.MD)", 0)
                .expect("uppercase unicode Markdown targets should index without panicking");
        assert_eq!(markdown.links.len(), 1);
        assert_eq!(
            link_with_raw_target(&markdown, "éé.MD")
                .target_document_id
                .as_deref(),
            Some("document:éé.MD")
        );
    }

    #[test]
    fn treats_only_an_ascii_case_insensitive_md_suffix_as_markdown() {
        for markdown in ["x.md", "x.MD", "é.md", "éé.MD"] {
            assert!(super::has_markdown_extension(markdown), "{markdown}");
            assert_eq!(
                super::without_markdown_extension(markdown).len(),
                markdown.len() - 3
            );
        }
        for non_markdown in ["", "é", "éé", "x.mḋ", "x.mdé"] {
            assert!(
                !super::has_markdown_extension(non_markdown),
                "{non_markdown}"
            );
        }
    }

    #[test]
    fn ignores_external_links_images_and_wiki_syntax_in_code() {
        let mut project = project(&["page.md", "target.md"]);
        let update = refresh_document_index(
            &mut project,
            "page.md",
            "[Web](https://example.com) [Mail](mailto:test@example.com) [CDN](//example.com/a.md) ![Image](target.md) [[target|Label]] `[[target]]`\n\n```md\n[[target]]\n```",
            0,
        )
        .expect("document should index");

        assert!(update.links.is_empty(), "{:?}", update.links);
    }

    #[test]
    fn suppresses_nested_edges_inside_unsupported_wiki_links_and_images() {
        let mut project = project(&["page.md", "target.md"]);
        let content = "[[Outer|[[target]]]]\n[[Outer|[Target](target.md)]]\n[[Outer [Target](target.md)]]\n[[Outer [[target]]]]\n![Alt [Target](target.md)](image.png)\n![Alt [[target]]](image.png)";
        let update = refresh_document_index(&mut project, "page.md", content, 0)
            .expect("document should index");

        assert!(update.links.is_empty(), "{:?}", update.links);
    }

    #[test]
    fn suppresses_wiki_edges_inside_ordinary_markdown_link_labels() {
        let mut project = project(&["page.md", "outer.md", "target.md"]);
        let external = refresh_document_index(
            &mut project,
            "page.md",
            "[Outer [[target]]](https://example.com/)",
            0,
        )
        .expect("external wrapper should index");
        assert!(external.links.is_empty());

        let internal =
            refresh_document_index(&mut project, "page.md", "[Outer [[target]]](outer.md)", 0)
                .expect("internal wrapper should index");
        assert_eq!(internal.links.len(), 1);
        assert_eq!(internal.links[0].raw_target, "outer.md");
        assert_eq!(
            internal.links[0].target_document_id.as_deref(),
            Some("document:outer.md")
        );

        let adjacent =
            refresh_document_index(&mut project, "page.md", "[[target]] [Target](target.md)", 0)
                .expect("adjacent links should index");
        assert_eq!(adjacent.links.len(), 2);
        assert!(adjacent
            .links
            .iter()
            .all(|link| link.target_document_id.as_deref() == Some("document:target.md")));
    }

    #[test]
    fn unsupported_wiki_delimiters_do_not_cross_code_ranges() {
        let mut project = project(&["page.md", "target.md"]);
        let inline = refresh_document_index(
            &mut project,
            "page.md",
            "`[[outer|` [Target](target.md) `]]`",
            0,
        )
        .expect("inline code boundaries should index");
        assert_eq!(inline.links.len(), 1);
        assert_eq!(inline.links[0].raw_target, "target.md");

        let fenced = refresh_document_index(
            &mut project,
            "page.md",
            "```md\n[[outer|\n```\n[Target](target.md)\n```md\n]]\n```",
            0,
        )
        .expect("fenced code boundaries should index");
        assert_eq!(fenced.links.len(), 1);
        assert_eq!(fenced.links[0].raw_target, "target.md");

        let adjacent = refresh_document_index(
            &mut project,
            "page.md",
            "`[[outer|` [[target]] [Target](target.md) `]]`",
            0,
        )
        .expect("adjacent links around code should index");
        assert_eq!(adjacent.links.len(), 2);
        assert!(adjacent.links.iter().all(|link| link.resolved));
    }

    #[test]
    fn keeps_unresolved_targets_with_diagnostic_reasons() {
        let mut project = project(&["nested/page.md"]);
        let update = refresh_document_index(
            &mut project,
            "nested/page.md",
            "[Missing](missing.md) [Text](notes.txt) [Outside](../../outside.md) [[unknown]]",
            0,
        )
        .expect("document should index");

        assert_eq!(update.links.len(), 4);
        assert!(update.links.iter().all(|link| !link.resolved));
        assert!(update
            .links
            .iter()
            .all(|link| link.target_document_id.is_none()));
        assert_eq!(
            link_with_raw_target(&update, "missing.md")
                .unresolved_reason
                .as_deref(),
            Some("Target document was not found")
        );
        assert_eq!(
            link_with_raw_target(&update, "notes.txt")
                .unresolved_reason
                .as_deref(),
            Some("Markdown targets must name a .md file")
        );
        assert_eq!(
            link_with_raw_target(&update, "../../outside.md")
                .unresolved_reason
                .as_deref(),
            Some("Target path escapes the workspace")
        );
    }

    #[test]
    fn leaves_ambiguous_bare_wiki_links_unresolved() {
        let mut project = project(&["guide/start.md", "alpha/database.md", "beta/database.md"]);
        let update = refresh_document_index(&mut project, "guide/start.md", "[[database]]", 0)
            .expect("document should index");

        assert_eq!(update.links.len(), 1);
        assert!(!update.links[0].resolved);
        assert_eq!(
            update.links[0].unresolved_reason.as_deref(),
            Some("Wiki target is ambiguous")
        );
    }

    #[test]
    fn prefers_a_same_folder_wiki_target_over_global_duplicates() {
        let mut project = project(&["alpha/start.md", "alpha/database.md", "beta/database.md"]);
        let update = refresh_document_index(&mut project, "alpha/start.md", "[[database]]", 0)
            .expect("document should index");

        assert_eq!(
            update.links[0].target_document_id.as_deref(),
            Some("document:alpha/database.md")
        );
    }

    #[test]
    fn resolves_explicit_nested_wiki_paths_without_an_extension() {
        let mut project = project(&["overview.md", "backend/api.MD"]);
        let update = refresh_document_index(&mut project, "overview.md", "[[backend/api]]", 0)
            .expect("document should index");

        assert_eq!(
            update.links[0].target_document_id.as_deref(),
            Some("document:backend/api.MD")
        );
    }

    #[test]
    fn uses_filename_when_there_is_no_nonempty_h1() {
        let mut project = project(&["guide/getting-started.md"]);
        let update = refresh_document_index(
            &mut project,
            "guide/getting-started.md",
            "## Introduction\n\n#",
            0,
        )
        .expect("document should index");

        assert_eq!(update.document.title.as_deref(), Some("getting-started"));
        assert_eq!(update.document.headings.len(), 2);
    }

    #[test]
    fn replaces_only_the_changed_documents_metadata_and_links() {
        let mut project = project(&["first.md", "second.md", "target.md"]);
        refresh_document_index(
            &mut project,
            "first.md",
            "# First\n\n[Target](target.md)",
            0,
        )
        .expect("first document should index");
        refresh_document_index(&mut project, "second.md", "# Second\n\n[[target]]", 0)
            .expect("second document should index");
        let second_document = project.documents["document:second.md"].clone();
        let second_links: Vec<_> = project
            .links
            .iter()
            .filter(|link| link.source_document_id == second_document.id)
            .cloned()
            .collect();

        let update = refresh_document_index(&mut project, "first.md", "# Changed", 0)
            .expect("first document should reindex");

        assert_eq!(update.document.title.as_deref(), Some("Changed"));
        assert!(update.links.is_empty());
        assert_eq!(project.documents["document:second.md"], second_document);
        assert_eq!(
            project
                .links
                .iter()
                .filter(|link| link.source_document_id == "document:second.md")
                .cloned()
                .collect::<Vec<_>>(),
            second_links
        );
    }

    #[test]
    fn keeps_ids_stable_when_a_different_link_is_inserted_earlier() {
        let mut project = project(&["page.md", "earlier.md", "later.md"]);
        let first = refresh_document_index(
            &mut project,
            "page.md",
            "[Later](later.md)\n[Later again](later.md)",
            0,
        )
        .expect("first version should index");
        let first_later_ids: Vec<_> = first
            .links
            .iter()
            .filter(|link| link.raw_target == "later.md")
            .map(|link| link.id.clone())
            .collect();

        let second = refresh_document_index(
            &mut project,
            "page.md",
            "[Earlier](earlier.md)\n[Later](later.md)\n[Later again](later.md)",
            0,
        )
        .expect("second version should index");
        let second_later_ids: Vec<_> = second
            .links
            .iter()
            .filter(|link| link.raw_target == "later.md")
            .map(|link| link.id.clone())
            .collect();

        assert_eq!(first_later_ids, second_later_ids);
        assert_eq!(second_later_ids.len(), 2);
        assert_ne!(second_later_ids[0], second_later_ids[1]);
    }

    #[test]
    fn incremental_reindex_matches_full_index_and_canonical_order() {
        let workspace = TestWorkspace::new();
        let contents = [
            ("alpha.md", "# Alpha\n\n[Target](target.md) [[missing]]"),
            ("beta.md", "# Beta\n\n[[target]]"),
            ("target.md", "# Target"),
        ];

        for (path, content) in contents {
            fs::write(workspace.path().join(path), content)
                .expect("fixture document should be written");
        }

        let base = project(&["alpha.md", "beta.md", "target.md"]);
        let mut full = base.clone();
        index_workspace_documents(workspace.path(), &mut full);
        let mut incremental = base;
        for (path, content) in contents.into_iter().rev() {
            refresh_document_index(&mut incremental, path, content, 0)
                .expect("incremental document should index");
        }

        assert_eq!(incremental, full);
        let before_unchanged_reindex = incremental.clone();
        let update = refresh_document_index(&mut incremental, "alpha.md", contents[0].1, 0)
            .expect("unchanged document should reindex");
        assert_eq!(incremental, before_unchanged_reindex);
        assert_eq!(
            update.links,
            full.links
                .iter()
                .filter(|link| link.source_document_id == "document:alpha.md")
                .cloned()
                .collect::<Vec<_>>()
        );
        assert!(full.links.windows(2).all(|links| {
            (&links[0].source_document_id, &links[0].id)
                <= (&links[1].source_document_id, &links[1].id)
        }));
    }

    #[test]
    fn changing_a_target_title_preserves_incoming_resolution() {
        let mut project = project(&["source.md", "target.md"]);
        refresh_document_index(&mut project, "source.md", "[Target](target.md)", 0)
            .expect("source should index");
        let incoming = project.links.clone();

        refresh_document_index(&mut project, "target.md", "# Renamed title", 0)
            .expect("target should index");

        assert_eq!(project.links, incoming);
        assert_eq!(
            project.documents["document:target.md"].title.as_deref(),
            Some("Renamed title")
        );
    }

    #[test]
    fn indexes_all_readable_documents_during_the_initial_pass() {
        let workspace = TestWorkspace::new();
        fs::write(workspace.path().join("first.md"), "# First\n\n[[second]]")
            .expect("first document should be written");
        fs::write(workspace.path().join("second.md"), "# Second")
            .expect("second document should be written");
        let mut project = project(&["first.md", "second.md"]);

        index_workspace_documents(workspace.path(), &mut project);

        assert_eq!(
            project.documents["document:first.md"].title.as_deref(),
            Some("First")
        );
        assert_eq!(project.links.len(), 1);
        assert_eq!(
            project.links[0].target_document_id.as_deref(),
            Some("document:second.md")
        );
    }

    #[test]
    fn handles_a_moderately_large_document_in_one_document_pass() {
        let mut project = project(&["large.md", "target.md"]);
        let content = format!("# Large\n\n{}", "[Target](target.md)\n".repeat(20_000));
        let update = refresh_document_index(&mut project, "large.md", &content, 0)
            .expect("large document should index");

        assert_eq!(update.document.headings.len(), 1);
        assert_eq!(update.links.len(), 20_000);
        assert!(update.links.iter().all(|link| link.resolved));
    }

    #[test]
    fn resolves_many_links_in_a_large_workspace() {
        let document_count = 10_000;
        let mut paths: Vec<_> = (0..document_count)
            .map(|index| format!("docs/document-{index:05}.md"))
            .collect();
        paths.push("source.md".to_owned());
        let content: String = (0..document_count)
            .map(|index| format!("[Document](docs/document-{index:05}.md)\n"))
            .collect();
        let mut project = project_owned(paths);

        let update = refresh_document_index(&mut project, "source.md", &content, 0)
            .expect("large workspace source should index");

        assert_eq!(update.links.len(), document_count);
        assert!(update.links.iter().all(|link| link.resolved));
    }

    #[test]
    fn reports_a_document_missing_from_the_model() {
        let mut project = project(&["known.md"]);
        let error = refresh_document_index(&mut project, "missing.md", "# Missing", 0)
            .expect_err("unknown document should fail");

        assert!(error.contains("not part of the workspace"));
    }

    fn project(paths: &[&str]) -> ProjectModel {
        project_owned(paths.iter().map(|path| (*path).to_owned()))
    }

    fn project_owned(paths: impl IntoIterator<Item = String>) -> ProjectModel {
        let documents = paths
            .into_iter()
            .map(|path| {
                let id = format!("document:{path}");
                let name = path
                    .rsplit_once('/')
                    .map_or(path.as_str(), |(_, name)| name);
                (
                    id.clone(),
                    Document {
                        id,
                        name: name.to_owned(),
                        title: None,
                        headings: Vec::new(),
                        path,
                        parent_id: "folder:.".to_owned(),
                    },
                )
            })
            .collect();

        ProjectModel {
            root_path: "/workspace".to_owned(),
            folders: BTreeMap::from([(
                "folder:.".to_owned(),
                Folder {
                    id: "folder:.".to_owned(),
                    name: "workspace".to_owned(),
                    path: String::new(),
                    parent_id: None,
                    child_folder_ids: Vec::new(),
                    document_ids: Vec::new(),
                },
            )]),
            documents,
            links: Vec::new(),
        }
    }

    fn link_with_raw_target<'a>(
        update: &'a crate::models::workspace::DocumentIndexUpdate,
        raw_target: &str,
    ) -> &'a crate::models::workspace::DocumentLink {
        update
            .links
            .iter()
            .find(|link| link.raw_target == raw_target)
            .expect("link target should exist")
    }
}
