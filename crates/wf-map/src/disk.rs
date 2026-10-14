//! What the data folder takes on the disk, by part, and how much is free: for the admin page.

use std::path::Path;

use serde::Serialize;
use ts_rs::TS;

/// Bytes per part of the data folder.
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DiskUsage {
    /// Free for the server on the file system of the data folder
    #[ts(type = "number")]
    pub free: u64,
    /// The wind map's runs per model (model id, bytes)
    #[ts(type = "Array<[string, number]>")]
    pub models: Vec<(String, u64)>,
    #[ts(type = "number")]
    pub basemap: u64,
    #[ts(type = "number")]
    pub terrain: u64,
    #[ts(type = "number")]
    pub spots: u64,
    #[ts(type = "number")]
    pub spot_files: u64,
    /// The database with its write-ahead log
    #[ts(type = "number")]
    pub database: u64,
    /// Unfinished cuts and downloads (`.part`), what a restart left
    #[ts(type = "number")]
    pub partial: u64,
    /// Everything else: the ICON global grid table, the push key ...
    #[ts(type = "number")]
    pub other: u64,
}

impl DiskUsage {
    pub fn used(&self) -> u64 {
        self.models.iter().map(|(_, b)| b).sum::<u64>()
            + self.basemap
            + self.terrain
            + self.spots
            + self.database
            + self.partial
            + self.other
    }
}

/// Walks the data folder once; a few thousand files, well under a second.
pub async fn usage(data_dir: &Path) -> DiskUsage {
    let data_dir = data_dir.to_path_buf();
    tokio::task::spawn_blocking(move || usage_now(&data_dir))
        .await
        .unwrap_or_default()
}

fn usage_now(data_dir: &Path) -> DiskUsage {
    let map = data_dir.join("map");
    let mut u = DiskUsage {
        free: crate::basemap::download::progress::free_bytes(data_dir).unwrap_or(0),
        ..DiskUsage::default()
    };
    for model in crate::MODELS {
        u.models.push(((*model).to_owned(), size(&map.join(model))));
    }
    u.basemap = size(&crate::basemap::path(&map));
    u.terrain = size(&crate::basemap::terrain::path(&map));
    let spots = crate::basemap::spots::dir(&map);
    for (file, len) in files(&spots) {
        if file.extension().is_some_and(|x| x == "pmtiles") {
            u.spots += len;
            u.spot_files += 1;
        }
    }
    for (file, len) in files(data_dir) {
        let name = file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        let in_model = crate::MODELS.iter().any(|m| file.starts_with(map.join(m)));
        if name.starts_with("windfenster.db") {
            u.database += len;
        } else if name.ends_with(".part") || name.ends_with(".part.json") {
            u.partial += len;
        } else if !in_model
            && file != crate::basemap::path(&map)
            && file != crate::basemap::terrain::path(&map)
            && !(file.starts_with(&spots) && name.ends_with(".pmtiles"))
        {
            u.other += len;
        }
    }
    u
}

/// Bytes of a file, or of all files under a folder
fn size(path: &Path) -> u64 {
    match std::fs::metadata(path) {
        Ok(m) if m.is_file() => m.len(),
        Ok(_) => files(path).iter().map(|(_, len)| len).sum(),
        Err(_) => 0,
    }
}

/// Every file under a folder with its length
fn files(dir: &Path) -> Vec<(std::path::PathBuf, u64)> {
    let mut out = Vec::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let Ok(meta) = e.metadata() else { continue };
            if meta.is_dir() {
                todo.push(e.path());
            } else {
                out.push((e.path(), meta.len()));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sizes_the_parts() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let map = root.join("map");
        std::fs::create_dir_all(map.join("icon_d2/2026101412")).unwrap();
        std::fs::write(map.join("icon_d2/2026101412/000.rgba"), vec![0; 100]).unwrap();
        std::fs::write(crate::basemap::path(&map), vec![0; 50]).unwrap();
        std::fs::create_dir_all(crate::basemap::spots::dir(&map)).unwrap();
        std::fs::write(
            crate::basemap::spots::dir(&map).join("48.8_8.0.pmtiles"),
            vec![0; 7],
        )
        .unwrap();
        std::fs::write(
            crate::basemap::spots::dir(&map).join("1.0_1.0.part"),
            vec![0; 3],
        )
        .unwrap();
        std::fs::write(root.join("windfenster.db"), vec![0; 20]).unwrap();
        std::fs::write(root.join("windfenster.db-wal"), vec![0; 5]).unwrap();
        std::fs::write(root.join("vapid.pem"), vec![0; 2]).unwrap();
        let u = usage(root).await;
        assert_eq!(
            u.models.iter().find(|(m, _)| m == "icon_d2").unwrap().1,
            100
        );
        assert_eq!((u.basemap, u.spots, u.spot_files), (50, 7, 1));
        assert_eq!((u.database, u.partial, u.other), (25, 3, 2));
        assert_eq!(u.used(), 187);
        assert!(u.free > 0);
    }
}
