//! Pieces of a world texture: ICON global at 0.125° is 2880 × 1441 texels, 4 to 5 MB a PNG,
//! too much to send per hour to a phone. The app asks for the piece around what it shows instead,
//! its edges on whole 10° so that pieces repeat and cache; the server cuts it from the hour's
//! texels (`{step}.rgba`, north row first) and encodes it.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::texture::{self, TextureError, TextureMeta};

/// Edges are multiples of this, degrees
pub const SNAP_DEG: i32 = 10;
/// A piece is at most this wide and high, degrees (800 × 640 texels at 0.125°)
pub const MAX_W: i32 = 100;
pub const MAX_H: i32 = 80;

/// A piece: west, south, east, north in whole degrees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Piece {
    pub west: i32,
    pub south: i32,
    pub east: i32,
    pub north: i32,
}

impl Piece {
    /// `{west}_{south}_{east}_{north}`, as the app names it; None unless snapped, in the world
    /// (−180…180, −80…80) and not too large.
    pub fn parse(s: &str) -> Option<Self> {
        let mut it = s.split('_').map(|p| p.parse::<i32>().ok());
        let (west, south, east, north) = (it.next()??, it.next()??, it.next()??, it.next()??);
        let p = Piece {
            west,
            south,
            east,
            north,
        };
        let snapped = [west, south, east, north].iter().all(|v| v % SNAP_DEG == 0);
        let inside = -180 <= west && east <= 180 && -80 <= south && north <= 80;
        let size = east > west && north > south && east - west <= MAX_W && north - south <= MAX_H;
        (it.next().is_none() && snapped && inside && size).then_some(p)
    }
}

/// The piece of an hour's world texture: its PNG and description. The texel columns whose
/// centres lie in [west, east), the rows whose centres lie in (south, north].
pub fn cut(
    rgba_path: &Path,
    world: &TextureMeta,
    p: Piece,
) -> Result<(Vec<u8>, TextureMeta), TextureError> {
    let (w, h) = (world.width as usize, world.height as usize);
    let dlon = (world.east - world.west) / w as f64;
    let dlat = (world.north - world.south) / h as f64;
    // Centres: lon(i) = west + (i + ½)·dlon, lat(r) = north − (r + ½)·dlat
    let col = |lon: f64| (((lon - world.west) / dlon - 0.5).ceil().max(0.0) as usize).min(w);
    let row = |lat: f64| (((world.north - lat) / dlat - 0.5).ceil().max(0.0) as usize).min(h);
    let (i0, i1) = (col(f64::from(p.west)), col(f64::from(p.east)));
    let (r0, r1) = (row(f64::from(p.north)), row(f64::from(p.south)));
    if i1 <= i0 || r1 <= r0 {
        return Err(TextureError::Outside);
    }
    let (cw, ch) = (i1 - i0, r1 - r0);
    let mut file = std::fs::File::open(rgba_path)?;
    let mut out = vec![0u8; cw * ch * 4];
    for (k, r) in (r0..r1).enumerate() {
        file.seek(SeekFrom::Start(((r * w + i0) * 4) as u64))?;
        file.read_exact(&mut out[k * cw * 4..(k + 1) * cw * 4])?;
    }
    let meta = TextureMeta {
        west: world.west + i0 as f64 * dlon,
        east: world.west + i1 as f64 * dlon,
        north: world.north - r0 as f64 * dlat,
        south: world.north - r1 as f64 * dlat,
        width: cw as u32,
        height: ch as u32,
        domain: vec![],
        ..world.clone()
    };
    Ok((texture::png(&out, meta.width, meta.height)?, meta))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_only_snapped_pieces_in_the_world() {
        assert_eq!(
            Piece::parse("-10_30_40_60"),
            Some(Piece {
                west: -10,
                south: 30,
                east: 40,
                north: 60
            })
        );
        for bad in [
            "-10_30_40",
            "-15_30_40_60",
            "170_30_190_60",
            "-10_30_100_60",
            "0_-90_10_0",
            "x_1_2_3",
            "0_0_10_10_0",
        ] {
            assert_eq!(Piece::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn cuts_the_texels_of_a_piece() {
        // A world of 1° texels, 360 × 180, each texel's red its column and green its row
        let (w, h) = (360usize, 180usize);
        let rgba: Vec<u8> = (0..h)
            .flat_map(|r| (0..w).flat_map(move |i| [(i % 256) as u8, r as u8, 0, 255]))
            .collect();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("003.rgba");
        std::fs::write(&path, &rgba).unwrap();
        let world = TextureMeta {
            model: "icon_global".into(),
            run_at: 0,
            step: 3,
            valid_at: 10_800,
            west: -180.0,
            south: -90.0,
            east: 180.0,
            north: 90.0,
            width: w as u32,
            height: h as u32,
            domain: vec![],
            u_min: -10.0,
            u_max: 10.0,
            v_min: -10.0,
            v_max: 10.0,
            gust_max: None,
        };
        let (png, meta) = cut(
            &path,
            &world,
            Piece {
                west: 0,
                south: 40,
                east: 10,
                north: 50,
            },
        )
        .unwrap();
        assert_eq!(
            (meta.west, meta.east, meta.south, meta.north),
            (0.0, 10.0, 40.0, 50.0)
        );
        assert_eq!((meta.width, meta.height, meta.u_max), (10, 10, 10.0));
        let decoder = png::Decoder::new(std::io::Cursor::new(png));
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut buf).unwrap();
        // First texel: column 180 (0°E), row 40 (50°N to 49°N)
        assert_eq!(&buf[..2], &[180u8, 40]);
    }
}
