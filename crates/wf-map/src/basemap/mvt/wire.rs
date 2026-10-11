//! The protobuf of a vector tile: layers, features and their geometry commands read into
//! paths and written back (MVT 2.1, sections 4.1–4.3).

use super::{Error, Feature, Kind, Layer, Path};

struct Reader<'a> {
    buf: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Reader { buf, at: 0 }
    }

    fn done(&self) -> bool {
        self.at >= self.buf.len()
    }

    fn varint(&mut self) -> Result<u64, Error> {
        let mut v = 0u64;
        for shift in (0..64).step_by(7) {
            let b = *self.buf.get(self.at).ok_or(Error("varint cut short"))?;
            self.at += 1;
            v |= u64::from(b & 0x7f) << shift;
            if b < 0x80 {
                return Ok(v);
            }
        }
        Err(Error("varint too long"))
    }

    fn bytes(&mut self) -> Result<&'a [u8], Error> {
        let n = usize::try_from(self.varint()?).map_err(|_| Error("length"))?;
        let end = self.at.checked_add(n).ok_or(Error("length"))?;
        let b = self.buf.get(self.at..end).ok_or(Error("field cut short"))?;
        self.at = end;
        Ok(b)
    }

    /// A field's number and wire type
    fn key(&mut self) -> Result<(u64, u64), Error> {
        let k = self.varint()?;
        Ok((k >> 3, k & 7))
    }

    fn skip(&mut self, wire: u64) -> Result<(), Error> {
        match wire {
            0 => self.varint().map(drop),
            1 => self.advance(8),
            2 => self.bytes().map(drop),
            5 => self.advance(4),
            _ => Err(Error("wire type")),
        }
    }

    fn advance(&mut self, n: usize) -> Result<(), Error> {
        if self.at + n > self.buf.len() {
            return Err(Error("field cut short"));
        }
        self.at += n;
        Ok(())
    }

    fn packed(&mut self) -> Result<Vec<u64>, Error> {
        let mut r = Reader::new(self.bytes()?);
        let mut out = Vec::new();
        while !r.done() {
            out.push(r.varint()?);
        }
        Ok(out)
    }
}

fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn put_key(out: &mut Vec<u8>, field: u64, wire: u64) {
    put_varint(out, (field << 3) | wire);
}

fn put_bytes(out: &mut Vec<u8>, field: u64, b: &[u8]) {
    put_key(out, field, 2);
    put_varint(out, b.len() as u64);
    out.extend_from_slice(b);
}

fn zigzag(v: i64) -> u64 {
    ((v << 1) ^ (v >> 63)) as u64
}

fn unzigzag(v: u64) -> i64 {
    ((v >> 1) as i64) ^ -((v & 1) as i64)
}

fn decode_geometry(cmds: &[u64], kind: Kind) -> Result<Vec<Path>, Error> {
    let (mut x, mut y) = (0i64, 0i64);
    let mut paths: Vec<Path> = Vec::new();
    let mut i = 0;
    while i < cmds.len() {
        let (id, count) = (cmds[i] & 7, (cmds[i] >> 3) as usize);
        i += 1;
        match id {
            1 | 2 => {
                for _ in 0..count {
                    let (dx, dy) = (
                        cmds.get(i).ok_or(Error("geometry"))?,
                        cmds.get(i + 1).ok_or(Error("geometry"))?,
                    );
                    i += 2;
                    x += unzigzag(*dx);
                    y += unzigzag(*dy);
                    let p = (x as f64, y as f64);
                    // A move starts a path; each point of a multi-point is one
                    if id == 1 || paths.is_empty() {
                        paths.push(vec![p]);
                    } else if let Some(path) = paths.last_mut() {
                        path.push(p);
                    }
                    if id == 1 && kind != Kind::Point && count > 1 {
                        return Err(Error("move to more than one point"));
                    }
                }
            }
            7 => {}
            _ => return Err(Error("geometry command")),
        }
    }
    Ok(paths)
}

fn decode_feature(buf: &[u8]) -> Result<Feature, Error> {
    let mut r = Reader::new(buf);
    let (mut id, mut tags, mut kind, mut geometry) = (None, Vec::new(), Kind::Unknown, Vec::new());
    while !r.done() {
        match r.key()? {
            (1, 0) => id = Some(r.varint()?),
            (2, 2) => tags = r.bytes()?.to_vec(),
            (3, 0) => {
                kind = match r.varint()? {
                    1 => Kind::Point,
                    2 => Kind::Line,
                    3 => Kind::Polygon,
                    _ => Kind::Unknown,
                }
            }
            (4, 2) => geometry = r.packed()?,
            (_, wire) => r.skip(wire)?,
        }
    }
    Ok(Feature {
        id,
        tags,
        kind,
        paths: decode_geometry(&geometry, kind)?,
    })
}

fn decode_layer(buf: &[u8]) -> Result<Layer, Error> {
    let mut r = Reader::new(buf);
    let mut layer = Layer {
        version: 2,
        name: Vec::new(),
        features: Vec::new(),
        keys: Vec::new(),
        values: Vec::new(),
        extent: 4096,
    };
    while !r.done() {
        match r.key()? {
            (15, 0) => layer.version = r.varint()?,
            (1, 2) => layer.name = r.bytes()?.to_vec(),
            (2, 2) => layer.features.push(decode_feature(r.bytes()?)?),
            (3, 2) => layer.keys.push(r.bytes()?.to_vec()),
            (4, 2) => layer.values.push(r.bytes()?.to_vec()),
            (5, 0) => layer.extent = r.varint()?,
            (_, wire) => r.skip(wire)?,
        }
    }
    Ok(layer)
}

pub(super) fn decode(buf: &[u8]) -> Result<Vec<Layer>, Error> {
    let mut r = Reader::new(buf);
    let mut layers = Vec::new();
    while !r.done() {
        match r.key()? {
            (3, 2) => layers.push(decode_layer(r.bytes()?)?),
            (_, wire) => r.skip(wire)?,
        }
    }
    Ok(layers)
}

fn command(id: u64, count: usize) -> u64 {
    id | ((count as u64) << 3)
}

fn encode_geometry(kind: Kind, paths: &[Path]) -> Vec<u64> {
    let (mut x, mut y) = (0i64, 0i64);
    let mut out = Vec::new();
    let mut step = |out: &mut Vec<u64>, (px, py): (f64, f64)| {
        let (nx, ny) = (px.round() as i64, py.round() as i64);
        out.push(zigzag(nx - x));
        out.push(zigzag(ny - y));
        (x, y) = (nx, ny);
    };
    if kind == Kind::Point {
        out.push(command(1, paths.len()));
        for p in paths {
            step(&mut out, p[0]);
        }
        return out;
    }
    for path in paths {
        out.push(command(1, 1));
        step(&mut out, path[0]);
        out.push(command(2, path.len() - 1));
        for &p in &path[1..] {
            step(&mut out, p);
        }
        if kind == Kind::Polygon {
            out.push(command(7, 1));
        }
    }
    out
}

fn encode_packed(field: u64, values: &[u64], out: &mut Vec<u8>) {
    let mut body = Vec::with_capacity(values.len() * 2);
    for v in values {
        put_varint(&mut body, *v);
    }
    put_bytes(out, field, &body);
}

fn encode_feature(f: &Feature) -> Vec<u8> {
    let mut out = Vec::new();
    if let Some(id) = f.id {
        put_key(&mut out, 1, 0);
        put_varint(&mut out, id);
    }
    put_bytes(&mut out, 2, &f.tags);
    put_key(&mut out, 3, 0);
    put_varint(
        &mut out,
        match f.kind {
            Kind::Unknown => 0,
            Kind::Point => 1,
            Kind::Line => 2,
            Kind::Polygon => 3,
        },
    );
    encode_packed(4, &encode_geometry(f.kind, &f.paths), &mut out);
    out
}

pub(super) fn encode(layers: &[Layer]) -> Vec<u8> {
    let mut out = Vec::new();
    for l in layers {
        let mut body = Vec::new();
        put_key(&mut body, 15, 0);
        put_varint(&mut body, l.version);
        put_bytes(&mut body, 1, &l.name);
        for f in &l.features {
            put_bytes(&mut body, 2, &encode_feature(f));
        }
        for k in &l.keys {
            put_bytes(&mut body, 3, k);
        }
        for v in &l.values {
            put_bytes(&mut body, 4, v);
        }
        put_key(&mut body, 5, 0);
        put_varint(&mut body, l.extent);
        put_bytes(&mut out, 3, &body);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basemap::mvt::tests::feature;

    #[test]
    fn reads_what_it_writes() {
        let layers = vec![Layer {
            version: 2,
            name: b"places".to_vec(),
            features: vec![
                feature(Kind::Point, vec![vec![(10.0, 20.0)], vec![(30.0, 40.0)]]),
                feature(
                    Kind::Line,
                    vec![vec![(0.0, 0.0), (100.0, 50.0), (200.0, 0.0)]],
                ),
                feature(
                    Kind::Polygon,
                    vec![vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)]],
                ),
            ],
            keys: vec![b"name".to_vec()],
            values: vec![vec![10, 1, b'x']],
            extent: 4096,
        }];
        assert_eq!(decode(&encode(&layers)).unwrap(), layers);
        assert!(decode(&[0x1a, 0x05, 0x01]).is_err());
    }

    #[test]
    fn reads_the_spec_example() {
        // The polygon of the MVT 2.1 spec, section 4.3.5.1: (3, 6), (8, 12), (20, 34)
        let paths = decode_geometry(&[9, 6, 12, 18, 10, 12, 24, 44, 15], Kind::Polygon).unwrap();
        assert_eq!(paths, vec![vec![(3.0, 6.0), (8.0, 12.0), (20.0, 34.0)]]);
    }
}
