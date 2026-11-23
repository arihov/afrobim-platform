//! Fetching map tiles: worker threads, a disk cache and de-duplication.
//!
//! The view asks for the tiles it needs each frame with [`TileService::request`]; a tile already
//! queued or being fetched is not asked for twice. Requests are served newest first, so that after
//! a pan the tiles now on screen come before the ones that scrolled away, and [`TileService::retain`]
//! drops queued requests the view no longer needs. A tile is looked for in the disk cache first,
//! fetched only when it is missing or older than the cache's age limit, and written back
//! atomically (a temporary file renamed into place).
//!
//! The fetcher is a trait. The application uses the HTTP one (feature `http`); tests use a fake,
//! so no test touches the network.

use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use crate::tiles::TileKey;

/// Gets the bytes at a URL.
pub trait TileFetcher: Send + Sync {
    fn fetch(&self, url: &str) -> Result<Vec<u8>, String>;
}

/// Tiles on disk: `<root>/<provider>/<z>/<x>/<y>.tile`.
#[derive(Debug, Clone)]
pub struct DiskCache {
    pub root: PathBuf,
    pub max_age: Duration,
}

impl DiskCache {
    pub fn new(root: PathBuf) -> DiskCache {
        DiskCache { root, max_age: Duration::from_secs(30 * 24 * 3600) }
    }

    /// The platform's cache folder for AfroBIM tiles: `AFROBIM_TILE_CACHE` when set.
    pub fn default_root() -> Option<PathBuf> {
        if let Some(p) = std::env::var_os("AFROBIM_TILE_CACHE") {
            return Some(PathBuf::from(p));
        }
        let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
        let base = if cfg!(windows) {
            var("LOCALAPPDATA")?.join("AfroBIM")
        } else if cfg!(target_os = "macos") {
            var("HOME")?.join("Library").join("Caches").join("AfroBIM")
        } else {
            var("XDG_CACHE_HOME").or_else(|| var("HOME").map(|h| h.join(".cache")))?.join("afrobim")
        };
        Some(base.join("tiles"))
    }

    pub fn path(&self, k: &TileKey) -> PathBuf {
        // The provider id is ours or a hash of a custom template: never a path from outside.
        let safe: String = k.provider.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
        self.root.join(safe).join(k.z.to_string()).join(k.x.to_string()).join(format!("{}.tile", k.y))
    }

    /// The cached bytes when they are younger than the age limit.
    pub fn get(&self, k: &TileKey) -> Option<Vec<u8>> {
        let p = self.path(k);
        let age = std::fs::metadata(&p).ok()?.modified().ok()?.elapsed().unwrap_or_default();
        if age > self.max_age {
            return None;
        }
        std::fs::read(p).ok()
    }

    pub fn put(&self, k: &TileKey, bytes: &[u8]) -> std::io::Result<()> {
        let p = self.path(k);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp = p.with_extension(format!("tmp{}", std::process::id()));
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, &p)
    }
}

/// A decoded tile: width, height and RGBA bytes.
pub type TilePixels = (u32, u32, Vec<u8>);

/// What came back for one tile.
#[derive(Debug)]
pub struct TileResult {
    pub key: TileKey,
    pub pixels: Result<TilePixels, String>,
    /// True when it came from the disk cache.
    pub cached: bool,
}

struct Queue {
    items: VecDeque<(TileKey, String)>,
    shut: bool,
}

/// The tile workers.
pub struct TileService {
    queue: Arc<(Mutex<Queue>, Condvar)>,
    in_flight: Arc<Mutex<HashSet<TileKey>>>,
    rx: Receiver<TileResult>,
}

/// The most requests waiting; beyond it the oldest are dropped.
const QUEUE_CAP: usize = 256;

impl TileService {
    /// `workers` threads (2 to 4 keeps within the providers' fair-use terms).
    pub fn new(fetcher: Arc<dyn TileFetcher>, cache: Option<DiskCache>, workers: usize) -> TileService {
        let queue = Arc::new((Mutex::new(Queue { items: VecDeque::new(), shut: false }), Condvar::new()));
        let in_flight: Arc<Mutex<HashSet<TileKey>>> = Arc::new(Mutex::new(HashSet::new()));
        let (tx, rx) = channel();
        for _ in 0..workers.clamp(1, 8) {
            let (queue, in_flight, fetcher, cache, tx): (_, _, _, _, Sender<TileResult>) = (queue.clone(), in_flight.clone(), fetcher.clone(), cache.clone(), tx.clone());
            std::thread::spawn(move || loop {
                let job = {
                    let (m, cv) = &*queue;
                    let mut q = match m.lock() {
                        Ok(q) => q,
                        Err(_) => return,
                    };
                    while q.items.is_empty() && !q.shut {
                        q = match cv.wait(q) {
                            Ok(q) => q,
                            Err(_) => return,
                        };
                    }
                    if q.shut {
                        return;
                    }
                    // Newest first.
                    q.items.pop_back()
                };
                let Some((key, url)) = job else { continue };
                let res = fetch_one(&*fetcher, cache.as_ref(), &key, &url);
                if let Ok(mut f) = in_flight.lock() {
                    f.remove(&key);
                }
                if tx.send(res).is_err() {
                    return;
                }
            });
        }
        TileService { queue, in_flight, rx }
    }

    /// Asks for a tile. False when it is already queued or being fetched.
    pub fn request(&self, key: TileKey, url: String) -> bool {
        {
            let Ok(mut f) = self.in_flight.lock() else { return false };
            if !f.insert(key.clone()) {
                return false;
            }
        }
        let (m, cv) = &*self.queue;
        if let Ok(mut q) = m.lock() {
            q.items.push_back((key, url));
            while q.items.len() > QUEUE_CAP {
                if let Some((old, _)) = q.items.pop_front() {
                    if let Ok(mut f) = self.in_flight.lock() {
                        f.remove(&old);
                    }
                }
            }
        }
        cv.notify_one();
        true
    }

    /// Drops queued requests (not those being fetched) that `keep` rejects.
    pub fn retain(&self, keep: impl Fn(&TileKey) -> bool) {
        let (m, _) = &*self.queue;
        if let Ok(mut q) = m.lock() {
            let mut dropped = Vec::new();
            q.items.retain(|(k, _)| {
                let ok = keep(k);
                if !ok {
                    dropped.push(k.clone());
                }
                ok
            });
            if let Ok(mut f) = self.in_flight.lock() {
                for k in dropped {
                    f.remove(&k);
                }
            }
        }
    }

    /// What has finished since the last call.
    pub fn poll(&self) -> Vec<TileResult> {
        self.rx.try_iter().collect()
    }

    /// Tiles queued or being fetched.
    pub fn pending(&self) -> usize {
        self.in_flight.lock().map(|f| f.len()).unwrap_or(0)
    }
}

impl Drop for TileService {
    fn drop(&mut self) {
        let (m, cv) = &*self.queue;
        if let Ok(mut q) = m.lock() {
            q.shut = true;
        }
        cv.notify_all();
    }
}

/// The most bytes a tile may have, and the largest side, so that a bad server cannot exhaust memory.
pub const MAX_TILE_BYTES: usize = 8 * 1024 * 1024;
const MAX_TILE_SIDE: u32 = 1024;

fn decode_tile(bytes: &[u8]) -> Result<TilePixels, String> {
    let img = image::load_from_memory(bytes).map_err(|e| format!("not an image: {e}"))?;
    if img.width() > MAX_TILE_SIDE || img.height() > MAX_TILE_SIDE || img.width() == 0 || img.height() == 0 {
        return Err(format!("a tile of {} x {} pixels is not accepted", img.width(), img.height()));
    }
    let rgba = img.to_rgba8();
    Ok((rgba.width(), rgba.height(), rgba.into_raw()))
}

fn fetch_one(fetcher: &dyn TileFetcher, cache: Option<&DiskCache>, key: &TileKey, url: &str) -> TileResult {
    if let Some(bytes) = cache.and_then(|c| c.get(key)) {
        if let Ok(px) = decode_tile(&bytes) {
            return TileResult { key: key.clone(), pixels: Ok(px), cached: true };
        }
    }
    let pixels = fetcher.fetch(url).and_then(|bytes| {
        if bytes.len() > MAX_TILE_BYTES {
            return Err("the tile is larger than 8 MB".into());
        }
        let px = decode_tile(&bytes)?;
        if let Some(c) = cache {
            let _ = c.put(key, &bytes);
        }
        Ok(px)
    });
    TileResult { key: key.clone(), pixels, cached: false }
}

/// The HTTP fetcher: HTTPS, a generic user agent, timeouts, a size limit.
#[cfg(feature = "http")]
pub struct HttpFetcher {
    agent: ureq::Agent,
}

#[cfg(feature = "http")]
impl HttpFetcher {
    pub fn new() -> HttpFetcher {
        let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(10)).timeout(Duration::from_secs(25)).user_agent(concat!("AfroBIM/", env!("CARGO_PKG_VERSION"), " (desktop road design)")).https_only(true).build();
        HttpFetcher { agent }
    }
}

#[cfg(feature = "http")]
impl Default for HttpFetcher {
    fn default() -> Self {
        HttpFetcher::new()
    }
}

#[cfg(feature = "http")]
impl TileFetcher for HttpFetcher {
    fn fetch(&self, url: &str) -> Result<Vec<u8>, String> {
        use std::io::Read;
        let resp = self.agent.get(url).call().map_err(|e| match e {
            ureq::Error::Status(code, _) => format!("the server answered {code}"),
            other => other.kind().to_string(),
        })?;
        let mut buf = Vec::new();
        resp.into_reader().take(MAX_TILE_BYTES as u64 + 1).read_to_end(&mut buf).map_err(|e| e.to_string())?;
        Ok(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Fake {
        calls: AtomicUsize,
        fail: bool,
    }

    impl TileFetcher for Fake {
        fn fetch(&self, url: &str) -> Result<Vec<u8>, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(5));
            if self.fail || url.contains("bad") {
                return Err("the server answered 404".into());
            }
            let mut out = Vec::new();
            image::RgbaImage::from_pixel(4, 4, image::Rgba([1, 2, 3, 255])).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
            Ok(out)
        }
    }

    fn collect(s: &TileService, n: usize) -> Vec<TileResult> {
        let mut out = Vec::new();
        for _ in 0..2000 {
            out.extend(s.poll());
            if out.len() >= n && s.pending() == 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        out
    }

    #[test]
    fn a_tile_is_fetched_once_then_served_from_the_disk_cache() {
        let root = std::env::temp_dir().join(format!("afrobim-tiles-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let fake = Arc::new(Fake { calls: AtomicUsize::new(0), fail: false });
        let s = TileService::new(fake.clone(), Some(DiskCache::new(root.clone())), 2);
        let k = TileKey::new("osm", 3, 4, 5);
        assert!(s.request(k.clone(), "https://x/3/4/5".into()));
        // Asked again while in flight: not queued twice.
        assert!(!s.request(k.clone(), "https://x/3/4/5".into()));
        let r = collect(&s, 1);
        assert_eq!(r.len(), 1);
        let (w, h, px) = r[0].pixels.as_ref().unwrap();
        assert_eq!((*w, *h, &px[0..4]), (4, 4, &[1u8, 2, 3, 255][..]));
        assert!(!r[0].cached);
        assert!(root.join("osm/3/4/5.tile").is_file());
        // A second service finds it on disk and does not fetch.
        let s2 = TileService::new(fake.clone(), Some(DiskCache::new(root.clone())), 1);
        s2.request(k.clone(), "https://x/3/4/5".into());
        let r2 = collect(&s2, 1);
        assert!(r2[0].cached);
        assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn failures_come_back_as_errors_and_can_be_asked_for_again() {
        let fake = Arc::new(Fake { calls: AtomicUsize::new(0), fail: true });
        let s = TileService::new(fake.clone(), None, 1);
        let k = TileKey::new("osm", 1, 0, 0);
        s.request(k.clone(), "https://x".into());
        let r = collect(&s, 1);
        assert!(r[0].pixels.as_ref().unwrap_err().contains("404"));
        assert!(s.request(k, "https://x".into()));
    }

    #[test]
    fn queued_requests_the_view_no_longer_needs_are_dropped() {
        let fake = Arc::new(Fake { calls: AtomicUsize::new(0), fail: false });
        let s = TileService::new(fake.clone(), None, 1);
        for x in 0..40 {
            s.request(TileKey::new("osm", 10, x, 0), format!("https://x/{x}"));
        }
        s.retain(|k| k.x >= 38);
        let r = collect(&s, 2);
        // At most the one being fetched when retain ran, plus the two kept.
        assert!(r.len() <= 3, "{}", r.len());
        assert!(r.iter().any(|t| t.key.x == 39));
        assert_eq!(s.pending(), 0);
    }

    #[test]
    fn the_cache_path_never_leaves_its_root() {
        let c = DiskCache::new(PathBuf::from("/c"));
        assert_eq!(c.path(&TileKey::new("../../etc", 1, 2, 3)), PathBuf::from("/c/______etc/1/2/3.tile"));
    }

    #[test]
    fn a_tile_that_is_not_an_image_or_is_too_large_is_refused() {
        assert!(decode_tile(b"<html>rate limited</html>").is_err());
        let mut big = Vec::new();
        image::RgbaImage::new(2048, 4).write_to(&mut std::io::Cursor::new(&mut big), image::ImageFormat::Png).unwrap();
        assert!(decode_tile(&big).is_err());
    }
}
