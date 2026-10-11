//! Cutting geometry to a box: lines with Liang–Barsky, rings with Sutherland–Hodgman.

use super::Path;

/// A box in tile units: min x, min y, max x, max y
pub(super) type Rect = (f64, f64, f64, f64);

/// The parts of a line inside the box (Liang–Barsky on each segment)
pub(super) fn clip_line(line: &Path, (x0, y0, x1, y1): Rect) -> Vec<Path> {
    let mut parts: Vec<Path> = Vec::new();
    let mut current: Path = Vec::new();
    for w in line.windows(2) {
        let ((ax, ay), (bx, by)) = (w[0], w[1]);
        let (dx, dy) = (bx - ax, by - ay);
        let (mut t0, mut t1) = (0.0f64, 1.0f64);
        let inside = [(-dx, ax - x0), (dx, x1 - ax), (-dy, ay - y0), (dy, y1 - ay)]
            .iter()
            .all(|&(p, q)| {
                if p == 0.0 {
                    return q >= 0.0;
                }
                let t = q / p;
                if p < 0.0 {
                    t0 = t0.max(t);
                } else {
                    t1 = t1.min(t);
                }
                t0 <= t1
            });
        if !inside {
            if current.len() > 1 {
                parts.push(std::mem::take(&mut current));
            }
            current.clear();
            continue;
        }
        let a = (ax + t0 * dx, ay + t0 * dy);
        let b = (ax + t1 * dx, ay + t1 * dy);
        if current.last() != Some(&a) {
            if current.len() > 1 {
                parts.push(std::mem::take(&mut current));
            }
            current = vec![a];
        }
        current.push(b);
        // Left the box: the line goes on outside
        if t1 < 1.0 {
            parts.push(std::mem::take(&mut current));
        }
    }
    if current.len() > 1 {
        parts.push(current);
    }
    parts
}

/// A ring cut to the box (Sutherland–Hodgman); None when nothing of it is left.
pub(super) fn clip_ring(ring: &Path, (x0, y0, x1, y1): Rect) -> Option<Path> {
    let mut pts = ring.clone();
    type Edge = (fn(f64, f64, f64) -> bool, usize, f64);
    let edges: [Edge; 4] = [
        (|v, _, e| v >= e, 0, x0),
        (|v, _, e| v <= e, 0, x1),
        (|v, _, e| v >= e, 1, y0),
        (|v, _, e| v <= e, 1, y1),
    ];
    for (inside, axis, e) in edges {
        let get = |p: (f64, f64)| if axis == 0 { p.0 } else { p.1 };
        let cross = |a: (f64, f64), b: (f64, f64)| {
            let t = (e - get(a)) / (get(b) - get(a));
            let p = (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1));
            if axis == 0 { (e, p.1) } else { (p.0, e) }
        };
        let mut next = Vec::with_capacity(pts.len() + 4);
        for (i, &b) in pts.iter().enumerate() {
            let a = pts[(i + pts.len() - 1) % pts.len()];
            let (ia, ib) = (inside(get(a), 0.0, e), inside(get(b), 0.0, e));
            if ib {
                if !ia {
                    next.push(cross(a, b));
                }
                next.push(b);
            } else if ia {
                next.push(cross(a, b));
            }
        }
        pts = next;
        if pts.len() < 3 {
            return None;
        }
    }
    Some(pts)
}

/// Twice the ring's signed area: positive for an outer ring in MVT's y-down coordinates
pub(super) fn area2(ring: &Path) -> f64 {
    ring.iter()
        .zip(ring.iter().cycle().skip(1))
        .map(|(a, b)| a.0 * b.1 - b.0 * a.1)
        .sum()
}

/// Rounded to whole units with repeated points dropped, as the tile will store it
pub(super) fn snapped(path: Path) -> Path {
    let mut out: Path = Vec::with_capacity(path.len());
    for (x, y) in path {
        let p = (x.round(), y.round());
        if out.last() != Some(&p) {
            out.push(p);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clips_lines_and_rings() {
        let rect = (0.0, 0.0, 10.0, 10.0);
        // In, out and in again: two parts
        let parts = clip_line(
            &vec![(5.0, 5.0), (15.0, 5.0), (15.0, 8.0), (5.0, 8.0)],
            rect,
        );
        assert_eq!(
            parts,
            vec![vec![(5.0, 5.0), (10.0, 5.0)], vec![(10.0, 8.0), (5.0, 8.0)]]
        );
        assert!(clip_line(&vec![(20.0, 20.0), (30.0, 30.0)], rect).is_empty());
        // A triangle sticking out on the right
        let ring = clip_ring(&vec![(5.0, 2.0), (15.0, 5.0), (5.0, 8.0)], rect).unwrap();
        assert!(ring.iter().all(|&(x, _)| x <= 10.0));
        assert_eq!(
            clip_ring(&vec![(20.0, 2.0), (25.0, 5.0), (20.0, 8.0)], rect),
            None
        );
    }
}
