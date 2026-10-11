//! PEGELONLINE (WSV): the gauge list with current values, and one gauge's level series.

use serde::Deserialize;
use wf_core::gauge::Gauge;

#[derive(Deserialize)]
struct GaugeJson {
    uuid: String,
    number: String,
    longname: String,
    km: Option<f64>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    water: Option<WaterJson>,
    #[serde(default)]
    timeseries: Vec<SeriesJson>,
}

#[derive(Deserialize)]
struct WaterJson {
    longname: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeriesJson {
    shortname: String,
    current_measurement: Option<CurrentJson>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurrentJson {
    timestamp: String,
    value: Option<f64>,
    state_mnw_mhw: Option<String>,
    state_nsw_hsw: Option<String>,
}

/// Parses PEGELONLINE's station list with time series and current values: level W (cm), flow Q
/// (m³/s), water temperature WT (°C). Gauges without coordinates (some abroad) are left out;
/// names come in upper case and are made readable.
pub fn parse_gauges(body: &[u8]) -> serde_json::Result<Vec<Gauge>> {
    let list: Vec<GaugeJson> = serde_json::from_slice(body)?;
    Ok(list
        .into_iter()
        .filter_map(|g| {
            let (lat, lon) = (g.latitude?, g.longitude?);
            let current = |name: &str| {
                g.timeseries
                    .iter()
                    .find(|t| t.shortname == name)
                    .and_then(|t| t.current_measurement.as_ref())
            };
            let reading = |name: &str| {
                current(name)
                    .and_then(|c| Some((c.value?, wf_core::time::parse_iso(&c.timestamp)?)))
            };
            let level = reading("W");
            let flow = reading("Q");
            let temp = reading("WT");
            let w = current("W");
            Some(Gauge {
                uuid: g.uuid,
                number: g.number,
                name: wf_core::text::title_case(&g.longname),
                water: g.water.map(|w| wf_core::text::title_case(&w.longname)),
                km: g.km,
                lat,
                lon,
                level_cm: level.map(|r| r.0),
                level_at: level.map(|r| r.1),
                level_state: w.and_then(|c| c.state_mnw_mhw.clone()),
                shipping_state: w.and_then(|c| c.state_nsw_hsw.clone()),
                flow_m3s: flow.map(|r| r.0),
                flow_at: flow.map(|r| r.1),
                water_temp_c: temp.map(|r| r.0),
                water_temp_at: temp.map(|r| r.1),
            })
        })
        .collect())
}

#[derive(Deserialize)]
struct MeasurementJson {
    timestamp: String,
    value: Option<f32>,
}

/// Parses a PEGELONLINE measurement series (`…/W/measurements.json`).
pub fn parse_measurements(body: &[u8]) -> serde_json::Result<Vec<(i64, f32)>> {
    let list: Vec<MeasurementJson> = serde_json::from_slice(body)?;
    Ok(list
        .into_iter()
        .filter_map(|m| Some((wf_core::time::parse_iso(&m.timestamp)?, m.value?)))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pegelonline() {
        // From the real answers (2026-10-11), cut: Iffezheim with W and Q, a gauge abroad
        // without coordinates.
        let body = r#"[{"uuid":"b02be240-1364-4c97-8bb6-675d7d842332","number":"23500600","shortname":"IFFEZHEIM","longname":"IFFEZHEIM","km":336.2,"agency":"STANDORT FREIBURG","longitude":8.116902,"latitude":48.852149,"voiceServiceNumber":"+49228 286527 546","water":{"shortname":"RHEIN","longname":"RHEIN"},"timeseries":[{"shortname":"Q","longname":"ABFLUSS_ROHDATEN","unit":"m³/s","equidistance":15,"currentMeasurement":{"timestamp":"2026-10-11T11:00:00+02:00","value":358.0}},{"shortname":"W","longname":"WASSERSTAND ROHDATEN","unit":"cm","equidistance":15,"currentMeasurement":{"timestamp":"2026-10-11T11:15:00+02:00","value":62.0,"stateMnwMhw":"low","stateNswHsw":"unknown"},"gaugeZero":{"unit":"m. ü. NHN","value":110.019,"validFrom":"2018-11-01"}}]},
            {"uuid":"x","number":"1","shortname":"DORDRECHT","longname":"DORDRECHT","water":{"shortname":"RHEIN","longname":"RHEIN"},"timeseries":[]}]"#;
        let gauges = parse_gauges(body.as_bytes()).unwrap();
        assert_eq!(gauges.len(), 1);
        let g = &gauges[0];
        assert_eq!(
            (g.name.as_str(), g.water.as_deref(), g.km),
            ("Iffezheim", Some("Rhein"), Some(336.2))
        );
        assert_eq!((g.level_cm, g.level_at), (Some(62.0), Some(1_791_710_100)));
        assert_eq!(
            (g.level_state.as_deref(), g.shipping_state.as_deref()),
            (Some("low"), Some("unknown"))
        );
        assert_eq!((g.flow_m3s, g.flow_at), (Some(358.0), Some(1_791_709_200)));
        assert_eq!(g.water_temp_c, None);

        let history = br#"[{"timestamp":"2026-10-04T11:45:00+02:00","value":43.0},{"timestamp":"2026-10-11T11:15:00+02:00","value":62.0}]"#;
        assert_eq!(
            parse_measurements(history).unwrap(),
            [(1_791_107_100, 43.0), (1_791_710_100, 62.0)]
        );
    }
}
