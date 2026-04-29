//! Ebook conversion pipeline for Kindle delivery.
//!
//! The desktop shell needs two concrete capabilities before a USB copy becomes
//! useful on real Kindle hardware:
//!
//! - EPUB packages should be normalized into a Kindle-friendly TOC shape.
//! - EPUB files should be converted natively into KF8-only AZW3.
//! - Other Kindle-compatible source files should be passed through without
//!   invoking external converter tools.
//!
//! This module keeps that preparation step isolated from the uploader. The
//! uploader only receives already-prepared local files and remains focused on
//! transport concerns.

use crate::toc::toc_optimizer::TocOptimizer;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::io::Read;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use thiserror::Error;
use tokio::fs;
use tokio::task;
use tracing::{info, warn};
use uuid::Uuid;

/// Kindle output format currently used by the desktop shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindleFormat {
    /// Modern Kindle container suitable for USB sideloading.
    Azw3,
}

impl KindleFormat {
    /// Uppercase label used by the frontend.
    pub fn as_label(self) -> &'static str {
        match self {
            Self::Azw3 => "AZW3",
        }
    }

    /// Lowercase file extension written to disk.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Azw3 => "azw3",
        }
    }
}

/// Input metadata required for a single conversion.
#[derive(Debug, Clone)]
pub struct ConversionRequest {
    /// Original source file chosen by the user.
    pub source_path: PathBuf,
    /// Kindle output format selected by the device profile.
    pub preferred_format: KindleFormat,
}

impl ConversionRequest {
    /// Construct a conversion request from an absolute source path.
    pub fn new(source_path: impl Into<PathBuf>, preferred_format: KindleFormat) -> Self {
        Self {
            source_path: source_path.into(),
            preferred_format,
        }
    }
}

/// Prepared local file that is ready for USB upload.
#[derive(Debug, Clone)]
pub struct PreparedBook {
    /// Effective on-disk source used by the uploader.
    pub prepared_path: PathBuf,
    /// Final file name that should appear on the Kindle volume.
    pub destination_file_name: String,
    /// Output format label used by UI and history records.
    pub output_format: String,
    /// Number of bytes that will actually be copied to the Kindle.
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConversionBackend {
    NativeAzw3,
    NativePassthrough,
}

impl ConversionBackend {
    fn as_label(self) -> &'static str {
        match self {
            Self::NativeAzw3 => "Native AZW3",
            Self::NativePassthrough => "Native Passthrough",
        }
    }
}

/// Temporary workspace that keeps converted files alive until upload ends.
#[derive(Debug)]
pub struct ConversionWorkspace {
    temp_dir: TempDir,
}

impl ConversionWorkspace {
    /// Allocate a fresh temporary workspace for a batch upload.
    pub fn new() -> Result<Self, ConverterError> {
        Ok(Self {
            temp_dir: tempfile::tempdir()?,
        })
    }

    fn path(&self) -> &Path {
        self.temp_dir.path()
    }
}

/// High-level conversion service used by the desktop runtime.
#[derive(Debug, Clone, Default)]
pub struct EbookConversionService;

impl EbookConversionService {
    /// Prepare a single source file for Kindle USB delivery.
    pub async fn prepare_for_kindle(
        &self,
        request: &ConversionRequest,
        workspace: &ConversionWorkspace,
    ) -> Result<PreparedBook, ConverterError> {
        let source_path = &request.source_path;
        let source_file_name = source_path
            .file_name()
            .and_then(|value| value.to_str())
            .map(str::to_string)
            .ok_or_else(|| {
                ConverterError::InvalidSource(format!(
                    "source path has no file name: {}",
                    source_path.display()
                ))
            })?;
        let source_stem = source_path
            .file_stem()
            .and_then(|value| value.to_str())
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| "book".to_string());
        let source_extension = lower_extension(source_path);

        if source_extension.as_deref() == Some(request.preferred_format.extension()) {
            return prepare_native_passthrough(
                source_path,
                source_file_name,
                request.preferred_format.as_label().to_string(),
            )
            .await;
        }

        let is_epub = source_extension.as_deref() == Some("epub");
        let conversion_input = if is_epub {
            repair_epub_best_effort(source_path, workspace.path()).await
        } else {
            source_path.clone()
        };

        match request.preferred_format {
            KindleFormat::Azw3 => {
                if is_epub {
                    return prepare_native_epub_azw3(
                        &conversion_input,
                        &source_stem,
                        workspace.path(),
                    )
                    .await;
                }

                if is_native_passthrough_extension(source_extension.as_deref()) {
                    return prepare_native_passthrough(
                        source_path,
                        source_file_name,
                        format_label_for_extension(source_extension.as_deref().unwrap_or("file")),
                    )
                    .await;
                }
            }
        }

        Err(ConverterError::NativeUnsupported {
            extension: source_extension
                .map(|extension| extension.to_ascii_uppercase())
                .unwrap_or_else(|| "UNKNOWN".to_string()),
        })
    }
}

/// Errors raised while preparing a file for Kindle upload.
#[derive(Debug, Error)]
pub enum ConverterError {
    #[error("conversion source is invalid: {0}")]
    InvalidSource(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("EPUB archive error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("EPUB XML parse error: {0}")]
    XmlParse(#[from] xmltree::ParseError),
    #[error("native AZW3 conversion failed: {0}")]
    NativeAzw3(String),
    #[error(
        "built-in converter cannot convert {extension} yet. Upload EPUB/AZW3/MOBI/PDF directly."
    )]
    NativeUnsupported { extension: String },
    #[error("background task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

async fn prepare_native_epub_azw3(
    source_path: &Path,
    source_stem: &str,
    workspace_dir: &Path,
) -> Result<PreparedBook, ConverterError> {
    let output_path = workspace_dir.join(build_temp_file_name(source_stem, "azw3"));
    let input_path = source_path.to_path_buf();
    let destination_file_name = format!("{}.azw3", sanitize_stem(source_stem));
    let output_path_for_task = output_path.clone();

    task::spawn_blocking(move || convert_epub_to_azw3(&input_path, &output_path_for_task))
        .await??;

    info!(
        source = %source_path.display(),
        backend = ConversionBackend::NativeAzw3.as_label(),
        "ebook prepared with native AZW3 backend"
    );

    let size_bytes = fs::metadata(&output_path).await?.len();
    Ok(PreparedBook {
        prepared_path: output_path,
        destination_file_name,
        output_format: "AZW3".to_string(),
        size_bytes,
    })
}

async fn prepare_native_passthrough(
    source_path: &Path,
    destination_file_name: String,
    output_format: String,
) -> Result<PreparedBook, ConverterError> {
    info!(
        source = %source_path.display(),
        backend = ConversionBackend::NativePassthrough.as_label(),
        "ebook prepared with native passthrough backend"
    );

    let size_bytes = fs::metadata(source_path).await?.len();
    Ok(PreparedBook {
        prepared_path: source_path.to_path_buf(),
        destination_file_name,
        output_format,
        size_bytes,
    })
}

async fn repair_epub_best_effort(source_path: &Path, workspace_dir: &Path) -> PathBuf {
    let repaired_path = workspace_dir.join(build_temp_file_name("toc-repaired", "epub"));
    let input_path = source_path.to_path_buf();
    let output_path = repaired_path.clone();

    let result =
        task::spawn_blocking(move || TocOptimizer::new().optimize_epub(&input_path, &output_path))
            .await;

    match result {
        Ok(Ok(_report)) => repaired_path,
        Ok(Err(error)) => {
            warn!(
                source = %source_path.display(),
                "EPUB TOC repair failed before conversion, falling back to original source: {error}"
            );
            source_path.to_path_buf()
        }
        Err(error) => {
            warn!(
                source = %source_path.display(),
                "EPUB TOC repair task failed before conversion, falling back to original source: {error}"
            );
            source_path.to_path_buf()
        }
    }
}

fn lower_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
}

fn is_native_passthrough_extension(extension: Option<&str>) -> bool {
    matches!(
        extension,
        Some("azw" | "azw1" | "azw3" | "azw4" | "mobi" | "pdf" | "prc" | "txt")
    )
}

fn format_label_for_extension(extension: &str) -> String {
    extension.to_ascii_uppercase()
}

fn convert_epub_to_azw3(input_path: &Path, output_path: &Path) -> Result<(), ConverterError> {
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let (temp_dir, opf_path) = kindling::epub::extract_epub(input_path)
        .map_err(|error| ConverterError::NativeAzw3(error.to_string()))?;
    let result = kindling::mobi::build_mobi(
        &opf_path,
        output_path,
        false, // no_compress
        false, // headwords_only
        None,  // srcs_data
        false, // include_cmet
        true,  // no_hd_images: compact output; original images remain embedded.
        false, // creator_tag
        true,  // kf8_only: modern AZW3/KF8 output
        Some("EBOK"),
        false, // kindle_limits
        true,  // self_check
        false, // kindlegen_parity
        false, // strict_accents
    )
    .map_err(|error| ConverterError::NativeAzw3(error.to_string()));

    kindling::epub::cleanup_temp_dir(&temp_dir);
    result?;
    stamp_native_azw3_identity(input_path, output_path)?;
    Ok(())
}

fn stamp_native_azw3_identity(input_path: &Path, output_path: &Path) -> Result<(), ConverterError> {
    let native_asin = build_native_asin(input_path)?;
    let metadata = extract_epub_metadata(input_path).unwrap_or_default();
    let parent = output_path.parent().unwrap_or_else(|| Path::new("."));
    let backup_path = parent.join(format!(
        "unstamped-{}-{}.azw3",
        output_path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("book"),
        Uuid::new_v4()
    ));

    std::fs::rename(output_path, &backup_path)?;

    let updates = kindling::mobi_rewrite::MetadataUpdates {
        asin: Some(native_asin),
        authors: if metadata.authors.is_empty() {
            None
        } else {
            Some(metadata.authors)
        },
        title: metadata.title,
        language: metadata.language,
        ..Default::default()
    };
    let rewrite_result =
        kindling::mobi_rewrite::rewrite_mobi_metadata(&backup_path, output_path, &updates);

    if let Err(error) = rewrite_result {
        let _ = std::fs::rename(&backup_path, output_path);
        return Err(ConverterError::NativeAzw3(format!(
            "failed to stamp native AZW3 identity: {error}"
        )));
    }

    let _ = std::fs::remove_file(&backup_path);
    Ok(())
}

fn build_native_asin(input_path: &Path) -> Result<String, ConverterError> {
    let mut file = std::fs::File::open(input_path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let digest = hasher.finalize();
    let mut asin = String::from("KT");
    for byte in digest.iter().take(15) {
        write!(&mut asin, "{byte:02X}").expect("write to string");
    }
    Ok(asin)
}

#[derive(Debug, Default)]
struct EpubMetadata {
    title: Option<String>,
    authors: Vec<String>,
    language: Option<String>,
}

fn extract_epub_metadata(input_path: &Path) -> Result<EpubMetadata, ConverterError> {
    let file = std::fs::File::open(input_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut container_xml = String::new();
    archive
        .by_name("META-INF/container.xml")?
        .read_to_string(&mut container_xml)?;

    let container = roxmltree::Document::parse(&container_xml)
        .map_err(|error| ConverterError::NativeAzw3(error.to_string()))?;
    let opf_path = container
        .descendants()
        .find(|node| node.has_tag_name("rootfile"))
        .and_then(|node| node.attribute("full-path"))
        .ok_or_else(|| {
            ConverterError::NativeAzw3(
                "EPUB container does not declare an OPF rootfile".to_string(),
            )
        })?
        .to_string();

    let mut opf_xml = String::new();
    archive.by_name(&opf_path)?.read_to_string(&mut opf_xml)?;
    let opf = roxmltree::Document::parse(&opf_xml)
        .map_err(|error| ConverterError::NativeAzw3(error.to_string()))?;

    let title = first_text_by_local_name(&opf, "title");
    let language = first_text_by_local_name(&opf, "language");
    let mut authors = Vec::new();
    for node in opf
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "creator")
    {
        if let Some(author) = clean_metadata_text(node.text()) {
            if !authors.iter().any(|existing| existing == &author) {
                authors.push(author);
            }
        }
    }

    Ok(EpubMetadata {
        title,
        authors,
        language,
    })
}

fn first_text_by_local_name(
    document: &roxmltree::Document<'_>,
    local_name: &str,
) -> Option<String> {
    document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == local_name)
        .and_then(|node| clean_metadata_text(node.text()))
}

fn clean_metadata_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn build_temp_file_name(stem: &str, extension: &str) -> String {
    let sanitized_stem = sanitize_stem(stem);
    format!("{sanitized_stem}-{}.{}", Uuid::new_v4(), extension)
}

fn sanitize_stem(stem: &str) -> String {
    let compact = stem
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            character if character.is_control() => '_',
            character => character,
        })
        .collect::<String>()
        .trim()
        .to_string();

    if compact.is_empty() {
        "book".to_string()
    } else {
        compact
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs as std_fs;
    use std::fs::File;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    #[tokio::test]
    async fn azw3_uses_native_passthrough() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path().join("ready.azw3");
        std_fs::write(&source, b"azw3").expect("source");
        let workspace = ConversionWorkspace::new().expect("workspace");
        let converter = EbookConversionService;

        let prepared = converter
            .prepare_for_kindle(
                &ConversionRequest::new(source.clone(), KindleFormat::Azw3),
                &workspace,
            )
            .await
            .expect("native passthrough");

        assert_eq!(prepared.prepared_path, source);
        assert_eq!(prepared.destination_file_name, "ready.azw3");
        assert_eq!(prepared.output_format, "AZW3");
        assert_eq!(prepared.size_bytes, 4);
    }

    #[tokio::test]
    async fn kindle_compatible_files_use_native_passthrough() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path().join("manual.pdf");
        std_fs::write(&source, b"pdf").expect("source");
        let workspace = ConversionWorkspace::new().expect("workspace");
        let converter = EbookConversionService;

        let prepared = converter
            .prepare_for_kindle(
                &ConversionRequest::new(source.clone(), KindleFormat::Azw3),
                &workspace,
            )
            .await
            .expect("native fallback");

        assert_eq!(prepared.prepared_path, source);
        assert_eq!(prepared.destination_file_name, "manual.pdf");
        assert_eq!(prepared.output_format, "PDF");
        assert_eq!(prepared.size_bytes, 3);
    }

    #[tokio::test]
    async fn epub_converts_to_native_azw3() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path().join("book.epub");
        write_minimal_epub(&source).expect("epub");
        let workspace = ConversionWorkspace::new().expect("workspace");
        let converter = EbookConversionService;

        let prepared = converter
            .prepare_for_kindle(
                &ConversionRequest::new(source.clone(), KindleFormat::Azw3),
                &workspace,
            )
            .await
            .expect("epub native azw3 fallback");

        assert_eq!(prepared.destination_file_name, "book.azw3");
        assert_eq!(prepared.output_format, "AZW3");

        let azw3 = std_fs::read(prepared.prepared_path).expect("azw3 output");
        assert_eq!(&azw3[60..64], b"BOOK");
        assert_eq!(&azw3[64..68], b"MOBI");
        assert!(azw3.windows(4).any(|window| window == b"MOBI"));
    }

    fn write_minimal_epub(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let file = File::create(path)?;
        let mut writer = ZipWriter::new(file);
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

        writer.start_file("mimetype", stored)?;
        writer.write_all(b"application/epub+zip")?;
        writer.start_file("META-INF/container.xml", deflated)?;
        writer.write_all(
            r#"<?xml version="1.0" encoding="utf-8"?>
            <container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
              <rootfiles>
                <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
              </rootfiles>
            </container>"#
                .as_bytes(),
        )?;
        writer.start_file("OEBPS/content.opf", deflated)?;
        writer.write_all(
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
            </package>"#
                .as_bytes(),
        )?;
        writer.start_file("OEBPS/chapter1.xhtml", deflated)?;
        writer.write_all(
            r#"<?xml version="1.0" encoding="utf-8"?>
            <html xmlns="http://www.w3.org/1999/xhtml">
              <body>
                <h1>第一章 稀缺</h1>
                <p>内容 &amp; 细节</p>
              </body>
            </html>"#
                .as_bytes(),
        )?;
        writer.start_file("OEBPS/chapter2.xhtml", deflated)?;
        writer.write_all(
            r#"<?xml version="1.0" encoding="utf-8"?>
            <html xmlns="http://www.w3.org/1999/xhtml">
              <body>
                <h1>第二章 继续</h1>
              </body>
            </html>"#
                .as_bytes(),
        )?;
        writer.finish()?;
        Ok(())
    }
}
