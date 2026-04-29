//! Kindle on-device library scanning, rename and deletion.
//!
//! The module intentionally limits all operations to the mounted Kindle
//! `documents/` directory. Mutating operations accept a stable book id,
//! re-scan the device, and only operate on a file that was found by the
//! scanner.

use chrono::{DateTime, Local};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::task;
use walkdir::WalkDir;

const DOCUMENTS_DIR: &str = "documents";
const SUPPORTED_BOOK_EXTENSIONS: &[&str] = &[
    "azw", "azw1", "azw3", "azw4", "epub", "mobi", "pdf", "prc", "txt",
];
const SIDECAR_EXTENSIONS: &[&str] = &["apnx", "ea", "mbp", "pdr", "phl", "tan"];
const UNIQUE_SUFFIX_MIN_LEN: usize = 20;
const UNIQUE_SUFFIX_MAX_LEN: usize = 64;
const MAX_BOOK_TITLE_CHARS: usize = 180;

/// Frontend-facing Kindle book record.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KindleLibraryBook {
    pub id: String,
    pub title: String,
    pub format: String,
    pub size_mb: f64,
    pub size_label: String,
    pub modified_at: String,
    pub path: String,
    pub relative_path: String,
    pub sidecar_path: Option<String>,
}

/// Result returned after deleting a Kindle book.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteKindleBookResult {
    pub deleted_book_id: String,
    pub deleted_title: String,
    pub removed_paths: Vec<String>,
}

/// Scan a mounted Kindle for readable ebook files.
pub async fn scan_kindle_books(
    mount_path: impl Into<PathBuf>,
) -> Result<Vec<KindleLibraryBook>, KindleLibraryError> {
    let mount_path = mount_path.into();
    task::spawn_blocking(move || scan_kindle_books_blocking(&mount_path)).await?
}

/// Delete a scanned Kindle book by id.
pub async fn delete_kindle_book_by_id(
    mount_path: impl Into<PathBuf>,
    book_id: String,
) -> Result<DeleteKindleBookResult, KindleLibraryError> {
    let mount_path = mount_path.into();
    task::spawn_blocking(move || delete_kindle_book_by_id_blocking(&mount_path, &book_id)).await?
}

/// Rename a scanned Kindle book while preserving its original file extension.
pub async fn rename_kindle_book_by_id(
    mount_path: impl Into<PathBuf>,
    book_id: String,
    new_title: String,
) -> Result<KindleLibraryBook, KindleLibraryError> {
    let mount_path = mount_path.into();
    task::spawn_blocking(move || {
        rename_kindle_book_by_id_blocking(&mount_path, &book_id, &new_title)
    })
    .await?
}

/// Library scan, rename and delete failures.
#[derive(Debug, Error)]
pub enum KindleLibraryError {
    #[error("Kindle documents directory was not found: {0}")]
    DocumentsMissing(String),
    #[error("book was not found on the selected Kindle: {0}")]
    BookNotFound(String),
    #[error("book title is invalid after sanitizing: {0}")]
    InvalidBookTitle(String),
    #[error("rename target already exists: {0}")]
    RenameTargetExists(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("background task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

fn scan_kindle_books_blocking(
    mount_path: &Path,
) -> Result<Vec<KindleLibraryBook>, KindleLibraryError> {
    let documents_path = documents_path(mount_path);
    if !documents_path.is_dir() {
        return Err(KindleLibraryError::DocumentsMissing(
            documents_path.display().to_string(),
        ));
    }

    let mut books = Vec::new();
    for entry in WalkDir::new(&documents_path)
        .follow_links(false)
        .min_depth(1)
        .max_depth(8)
        .into_iter()
        .filter_entry(|entry| !is_sdr_dir(entry.path()))
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                let io_error = error.into_io_error().unwrap_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::Other, "walkdir failed")
                });
                return Err(KindleLibraryError::Io(io_error));
            }
        };

        if !entry.file_type().is_file() || !is_supported_book(entry.path()) {
            continue;
        }

        books.push(book_from_path(&documents_path, entry.path())?);
    }

    books.sort_by(|left, right| {
        left.title
            .cmp(&right.title)
            .then(left.format.cmp(&right.format))
            .then(left.path.cmp(&right.path))
    });
    Ok(books)
}

fn delete_kindle_book_by_id_blocking(
    mount_path: &Path,
    book_id: &str,
) -> Result<DeleteKindleBookResult, KindleLibraryError> {
    let books = scan_kindle_books_blocking(mount_path)?;
    let book = books
        .into_iter()
        .find(|book| book.id == book_id)
        .ok_or_else(|| KindleLibraryError::BookNotFound(book_id.to_string()))?;
    let book_path = PathBuf::from(&book.path);
    let mut removed_paths = Vec::new();

    remove_file_if_exists(&book_path, &mut removed_paths)?;
    remove_sidecars_for_book(&book_path, &mut removed_paths)?;

    Ok(DeleteKindleBookResult {
        deleted_book_id: book.id,
        deleted_title: book.title,
        removed_paths,
    })
}

fn rename_kindle_book_by_id_blocking(
    mount_path: &Path,
    book_id: &str,
    new_title: &str,
) -> Result<KindleLibraryBook, KindleLibraryError> {
    let documents_path = documents_path(mount_path);
    let books = scan_kindle_books_blocking(mount_path)?;
    let book = books
        .into_iter()
        .find(|book| book.id == book_id)
        .ok_or_else(|| KindleLibraryError::BookNotFound(book_id.to_string()))?;
    let source_path = PathBuf::from(&book.path);
    let extension = source_path.extension().and_then(|value| value.to_str());
    let sanitized_title = sanitize_book_title(new_title, extension)?;
    let target_path = renamed_book_path(&source_path, &sanitized_title);

    if source_path == target_path {
        return book_from_path(&documents_path, &source_path);
    }

    let sidecar_moves = sidecar_rename_pairs(&source_path, &target_path);
    ensure_rename_target_available(&source_path, &target_path)?;
    for (source, target) in &sidecar_moves {
        ensure_rename_target_available(source, target)?;
    }

    rename_existing_path(&source_path, &target_path)?;
    for (source, target) in sidecar_moves {
        rename_existing_path(&source, &target)?;
    }

    remove_appledouble_metadata_after_rename(&source_path, &target_path)?;
    book_from_path(&documents_path, &target_path)
}

fn book_from_path(
    documents_path: &Path,
    path: &Path,
) -> Result<KindleLibraryBook, KindleLibraryError> {
    let metadata = fs::metadata(path)?;
    let title = display_title_from_path(path);
    let format = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("file")
        .to_ascii_uppercase();
    let size_bytes = metadata.len();
    let sidecar_path = sidecar_dir_for_book(path).filter(|path| path.exists());
    let relative_path = path
        .strip_prefix(documents_path)
        .unwrap_or(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/");

    Ok(KindleLibraryBook {
        id: book_id(path),
        title,
        format,
        size_mb: round_size_mb(size_bytes),
        size_label: format_size_label(size_bytes),
        modified_at: metadata
            .modified()
            .ok()
            .map(format_system_time)
            .unwrap_or_else(|| "未知".to_string()),
        path: path.display().to_string(),
        relative_path,
        sidecar_path: sidecar_path.map(|path| path.display().to_string()),
    })
}

fn remove_sidecars_for_book(
    book_path: &Path,
    removed_paths: &mut Vec<String>,
) -> Result<(), KindleLibraryError> {
    if let Some(appledouble_path) = appledouble_path_for_book(book_path) {
        remove_file_if_exists(&appledouble_path, removed_paths)?;
    }

    if let Some(sidecar_dir) = sidecar_dir_for_book(book_path) {
        if sidecar_dir.is_dir() {
            fs::remove_dir_all(&sidecar_dir)?;
            removed_paths.push(sidecar_dir.display().to_string());
        } else if sidecar_dir.is_file() {
            fs::remove_file(&sidecar_dir)?;
            removed_paths.push(sidecar_dir.display().to_string());
        }
    }

    for extension in SIDECAR_EXTENSIONS {
        let sidecar_path = book_path.with_extension(extension);
        remove_file_if_exists(&sidecar_path, removed_paths)?;
    }

    Ok(())
}

fn remove_file_if_exists(
    path: &Path,
    removed_paths: &mut Vec<String>,
) -> Result<(), KindleLibraryError> {
    match fs::remove_file(path) {
        Ok(()) => {
            removed_paths.push(path.display().to_string());
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(KindleLibraryError::Io(error)),
    }
}

fn documents_path(mount_path: &Path) -> PathBuf {
    mount_path.join(DOCUMENTS_DIR)
}

fn sidecar_dir_for_book(path: &Path) -> Option<PathBuf> {
    let stem = path.file_stem()?.to_str()?;
    Some(path.with_file_name(format!("{stem}.sdr")))
}

fn appledouble_path_for_book(path: &Path) -> Option<PathBuf> {
    let file_name = path.file_name()?.to_str()?;
    Some(path.with_file_name(format!("._{file_name}")))
}

fn sanitize_book_title(title: &str, extension: Option<&str>) -> Result<String, KindleLibraryError> {
    let title_without_extension = strip_matching_extension(title.trim(), extension);
    let mut normalized = String::new();
    let mut previous_was_space = false;

    for character in title_without_extension.chars() {
        let next_character = if is_invalid_filename_character(character) {
            ' '
        } else {
            character
        };

        if next_character.is_whitespace() {
            if !previous_was_space {
                normalized.push(' ');
                previous_was_space = true;
            }
            continue;
        }

        normalized.push(next_character);
        previous_was_space = false;
    }

    let mut sanitized = normalized
        .trim_matches(|character| character == ' ' || character == '.')
        .chars()
        .take(MAX_BOOK_TITLE_CHARS)
        .collect::<String>()
        .trim_matches(|character| character == ' ' || character == '.')
        .to_string();

    if is_windows_reserved_file_stem(&sanitized) {
        sanitized.push_str("_book");
    }

    if sanitized.is_empty() {
        return Err(KindleLibraryError::InvalidBookTitle(title.to_string()));
    }

    Ok(sanitized)
}

fn strip_matching_extension<'a>(title: &'a str, extension: Option<&str>) -> &'a str {
    let Some(extension) = extension.filter(|extension| !extension.is_empty()) else {
        return title;
    };
    let suffix = format!(".{extension}");

    if title.len() > suffix.len()
        && title
            .to_ascii_lowercase()
            .ends_with(&suffix.to_ascii_lowercase())
    {
        &title[..title.len() - suffix.len()]
    } else {
        title
    }
}

fn is_invalid_filename_character(character: char) -> bool {
    character.is_control()
        || matches!(
            character,
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
        )
}

fn is_windows_reserved_file_stem(stem: &str) -> bool {
    let upper = stem.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

fn renamed_book_path(source_path: &Path, sanitized_title: &str) -> PathBuf {
    let file_name = match source_path.extension().and_then(|value| value.to_str()) {
        Some(extension) if !extension.is_empty() => format!("{sanitized_title}.{extension}"),
        _ => sanitized_title.to_string(),
    };

    source_path.with_file_name(file_name)
}

fn sidecar_rename_pairs(source_path: &Path, target_path: &Path) -> Vec<(PathBuf, PathBuf)> {
    let mut pairs = Vec::new();

    if let (Some(source_sidecar), Some(target_sidecar)) = (
        sidecar_dir_for_book(source_path),
        sidecar_dir_for_book(target_path),
    ) {
        if source_sidecar.exists() {
            pairs.push((source_sidecar, target_sidecar));
        }
    }

    for extension in SIDECAR_EXTENSIONS {
        let source_sidecar = source_path.with_extension(extension);
        if source_sidecar.exists() {
            pairs.push((source_sidecar, target_path.with_extension(extension)));
        }
    }

    pairs
}

fn ensure_rename_target_available(source: &Path, target: &Path) -> Result<(), KindleLibraryError> {
    if source == target || !target.exists() || paths_point_to_same_file(source, target) {
        return Ok(());
    }

    Err(KindleLibraryError::RenameTargetExists(
        target.display().to_string(),
    ))
}

fn rename_existing_path(source: &Path, target: &Path) -> Result<(), KindleLibraryError> {
    if source == target {
        return Ok(());
    }

    if target.exists() && paths_point_to_same_file(source, target) {
        let temporary_path = unique_temporary_rename_path(source)?;
        fs::rename(source, &temporary_path)?;
        fs::rename(&temporary_path, target)?;
        return Ok(());
    }

    fs::rename(source, target)?;
    Ok(())
}

fn paths_point_to_same_file(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn unique_temporary_rename_path(source: &Path) -> Result<PathBuf, KindleLibraryError> {
    let parent = source.parent().unwrap_or_else(|| Path::new("."));

    for _ in 0..100 {
        let candidate = parent.join(format!(".kindle-transfer-rename-{}", uuid::Uuid::new_v4()));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }

    Err(KindleLibraryError::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "failed to allocate temporary rename path",
    )))
}

fn remove_appledouble_metadata_after_rename(
    source_path: &Path,
    target_path: &Path,
) -> Result<(), KindleLibraryError> {
    let mut removed_paths = Vec::new();

    if let Some(source_appledouble_path) = appledouble_path_for_book(source_path) {
        remove_file_if_exists(&source_appledouble_path, &mut removed_paths)?;
    }

    if let Some(target_appledouble_path) = appledouble_path_for_book(target_path) {
        remove_file_if_exists(&target_appledouble_path, &mut removed_paths)?;
    }

    Ok(())
}

fn display_title_from_path(path: &Path) -> String {
    let raw_title = path
        .file_stem()
        .and_then(|value| value.to_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("未命名书籍");

    strip_generated_unique_suffix(raw_title)
        .unwrap_or(raw_title)
        .to_string()
}

fn strip_generated_unique_suffix(title: &str) -> Option<&str> {
    let (base, suffix) = title.rsplit_once('_')?;
    let base = base.trim_end();
    if base.is_empty() || !looks_like_generated_unique_suffix(suffix) {
        return None;
    }

    Some(base)
}

fn looks_like_generated_unique_suffix(suffix: &str) -> bool {
    let length = suffix.len();
    length >= UNIQUE_SUFFIX_MIN_LEN
        && length <= UNIQUE_SUFFIX_MAX_LEN
        && suffix
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
        && suffix.bytes().any(|byte| byte.is_ascii_digit())
}

fn is_sdr_dir(path: &Path) -> bool {
    path.is_dir()
        && path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("sdr"))
}

fn is_supported_book(path: &Path) -> bool {
    if is_appledouble_metadata(path) {
        return false;
    }

    path.extension()
        .and_then(|value| value.to_str())
        .map(|extension| {
            SUPPORTED_BOOK_EXTENSIONS
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        })
        .unwrap_or(false)
}

fn is_appledouble_metadata(path: &Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|file_name| file_name.starts_with("._"))
}

fn book_id(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    let digest = Sha256::digest(normalized.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn round_size_mb(bytes: u64) -> f64 {
    ((bytes as f64 / 1024.0 / 1024.0) * 10.0).round() / 10.0
}

fn format_size_label(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1024.0 / 1024.0)
}

fn format_system_time(time: std::time::SystemTime) -> String {
    let timestamp: DateTime<Local> = time.into();
    timestamp.format("%Y-%m-%d %H:%M").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn scan_lists_supported_books_and_skips_sdr_contents() {
        let temp = tempdir().expect("tempdir");
        let documents = temp.path().join(DOCUMENTS_DIR);
        fs::create_dir_all(documents.join("book.sdr")).expect("sidecar dir");
        fs::write(documents.join("book.azw3"), b"book").expect("book");
        fs::write(documents.join("book.sdr").join("cache.azw3f"), b"cache").expect("cache");
        fs::write(documents.join("._book.azw3"), b"appledouble").expect("appledouble");

        let books = scan_kindle_books(temp.path()).await.expect("scan succeeds");

        assert_eq!(books.len(), 1);
        assert_eq!(books[0].title, "book");
        assert_eq!(books[0].format, "AZW3");
    }

    #[test]
    fn display_title_hides_generated_kindle_suffix() {
        let title =
            display_title_from_path(Path::new("稀缺_XH564H5KFIM4E2LSXXA3TBC3VC3R6BDK.azw3"));

        assert_eq!(title, "稀缺");
    }

    #[test]
    fn display_title_keeps_normal_underscore_names() {
        let title = display_title_from_path(Path::new("Book_2024.azw3"));

        assert_eq!(title, "Book_2024");
    }

    #[tokio::test]
    async fn rename_updates_book_file_and_matching_sidecars() {
        let temp = tempdir().expect("tempdir");
        let documents = temp.path().join(DOCUMENTS_DIR);
        let book = documents.join("old.azw3");
        let appledouble = documents.join("._old.azw3");
        let sidecar = documents.join("old.sdr");
        let apnx = documents.join("old.apnx");
        fs::create_dir_all(&sidecar).expect("sidecar dir");
        fs::write(&book, b"book").expect("book");
        fs::write(&appledouble, b"appledouble").expect("appledouble");
        fs::write(&apnx, b"page map").expect("apnx");

        let books = scan_kindle_books(temp.path()).await.expect("scan succeeds");
        let renamed = rename_kindle_book_by_id(
            temp.path(),
            books[0].id.clone(),
            "新书: 名.azw3".to_string(),
        )
        .await
        .expect("rename succeeds");

        assert_eq!(renamed.title, "新书 名");
        assert!(documents.join("新书 名.azw3").exists());
        assert!(documents.join("新书 名.sdr").exists());
        assert!(documents.join("新书 名.apnx").exists());
        assert!(!book.exists());
        assert!(!appledouble.exists());
        assert!(!sidecar.exists());
        assert!(!apnx.exists());
    }

    #[tokio::test]
    async fn rename_rejects_existing_target_book() {
        let temp = tempdir().expect("tempdir");
        let documents = temp.path().join(DOCUMENTS_DIR);
        fs::create_dir_all(&documents).expect("documents dir");
        fs::write(documents.join("source.azw3"), b"source").expect("source");
        fs::write(documents.join("target.azw3"), b"target").expect("target");

        let books = scan_kindle_books(temp.path()).await.expect("scan succeeds");
        let source = books
            .iter()
            .find(|book| book.title == "source")
            .expect("source book");
        let error = rename_kindle_book_by_id(temp.path(), source.id.clone(), "target".to_string())
            .await
            .expect_err("rename should reject overwrite");

        assert!(matches!(error, KindleLibraryError::RenameTargetExists(_)));
        assert!(documents.join("source.azw3").exists());
        assert_eq!(
            fs::read(documents.join("target.azw3")).expect("target still exists"),
            b"target"
        );
    }

    #[tokio::test]
    async fn delete_removes_book_and_matching_sidecar_dir() {
        let temp = tempdir().expect("tempdir");
        let documents = temp.path().join(DOCUMENTS_DIR);
        let book = documents.join("book.azw3");
        let appledouble = documents.join("._book.azw3");
        let sidecar = documents.join("book.sdr");
        fs::create_dir_all(&sidecar).expect("sidecar dir");
        fs::write(&book, b"book").expect("book");
        fs::write(&appledouble, b"appledouble").expect("appledouble");

        let books = scan_kindle_books(temp.path()).await.expect("scan succeeds");
        let result = delete_kindle_book_by_id(temp.path(), books[0].id.clone())
            .await
            .expect("delete succeeds");

        assert_eq!(result.removed_paths.len(), 3);
        assert!(!book.exists());
        assert!(!appledouble.exists());
        assert!(!sidecar.exists());
    }
}
