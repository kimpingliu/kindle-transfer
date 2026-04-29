//! Kindle cover thumbnail sidecar support.
//!
//! Kindle sideloaded books sometimes need an explicit JPEG thumbnail written
//! into `system/thumbnails/` before the library shows the cover reliably. This
//! module keeps that behavior fully native: it parses MOBI/AZW3 PalmDB records
//! and EXTH metadata directly instead of shelling out to external ebook tools.

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::fs::{self, File, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::task;

const THUMBNAIL_HEIGHT: u32 = 68;
const THUMBNAIL_JPEG_QUALITY: u8 = 75;
const AMAZON_THUMBNAILS_DIR: &str = "system/thumbnails";
const AMAZON_COVER_BUG_CACHE_DIR: &str = "amazon-cover-bug";

const PALMDB_NUM_RECORDS_OFFSET: usize = 76;
const PALMDB_RECORD_LIST_OFFSET: usize = 78;
const PALMDB_RECORD_ENTRY_LEN: usize = 8;
const PALMDOC_TEXT_RECORD_COUNT_OFFSET: usize = 8;
const MOBI_MAGIC_OFFSET: usize = 16;
const MOBI_HEADER_LENGTH_OFFSET: usize = 20;
const MOBI_FIRST_IMAGE_INDEX_OFFSET: usize = 0x6c;
const EXTH_UUID: u32 = 113;
const EXTH_COVER_OFFSET: u32 = 201;
const EXTH_THUMBNAIL_OFFSET: u32 = 202;
const EXTH_CDE_TYPE: u32 = 501;
const EXTH_ASIN_ALT: u32 = 504;
const NULL_RECORD_INDEX: u32 = 0xFFFF_FFFF;

/// Best-effort service for maintaining Kindle cover thumbnails.
#[derive(Debug, Clone, Default)]
pub struct KindleThumbnailService;

impl KindleThumbnailService {
    /// Restore cached thumbnails into the active Kindle thumbnail directory.
    pub async fn sync_cached_thumbnails(
        &self,
        mount_path: &Path,
    ) -> Result<usize, KindleThumbnailError> {
        let source_dir = mount_path.join(AMAZON_COVER_BUG_CACHE_DIR);
        let destination_dir = mount_path.join(AMAZON_THUMBNAILS_DIR);

        if !path_is_dir(&source_dir).await? || !path_is_dir(&destination_dir).await? {
            return Ok(0);
        }

        let mut restored = 0usize;
        let mut entries = fs::read_dir(&source_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            if !entry.file_type().await?.is_file() {
                continue;
            }

            let source_path = entry.path();
            let destination_path = destination_dir.join(entry.file_name());
            let source_size = entry.metadata().await?.len();

            let needs_sync = match fs::metadata(&destination_path).await {
                Ok(metadata) => metadata.len() != source_size,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
                Err(error) => return Err(KindleThumbnailError::Io(error)),
            };

            if !needs_sync {
                continue;
            }

            let bytes = fs::read(&source_path).await?;
            write_sync_file(&destination_path, &bytes).await?;
            restored += 1;
        }

        Ok(restored)
    }

    /// Extract a MOBI/AZW3 cover or embedded thumbnail, render a Kindle-sized
    /// JPEG thumbnail, and write both the active thumbnail and cache copies.
    ///
    /// Some third-party books do not contain the EXTH UUID/CDE metadata needed
    /// to derive Kindle's exact thumbnail file name. In that case the upload is
    /// left untouched and the Kindle can still index the cover from the AZW3
    /// container itself.
    pub async fn upload_thumbnail_for_book(
        &self,
        book_path: &Path,
        mount_path: &Path,
    ) -> Result<Option<PathBuf>, KindleThumbnailError> {
        let book_bytes = fs::read(book_path).await?;
        let Some(thumbnail_source) =
            task::spawn_blocking(move || parse_mobi_thumbnail_source(&book_bytes)).await??
        else {
            return Ok(None);
        };

        let thumbnail_bytes = render_thumbnail_bytes(thumbnail_source.image_bytes).await?;

        let thumbnails_dir = mount_path.join(AMAZON_THUMBNAILS_DIR);
        fs::create_dir_all(&thumbnails_dir).await?;

        let thumbnail_path = thumbnails_dir.join(&thumbnail_source.file_name);
        write_sync_file(&thumbnail_path, &thumbnail_bytes).await?;

        let cache_dir = mount_path.join(AMAZON_COVER_BUG_CACHE_DIR);
        fs::create_dir_all(&cache_dir).await?;
        write_sync_file(
            &cache_dir.join(&thumbnail_source.file_name),
            &thumbnail_bytes,
        )
        .await?;

        Ok(Some(thumbnail_path))
    }
}

/// Thumbnail synchronization failures are intentionally isolated from the main
/// file transfer so the upload can still succeed even if cover generation does not.
#[derive(Debug, Error)]
pub enum KindleThumbnailError {
    #[error("MOBI/AZW3 thumbnail metadata is invalid: {0}")]
    InvalidMobi(String),
    #[error("image thumbnail rendering failed: {0}")]
    Image(#[from] image::ImageError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("background task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

#[derive(Debug, Clone)]
struct ThumbnailSource {
    file_name: String,
    image_bytes: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
struct MobiThumbnailMetadata {
    uuid: Option<String>,
    cde_type: Option<String>,
    cover_offset: Option<u32>,
    thumbnail_offset: Option<u32>,
}

fn parse_mobi_thumbnail_source(
    data: &[u8],
) -> Result<Option<ThumbnailSource>, KindleThumbnailError> {
    let records = parse_record_offsets(data)?;
    let Some(record0) = record_slice(data, &records, 0) else {
        return Ok(None);
    };
    if record0.get(MOBI_MAGIC_OFFSET..MOBI_MAGIC_OFFSET + 4) != Some(b"MOBI".as_slice()) {
        return Ok(None);
    }

    let metadata = parse_exth_metadata(record0)?;
    let Some(uuid) = metadata.uuid.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    let Some(cde_type) = metadata.cde_type.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };

    let image_records = collect_image_record_indices(data, &records, record0);
    if image_records.is_empty() {
        return Ok(None);
    }

    let image_index = metadata
        .thumbnail_offset
        .and_then(|offset| image_records.get(offset as usize).copied())
        .or_else(|| {
            metadata
                .cover_offset
                .and_then(|offset| image_records.get(offset as usize).copied())
        });

    let Some(image_index) = image_index else {
        return Ok(None);
    };
    let Some(image_record) = record_slice(data, &records, image_index) else {
        return Ok(None);
    };

    Ok(Some(ThumbnailSource {
        file_name: format!("thumbnail_{uuid}_{cde_type}_portrait.jpg"),
        image_bytes: image_record.to_vec(),
    }))
}

fn parse_record_offsets(data: &[u8]) -> Result<Vec<usize>, KindleThumbnailError> {
    if data.len() < PALMDB_RECORD_LIST_OFFSET {
        return Err(KindleThumbnailError::InvalidMobi(
            "PalmDB header is truncated".to_string(),
        ));
    }

    let record_count = read_u16_be(data, PALMDB_NUM_RECORDS_OFFSET)
        .ok_or_else(|| KindleThumbnailError::InvalidMobi("missing record count".to_string()))?
        as usize;
    let record_list_end = PALMDB_RECORD_LIST_OFFSET + record_count * PALMDB_RECORD_ENTRY_LEN;
    if data.len() < record_list_end {
        return Err(KindleThumbnailError::InvalidMobi(
            "PalmDB record list is truncated".to_string(),
        ));
    }

    let mut offsets = Vec::with_capacity(record_count);
    let mut previous = 0usize;
    for index in 0..record_count {
        let offset = read_u32_be(
            data,
            PALMDB_RECORD_LIST_OFFSET + index * PALMDB_RECORD_ENTRY_LEN,
        )
        .ok_or_else(|| KindleThumbnailError::InvalidMobi("missing record offset".to_string()))?
            as usize;
        if offset > data.len() || (index > 0 && offset < previous) {
            return Err(KindleThumbnailError::InvalidMobi(
                "PalmDB record offsets are invalid".to_string(),
            ));
        }
        offsets.push(offset);
        previous = offset;
    }

    Ok(offsets)
}

fn record_slice<'a>(data: &'a [u8], records: &[usize], index: usize) -> Option<&'a [u8]> {
    let start = *records.get(index)?;
    let end = records.get(index + 1).copied().unwrap_or(data.len());
    data.get(start..end)
}

fn parse_exth_metadata(record0: &[u8]) -> Result<MobiThumbnailMetadata, KindleThumbnailError> {
    let header_length = read_u32_be(record0, MOBI_HEADER_LENGTH_OFFSET).ok_or_else(|| {
        KindleThumbnailError::InvalidMobi("missing MOBI header length".to_string())
    })? as usize;
    let exth_start = MOBI_MAGIC_OFFSET + header_length;
    if record0.get(exth_start..exth_start + 4) != Some(b"EXTH".as_slice()) {
        return Ok(MobiThumbnailMetadata::default());
    }

    let exth_length = read_u32_be(record0, exth_start + 4)
        .ok_or_else(|| KindleThumbnailError::InvalidMobi("missing EXTH length".to_string()))?
        as usize;
    let exth_count = read_u32_be(record0, exth_start + 8)
        .ok_or_else(|| KindleThumbnailError::InvalidMobi("missing EXTH record count".to_string()))?
        as usize;
    if exth_length < 12 || exth_start + exth_length > record0.len() {
        return Err(KindleThumbnailError::InvalidMobi(
            "EXTH block length is invalid".to_string(),
        ));
    }

    let mut metadata = MobiThumbnailMetadata::default();
    let mut cursor = exth_start + 12;
    let exth_end = exth_start + exth_length;

    for _ in 0..exth_count {
        if cursor + 8 > exth_end {
            return Err(KindleThumbnailError::InvalidMobi(
                "EXTH record header is truncated".to_string(),
            ));
        }

        let record_type = read_u32_be(record0, cursor).unwrap_or_default();
        let record_size = read_u32_be(record0, cursor + 4).unwrap_or_default() as usize;
        if record_size < 8 || cursor + record_size > exth_end {
            return Err(KindleThumbnailError::InvalidMobi(
                "EXTH record size is invalid".to_string(),
            ));
        }

        let payload = &record0[cursor + 8..cursor + record_size];
        match record_type {
            EXTH_UUID | EXTH_ASIN_ALT => {
                if metadata.uuid.is_none() {
                    metadata.uuid = decode_ascii(payload);
                }
            }
            EXTH_CDE_TYPE => {
                metadata.cde_type = decode_ascii(payload);
            }
            EXTH_COVER_OFFSET => {
                metadata.cover_offset = read_u32_payload(payload);
            }
            EXTH_THUMBNAIL_OFFSET => {
                metadata.thumbnail_offset = read_u32_payload(payload);
            }
            _ => {}
        }

        cursor += record_size;
    }

    Ok(metadata)
}

fn collect_image_record_indices(data: &[u8], records: &[usize], record0: &[u8]) -> Vec<usize> {
    let explicit_first_image = read_u32_be(record0, MOBI_FIRST_IMAGE_INDEX_OFFSET)
        .filter(|value| *value != 0 && *value != NULL_RECORD_INDEX)
        .map(|value| value as usize);
    let text_record_count = read_u16_be(record0, PALMDOC_TEXT_RECORD_COUNT_OFFSET)
        .map(|value| value as usize)
        .unwrap_or_default();

    let scan_start = explicit_first_image.unwrap_or(text_record_count.saturating_add(1));
    let mut image_records = Vec::new();
    for index in scan_start..records.len() {
        let Some(record) = record_slice(data, records, index) else {
            continue;
        };
        if is_embedded_image(record) {
            image_records.push(index);
        } else if !image_records.is_empty() {
            break;
        }
    }

    if image_records.is_empty() && explicit_first_image.is_some() {
        for index in 1..records.len() {
            let Some(record) = record_slice(data, records, index) else {
                continue;
            };
            if is_embedded_image(record) {
                image_records.push(index);
            } else if !image_records.is_empty() {
                break;
            }
        }
    }

    image_records
}

fn is_embedded_image(record: &[u8]) -> bool {
    record.starts_with(&[0xFF, 0xD8, 0xFF])
        || record.starts_with(b"\x89PNG\r\n\x1A\n")
        || record.starts_with(b"GIF87a")
        || record.starts_with(b"GIF89a")
}

fn read_u16_be(data: &[u8], offset: usize) -> Option<u16> {
    data.get(offset..offset + 2)
        .map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]))
}

fn read_u32_be(data: &[u8], offset: usize) -> Option<u32> {
    data.get(offset..offset + 4)
        .map(|bytes| u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_u32_payload(payload: &[u8]) -> Option<u32> {
    if payload.len() == 4 {
        Some(u32::from_be_bytes([
            payload[0], payload[1], payload[2], payload[3],
        ]))
    } else {
        None
    }
}

fn decode_ascii(payload: &[u8]) -> Option<String> {
    std::str::from_utf8(payload)
        .ok()
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.is_ascii())
        .map(str::to_string)
}

async fn render_thumbnail_bytes(cover_bytes: Vec<u8>) -> Result<Vec<u8>, KindleThumbnailError> {
    task::spawn_blocking(move || {
        let image = image::load_from_memory(&cover_bytes)?;
        let (width, height) = (image.width().max(1), image.height().max(1));
        let target_width =
            ((width as f64 * THUMBNAIL_HEIGHT as f64) / height as f64).round() as u32;
        let resized =
            image.resize_exact(target_width.max(1), THUMBNAIL_HEIGHT, FilterType::Lanczos3);

        let mut buffer = Cursor::new(Vec::new());
        let mut encoder = JpegEncoder::new_with_quality(&mut buffer, THUMBNAIL_JPEG_QUALITY);
        encoder.encode_image(&resized)?;
        Ok::<Vec<u8>, image::ImageError>(buffer.into_inner())
    })
    .await?
    .map_err(KindleThumbnailError::Image)
}

async fn path_is_dir(path: &Path) -> Result<bool, std::io::Error> {
    match fs::metadata(path).await {
        Ok(metadata) => Ok(metadata.is_dir()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

async fn write_sync_file(path: &Path, bytes: &[u8]) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }

    let mut file: File = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .await?;
    file.write_all(bytes).await?;
    file.flush().await?;
    file.sync_all().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn render_thumbnail_bytes_normalizes_height() {
        let source = task::spawn_blocking(|| {
            let image = image::DynamicImage::new_rgb8(320, 640);
            let mut buffer = Cursor::new(Vec::new());
            let mut encoder = JpegEncoder::new_with_quality(&mut buffer, 90);
            encoder.encode_image(&image).expect("encode source image");
            buffer.into_inner()
        })
        .await
        .expect("spawn blocking");

        let thumbnail = render_thumbnail_bytes(source)
            .await
            .expect("thumbnail bytes");
        let decoded = image::load_from_memory(&thumbnail).expect("decode thumbnail");

        assert_eq!(decoded.height(), THUMBNAIL_HEIGHT);
        assert_eq!(decoded.width(), 34);
    }

    #[tokio::test]
    async fn sync_cached_thumbnails_restores_missing_files() {
        let temp_dir = tempfile::TempDir::new().expect("tempdir");
        let mount_path = temp_dir.path();
        let cache_dir = mount_path.join(AMAZON_COVER_BUG_CACHE_DIR);
        let thumb_dir = mount_path.join(AMAZON_THUMBNAILS_DIR);
        fs::create_dir_all(&cache_dir)
            .await
            .expect("create cache dir");
        fs::create_dir_all(&thumb_dir)
            .await
            .expect("create thumb dir");
        fs::write(cache_dir.join("thumbnail_test.jpg"), b"cover-bytes")
            .await
            .expect("write cache file");

        let restored = KindleThumbnailService
            .sync_cached_thumbnails(mount_path)
            .await
            .expect("sync succeeds");

        assert_eq!(restored, 1);
        let restored_bytes = fs::read(thumb_dir.join("thumbnail_test.jpg"))
            .await
            .expect("restored thumb exists");
        assert_eq!(restored_bytes, b"cover-bytes");
    }

    #[test]
    fn parse_mobi_thumbnail_source_reads_exth_and_image_record() {
        let image = make_test_jpeg();
        let data = make_test_mobi(&image);

        let source = parse_mobi_thumbnail_source(&data)
            .expect("parse succeeds")
            .expect("thumbnail source");

        assert_eq!(source.file_name, "thumbnail_TESTASIN_PD0C_portrait.jpg");
        assert_eq!(source.image_bytes, image);
    }

    fn make_test_mobi(image: &[u8]) -> Vec<u8> {
        let exth = make_exth(&[
            (EXTH_ASIN_ALT, b"TESTASIN".as_slice()),
            (EXTH_CDE_TYPE, b"PD0C".as_slice()),
            (EXTH_COVER_OFFSET, &0u32.to_be_bytes()),
        ]);

        let mut record0 = vec![0u8; MOBI_MAGIC_OFFSET + 0xE8];
        record0[PALMDOC_TEXT_RECORD_COUNT_OFFSET..PALMDOC_TEXT_RECORD_COUNT_OFFSET + 2]
            .copy_from_slice(&0u16.to_be_bytes());
        record0[MOBI_MAGIC_OFFSET..MOBI_MAGIC_OFFSET + 4].copy_from_slice(b"MOBI");
        record0[MOBI_HEADER_LENGTH_OFFSET..MOBI_HEADER_LENGTH_OFFSET + 4]
            .copy_from_slice(&0xE8u32.to_be_bytes());
        record0.extend_from_slice(&exth);

        let records = vec![record0, image.to_vec()];
        let record_count = records.len();
        let mut data = vec![0u8; PALMDB_RECORD_LIST_OFFSET + record_count * 8];
        data[60..64].copy_from_slice(b"BOOK");
        data[64..68].copy_from_slice(b"MOBI");
        data[PALMDB_NUM_RECORDS_OFFSET..PALMDB_NUM_RECORDS_OFFSET + 2]
            .copy_from_slice(&(record_count as u16).to_be_bytes());

        let mut cursor = data.len();
        for (index, record) in records.iter().enumerate() {
            data[PALMDB_RECORD_LIST_OFFSET + index * 8..PALMDB_RECORD_LIST_OFFSET + index * 8 + 4]
                .copy_from_slice(&(cursor as u32).to_be_bytes());
            data.extend_from_slice(record);
            cursor += record.len();
        }

        data
    }

    fn make_exth(records: &[(u32, &[u8])]) -> Vec<u8> {
        let length = 12
            + records
                .iter()
                .map(|(_, payload)| 8 + payload.len())
                .sum::<usize>();
        let mut exth = Vec::with_capacity(length);
        exth.extend_from_slice(b"EXTH");
        exth.extend_from_slice(&(length as u32).to_be_bytes());
        exth.extend_from_slice(&(records.len() as u32).to_be_bytes());
        for (record_type, payload) in records {
            exth.extend_from_slice(&record_type.to_be_bytes());
            exth.extend_from_slice(&((8 + payload.len()) as u32).to_be_bytes());
            exth.extend_from_slice(payload);
        }
        exth
    }

    fn make_test_jpeg() -> Vec<u8> {
        let image = image::DynamicImage::new_rgb8(32, 48);
        let mut buffer = Cursor::new(Vec::new());
        let mut encoder = JpegEncoder::new_with_quality(&mut buffer, 90);
        encoder.encode_image(&image).expect("encode jpeg");
        buffer.into_inner()
    }
}
