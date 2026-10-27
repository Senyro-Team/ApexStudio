use super::{storage, Workspace};
use apex_core::Error;
use rusqlite::params;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    sync::{Arc, Condvar, Mutex, OnceLock, Weak},
};
const BUDGET: u64 = 1024 * 1024 * 1024;
struct Pool {
    used: Mutex<(usize, u64)>,
    wake: Condvar,
}
static POOL: OnceLock<Pool> = OnceLock::new();
static PEAK_WORKERS: AtomicUsize = AtomicUsize::new(0);
static PEAK_BYTES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
struct Permit(u64);
impl Permit {
    fn acquire(bytes: u64) -> Result<Self, Error> {
        let p = POOL.get_or_init(|| Pool {
            used: Mutex::new((0, 0)),
            wake: Condvar::new(),
        });
        let bytes = bytes.min(BUDGET);
        let mut used = p.used.lock().map_err(storage)?;
        while used.0 >= 4 || used.1 + bytes > BUDGET {
            used = p.wake.wait(used).map_err(storage)?;
        }
        used.0 += 1;
        used.1 += bytes;
        PEAK_WORKERS.fetch_max(used.0, Ordering::Relaxed);
        PEAK_BYTES.fetch_max(used.1, Ordering::Relaxed);
        Ok(Self(bytes))
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        if let Some(p) = POOL.get() {
            if let Ok(mut u) = p.used.lock() {
                u.0 -= 1;
                u.1 -= self.0;
                p.wake.notify_all();
            }
        }
    }
}
struct Prepared {
    root: PathBuf,
    file: tempfile::NamedTempFile,
    hash: String,
    size: u64,
    width: u32,
    height: u32,
}
#[derive(Clone)]
pub struct PreparedTexture(Arc<Prepared>);
static CACHE: OnceLock<Mutex<HashMap<PathBuf, Weak<Prepared>>>> = OnceLock::new();
static DECODES: AtomicUsize = AtomicUsize::new(0);
#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PreparationProgress {
    pub total: usize,
    pub ready: usize,
    pub errors: Vec<String>,
}
impl PreparedTexture {
    pub fn hash(&self) -> &str {
        &self.0.hash
    }
    pub fn size(&self) -> u64 {
        self.0.size
    }
}
pub fn png_dimensions(path: &Path) -> Result<(u32, u32, u64), Error> {
    let meta = fs::symlink_metadata(path).map_err(storage)?;
    if !meta.file_type().is_file() || meta.len() > 20 * 1024 * 1024 {
        return Err(Error::InvalidContent(
            "Select a regular PNG up to 20 MB.".into(),
        ));
    }
    let mut h = [0; 24];
    fs::File::open(path)
        .map_err(storage)?
        .read_exact(&mut h)
        .map_err(|_| Error::InvalidContent("Invalid PNG header.".into()))?;
    if &h[..8] != b"\x89PNG\r\n\x1a\n" || &h[12..16] != b"IHDR" {
        return Err(Error::InvalidContent("Select a PNG image.".into()));
    }
    let width = u32::from_be_bytes(h[16..20].try_into().map_err(storage)?);
    let height = u32::from_be_bytes(h[20..24].try_into().map_err(storage)?);
    if width == 0 || height == 0 || width > 8000 || height > 8000 {
        return Err(Error::InvalidContent(
            "PNG dimensions must be up to 8000 × 8000 pixels.".into(),
        ));
    }
    Ok((width, height, meta.len()))
}
struct Decoded {
    image: image::DynamicImage,
    _permit: Permit,
}
impl std::ops::Deref for Decoded {
    type Target = image::DynamicImage;
    fn deref(&self) -> &Self::Target {
        &self.image
    }
}
fn decode(path: &Path) -> Result<Decoded, Error> {
    let (w, h, _) = png_dimensions(path)?;
    let permit = Permit::acquire(u64::from(w) * u64::from(h) * 8 + 64 * 1024 * 1024)?;
    let mut reader = image::ImageReader::with_format(
        std::io::BufReader::new(fs::File::open(path).map_err(storage)?),
        image::ImageFormat::Png,
    );
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8000);
    limits.max_image_height = Some(8000);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    DECODES.fetch_add(1, Ordering::Relaxed);
    reader
        .decode()
        .map(|image| Decoded {
            image,
            _permit: permit,
        })
        .map_err(|_| Error::InvalidContent("Invalid or corrupt PNG image.".into()))
}
fn pending_thumbnails() -> &'static (Mutex<std::collections::HashSet<PathBuf>>, Condvar) {
    static PENDING: OnceLock<(Mutex<std::collections::HashSet<PathBuf>>, Condvar)> =
        OnceLock::new();
    PENDING.get_or_init(|| (Mutex::new(Default::default()), Condvar::new()))
}

type ThumbnailJob = (PathBuf, image::DynamicImage, Permit);
fn thumbnail_queue() -> &'static std::sync::mpsc::Sender<ThumbnailJob> {
    static QUEUE: OnceLock<std::sync::mpsc::Sender<ThumbnailJob>> = OnceLock::new();
    QUEUE.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<ThumbnailJob>();
        let rx = Arc::new(Mutex::new(rx));
        for _ in 0..2 {
            let rx = rx.clone();
            std::thread::spawn(move || loop {
                let job = match rx.lock() {
                    Ok(r) => r.recv(),
                    Err(_) => return,
                };
                let Ok((path, image, _permit)) = job else {
                    return;
                };
                let _ = (|| -> Result<(), Error> {
                    let mut bytes = std::io::Cursor::new(Vec::new());
                    image
                        .write_to(&mut bytes, image::ImageFormat::Png)
                        .map_err(storage)?;
                    let parent = path.parent().ok_or(Error::NotFound)?;
                    let mut tmp = tempfile::NamedTempFile::new_in(parent).map_err(storage)?;
                    tmp.write_all(bytes.get_ref()).map_err(storage)?;
                    match tmp.persist_noclobber(&path) {
                        Ok(_) => Ok(()),
                        Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
                        Err(e) => Err(storage(e)),
                    }
                })();
                let (pending, wake) = pending_thumbnails();
                if let Ok(mut p) = pending.lock() {
                    p.remove(&path);
                    wake.notify_all();
                }
            });
        }
        tx
    })
}
fn thumbnail_gate(path: &Path) -> Result<Arc<Mutex<()>>, Error> {
    static GATES: OnceLock<Mutex<HashMap<PathBuf, Weak<Mutex<()>>>>> = OnceLock::new();
    let mut gates = GATES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(storage)?;
    gates.retain(|_, g| g.strong_count() > 0);
    if let Some(g) = gates.get(path).and_then(Weak::upgrade) {
        return Ok(g);
    }
    let gate = Arc::new(Mutex::new(()));
    gates.insert(path.to_path_buf(), Arc::downgrade(&gate));
    Ok(gate)
}
impl Workspace {
    pub fn prepare_texture(&self, source: &Path) -> Result<PreparedTexture, Error> {
        let (width, height, _) = png_dimensions(source)?;
        let mut file =
            tempfile::NamedTempFile::new_in(self.root.join("staging")).map_err(storage)?;
        let copy_permit = Permit::acquire(64 * 1024)?;
        let mut input = fs::File::open(source).map_err(storage)?;
        let mut hasher = Sha256::new();
        let mut size = 0;
        let mut buffer = [0; 64 * 1024];
        loop {
            let n = input.read(&mut buffer).map_err(storage)?;
            if n == 0 {
                break;
            }
            size += n as u64;
            if size > 20 * 1024 * 1024 {
                return Err(Error::InvalidContent("PNG exceeds 20 MB.".into()));
            }
            hasher.update(&buffer[..n]);
            file.write_all(&buffer[..n]).map_err(storage)?;
        }
        let hash = format!("{:x}", hasher.finalize());
        drop(copy_permit);
        let key = self.root.join(&hash);
        let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        // Lock only per-blob preparation to deduplicate identical bytes across folders.
        static LOCKS: OnceLock<Mutex<HashMap<PathBuf, Weak<Mutex<()>>>>> = OnceLock::new();
        let locks = LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
        let lock = {
            let mut map = locks.lock().map_err(storage)?;
            map.retain(|_, v| v.strong_count() > 0);
            if let Some(l) = map.get(&key).and_then(Weak::upgrade) {
                l
            } else {
                let l = Arc::new(Mutex::new(()));
                map.insert(key.clone(), Arc::downgrade(&l));
                l
            }
        };
        let _guard = lock.lock().map_err(storage)?;
        if let Some(p) = cache
            .lock()
            .map_err(storage)?
            .get(&key)
            .and_then(Weak::upgrade)
        {
            return Ok(PreparedTexture(p));
        }
        let thumb = self.root.join("thumbnails").join(format!("{hash}.png"));
        let validated: bool = self
            .connection()?
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM image_validation WHERE hash=?1 AND validator=1)",
                [&hash],
                |r| r.get(0),
            )
            .map_err(storage)?;
        let thumbnail_pending = pending_thumbnails()
            .0
            .lock()
            .map_err(storage)?
            .contains(&thumb);
        if !validated || (!thumb.exists() && !thumbnail_pending) {
            let image = decode(file.path())?;
            if image.width() != width || image.height() != height {
                return Err(Error::InvalidContent(
                    "PNG changed during preparation.".into(),
                ));
            }
            let small = image.thumbnail(320, 200);
            drop(image);
            pending_thumbnails()
                .0
                .lock()
                .map_err(storage)?
                .insert(thumb.clone());
            thumbnail_queue()
                .send((thumb, small, Permit::acquire(320 * 200 * 4)?))
                .map_err(storage)?;
        }
        let p = Arc::new(Prepared {
            root: self.root.clone(),
            file,
            hash,
            size,
            width,
            height,
        });
        let mut map = cache.lock().map_err(storage)?;
        map.retain(|_, v| v.strong_count() > 0);
        map.insert(key, Arc::downgrade(&p));
        Ok(PreparedTexture(p))
    }
    pub(super) fn promote_texture(&self, p: &PreparedTexture) -> Result<(), Error> {
        if p.0.root != self.root {
            return Err(Error::InvalidContent(
                "Prepared texture belongs to a different workspace.".into(),
            ));
        }
        p.0.file.as_file().sync_all().map_err(storage)?;
        match fs::hard_link(p.0.file.path(), self.root.join("files").join(&p.0.hash)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            Err(e) => Err(storage(e)),
        }
    }
    pub(super) fn register_texture(
        tx: &rusqlite::Transaction<'_>,
        p: &PreparedTexture,
    ) -> Result<(), Error> {
        tx.execute(
            "INSERT OR IGNORE INTO managed_files(hash,size)VALUES(?1,?2)",
            params![p.0.hash, i64::try_from(p.0.size).map_err(storage)?],
        )
        .map_err(storage)?;
        tx.execute("INSERT OR REPLACE INTO image_validation(hash,width,height,validator)VALUES(?1,?2,?3,1)",params![p.0.hash,p.0.width,p.0.height]).map_err(storage)?;
        Ok(())
    }
    pub fn thumbnail_bytes(&self, hash: &str) -> Result<Vec<u8>, Error> {
        if !apex_core::valid_hash(hash) {
            return Err(Error::NotFound);
        }
        let path = self.root.join("thumbnails").join(format!("{hash}.png"));
        let gate = thumbnail_gate(&path)?;
        let _guard = gate.lock().map_err(storage)?;
        let (pending, wake) = pending_thumbnails();
        let mut p = pending.lock().map_err(storage)?;
        while p.contains(&path) {
            p = wake.wait(p).map_err(storage)?;
        }
        drop(p);
        if !path.exists() {
            let original = self.root.join("files").join(hash);
            let image = decode(&original)?;
            let small = image.thumbnail(320, 200);
            drop(image);
            let _encoding = Permit::acquire(320 * 200 * 4)?;
            let mut out = std::io::Cursor::new(Vec::new());
            small
                .write_to(&mut out, image::ImageFormat::Png)
                .map_err(storage)?;
            let bytes = out.into_inner();
            let mut temp =
                tempfile::NamedTempFile::new_in(self.root.join("thumbnails")).map_err(storage)?;
            temp.write_all(&bytes).map_err(storage)?;
            let _ = temp.persist_noclobber(&path);
            return Ok(bytes);
        }
        fs::read(path).map_err(storage)
    }
    pub fn pipeline_peak(&self) -> (usize, u64) {
        (
            PEAK_WORKERS.load(Ordering::Relaxed),
            PEAK_BYTES.load(Ordering::Relaxed),
        )
    }
    pub fn validation_count(&self) -> usize {
        DECODES.load(Ordering::Relaxed)
    }
    pub fn validated_texture(&self, hash: &str) -> Result<(), Error> {
        if !apex_core::valid_hash(hash) {
            return Err(Error::NotFound);
        }
        let known: bool = self
            .connection()?
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM image_validation WHERE hash=?1 AND validator=1)",
                [hash],
                |r| r.get(0),
            )
            .map_err(storage)?;
        if !known {
            let path = self.root.join("files").join(hash);
            let image = decode(&path)?;
            self.connection()?.execute("INSERT OR REPLACE INTO image_validation(hash,width,height,validator)VALUES(?1,?2,?3,1)",params![hash,image.width(),image.height()]).map_err(storage)?;
        }
        Ok(())
    }
}
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn prepared_handles_deduplicate_and_commit_without_decode_or_source_change() {
        let d = tempfile::tempdir().unwrap();
        let ws = Workspace::open(d.path().join("ws")).unwrap();
        let p = ws
            .save_project(
                None,
                apex_core::ProjectInput {
                    name: "Prepared".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let path = d.path().join("unique.png");
        image::RgbImage::from_pixel(5, 3, image::Rgb([9, 7, 5]))
            .save(&path)
            .unwrap();
        let original = fs::read(&path).unwrap();
        let a = ws.prepare_texture(&path).unwrap();
        let b = ws.prepare_texture(&path).unwrap();
        assert!(Arc::ptr_eq(&a.0, &b.0));
        let v = apex_core::vehicle_catalogue().unwrap().vehicles.remove(0);
        ws.import_prepared_livery(
            &p.id,
            None,
            None,
            "Livery",
            &v.id,
            &[(a.clone(), "Left1.png".into(), Some("Left 1".into()))],
            None,
        )
        .unwrap();
        assert_eq!(fs::read(path).unwrap(), original);
        assert_eq!(ws.decal_bytes(a.hash()).unwrap(), original);
        assert!(!ws.thumbnail_bytes(a.hash()).unwrap().is_empty());
        let other = Workspace::open(d.path().join("other")).unwrap();
        assert!(other.promote_texture(&a).is_err());
    }
    #[test]
    fn corrupt_png_never_produces_a_trusted_handle() {
        let d = tempfile::tempdir().unwrap();
        let ws = Workspace::open(d.path().join("ws")).unwrap();
        let path = d.path().join("corrupt.png");
        image::RgbImage::new(4, 4).save(&path).unwrap();
        let mut b = fs::read(&path).unwrap();
        b.truncate(35);
        fs::write(&path, b).unwrap();
        assert!(ws.prepare_texture(&path).is_err());
    }
}
