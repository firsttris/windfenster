//! Textures on disk: `{root}/{model}/{YYYYMMDDHH}/{step:03}.png`, the description as `.json`
//! and the texels uncompressed as `.rgba` (four bytes each, for reading one place over all
//! hours without decoding the PNGs; `crate::series`).

use std::path::{Path, PathBuf};

use serde::Serialize;
use ts_rs::TS;

use crate::texture::TextureMeta;

/// One run of a model with the hours that are ready.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MapRun {
    /// The run's folder name, YYYYMMDDHH (UTC), as the texture paths use it
    pub id: String,
    #[ts(type = "number")]
    pub run_at: i64,
    /// Hours after the run with a texture, ascending
    pub steps: Vec<u32>,
}

/// YYYYMMDDHH of a Unix time
pub fn run_id(run_at: i64) -> String {
    format!(
        "{}{:02}",
        wf_core::time::yyyymmdd(run_at),
        run_at.rem_euclid(86_400) / 3_600
    )
}

fn parse_run_id(id: &str) -> Option<i64> {
    if id.len() != 10 || !id.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n = |r: std::ops::Range<usize>| id[r].parse::<u32>().ok();
    Some(wf_core::time::unix(
        i64::from(n(0..4)?),
        n(4..6)?,
        n(6..8)?,
        n(8..10)?,
        0,
        0,
    ))
}

pub fn run_dir(root: &Path, model: &str, run_at: i64) -> PathBuf {
    root.join(model).join(run_id(run_at))
}

pub async fn write_step(
    root: &Path,
    model: &str,
    png: &[u8],
    rgba: &[u8],
    meta: &TextureMeta,
) -> std::io::Result<()> {
    let dir = run_dir(root, model, meta.run_at);
    tokio::fs::create_dir_all(&dir).await?;
    let name = format!("{:03}", meta.step);
    // The JSON last: a step counts once its description is there.
    tokio::fs::write(dir.join(format!("{name}.png")), png).await?;
    tokio::fs::write(dir.join(format!("{name}.rgba")), rgba).await?;
    let json = serde_json::to_vec(meta).map_err(std::io::Error::other)?;
    tokio::fs::write(dir.join(format!("{name}.json")), json).await
}

/// The runs of a model on disk, newest first.
pub async fn runs(root: &Path, model: &str) -> std::io::Result<Vec<MapRun>> {
    let mut out = Vec::new();
    let mut dirs = match tokio::fs::read_dir(root.join(model)).await {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(e),
    };
    while let Some(entry) = dirs.next_entry().await? {
        let id = entry.file_name().to_string_lossy().into_owned();
        let Some(run_at) = parse_run_id(&id) else {
            continue;
        };
        let mut steps = Vec::new();
        let mut files = tokio::fs::read_dir(entry.path()).await?;
        while let Some(f) = files.next_entry().await? {
            let name = f.file_name().to_string_lossy().into_owned();
            if let Some(step) = name
                .strip_suffix(".json")
                .and_then(|s| s.parse::<u32>().ok())
            {
                steps.push(step);
            }
        }
        steps.sort_unstable();
        out.push(MapRun { id, run_at, steps });
    }
    out.sort_by_key(|r| std::cmp::Reverse(r.run_at));
    Ok(out)
}

/// Keeps the newest `keep` runs of a model; returns how many were removed.
pub async fn prune(root: &Path, model: &str, keep: usize) -> std::io::Result<usize> {
    let old: Vec<MapRun> = runs(root, model).await?.into_iter().skip(keep).collect();
    for r in &old {
        tokio::fs::remove_dir_all(run_dir(root, model, r.run_at)).await?;
    }
    Ok(old.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(run_at: i64, step: u32) -> TextureMeta {
        TextureMeta {
            model: "icon_d2".into(),
            run_at,
            step,
            valid_at: run_at + i64::from(step) * 3_600,
            west: 0.0,
            south: 0.0,
            east: 1.0,
            north: 1.0,
            width: 1,
            height: 1,
            domain: vec![],
            u_min: 0.0,
            u_max: 0.0,
            v_min: 0.0,
            v_max: 0.0,
            gust_max: None,
        }
    }

    #[tokio::test]
    async fn writes_lists_and_prunes() {
        let dir = tempfile::tempdir().unwrap();
        // 2026-10-11 06 and 09 UTC
        let (r06, r09) = (1_791_698_400, 1_791_709_200);
        assert_eq!(run_id(r06), "2026101106");
        assert_eq!(parse_run_id("2026101106"), Some(r06));
        for (run, step) in [(r06, 0), (r06, 1), (r09, 2), (r09, 0)] {
            write_step(dir.path(), "icon_d2", b"png", b"rgba", &meta(run, step))
                .await
                .unwrap();
        }
        let list = runs(dir.path(), "icon_d2").await.unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(
            (list[0].id.as_str(), list[0].steps.clone()),
            ("2026101109", vec![0, 2])
        );
        assert!(runs(dir.path(), "none").await.unwrap().is_empty());
        assert_eq!(prune(dir.path(), "icon_d2", 1).await.unwrap(), 1);
        assert_eq!(runs(dir.path(), "icon_d2").await.unwrap().len(), 1);
    }
}
