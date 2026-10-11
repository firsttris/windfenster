//! GRIB2 messages built by hand and small PMTiles archives, for the tests here and in `tests/`
//! and in the API's.

/// A PMTiles archive of MVT tiles stored uncompressed: (z, x, y, bytes).
pub fn archive(tiles: &[(u8, u32, u32, &[u8])]) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    let mut w = pmtiles::PmTilesWriter::new(pmtiles::TileType::Mvt)
        .tile_compression(pmtiles::Compression::None)
        .create(&mut out)
        .expect("writer");
    let mut sorted = tiles.to_vec();
    sorted.sort_by_key(|&(z, x, y, _)| {
        pmtiles::TileId::from(pmtiles::TileCoord::new(z, x, y).expect("tile")).value()
    });
    for (z, x, y, data) in sorted {
        w.add_raw_tile(pmtiles::TileCoord::new(z, x, y).expect("tile"), data)
            .expect("tile");
    }
    w.finalize().expect("archive");
    out.into_inner()
}

/// A GRIB2 message by hand: a regular grid (template 3.0), simple packing with 8 bits
/// (template 5.0) and a bitmap, laid out as WMO FM 92 describes the sections.
pub fn message(
    ni: u32,
    nj: u32,
    first: (i32, u32),
    last: (i32, u32),
    inc: u32,
    scan: u8,
    values: &[Option<u8>],
) -> Vec<u8> {
    let mut s1 = vec![0, 0, 0, 21, 1];
    s1.extend([0, 78, 0, 0, 2, 1, 1, 0x07, 0xea, 10, 11, 6, 0, 0, 0, 1]);
    let mut s3 = vec![0, 0, 0, 72, 3, 0];
    s3.extend((ni * nj).to_be_bytes());
    s3.extend([0, 0, 0, 0]); // no optional list, template 3.0
    s3.extend([6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]); // spherical earth
    s3.extend(ni.to_be_bytes());
    s3.extend(nj.to_be_bytes());
    s3.extend([0, 0, 0, 0, 0xff, 0xff, 0xff, 0xff]);
    let lat = |v: i32| {
        if v < 0 {
            (v.unsigned_abs() | 0x8000_0000).to_be_bytes()
        } else {
            (v as u32).to_be_bytes()
        }
    };
    s3.extend(lat(first.0));
    s3.extend(first.1.to_be_bytes());
    s3.push(0x30);
    s3.extend(lat(last.0));
    s3.extend(last.1.to_be_bytes());
    s3.extend(inc.to_be_bytes());
    s3.extend(inc.to_be_bytes());
    s3.push(scan);
    assert_eq!(s3.len(), 72);
    let mut s4 = vec![0, 0, 0, 34, 4, 0, 0, 0, 0];
    // template 4.0: u at 10 m (0.2.2, surface 103 at 10), 3 h ahead
    s4.extend([
        2, 2, 2, 0, 0, 0, 0, 0, 1, 0, 0, 0, 3, 103, 0, 0, 0, 0, 10, 255, 0, 0, 0, 0, 0,
    ]);
    assert_eq!(s4.len(), 34);
    let present: Vec<u8> = values.iter().flatten().copied().collect();
    let mut s5 = vec![0, 0, 0, 21, 5];
    s5.extend((present.len() as u32).to_be_bytes());
    s5.extend([0, 0]);
    s5.extend(0f32.to_be_bytes()); // R = 0
    s5.extend([0, 0, 0, 0, 8, 0]); // E = 0, D = 0, 8 bits, floating point
    let mut bitmap = vec![0u8; values.len().div_ceil(8)];
    for (k, v) in values.iter().enumerate() {
        if v.is_some() {
            bitmap[k / 8] |= 0x80 >> (k % 8);
        }
    }
    let mut s6 = ((6 + bitmap.len()) as u32).to_be_bytes().to_vec();
    s6.extend([6, 0]);
    s6.extend(bitmap);
    let mut s7 = ((5 + present.len()) as u32).to_be_bytes().to_vec();
    s7.push(7);
    s7.extend(&present);
    let body: Vec<u8> = [s1, s3, s4, s5, s6, s7].concat();
    let total = (16 + body.len() + 4) as u64;
    let mut out = b"GRIB".to_vec();
    out.extend([0, 0, 0, 2]);
    out.extend(total.to_be_bytes());
    out.extend(body);
    out.extend(b"7777");
    out
}

/// A vector tile of one layer, "places", with a point at each of `points` (tile units of 4096).
pub fn vector_tile(points: &[(u32, u32)]) -> Vec<u8> {
    crate::basemap::mvt::places(points)
}

/// The points of such a tile; None when it is no vector tile.
pub fn points_in(tile: &[u8]) -> Option<Vec<(f64, f64)>> {
    crate::basemap::mvt::places_in(tile)
}
