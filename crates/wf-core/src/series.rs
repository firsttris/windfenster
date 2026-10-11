//! The array format of `forecast_series`: one BLOB per quantity, one value per time step.
//!
//! Each value is a little-endian `i16` holding the value times the quantity's scale, so 0.1 kn
//! of wind or 0.1 °C fit in two bytes. [`MISSING`] marks a step without a value. A 10-day
//! hourly series is 480 bytes per quantity.

use thiserror::Error;

/// Raw value of a step the model did not deliver.
pub const MISSING: i16 = i16::MIN;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quantity {
    /// Mean wind at 10 m, kn.
    Wind,
    /// Gusts at 10 m, kn.
    Gust,
    /// Direction the wind comes from, degrees 0–360.
    Direction,
    /// Air temperature at 2 m, °C.
    Temperature,
    /// Precipitation in the step before the timestamp, mm.
    Rain,
    /// Total cloud cover, %.
    Cloud,
}

impl Quantity {
    /// Stored value = real value × scale, rounded.
    pub fn scale(self) -> f32 {
        match self {
            Quantity::Wind | Quantity::Gust | Quantity::Temperature | Quantity::Rain => 10.0,
            Quantity::Direction | Quantity::Cloud => 1.0,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DecodeError {
    #[error("series blob has odd length {0}")]
    OddLength(usize),
}

/// Encodes a series; `None` and non-finite values become [`MISSING`], values out of range
/// are clamped.
pub fn encode(quantity: Quantity, values: &[Option<f32>]) -> Vec<u8> {
    let scale = quantity.scale();
    let mut out = Vec::with_capacity(values.len() * 2);
    for v in values {
        let raw = match v {
            Some(v) if v.is_finite() => (v * scale)
                .round()
                .clamp(f32::from(MISSING + 1), f32::from(i16::MAX))
                as i16,
            _ => MISSING,
        };
        out.extend_from_slice(&raw.to_le_bytes());
    }
    out
}

pub fn decode(quantity: Quantity, blob: &[u8]) -> Result<Vec<Option<f32>>, DecodeError> {
    if !blob.len().is_multiple_of(2) {
        return Err(DecodeError::OddLength(blob.len()));
    }
    let scale = quantity.scale();
    Ok(blob
        .chunks_exact(2)
        .map(|b| match i16::from_le_bytes([b[0], b[1]]) {
            MISSING => None,
            raw => Some(f32::from(raw) / scale),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let wind = [Some(0.0), Some(14.26), None, Some(31.04)];
        let blob = encode(Quantity::Wind, &wind);
        assert_eq!(blob.len(), 8);
        assert_eq!(
            decode(Quantity::Wind, &blob).unwrap(),
            [Some(0.0), Some(14.3), None, Some(31.0)]
        );

        let temp = [Some(-12.34), Some(35.0)];
        let back = decode(Quantity::Temperature, &encode(Quantity::Temperature, &temp)).unwrap();
        assert_eq!(back, [Some(-12.3), Some(35.0)]);

        let dir = [Some(359.6), Some(0.4)];
        let back = decode(Quantity::Direction, &encode(Quantity::Direction, &dir)).unwrap();
        assert_eq!(back, [Some(360.0), Some(0.0)]);
    }

    #[test]
    fn missing_and_out_of_range() {
        let blob = encode(
            Quantity::Rain,
            &[Some(f32::NAN), Some(1e9), Some(-1e9), Some(f32::INFINITY)],
        );
        assert_eq!(
            decode(Quantity::Rain, &blob).unwrap(),
            [None, Some(3276.7), Some(-3276.7), None]
        );
    }

    #[test]
    fn rejects_odd_length() {
        assert_eq!(
            decode(Quantity::Wind, &[1, 2, 3]),
            Err(DecodeError::OddLength(3))
        );
    }
}
