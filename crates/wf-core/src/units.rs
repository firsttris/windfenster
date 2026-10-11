use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Metres per second in one knot.
pub const MS_PER_KN: f64 = 1852.0 / 3600.0;
/// Kilometres per hour in one knot.
pub const KMH_PER_KN: f64 = 1.852;

/// Unit the wind is shown in. The server always stores and sends knots; the SPA converts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum WindUnit {
    #[default]
    Kn,
    Ms,
    Kmh,
}

impl WindUnit {
    pub fn from_kn(self, kn: f64) -> f64 {
        match self {
            WindUnit::Kn => kn,
            WindUnit::Ms => kn * MS_PER_KN,
            WindUnit::Kmh => kn * KMH_PER_KN,
        }
    }

    pub fn to_kn(self, value: f64) -> f64 {
        match self {
            WindUnit::Kn => value,
            WindUnit::Ms => value / MS_PER_KN,
            WindUnit::Kmh => value / KMH_PER_KN,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions() {
        assert!((WindUnit::Ms.from_kn(10.0) - 5.144).abs() < 0.001);
        assert!((WindUnit::Kmh.from_kn(10.0) - 18.52).abs() < 1e-9);
        assert_eq!(WindUnit::Kn.from_kn(14.0), 14.0);
        for unit in [WindUnit::Kn, WindUnit::Ms, WindUnit::Kmh] {
            assert!((unit.to_kn(unit.from_kn(17.3)) - 17.3).abs() < 1e-9);
        }
    }

    #[test]
    fn serde_names() {
        assert_eq!(serde_json::to_string(&WindUnit::Ms).unwrap(), "\"ms\"");
        assert_eq!(
            serde_json::from_str::<WindUnit>("\"kmh\"").unwrap(),
            WindUnit::Kmh
        );
    }
}
