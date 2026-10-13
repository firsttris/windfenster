//! The PMTiles directories a reader has looked at, kept up to a size. The pmtiles crate's own
//! cache keeps every directory for good: cutting the world deep from the planet archive reads
//! millions of leaf directories, gigabytes in all. Tiles are read in id order, so only the few
//! directories around the current ids are needed again; the one used longest ago goes first.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use pmtiles::{DirEntry, Directory, DirectoryCache, PmtResult, TileId};

/// Bytes of directories kept per reader at most
const MAX_BYTES: usize = 64 << 20;

/// A directory's bytes, at least one, so that even empty ones count
fn weight(d: &Directory) -> usize {
    d.get_approx_byte_size().max(1)
}

#[derive(Default)]
struct Kept {
    /// Directory by its offset in the archive, with when it was last used
    dirs: HashMap<usize, (Arc<Directory>, u64)>,
    bytes: usize,
    clock: u64,
}

pub struct Bounded {
    kept: Mutex<Kept>,
    max_bytes: usize,
}

impl Bounded {
    pub fn new() -> Self {
        Self::with_capacity(MAX_BYTES)
    }

    fn with_capacity(max_bytes: usize) -> Self {
        Bounded {
            kept: Mutex::default(),
            max_bytes,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Kept> {
        self.kept.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn get(&self, offset: usize) -> Option<Arc<Directory>> {
        let mut k = self.lock();
        k.clock += 1;
        let now = k.clock;
        k.dirs.get_mut(&offset).map(|(d, used)| {
            *used = now;
            d.clone()
        })
    }

    fn put(&self, offset: usize, dir: Arc<Directory>) {
        let mut k = self.lock();
        k.clock += 1;
        let (now, size) = (k.clock, weight(&dir));
        if let Some((old, _)) = k.dirs.insert(offset, (dir, now)) {
            k.bytes -= weight(&old);
        }
        k.bytes += size;
        // A few hundred directories at most: finding the oldest by looking at all is cheap.
        while k.bytes > self.max_bytes && k.dirs.len() > 1 {
            let Some(oldest) = k
                .dirs
                .iter()
                .min_by_key(|(_, (_, used))| *used)
                .map(|(o, _)| *o)
            else {
                break;
            };
            if let Some((d, _)) = k.dirs.remove(&oldest) {
                k.bytes -= weight(&d);
            }
        }
    }
}

impl Default for Bounded {
    fn default() -> Self {
        Self::new()
    }
}

impl DirectoryCache for Bounded {
    async fn get_dir_entry_or_insert(
        &self,
        offset: usize,
        tile_id: TileId,
        fetcher: impl Future<Output = PmtResult<Directory>> + Send,
    ) -> PmtResult<Option<DirEntry>> {
        let dir = match self.get(offset) {
            Some(d) => d,
            None => {
                let d = Arc::new(fetcher.await?);
                self.put(offset, d.clone());
                d
            }
        };
        Ok(dir.find_tile_id(tile_id).cloned())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[tokio::test]
    async fn keeps_the_directories_last_used_up_to_its_size() {
        let size = weight(&Directory::default());
        let cache = Bounded::with_capacity(3 * size);
        let id = TileId::new(0).unwrap();
        let fetched = AtomicUsize::new(0);
        let get = async |offset| {
            let fetch = async {
                fetched.fetch_add(1, Ordering::SeqCst);
                Ok(Directory::default())
            };
            cache
                .get_dir_entry_or_insert(offset, id, fetch)
                .await
                .unwrap();
        };
        // The same directory is fetched once
        for _ in 0..3 {
            get(0).await;
        }
        assert_eq!(fetched.load(Ordering::SeqCst), 1);
        // 1 and 2 join; 0 is used again, so 3 pushes out 1, the one used longest ago
        get(1).await;
        get(2).await;
        get(0).await;
        get(3).await;
        let kept = |o| cache.lock().dirs.contains_key(&o);
        assert!(kept(0) && !kept(1) && kept(2) && kept(3));
        // Many more: the cache stays within its size
        for offset in 10..10_000 {
            get(offset).await;
        }
        assert!(cache.lock().bytes <= 3 * size);
        assert!(cache.lock().dirs.len() <= 3);
    }
}
