/// A place on the 0.01° grid (about 1.1 km north–south) that forecast series are stored on.
///
/// Free points are rounded to it, and so are stations: every model's grid is coarser than
/// 0.01°, so snapping a station by at most 0.005° does not change its forecast, and one key
/// covers both. Stored as degrees × 100 so the key is two integers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GridPoint {
    pub lat_e2: i32,
    pub lon_e2: i32,
}

impl GridPoint {
    /// Rounds to the grid; `None` outside valid coordinates.
    pub fn new(lat: f64, lon: f64) -> Option<Self> {
        if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
            return None;
        }
        Some(Self {
            lat_e2: (lat * 100.0).round() as i32,
            lon_e2: (lon * 100.0).round() as i32,
        })
    }

    pub fn lat(self) -> f64 {
        f64::from(self.lat_e2) / 100.0
    }

    pub fn lon(self) -> f64 {
        f64::from(self.lon_e2) / 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_to_hundredths() {
        // Baden Airpark (EDSB)
        let p = GridPoint::new(48.7794, 8.0805).unwrap();
        assert_eq!((p.lat_e2, p.lon_e2), (4878, 808));
        assert_eq!((p.lat(), p.lon()), (48.78, 8.08));
        let p = GridPoint::new(-33.865, -151.2094).unwrap();
        assert_eq!((p.lat_e2, p.lon_e2), (-3387, -15121));
    }

    #[test]
    fn rejects_invalid() {
        assert!(GridPoint::new(91.0, 0.0).is_none());
        assert!(GridPoint::new(0.0, 180.5).is_none());
        assert!(GridPoint::new(f64::NAN, 0.0).is_none());
    }
}
