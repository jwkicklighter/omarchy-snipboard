//! Bounded machine-local image history. Binary payloads never cross QML's JSON pipe.
use std::collections::HashSet;
use std::io::Cursor;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use image::{ImageFormat, ImageReader, Limits};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::search::Record;
use crate::storage::{atomic_write, atomic_write_bytes, read_private};
use crate::{Error, Result, sys};

pub const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
const MAX_TOTAL: usize = 128 * 1024 * 1024;
const MAX_ITEMS: usize = 200;
const MAX_AGE: f64 = 30.0 * 86400.0;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    hash: String,
    mime: String,
    width: u32,
    height: u32,
    bytes: usize,
    created: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::FAIL_REPLACE;
    use std::os::unix::fs::{PermissionsExt, symlink};

    fn png() -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(3, 2).write_to(&mut bytes, ImageFormat::Png).unwrap();
        bytes.into_inner()
    }

    #[test]
    fn formats_roundtrip_dedupe_restart_delete_and_private_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut images = Images::read(root, 1.0).unwrap();
        for format in [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::WebP, ImageFormat::Gif, ImageFormat::Bmp] {
            let mut bytes = Cursor::new(Vec::new());
            image::DynamicImage::new_rgb8(3, 2).write_to(&mut bytes, format).unwrap();
            let bytes = bytes.into_inner();
            let hash = stage(root, &bytes).unwrap();
            let id = images.capture_pending(root, &hash, 2.0).unwrap().unwrap();
            assert!(!root.join("image.pending").exists());
            assert_eq!(images.payload(root, &id).unwrap().0, bytes);
            assert_eq!(std::fs::metadata(blob(root, &hash)).unwrap().permissions().mode() & 0o777, 0o600);
            let before = images.records().len();
            stage(root, &bytes).unwrap();
            assert_eq!(images.capture_pending(root, &hash, 3.0).unwrap().unwrap(), id);
            assert_eq!(images.records().len(), before);
        }
        let restored = Images::read(root, 4.0).unwrap();
        assert_eq!(restored.records(), images.records());
        let id = images.records()[0].id.clone();
        let path = PathBuf::from(images.preview(root, &id).unwrap());
        FAIL_REPLACE.set(true);
        let failed = images.delete(root, &id);
        FAIL_REPLACE.set(false);
        assert!(failed.is_err());
        assert!(images.contains(&id) && path.exists());
        images.delete(root, &id).unwrap();
        assert!(!path.exists());
        assert!(!Images::read(root, 5.0).unwrap().contains(&id));
        assert!(Images::read(root, MAX_AGE + 6.0).unwrap().records().is_empty());
        assert_eq!(std::fs::read_dir(root.join("images")).unwrap().count(), 0);
    }

    #[test]
    fn rejects_bad_images_symlinks_and_preserves_history_on_failure() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut images = Images::read(root, 1.0).unwrap();
        let hash = stage(root, &png()).unwrap();
        images.capture_pending(root, &hash, 1.0).unwrap();
        let before = images.records();
        assert!(stage(root, &vec![0; MAX_IMAGE_BYTES + 1]).is_err());
        let bad = stage(root, b"not an image").unwrap();
        assert!(images.capture_pending(root, &bad, 2.0).is_err());
        assert!(!root.join("image.pending").exists());
        assert!(images.capture_pending(root, "../../elsewhere", 2.0).is_err());
        assert_eq!(images.records(), before);
        stage(root, &png()).unwrap();
        FAIL_REPLACE.set(true);
        let failed = images.capture_pending(root, &hash, 2.0);
        FAIL_REPLACE.set(false);
        assert!(failed.is_err());
        assert_eq!(images.records(), before);
        symlink(root.join("images.local.json"), root.join("image.pending")).unwrap();
        assert!(stage(root, &png()).is_err());
        assert!(images.capture_pending(root, &hash, 2.0).is_err());
    }

    #[test]
    fn coalesces_pending_events_and_prunes_by_count_and_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut images = Images::read(root, 1.0).unwrap();
        let old = stage(root, b"old event").unwrap();
        let latest = stage(root, &png()).unwrap();
        assert_eq!(images.capture_pending(root, &old, 2.0).unwrap(), None);
        images.capture_pending(root, &latest, 3.0).unwrap();
        let entry = images.entries[0].clone();
        images.entries = (0..MAX_ITEMS + 5).map(|i| Entry { hash: format!("{i:064x}"), created: i as f64, ..entry.clone() }).collect();
        images.prune(1000.0);
        assert_eq!(images.entries.len(), MAX_ITEMS);
        for e in &mut images.entries { e.bytes = MAX_IMAGE_BYTES; }
        images.prune(1000.0);
        assert_eq!(images.entries.len(), MAX_TOTAL / MAX_IMAGE_BYTES);
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Images {
    entries: Vec<Entry>,
}

fn hash(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn blob(root: &Path, hash: &str) -> PathBuf { root.join("images").join(hash) }

fn describe(bytes: &[u8]) -> Result<(String, u32, u32)> {
    if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES { return Err(Error::Invalid("image exceeds limit")); }
    let format = image::guess_format(bytes).map_err(|_| Error::Invalid("unsupported image"))?;
    if !matches!(format, ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP | ImageFormat::Gif | ImageFormat::Bmp) {
        return Err(Error::Invalid("unsupported image"));
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|_| Error::Invalid("invalid or oversized image"))?;
    if u64::from(decoded.width()) * u64::from(decoded.height()) > 32 * 1024 * 1024 {
        return Err(Error::Invalid("image dimensions exceed limit"));
    }
    Ok((format.to_mime_type().into(), decoded.width(), decoded.height()))
}

/// One atomic staging slot coalesces rapid image changes without accumulating orphan files.
pub fn stage(root: &Path, bytes: &[u8]) -> Result<String> {
    if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES { return Err(Error::Invalid("image exceeds limit")); }
    let _lock = staging_lock(root)?;
    atomic_write_bytes(&root.join("image.pending"), bytes)?;
    Ok(hash(bytes))
}

fn staging_lock(root: &Path) -> Result<std::fs::File> {
    let file = std::fs::OpenOptions::new().write(true).create(true).truncate(false).mode(0o600)
        .custom_flags(libc::O_NOFOLLOW).open(root.join("image.pending.lock"))?;
    file.lock()?;
    Ok(file)
}

impl Images {
    pub fn read(root: &Path, now: f64) -> Result<Self> {
        let directory = root.join("images");
        std::fs::DirBuilder::new().mode(0o700).create(&directory).or_else(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists { Ok(()) } else { Err(error) }
        })?;
        let info = directory.symlink_metadata()?;
        if !info.is_dir() || info.uid() != sys::uid() || info.mode() & 0o077 != 0 {
            return Err(Error::Invalid("images must be a private directory"));
        }
        let path = root.join("images.local.json");
        let mut images: Self = match read_private(&path) {
            Ok(raw) => serde_json::from_slice(&raw).map_err(|_| Error::Invalid("invalid image history"))?,
            Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => return Err(e),
        };
        let mut seen = HashSet::new();
        if images.entries.len() > MAX_ITEMS || images.entries.iter().any(|e| {
            !valid_hash(&e.hash) || !seen.insert(e.hash.clone()) || e.bytes == 0 || e.bytes > MAX_IMAGE_BYTES
                || !e.created.is_finite() || e.created < 0.0
                || !matches!(e.mime.as_str(), "image/png" | "image/jpeg" | "image/webp" | "image/gif" | "image/bmp")
                || e.width == 0 || e.height == 0 || e.width > 16384 || e.height > 16384
                || u64::from(e.width) * u64::from(e.height) > 32 * 1024 * 1024
        }) || images.entries.iter().map(|e| e.bytes).sum::<usize>() > MAX_TOTAL {
            return Err(Error::Invalid("invalid image history"));
        }
        // Missing/corrupt blobs do not make the text and snippet library unavailable.
        images.entries.retain(|e| read_private(&blob(root, &e.hash)).is_ok_and(|raw| raw.len() == e.bytes && hash(&raw) == e.hash));
        images.prune(now);
        atomic_write(&path, &images)?;
        images.gc(root);
        Ok(images)
    }

    fn prune(&mut self, now: f64) {
        self.entries.retain(|e| now - e.created <= MAX_AGE);
        self.entries.sort_by(|a, b| b.created.total_cmp(&a.created));
        self.entries.truncate(MAX_ITEMS);
        let mut bytes = 0;
        self.entries.retain(|e| { bytes += e.bytes; bytes <= MAX_TOTAL });
    }

    fn gc(&self, root: &Path) {
        if let Ok(files) = std::fs::read_dir(root.join("images")) {
            for file in files.flatten() {
                let name = file.file_name();
                if name.to_str().is_some_and(|s| valid_hash(s) && !self.entries.iter().any(|e| e.hash == s)) {
                    let _ = std::fs::remove_file(file.path());
                }
            }
        }
    }

    pub fn capture_pending(&mut self, root: &Path, expected: &str, now: f64) -> Result<Option<String>> {
        if !valid_hash(expected) { return Err(Error::Invalid("invalid image identity")); }
        let _lock = staging_lock(root)?;
        let bytes = match read_private(&root.join("image.pending")) {
            Ok(bytes) => bytes,
            Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        if hash(&bytes) != expected { return Ok(None); } // Superseded by a newer clipboard event.
        std::fs::remove_file(root.join("image.pending"))?;
        let (mime, width, height) = describe(&bytes)?;
        let mut fresh = self.clone();
        fresh.entries.retain(|e| e.hash != expected);
        fresh.entries.push(Entry { hash: expected.into(), mime, width, height, bytes: bytes.len(), created: now });
        fresh.prune(now);
        atomic_write_bytes(&blob(root, expected), &bytes)?;
        atomic_write(&root.join("images.local.json"), &fresh)?;
        *self = fresh;
        self.gc(root);
        Ok(Some(format!("clipboard-image-{expected}")))
    }

    pub fn contains(&self, record: &str) -> bool { self.entry(record).is_some() }
    fn entry(&self, record: &str) -> Option<&Entry> {
        let hash = record.strip_prefix("clipboard-image-")?;
        self.entries.iter().find(|e| e.hash == hash)
    }
    pub fn payload(&self, root: &Path, record: &str) -> Result<(Vec<u8>, String)> {
        let entry = self.entry(record).ok_or(Error::Invalid("image no longer exists"))?;
        let bytes = read_private(&blob(root, &entry.hash))?;
        if hash(&bytes) != entry.hash { return Err(Error::Invalid("image changed")); }
        Ok((bytes, entry.mime.clone()))
    }
    pub fn preview(&self, root: &Path, record: &str) -> Option<String> {
        self.entry(record).map(|e| blob(root, &e.hash).to_string_lossy().into_owned())
    }
    pub fn delete(&mut self, root: &Path, record: &str) -> Result<()> {
        if !self.contains(record) { return Err(Error::Invalid("image no longer exists")); }
        let mut fresh = self.clone();
        fresh.entries.retain(|e| format!("clipboard-image-{}", e.hash) != record);
        atomic_write(&root.join("images.local.json"), &fresh)?;
        *self = fresh;
        self.gc(root);
        Ok(())
    }
    pub fn records(&self) -> Vec<Record> {
        self.entries.iter().map(|e| Record {
            id: format!("clipboard-image-{}", e.hash),
            name: format!("Image · {} × {}", e.width, e.height),
            body: format!("{} · {} × {} · {} KiB", e.mime, e.width, e.height, e.bytes.div_ceil(1024)),
            created: e.created, image: true,
            ..Record::default()
        }).collect()
    }
}
