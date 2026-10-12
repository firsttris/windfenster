//! A wind field as a PNG texture: red is U (west→east), green is V (south→north), each scaled
//! from its minimum to its maximum, in knots; blue is the gust from 0 to its maximum where the
//! model gives one; alpha is 0 where the model has no value. Row 0 is the northern edge, as
//! images go.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use wf_core::units::MS_PER_KN;

use crate::grid::LatLonGrid;

#[derive(Debug, thiserror::Error)]
pub enum TextureError {
    #[error("U and V are on different grids")]
    Grids,
    #[error("PNG: {0}")]
    Png(#[from] png::EncodingError),
    #[error("the piece holds no texels")]
    Outside,
    #[error("reading texels: {0}")]
    Io(#[from] std::io::Error),
}

/// What the JSON next to a texture says: where it lies and how to read its colours.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TextureMeta {
    pub model: String,
    #[ts(type = "number")]
    pub run_at: i64,
    /// Hours after the run
    pub step: u32,
    #[ts(type = "number")]
    pub valid_at: i64,
    /// Outer edges of the texture in degrees: texel centres sit on the grid points
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
    pub width: u32,
    pub height: u32,
    /// The model's domain, (lon, lat) points of a ring, for the dashed outline
    pub domain: Vec<[f64; 2]>,
    /// kn
    pub u_min: f32,
    pub u_max: f32,
    pub v_min: f32,
    pub v_max: f32,
    /// kn at blue 255; null when the hour has no gusts (blue is 0 then)
    #[serde(default)]
    pub gust_max: Option<f32>,
}

impl TextureMeta {
    /// Whether the texture goes once around the world (its columns cover 360°), so it wraps
    /// at its west and east edges.
    pub fn wraps(&self) -> bool {
        (self.east - self.west - 360.0).abs() < 1e-6
    }
}

fn range(values: &[f32]) -> (f32, f32) {
    let (lo, hi) = values
        .iter()
        .filter(|v| v.is_finite())
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), v| {
            (lo.min(*v), hi.max(*v))
        });
    if lo > hi { (0.0, 0.0) } else { (lo, hi) }
}

fn byte(v: f32, (lo, hi): (f32, f32)) -> u8 {
    if hi <= lo {
        return 0;
    }
    (((v - lo) / (hi - lo)) * 255.0).round().clamp(0.0, 255.0) as u8
}

fn round3(x: f64) -> f64 {
    (x * 1e3).round() / 1e3
}

/// Encodes U and V (m/s, as models deliver them) into the texture (PNG) and its description.
pub fn encode(
    u: &LatLonGrid,
    v: &LatLonGrid,
    gust: Option<&LatLonGrid>,
    model: &str,
    run_at: i64,
    step: u32,
) -> Result<(Vec<u8>, TextureMeta), TextureError> {
    let (rgba, meta) = pixels(u, v, gust, model, run_at, step)?;
    Ok((png(&rgba, meta.width, meta.height)?, meta))
}

/// The texture's RGBA bytes, north row first, and its description.
pub fn pixels(
    u: &LatLonGrid,
    v: &LatLonGrid,
    gust: Option<&LatLonGrid>,
    model: &str,
    run_at: i64,
    step: u32,
) -> Result<(Vec<u8>, TextureMeta), TextureError> {
    if !u.same_points(v) || gust.is_some_and(|g| !u.same_points(g)) {
        return Err(TextureError::Grids);
    }
    let to_kn = |x: f32| x / MS_PER_KN as f32;
    let uk: Vec<f32> = u.values.iter().map(|x| to_kn(*x)).collect();
    let vk: Vec<f32> = v.values.iter().map(|x| to_kn(*x)).collect();
    let (ur, vr) = (range(&uk), range(&vk));
    let gk: Option<Vec<f32>> = gust.map(|g| g.values.iter().map(|x| to_kn(*x)).collect());
    let gmax = gk.as_deref().map(|g| range(g).1).filter(|m| *m > 0.0);
    let (w, h) = (u.nx, u.ny);
    let mut rgba = vec![0u8; w * h * 4];
    for row in 0..h {
        let iy = h - 1 - row; // north first
        for ix in 0..w {
            let k = iy * w + ix;
            if uk[k].is_finite() && vk[k].is_finite() {
                let o = (row * w + ix) * 4;
                rgba[o] = byte(uk[k], ur);
                rgba[o + 1] = byte(vk[k], vr);
                if let (Some(g), Some(max)) = (&gk, gmax)
                    && g[k].is_finite()
                {
                    rgba[o + 2] = byte(g[k], (0.0, max));
                }
                rgba[o + 3] = 255;
            }
        }
    }
    let meta = TextureMeta {
        model: model.to_owned(),
        run_at,
        step,
        valid_at: run_at + i64::from(step) * 3_600,
        west: u.lon0 - u.dlon / 2.0,
        south: u.lat0 - u.dlat / 2.0,
        east: u.lon0 + u.dlon * (w as f64 - 0.5),
        north: u.lat0 + u.dlat * (h as f64 - 0.5),
        width: w as u32,
        height: h as u32,
        // About 60 points; a thousandth of a degree is 100 m.
        domain: u
            .outline(h / 30)
            .into_iter()
            .map(|(lon, lat)| [round3(lon), round3(lat)])
            .collect(),
        u_min: ur.0,
        u_max: ur.1,
        v_min: vr.0,
        v_max: vr.1,
        gust_max: gmax,
    };
    Ok((rgba, meta))
}

/// RGBA bytes as a PNG.
pub fn png(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, TextureError> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, width, height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    // Balanced: 0.3 s and 480 KB for an ICON-D2 hour; High saves 5 % in 2.5 times the time.
    enc.set_compression(png::Compression::Balanced);
    enc.write_header()?.write_image_data(rgba)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(values: Vec<f32>) -> LatLonGrid {
        LatLonGrid {
            lon0: 10.0,
            lat0: 50.0,
            dlon: 0.5,
            dlat: 0.5,
            nx: 2,
            ny: 2,
            values,
        }
    }

    #[test]
    fn encodes_north_up_in_knots() {
        // Rows south to north: (10 m/s, 0) in the south-west, missing in the north-east
        let u = grid(vec![10.0, 0.0, -10.0, f32::NAN]);
        let v = grid(vec![0.0, 5.0, 5.0, 5.0]);
        let g = grid(vec![20.0, 10.0, 0.0, f32::NAN]);
        let (bytes, meta) = encode(&u, &v, Some(&g), "icon_d2", 1_791_698_400, 3).unwrap();
        assert_eq!(
            (meta.width, meta.height, meta.valid_at),
            (2, 2, 1_791_698_400 + 3 * 3_600)
        );
        assert_eq!(
            (meta.west, meta.south, meta.east, meta.north),
            (9.75, 49.75, 10.75, 50.75)
        );
        assert!((meta.u_max - 19.44).abs() < 0.01 && (meta.u_min + 19.44).abs() < 0.01);
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut buf).unwrap();
        // Top row is the north: (−10 kn·…, 5) and the missing point
        assert_eq!(&buf[0..4], &[0, 255, 0, 255]);
        assert_eq!(buf[7], 0);
        // Bottom-left: U at the maximum, V at the minimum, the strongest gust
        assert_eq!(&buf[8..12], &[255, 0, 255, 255]);
        assert!((meta.gust_max.unwrap() - 38.88).abs() < 0.01);
        // No gusts: blue stays 0 and the description says so.
        let (bytes, meta) = encode(&u, &v, None, "icon_d2", 1_791_698_400, 3).unwrap();
        assert_eq!(meta.gust_max, None);
        let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
            .read_info()
            .unwrap();
        reader.next_frame(&mut buf).unwrap();
        assert_eq!(&buf[8..12], &[255, 0, 0, 255]);
    }
}
