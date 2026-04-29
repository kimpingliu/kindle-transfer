//! EPUB TOC optimization and repair.
//!
//! This module focuses on Kindle-friendly EPUB navigation output. It parses the
//! three EPUB navigation artifacts that matter most in practice:
//!
//! - `content.opf`
//! - `nav.xhtml`
//! - `toc.ncx`
//!
//! Repair strategy:
//!
//! 1. Load the package spine and manifest from `content.opf`.
//! 2. Parse existing navigation from `nav.xhtml` or `toc.ncx`.
//! 3. Normalize hierarchy, remove duplicates and patch missing links.
//! 4. If no usable TOC exists, rebuild it from `h1`/`h2`/`h3` headings or
//!    Chinese chapter markers like `第十章`.
//! 5. Rewrite both `nav.xhtml` and `toc.ncx`, and update `content.opf` so
//!    Kindle conversion tools see a consistent, compatible navigation model.

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Cursor, Write};
use std::path::{Path, PathBuf};
use tempfile::tempdir;
use thiserror::Error;
use walkdir::WalkDir;
use xmltree::{Element, ParseError, XMLNode};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

static CHINESE_CHAPTER_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^\s*第[0-9０-９零〇一二三四五六七八九十百千两壹贰叁肆伍陆柒捌玖拾佰仟]+[章回节卷部篇集].*",
    )
    .expect("chapter regex must be valid")
});

/// High-level TOC optimizer service.
#[derive(Debug, Default, Clone)]
pub struct TocOptimizer;

/// Normalized TOC entry used both internally and by tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TocEntry {
    /// Display title shown by readers.
    pub title: String,
    /// Relative EPUB link target, usually `chapter.xhtml#anchor`.
    pub href: String,
    /// Nested child entries.
    pub children: Vec<TocEntry>,
}

/// Summary of the performed repairs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TocOptimizationReport {
    /// Number of root-level and nested entries after normalization.
    pub entry_count: usize,
    /// Whether the TOC had to be rebuilt from document headings.
    pub rebuilt_from_headings: bool,
    /// Whether a new `nav.xhtml` reference had to be created in OPF.
    pub nav_created: bool,
    /// Whether a new `toc.ncx` reference had to be created in OPF.
    pub ncx_created: bool,
    /// Number of duplicate TOC nodes removed.
    pub duplicates_removed: usize,
    /// Whether level jumps or malformed nesting had to be normalized.
    pub hierarchy_normalized: bool,
}

/// Error type returned by the TOC optimizer.
#[derive(Debug, Error)]
pub enum TocOptimizerError {
    #[error("I/O failure: {0}")]
    Io(#[from] io::Error),
    #[error("ZIP archive failure: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("XML parse failure: {0}")]
    XmlParse(#[from] ParseError),
    #[error("XML write failure: {0}")]
    XmlWrite(#[from] xmltree::Error),
    #[error("missing META-INF/container.xml in EPUB package")]
    MissingContainer,
    #[error("missing OPF rootfile entry in container.xml")]
    MissingRootfile,
    #[error("missing content.opf package document")]
    MissingPackage,
    #[error("missing manifest section in OPF")]
    MissingManifest,
    #[error("missing spine section in OPF")]
    MissingSpine,
    #[error("EPUB package does not contain a mimetype file")]
    MissingMimeType,
    #[error("EPUB does not contain any navigable XHTML spine content")]
    NoNavigableContent,
    #[error("invalid EPUB path in archive: {0}")]
    InvalidArchivePath(String),
}

impl TocOptimizer {
    /// Create a new optimizer instance.
    pub fn new() -> Self {
        Self
    }

    /// Repair the TOC of an EPUB and write the fixed package to `output_path`.
    ///
    /// The method is deterministic and safe for repeated execution: every run
    /// rewrites `nav.xhtml`, `toc.ncx` and the TOC-related fields in
    /// `content.opf` based on the normalized TOC tree.
    pub fn optimize_epub<P, Q>(
        &self,
        input_path: P,
        output_path: Q,
    ) -> Result<TocOptimizationReport, TocOptimizerError>
    where
        P: AsRef<Path>,
        Q: AsRef<Path>,
    {
        let input_path = input_path.as_ref();
        let output_path = output_path.as_ref();
        let workspace = tempdir()?;
        let root_dir = workspace.path().join("epub");

        fs::create_dir_all(&root_dir)?;
        extract_epub(input_path, &root_dir)?;

        let container_path = root_dir.join("META-INF").join("container.xml");
        if !container_path.exists() {
            return Err(TocOptimizerError::MissingContainer);
        }

        let opf_relative_path = parse_container_rootfile(&container_path)?;
        let opf_path = root_dir.join(&opf_relative_path);
        if !opf_path.exists() {
            return Err(TocOptimizerError::MissingPackage);
        }

        let opf_dir = opf_path
            .parent()
            .map(Path::to_path_buf)
            .ok_or(TocOptimizerError::MissingPackage)?;
        let mut opf = parse_xml_file(&opf_path)?;
        let package = PackageInfo::from_opf(&opf)?;

        let mut report = TocOptimizationReport::default();
        let mut entries = first_non_empty(vec![
            load_nav_entries(&opf_dir, package.nav_href.as_deref()),
            load_ncx_entries(&opf_dir, package.ncx_href.as_deref()),
        ]);

        let nav_exists_before = package.nav_href.is_some();
        let ncx_exists_before = package.ncx_href.is_some();

        if entries.is_empty() {
            entries = rebuild_entries_from_spine(&opf_dir, &package)?;
            report.rebuilt_from_headings = true;
        }

        let normalized = normalize_entries(entries);
        report.duplicates_removed = normalized.duplicates_removed;
        report.hierarchy_normalized = normalized.hierarchy_normalized;
        report.entry_count = count_entries(&normalized.entries);

        if normalized.entries.is_empty() {
            return Err(TocOptimizerError::NoNavigableContent);
        }

        let nav_href = package
            .nav_href
            .clone()
            .unwrap_or_else(|| choose_generated_href(&package.manifest_hrefs, "kindle-nav.xhtml"));
        let ncx_href = package
            .ncx_href
            .clone()
            .unwrap_or_else(|| choose_generated_href(&package.manifest_hrefs, "kindle-toc.ncx"));

        write_nav_document(
            &opf_dir.join(base_href(&nav_href)),
            &package.book_title,
            &normalized.entries,
        )?;
        write_ncx_document(
            &opf_dir.join(base_href(&ncx_href)),
            &package.book_title,
            package.book_identifier.as_deref(),
            &normalized.entries,
        )?;

        let ensured = ensure_toc_manifest_items(&mut opf, &package, &nav_href, &ncx_href)?;
        report.nav_created = !nav_exists_before && ensured.nav_created;
        report.ncx_created = !ncx_exists_before && ensured.ncx_created;

        write_xml_file(&opf_path, &opf)?;
        package_epub(&root_dir, output_path)?;

        Ok(report)
    }

    /// Repair an EPUB in place.
    pub fn optimize_epub_in_place<P: AsRef<Path>>(
        &self,
        epub_path: P,
    ) -> Result<TocOptimizationReport, TocOptimizerError> {
        let epub_path = epub_path.as_ref();
        let temp_output = epub_path.with_extension("repaired.epub");
        let report = self.optimize_epub(epub_path, &temp_output)?;
        if epub_path.exists() {
            fs::remove_file(epub_path)?;
        }
        fs::rename(&temp_output, epub_path)?;
        Ok(report)
    }
}

#[derive(Debug, Clone)]
struct PackageInfo {
    book_title: String,
    book_identifier: Option<String>,
    nav_id: Option<String>,
    nav_href: Option<String>,
    ncx_id: Option<String>,
    ncx_href: Option<String>,
    manifest_hrefs: HashSet<String>,
    manifest_items: HashMap<String, ManifestItem>,
    spine_itemrefs: Vec<String>,
}

#[derive(Debug, Clone)]
struct ManifestItem {
    href: String,
    media_type: String,
}

#[derive(Debug)]
struct EnsureManifestResult {
    nav_created: bool,
    ncx_created: bool,
}

#[derive(Debug, Clone)]
struct FlatTocEntry {
    title: String,
    href: String,
    level: usize,
}

#[derive(Debug, Default)]
struct NormalizationResult {
    entries: Vec<TocEntry>,
    duplicates_removed: usize,
    hierarchy_normalized: bool,
}

impl PackageInfo {
    fn from_opf(opf: &Element) -> Result<Self, TocOptimizerError> {
        let manifest = find_child(opf, "manifest").ok_or(TocOptimizerError::MissingManifest)?;
        let spine = find_child(opf, "spine").ok_or(TocOptimizerError::MissingSpine)?;
        let metadata = find_child(opf, "metadata");

        let mut manifest_items = HashMap::new();
        let mut manifest_hrefs = HashSet::new();
        let mut nav_id = None;
        let mut nav_href = None;
        let mut ncx_id = None;
        let mut ncx_href = None;
        let spine_toc_id = spine.attributes.get("toc").cloned();

        for element in child_elements(manifest) {
            if local_name(&element.name) != "item" {
                continue;
            }

            let Some(id) = attribute(element, "id").map(str::to_string) else {
                continue;
            };
            let href = attribute(element, "href").unwrap_or_default().to_string();
            let media_type = attribute(element, "media-type")
                .unwrap_or_default()
                .to_string();
            let properties = parse_properties(attribute(element, "properties"));

            if properties.contains("nav") {
                nav_id = Some(id.clone());
                nav_href = Some(href.clone());
            }

            if media_type == "application/x-dtbncx+xml"
                || spine_toc_id.as_deref() == Some(id.as_str())
            {
                ncx_id = Some(id.clone());
                ncx_href = Some(href.clone());
            }

            manifest_hrefs.insert(normalize_href(&href));
            manifest_items.insert(id, ManifestItem { href, media_type });
        }

        let mut spine_itemrefs = Vec::new();
        for element in child_elements(spine) {
            if local_name(&element.name) != "itemref" {
                continue;
            }

            if let Some(idref) = attribute(element, "idref") {
                spine_itemrefs.push(idref.to_string());
            }
        }

        let book_title = metadata
            .and_then(|metadata| find_first_text_by_name(metadata, &["dc:title", "title"]))
            .unwrap_or_else(|| "Table of Contents".to_string());
        let book_identifier = metadata.and_then(|metadata| {
            find_first_text_by_name(metadata, &["dc:identifier", "identifier"])
        });

        Ok(Self {
            book_title,
            book_identifier,
            nav_id,
            nav_href,
            ncx_id,
            ncx_href,
            manifest_hrefs,
            manifest_items,
            spine_itemrefs,
        })
    }
}

fn extract_epub(input_path: &Path, output_dir: &Path) -> Result<(), TocOptimizerError> {
    let input_file = File::open(input_path)?;
    let mut archive = ZipArchive::new(input_file)?;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let Some(relative_path) = entry.enclosed_name() else {
            return Err(TocOptimizerError::InvalidArchivePath(
                entry.name().to_string(),
            ));
        };
        let output_path = output_dir.join(relative_path);

        if entry.is_dir() {
            fs::create_dir_all(&output_path)?;
            continue;
        }

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut output_file = File::create(&output_path)?;
        io::copy(&mut entry, &mut output_file)?;
    }

    if !output_dir.join("mimetype").exists() {
        return Err(TocOptimizerError::MissingMimeType);
    }

    Ok(())
}

fn package_epub(root_dir: &Path, output_path: &Path) -> Result<(), TocOptimizerError> {
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let output_file = File::create(output_path)?;
    let mut writer = ZipWriter::new(output_file);
    let stored_options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated_options =
        SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    let mimetype_path = root_dir.join("mimetype");
    if !mimetype_path.exists() {
        return Err(TocOptimizerError::MissingMimeType);
    }

    let mimetype_content = fs::read(&mimetype_path)?;
    writer.start_file("mimetype", stored_options)?;
    writer.write_all(&mimetype_content)?;

    let mut files = WalkDir::new(root_dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.path().to_path_buf())
        .filter(|path| path != &mimetype_path)
        .collect::<Vec<_>>();
    files.sort();

    for file_path in files {
        let relative = file_path
            .strip_prefix(root_dir)
            .map_err(|_| TocOptimizerError::InvalidArchivePath(file_path.display().to_string()))?;
        let zip_name = to_zip_path(relative);
        let content = fs::read(&file_path)?;

        writer.start_file(zip_name, deflated_options)?;
        writer.write_all(&content)?;
    }

    writer.finish()?;
    Ok(())
}

fn parse_container_rootfile(container_path: &Path) -> Result<PathBuf, TocOptimizerError> {
    let container = parse_xml_file(container_path)?;
    let rootfile = find_descendant(&container, &|element| {
        local_name(&element.name) == "rootfile"
    })
    .ok_or(TocOptimizerError::MissingRootfile)?;
    let full_path = attribute(rootfile, "full-path").ok_or(TocOptimizerError::MissingRootfile)?;

    Ok(PathBuf::from(full_path))
}

fn load_nav_entries(opf_dir: &Path, nav_href: Option<&str>) -> Vec<TocEntry> {
    let Some(nav_href) = nav_href else {
        return Vec::new();
    };
    let nav_path = opf_dir.join(base_href(nav_href));
    let Ok(nav) = parse_xml_file(&nav_path) else {
        return Vec::new();
    };
    let nav_root = find_descendant(&nav, &|element| {
        if local_name(&element.name) != "nav" {
            return false;
        }

        attribute_any(element, &["epub:type", "type", "role"])
            .map(|value| value.contains("toc") || value.contains("doc-toc"))
            .unwrap_or(false)
    })
    .or_else(|| find_descendant(&nav, &|element| local_name(&element.name) == "nav"));

    let Some(nav_root) = nav_root else {
        return Vec::new();
    };
    let Some(list_root) = find_descendant(nav_root, &|element| local_name(&element.name) == "ol")
    else {
        return Vec::new();
    };

    parse_nav_list(list_root)
}

fn parse_nav_list(list_root: &Element) -> Vec<TocEntry> {
    let mut entries = Vec::new();

    for child in child_elements(list_root) {
        if local_name(&child.name) != "li" {
            continue;
        }

        if let Some(entry) = parse_nav_item(child) {
            entries.push(entry);
        }
    }

    entries
}

fn parse_nav_item(list_item: &Element) -> Option<TocEntry> {
    let mut title = None;
    let mut href = None;
    let mut children = Vec::new();

    for child in child_elements(list_item) {
        match local_name(&child.name) {
            "a" => {
                href = attribute(child, "href").map(normalize_href);
                title = non_empty_text(text_from_element(child));
            }
            "span" | "p" => {
                if let Some((nested_title, nested_href)) = extract_link_or_text(child) {
                    if title.is_none() {
                        title = Some(nested_title);
                    }
                    if href.is_none() && !nested_href.is_empty() {
                        href = Some(normalize_href(&nested_href));
                    }
                }
            }
            "ol" => {
                children = parse_nav_list(child);
            }
            _ => {
                if title.is_none() {
                    if let Some((nested_title, nested_href)) = extract_link_or_text(child) {
                        title = Some(nested_title);
                        if href.is_none() && !nested_href.is_empty() {
                            href = Some(normalize_href(&nested_href));
                        }
                    }
                }
            }
        }
    }

    if title.is_none() && !children.is_empty() {
        title = Some(children[0].title.clone());
    }

    if href.as_deref().unwrap_or("").is_empty() && !children.is_empty() {
        href = Some(children[0].href.clone());
    }

    title.map(|title| TocEntry {
        title,
        href: href.unwrap_or_default(),
        children,
    })
}

fn load_ncx_entries(opf_dir: &Path, ncx_href: Option<&str>) -> Vec<TocEntry> {
    let Some(ncx_href) = ncx_href else {
        return Vec::new();
    };
    let ncx_path = opf_dir.join(base_href(ncx_href));
    let Ok(ncx) = parse_xml_file(&ncx_path) else {
        return Vec::new();
    };

    let Some(nav_map) = find_descendant(&ncx, &|element| local_name(&element.name) == "navMap")
    else {
        return Vec::new();
    };

    let mut entries = Vec::new();
    for child in child_elements(nav_map) {
        if local_name(&child.name) != "navPoint" {
            continue;
        }

        if let Some(entry) = parse_ncx_nav_point(child) {
            entries.push(entry);
        }
    }

    entries
}

fn parse_ncx_nav_point(nav_point: &Element) -> Option<TocEntry> {
    let title = find_descendant(nav_point, &|element| local_name(&element.name) == "text")
        .and_then(|element| non_empty_text(text_from_element(element)));
    let href = find_descendant(nav_point, &|element| local_name(&element.name) == "content")
        .and_then(|element| attribute(element, "src"))
        .map(normalize_href)
        .unwrap_or_default();

    let mut children = Vec::new();
    for child in child_elements(nav_point) {
        if local_name(&child.name) != "navPoint" {
            continue;
        }

        if let Some(entry) = parse_ncx_nav_point(child) {
            children.push(entry);
        }
    }

    title.map(|title| TocEntry {
        title,
        href,
        children,
    })
}

fn rebuild_entries_from_spine(
    opf_dir: &Path,
    package: &PackageInfo,
) -> Result<Vec<TocEntry>, TocOptimizerError> {
    let mut flat_entries = Vec::new();
    let mut found_any_content = false;

    for idref in &package.spine_itemrefs {
        let Some(item) = package.manifest_items.get(idref) else {
            continue;
        };
        if !is_xhtml_media_type(&item.media_type) {
            continue;
        }

        found_any_content = true;
        let item_path = opf_dir.join(base_href(&item.href));
        if !item_path.exists() {
            continue;
        }

        match collect_document_headings(&item_path, &normalize_href(&item.href)) {
            Ok(mut document_entries) => {
                normalize_document_levels(&mut document_entries);
                flat_entries.extend(document_entries);
            }
            Err(TocOptimizerError::XmlParse(_)) => {
                continue;
            }
            Err(error) => return Err(error),
        }
    }

    if !found_any_content {
        return Err(TocOptimizerError::NoNavigableContent);
    }

    if flat_entries.is_empty() {
        return Err(TocOptimizerError::NoNavigableContent);
    }

    Ok(build_tree_from_flat(flat_entries))
}

fn collect_document_headings(
    document_path: &Path,
    document_href: &str,
) -> Result<Vec<FlatTocEntry>, TocOptimizerError> {
    let mut root = parse_xml_file(document_path)?;
    let mut used_ids = collect_existing_ids(&root);
    let mut generated_index = 1usize;
    let mut semantic_entries = Vec::new();
    let mut modified = collect_semantic_headings(
        &mut root,
        document_href,
        &mut used_ids,
        &mut generated_index,
        &mut semantic_entries,
    );

    if !semantic_entries.is_empty() {
        if modified {
            write_xml_file(document_path, &root)?;
        }
        return Ok(semantic_entries);
    }

    let mut chapter_entries = Vec::new();
    modified = collect_chapter_pattern_entries(
        &mut root,
        document_href,
        &mut used_ids,
        &mut generated_index,
        &mut chapter_entries,
    );

    if modified {
        write_xml_file(document_path, &root)?;
    }

    Ok(chapter_entries)
}

fn collect_semantic_headings(
    element: &mut Element,
    document_href: &str,
    used_ids: &mut HashSet<String>,
    generated_index: &mut usize,
    entries: &mut Vec<FlatTocEntry>,
) -> bool {
    let mut modified = false;

    if let Some(level) = semantic_heading_level(element) {
        let title = text_from_element(element);
        if let Some(title) = non_empty_text(title) {
            let anchor = ensure_anchor_id(element, &title, used_ids, generated_index);
            modified |= anchor.generated;
            entries.push(FlatTocEntry {
                title,
                href: format!("{document_href}#{}", anchor.id),
                level,
            });
        }
    }

    for child in element.children.iter_mut() {
        if let XMLNode::Element(child) = child {
            modified |=
                collect_semantic_headings(child, document_href, used_ids, generated_index, entries);
        }
    }

    modified
}

fn collect_chapter_pattern_entries(
    element: &mut Element,
    document_href: &str,
    used_ids: &mut HashSet<String>,
    generated_index: &mut usize,
    entries: &mut Vec<FlatTocEntry>,
) -> bool {
    let mut modified = false;

    if can_use_as_chapter_marker(element) {
        let title = text_from_element(element);
        if let Some(title) = non_empty_text(title) {
            if title.len() <= 80 && CHINESE_CHAPTER_REGEX.is_match(&title) {
                let anchor = ensure_anchor_id(element, &title, used_ids, generated_index);
                modified |= anchor.generated;
                entries.push(FlatTocEntry {
                    title,
                    href: format!("{document_href}#{}", anchor.id),
                    level: 1,
                });
            }
        }
    }

    for child in element.children.iter_mut() {
        if let XMLNode::Element(child) = child {
            modified |= collect_chapter_pattern_entries(
                child,
                document_href,
                used_ids,
                generated_index,
                entries,
            );
        }
    }

    modified
}

fn normalize_document_levels(entries: &mut [FlatTocEntry]) {
    let Some(min_level) = entries.iter().map(|entry| entry.level).min() else {
        return;
    };

    for entry in entries {
        entry.level = entry.level.saturating_sub(min_level).max(0) + 1;
    }
}

fn normalize_entries(mut entries: Vec<TocEntry>) -> NormalizationResult {
    propagate_missing_hrefs(&mut entries);

    let mut flattened = Vec::new();
    flatten_entries(&entries, 1, &mut flattened);

    let mut result = NormalizationResult::default();
    let mut normalized_flat = Vec::new();
    let mut seen = HashSet::new();
    let mut previous_level = 0usize;

    for mut entry in flattened {
        entry.title = collapse_whitespace(&entry.title);
        entry.href = normalize_href(&entry.href);

        if entry.title.is_empty() || entry.href.is_empty() {
            continue;
        }

        let desired_level = entry.level.max(1);
        let actual_level = if previous_level == 0 {
            1
        } else if desired_level > previous_level + 1 {
            result.hierarchy_normalized = true;
            previous_level + 1
        } else {
            desired_level
        };
        entry.level = actual_level;
        previous_level = actual_level;

        let duplicate_key = format!(
            "{}::{}",
            entry.title.to_lowercase(),
            entry.href.to_lowercase()
        );
        if !seen.insert(duplicate_key) {
            result.duplicates_removed += 1;
            continue;
        }

        normalized_flat.push(entry);
    }

    result.entries = build_tree_from_flat(normalized_flat);
    result
}

fn propagate_missing_hrefs(entries: &mut [TocEntry]) -> Option<String> {
    let mut first_href = None;

    for entry in entries.iter_mut() {
        let child_href = propagate_missing_hrefs(&mut entry.children);

        if entry.href.trim().is_empty() {
            if let Some(child_href) = child_href.clone() {
                entry.href = child_href;
            }
        }

        if first_href.is_none() && !entry.href.trim().is_empty() {
            first_href = Some(entry.href.clone());
        }
    }

    first_href
}

fn flatten_entries(entries: &[TocEntry], level: usize, output: &mut Vec<FlatTocEntry>) {
    for entry in entries {
        output.push(FlatTocEntry {
            title: entry.title.clone(),
            href: entry.href.clone(),
            level,
        });
        flatten_entries(&entry.children, level + 1, output);
    }
}

fn build_tree_from_flat(entries: Vec<FlatTocEntry>) -> Vec<TocEntry> {
    let mut root = Vec::new();
    let mut path = Vec::<usize>::new();

    for entry in entries {
        while path.len() >= entry.level {
            path.pop();
        }

        let target = get_child_vec_mut(&mut root, &path);
        target.push(TocEntry {
            title: entry.title,
            href: entry.href,
            children: Vec::new(),
        });

        let last_index = target.len().saturating_sub(1);
        path.push(last_index);
    }

    root
}

fn get_child_vec_mut<'a>(root: &'a mut Vec<TocEntry>, path: &[usize]) -> &'a mut Vec<TocEntry> {
    let Some((first, rest)) = path.split_first() else {
        return root;
    };
    get_child_vec_mut(&mut root[*first].children, rest)
}

fn write_nav_document(
    output_path: &Path,
    title: &str,
    entries: &[TocEntry],
) -> Result<(), TocOptimizerError> {
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut root = Element::new("html");
    root.attributes.insert(
        "xmlns".to_string(),
        "http://www.w3.org/1999/xhtml".to_string(),
    );
    root.attributes.insert(
        "xmlns:epub".to_string(),
        "http://www.idpf.org/2007/ops".to_string(),
    );

    let mut head = Element::new("head");
    let mut title_element = Element::new("title");
    title_element.children.push(XMLNode::Text(
        non_empty_text(title.to_string()).unwrap_or_else(|| "Table of Contents".to_string()),
    ));
    head.children.push(XMLNode::Element(title_element));

    let mut body = Element::new("body");
    let mut nav = Element::new("nav");
    nav.attributes
        .insert("epub:type".to_string(), "toc".to_string());
    nav.attributes.insert("id".to_string(), "toc".to_string());

    let mut heading = Element::new("h1");
    heading.children.push(XMLNode::Text("Contents".to_string()));
    nav.children.push(XMLNode::Element(heading));
    nav.children.push(XMLNode::Element(build_nav_list(entries)));
    body.children.push(XMLNode::Element(nav));

    root.children.push(XMLNode::Element(head));
    root.children.push(XMLNode::Element(body));

    write_xml_file(output_path, &root)
}

fn build_nav_list(entries: &[TocEntry]) -> Element {
    let mut list = Element::new("ol");

    for entry in entries {
        let mut item = Element::new("li");
        let mut anchor = Element::new("a");
        anchor
            .attributes
            .insert("href".to_string(), entry.href.clone());
        anchor.children.push(XMLNode::Text(entry.title.clone()));
        item.children.push(XMLNode::Element(anchor));

        if !entry.children.is_empty() {
            item.children
                .push(XMLNode::Element(build_nav_list(&entry.children)));
        }

        list.children.push(XMLNode::Element(item));
    }

    list
}

fn write_ncx_document(
    output_path: &Path,
    title: &str,
    book_identifier: Option<&str>,
    entries: &[TocEntry],
) -> Result<(), TocOptimizerError> {
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut root = Element::new("ncx");
    root.attributes.insert(
        "xmlns".to_string(),
        "http://www.daisy.org/z3986/2005/ncx/".to_string(),
    );
    root.attributes
        .insert("version".to_string(), "2005-1".to_string());

    let mut head = Element::new("head");
    head.children.push(XMLNode::Element(build_ncx_meta(
        "dtb:uid",
        book_identifier.unwrap_or("generated-book-id"),
    )));
    head.children.push(XMLNode::Element(build_ncx_meta(
        "dtb:depth",
        &max_depth(entries).to_string(),
    )));
    head.children
        .push(XMLNode::Element(build_ncx_meta("dtb:totalPageCount", "0")));
    head.children
        .push(XMLNode::Element(build_ncx_meta("dtb:maxPageNumber", "0")));

    let mut doc_title = Element::new("docTitle");
    let mut doc_title_text = Element::new("text");
    doc_title_text.children.push(XMLNode::Text(
        non_empty_text(title.to_string()).unwrap_or_else(|| "Table of Contents".to_string()),
    ));
    doc_title.children.push(XMLNode::Element(doc_title_text));

    let mut nav_map = Element::new("navMap");
    let mut play_order = 1usize;
    for entry in entries {
        nav_map
            .children
            .push(XMLNode::Element(build_nav_point(entry, &mut play_order)));
    }

    root.children.push(XMLNode::Element(head));
    root.children.push(XMLNode::Element(doc_title));
    root.children.push(XMLNode::Element(nav_map));

    write_xml_file(output_path, &root)
}

fn build_ncx_meta(name: &str, content: &str) -> Element {
    let mut meta = Element::new("meta");
    meta.attributes.insert("name".to_string(), name.to_string());
    meta.attributes
        .insert("content".to_string(), content.to_string());
    meta
}

fn build_nav_point(entry: &TocEntry, play_order: &mut usize) -> Element {
    let current_order = *play_order;
    *play_order += 1;

    let mut nav_point = Element::new("navPoint");
    nav_point
        .attributes
        .insert("id".to_string(), format!("navPoint-{current_order}"));
    nav_point
        .attributes
        .insert("playOrder".to_string(), current_order.to_string());

    let mut nav_label = Element::new("navLabel");
    let mut text = Element::new("text");
    text.children.push(XMLNode::Text(entry.title.clone()));
    nav_label.children.push(XMLNode::Element(text));

    let mut content = Element::new("content");
    content
        .attributes
        .insert("src".to_string(), entry.href.clone());

    nav_point.children.push(XMLNode::Element(nav_label));
    nav_point.children.push(XMLNode::Element(content));

    for child in &entry.children {
        nav_point
            .children
            .push(XMLNode::Element(build_nav_point(child, play_order)));
    }

    nav_point
}

fn ensure_toc_manifest_items(
    opf: &mut Element,
    package: &PackageInfo,
    nav_href: &str,
    ncx_href: &str,
) -> Result<EnsureManifestResult, TocOptimizerError> {
    let nav_item_id = package
        .nav_id
        .clone()
        .unwrap_or_else(|| unique_manifest_id(opf, "generated-nav"));
    let ncx_item_id = package
        .ncx_id
        .clone()
        .unwrap_or_else(|| unique_manifest_id(opf, "generated-ncx"));

    let nav_created = {
        let manifest = find_child_mut(opf, "manifest").ok_or(TocOptimizerError::MissingManifest)?;
        ensure_manifest_item(
            manifest,
            &nav_item_id,
            nav_href,
            "application/xhtml+xml",
            Some("nav"),
        )
    };
    let ncx_created = {
        let manifest = find_child_mut(opf, "manifest").ok_or(TocOptimizerError::MissingManifest)?;
        ensure_manifest_item(
            manifest,
            &ncx_item_id,
            ncx_href,
            "application/x-dtbncx+xml",
            None,
        )
    };

    let spine = find_child_mut(opf, "spine").ok_or(TocOptimizerError::MissingSpine)?;
    spine
        .attributes
        .insert("toc".to_string(), ncx_item_id.to_string());

    Ok(EnsureManifestResult {
        nav_created,
        ncx_created,
    })
}

fn ensure_manifest_item(
    manifest: &mut Element,
    target_id: &str,
    target_href: &str,
    media_type: &str,
    property: Option<&str>,
) -> bool {
    let normalized_target_href = normalize_href(target_href);

    for child in manifest.children.iter_mut() {
        let XMLNode::Element(item) = child else {
            continue;
        };
        if local_name(&item.name) != "item" {
            continue;
        }

        let same_id = attribute(item, "id") == Some(target_id);
        let same_href = attribute(item, "href").map(normalize_href).as_deref()
            == Some(normalized_target_href.as_str());

        if same_id || same_href {
            item.attributes
                .insert("id".to_string(), target_id.to_string());
            item.attributes
                .insert("href".to_string(), normalized_target_href.clone());
            item.attributes
                .insert("media-type".to_string(), media_type.to_string());

            if let Some(property) = property {
                let mut properties = parse_properties(attribute(item, "properties"));
                properties.insert(property.to_string());
                item.attributes.insert(
                    "properties".to_string(),
                    properties.into_iter().collect::<Vec<_>>().join(" "),
                );
            }

            return false;
        }
    }

    let mut item = Element::new("item");
    item.attributes
        .insert("id".to_string(), target_id.to_string());
    item.attributes
        .insert("href".to_string(), normalized_target_href);
    item.attributes
        .insert("media-type".to_string(), media_type.to_string());
    if let Some(property) = property {
        item.attributes
            .insert("properties".to_string(), property.to_string());
    }
    manifest.children.push(XMLNode::Element(item));
    true
}

fn unique_manifest_id(opf: &Element, prefix: &str) -> String {
    let existing_ids = find_child(opf, "manifest")
        .map(|manifest| {
            child_elements(manifest)
                .filter_map(|item| attribute(item, "id").map(str::to_string))
                .collect::<HashSet<_>>()
        })
        .unwrap_or_default();

    if !existing_ids.contains(prefix) {
        return prefix.to_string();
    }

    let mut index = 2usize;
    loop {
        let candidate = format!("{prefix}-{index}");
        if !existing_ids.contains(&candidate) {
            return candidate;
        }
        index += 1;
    }
}

fn choose_generated_href(existing_hrefs: &HashSet<String>, preferred: &str) -> String {
    let preferred = normalize_href(preferred);
    if !existing_hrefs.contains(&preferred) {
        return preferred;
    }

    let stem = Path::new(&preferred)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("generated-toc");
    let extension = Path::new(&preferred)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("xhtml");

    let mut index = 2usize;
    loop {
        let candidate = format!("{stem}-{index}.{extension}");
        if !existing_hrefs.contains(&candidate) {
            return candidate;
        }
        index += 1;
    }
}

fn parse_xml_file(path: &Path) -> Result<Element, TocOptimizerError> {
    let content = fs::read(path)?;
    Ok(Element::parse(Cursor::new(content))?)
}

fn write_xml_file(path: &Path, root: &Element) -> Result<(), TocOptimizerError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = File::create(path)?;
    root.write(&mut file)?;
    Ok(())
}

fn find_child<'a>(element: &'a Element, name: &str) -> Option<&'a Element> {
    child_elements(element).find(|child| local_name(&child.name) == name)
}

fn find_child_mut<'a>(element: &'a mut Element, name: &str) -> Option<&'a mut Element> {
    for child in element.children.iter_mut() {
        let XMLNode::Element(child) = child else {
            continue;
        };
        if local_name(&child.name) == name {
            return Some(child);
        }
    }
    None
}

fn find_descendant<'a, F>(element: &'a Element, predicate: &F) -> Option<&'a Element>
where
    F: Fn(&Element) -> bool,
{
    if predicate(element) {
        return Some(element);
    }

    for child in child_elements(element) {
        if let Some(found) = find_descendant(child, predicate) {
            return Some(found);
        }
    }

    None
}

fn child_elements(element: &Element) -> impl Iterator<Item = &Element> {
    element.children.iter().filter_map(|node| match node {
        XMLNode::Element(element) => Some(element),
        _ => None,
    })
}

fn attribute<'a>(element: &'a Element, name: &str) -> Option<&'a str> {
    element.attributes.get(name).map(String::as_str)
}

fn attribute_any<'a>(element: &'a Element, names: &[&str]) -> Option<&'a str> {
    names.iter().find_map(|name| attribute(element, name))
}

fn parse_properties(value: Option<&str>) -> HashSet<String> {
    value
        .unwrap_or_default()
        .split_whitespace()
        .map(|part| part.to_string())
        .collect()
}

fn local_name(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

fn first_non_empty(candidates: Vec<Vec<TocEntry>>) -> Vec<TocEntry> {
    candidates
        .into_iter()
        .find(|entries| !entries.is_empty())
        .unwrap_or_default()
}

fn extract_link_or_text(element: &Element) -> Option<(String, String)> {
    if local_name(&element.name) == "a" {
        return non_empty_text(text_from_element(element)).map(|title| {
            (
                title,
                attribute(element, "href")
                    .map(str::to_string)
                    .unwrap_or_default(),
            )
        });
    }

    for child in child_elements(element) {
        if let Some(value) = extract_link_or_text(child) {
            return Some(value);
        }
    }

    non_empty_text(text_from_element(element)).map(|title| (title, String::new()))
}

fn text_from_element(element: &Element) -> String {
    let mut output = String::new();
    collect_text(element, &mut output);
    collapse_whitespace(&output)
}

fn collect_text(element: &Element, output: &mut String) {
    for child in &element.children {
        match child {
            XMLNode::Text(text) | XMLNode::CData(text) => {
                if !output.is_empty() {
                    output.push(' ');
                }
                output.push_str(text);
            }
            XMLNode::Element(element) => collect_text(element, output),
            _ => {}
        }
    }
}

fn find_first_text_by_name(element: &Element, names: &[&str]) -> Option<String> {
    for child in child_elements(element) {
        if names
            .iter()
            .any(|name| local_name(name) == local_name(&child.name))
        {
            if let Some(text) = non_empty_text(text_from_element(child)) {
                return Some(text);
            }
        }
    }
    None
}

fn non_empty_text(value: String) -> Option<String> {
    let normalized = collapse_whitespace(&value);
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn semantic_heading_level(element: &Element) -> Option<usize> {
    match local_name(&element.name) {
        "h1" => Some(1),
        "h2" => Some(2),
        "h3" => Some(3),
        _ => None,
    }
}

fn can_use_as_chapter_marker(element: &Element) -> bool {
    matches!(
        local_name(&element.name),
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "p" | "div" | "span" | "strong"
    )
}

#[derive(Debug)]
struct AnchorAssignment {
    id: String,
    generated: bool,
}

fn ensure_anchor_id(
    element: &mut Element,
    title: &str,
    used_ids: &mut HashSet<String>,
    generated_index: &mut usize,
) -> AnchorAssignment {
    if let Some(existing) = attribute(element, "id").map(str::to_string) {
        let normalized = collapse_whitespace(&existing);
        if !normalized.is_empty() {
            return AnchorAssignment {
                id: normalized,
                generated: false,
            };
        }
    }

    loop {
        let slug = slugify_title(title);
        let candidate = if slug.is_empty() {
            format!("toc-anchor-{}", *generated_index)
        } else {
            format!("{slug}-{}", *generated_index)
        };
        *generated_index += 1;

        if used_ids.insert(candidate.clone()) {
            element
                .attributes
                .insert("id".to_string(), candidate.clone());
            return AnchorAssignment {
                id: candidate,
                generated: true,
            };
        }
    }
}

fn collect_existing_ids(element: &Element) -> HashSet<String> {
    let mut ids = HashSet::new();
    collect_existing_ids_recursive(element, &mut ids);
    ids
}

fn collect_existing_ids_recursive(element: &Element, ids: &mut HashSet<String>) {
    if let Some(id) = attribute(element, "id") {
        let normalized = collapse_whitespace(id);
        if !normalized.is_empty() {
            ids.insert(normalized);
        }
    }

    for child in child_elements(element) {
        collect_existing_ids_recursive(child, ids);
    }
}

fn slugify_title(title: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;

    for character in title.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash {
            slug.push('-');
            previous_dash = true;
        }
    }

    slug.trim_matches('-').to_string()
}

fn max_depth(entries: &[TocEntry]) -> usize {
    fn inner(entries: &[TocEntry], current: usize) -> usize {
        let mut max_seen = current;
        for entry in entries {
            max_seen = max_seen.max(inner(&entry.children, current + 1));
        }
        max_seen
    }

    if entries.is_empty() {
        1
    } else {
        inner(entries, 1)
    }
}

fn count_entries(entries: &[TocEntry]) -> usize {
    entries
        .iter()
        .map(|entry| 1 + count_entries(&entry.children))
        .sum()
}

fn is_xhtml_media_type(media_type: &str) -> bool {
    matches!(
        media_type,
        "application/xhtml+xml" | "text/html" | "application/xml"
    )
}

fn normalize_href(value: &str) -> String {
    value.trim().replace('\\', "/")
}

fn base_href(value: &str) -> &str {
    value.split('#').next().unwrap_or(value)
}

fn to_zip_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn normalize_entries_removes_duplicates_and_repairs_depth() {
        let entries = vec![TocEntry {
            title: "Chapter 1".to_string(),
            href: "chapter1.xhtml#c1".to_string(),
            children: vec![
                TocEntry {
                    title: String::new(),
                    href: String::new(),
                    children: vec![TocEntry {
                        title: "Section 1.1".to_string(),
                        href: "chapter1.xhtml#s1".to_string(),
                        children: Vec::new(),
                    }],
                },
                TocEntry {
                    title: "Chapter 2".to_string(),
                    href: "chapter2.xhtml#c2".to_string(),
                    children: vec![
                        TocEntry {
                            title: "Section 2.1".to_string(),
                            href: "chapter2.xhtml#s1".to_string(),
                            children: vec![TocEntry {
                                title: "Deep Section".to_string(),
                                href: "chapter2.xhtml#deep".to_string(),
                                children: Vec::new(),
                            }],
                        },
                        TocEntry {
                            title: "Section 2.1".to_string(),
                            href: "chapter2.xhtml#s1".to_string(),
                            children: Vec::new(),
                        },
                    ],
                },
            ],
        }];

        let normalized = normalize_entries(entries);

        assert_eq!(normalized.duplicates_removed, 1);
        assert_eq!(count_entries(&normalized.entries), 5);
        assert!(normalized.hierarchy_normalized);
    }

    #[test]
    fn collect_document_headings_generates_anchor_ids() {
        let temp = tempdir().expect("tempdir");
        let document_path = temp.path().join("chapter.xhtml");
        fs::write(
            &document_path,
            r#"<?xml version="1.0" encoding="utf-8"?>
            <html xmlns="http://www.w3.org/1999/xhtml">
              <body>
                <h1>Chapter One</h1>
                <h2>Background</h2>
              </body>
            </html>"#,
        )
        .expect("write test xhtml");

        let entries =
            collect_document_headings(&document_path, "chapter.xhtml").expect("headings parsed");

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].level, 1);
        assert_eq!(entries[1].level, 2);

        let written = fs::read_to_string(&document_path).expect("rewritten xhtml");
        assert!(written.contains("id=\"chapter-one-1\""));
        assert!(written.contains("id=\"background-2\""));
    }

    #[test]
    fn optimize_epub_rebuilds_missing_toc_and_writes_nav_and_ncx() {
        let temp = tempdir().expect("tempdir");
        let input_epub = temp.path().join("input.epub");
        let output_epub = temp.path().join("output.epub");
        let book_dir = temp.path().join("book");

        fs::create_dir_all(book_dir.join("META-INF")).expect("meta-inf");
        fs::create_dir_all(book_dir.join("OEBPS")).expect("oebps");
        fs::write(&book_dir.join("mimetype"), "application/epub+zip").expect("mimetype");
        fs::write(
            book_dir.join("META-INF").join("container.xml"),
            r#"<?xml version="1.0" encoding="utf-8"?>
            <container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
              <rootfiles>
                <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
              </rootfiles>
            </container>"#,
        )
        .expect("container");
        fs::write(
            book_dir.join("OEBPS").join("content.opf"),
            r#"<?xml version="1.0" encoding="utf-8"?>
            <package version="3.0" unique-identifier="BookId" xmlns="http://www.idpf.org/2007/opf">
              <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
                <dc:title>Test Book</dc:title>
                <dc:identifier id="BookId">urn:test-book</dc:identifier>
              </metadata>
              <manifest>
                <item id="chapter1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
                <item id="chapter2" href="chapter2.xhtml" media-type="application/xhtml+xml"/>
              </manifest>
              <spine>
                <itemref idref="chapter1"/>
                <itemref idref="chapter2"/>
              </spine>
            </package>"#,
        )
        .expect("opf");
        fs::write(
            book_dir.join("OEBPS").join("chapter1.xhtml"),
            r#"<?xml version="1.0" encoding="utf-8"?>
            <html xmlns="http://www.w3.org/1999/xhtml">
              <body>
                <h1>Chapter 1</h1>
                <h2>Section 1.1</h2>
              </body>
            </html>"#,
        )
        .expect("chapter1");
        fs::write(
            book_dir.join("OEBPS").join("chapter2.xhtml"),
            r#"<?xml version="1.0" encoding="utf-8"?>
            <html xmlns="http://www.w3.org/1999/xhtml">
              <body>
                <p>第2章 再会</p>
              </body>
            </html>"#,
        )
        .expect("chapter2");

        package_epub(&book_dir, &input_epub).expect("input epub");

        let report = TocOptimizer::new()
            .optimize_epub(&input_epub, &output_epub)
            .expect("toc optimized");

        assert!(report.rebuilt_from_headings);
        assert!(report.nav_created);
        assert!(report.ncx_created);
        assert_eq!(report.entry_count, 3);

        let mut archive =
            ZipArchive::new(File::open(&output_epub).expect("open output")).expect("zip output");

        let nav_content = read_zip_entry(&mut archive, "OEBPS/kindle-nav.xhtml");
        assert!(nav_content.contains("Chapter 1"));
        assert!(nav_content.contains("Section 1.1"));
        assert!(nav_content.contains("第2章 再会"));

        let ncx_content = read_zip_entry(&mut archive, "OEBPS/kindle-toc.ncx");
        assert!(ncx_content.contains("navPoint-1"));
        assert!(ncx_content.contains("chapter1.xhtml#chapter-1-1"));

        let opf_content = read_zip_entry(&mut archive, "OEBPS/content.opf");
        assert!(opf_content.contains("properties=\"nav\""));
        assert!(opf_content.contains("media-type=\"application/x-dtbncx+xml\""));
        assert!(opf_content.contains("toc=\"generated-ncx\""));
    }

    fn read_zip_entry<R: Read + io::Seek>(archive: &mut ZipArchive<R>, name: &str) -> String {
        let mut file = archive.by_name(name).expect("zip entry present");
        let mut content = String::new();
        file.read_to_string(&mut content).expect("zip read");
        content
    }
}
