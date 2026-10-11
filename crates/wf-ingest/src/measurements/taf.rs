//! TAFs from aviationweather.gov's API, which sends the change groups decoded.

use serde::Deserialize;
use wf_core::taf::{Taf, TafChange, TafCloud, TafGroup};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TafJson {
    icao_id: String,
    issue_time: String,
    valid_time_from: i64,
    valid_time_to: i64,
    #[serde(rename = "rawTAF")]
    raw_taf: String,
    #[serde(default)]
    fcsts: Vec<TafGroupJson>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TafGroupJson {
    time_from: i64,
    time_to: i64,
    time_bec: Option<i64>,
    fcst_change: Option<String>,
    probability: Option<u8>,
    wdir: Option<serde_json::Value>,
    wspd: Option<f32>,
    wgst: Option<f32>,
    /// Statute miles as a number, "6+" for 6 or more, "" when not given
    visib: Option<serde_json::Value>,
    wx_string: Option<String>,
    #[serde(default)]
    clouds: Vec<CloudJson>,
}

#[derive(Deserialize)]
struct CloudJson {
    cover: String,
    base: Option<f32>,
    #[serde(rename = "type")]
    kind: Option<String>,
}

/// Parses the TAF API's JSON (`/api/data/taf?format=json`): aviationweather.gov decodes the
/// groups already. Wind in knots, visibility in statute miles (turned into metres, 6+ = 9999).
pub fn parse_taf_json(body: &[u8], fetched_at: i64) -> serde_json::Result<Vec<(String, Taf)>> {
    let list: Vec<TafJson> = serde_json::from_slice(body)?;
    Ok(list
        .into_iter()
        .filter_map(|t| {
            let issued_at = wf_core::time::parse_iso(&t.issue_time)?;
            let groups = t
                .fcsts
                .into_iter()
                .map(|g| {
                    let change = match g.fcst_change.as_deref() {
                        Some("FM") => TafChange::From,
                        Some("BECMG") => TafChange::Becoming,
                        Some("TEMPO") => TafChange::Tempo,
                        Some("PROB") => TafChange::Prob,
                        _ => TafChange::Base,
                    };
                    let variable = g.wdir.as_ref().and_then(|d| d.as_str()) == Some("VRB");
                    let visibility_m = match &g.visib {
                        Some(serde_json::Value::String(s)) if s.ends_with('+') => Some(9999.0),
                        Some(v) => v
                            .as_f64()
                            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                            .map(|mi| {
                                ((mi * 1_609.344 / 100.0).round() * 100.0).min(9999.0) as f32
                            }),
                        None => None,
                    };
                    TafGroup {
                        change,
                        probability: g.probability,
                        from: g.time_from,
                        to: g.time_to,
                        by: g.time_bec,
                        dir: g.wdir.and_then(|d| d.as_f64()).map(|d| d as f32),
                        variable,
                        wind: g.wspd,
                        gust: g.wgst,
                        visibility_m,
                        weather: g.wx_string.filter(|w| !w.is_empty()),
                        clouds: g
                            .clouds
                            .into_iter()
                            .map(|c| TafCloud {
                                cover: c.cover,
                                base_ft: c.base,
                                kind: c.kind,
                            })
                            .collect(),
                    }
                })
                .collect();
            Some((
                t.icao_id,
                Taf::new(
                    issued_at,
                    t.valid_time_from,
                    t.valid_time_to,
                    t.raw_taf,
                    groups,
                    fetched_at,
                ),
            ))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taf_api_rows() {
        // From the real answer of /api/data/taf?ids=EDDF,LGRP&format=json (2026-10-11), cut.
        let body = br#"[{"icaoId":"EDDF","dbPopTime":"2026-10-11T06:34:25.763Z","bulletinTime":"2026-10-11T05:00:00.000Z","issueTime":"2026-10-11T06:31:00.000Z","validTimeFrom":1791698400,"validTimeTo":1791806400,"rawTAF":"TAF AMD EDDF 110631Z 1106/1212 20005KT CAVOK TEMPO 1106/1114 BKN025 BECMG 1107/1110 24012KT BECMG 1115/1118 26007KT BECMG 1119/1122 VRB03KT TEMPO 1202/1206 BKN045 PROB30 TEMPO 1202/1207 1200 BCFG","mostRecent":1,"remarks":"","lat":50.045,"lon":8.598,"elev":113,"prior":1,"name":"Frankfurt/Main Arpt","fcsts":[
            {"timeFrom":1791698400,"timeTo":1791702000,"timeBec":null,"fcstChange":null,"probability":null,"wdir":200,"wspd":5,"wgst":null,"wshearHgt":null,"wshearDir":null,"wshearSpd":null,"visib":"6+","altim":null,"vertVis":null,"wxString":"NSW","notDecoded":null,"clouds":[{"cover":"NSC","base":null,"type":null}],"icgTurb":[],"temp":[]},
            {"timeFrom":1791698400,"timeTo":1791727200,"timeBec":null,"fcstChange":"TEMPO","probability":null,"wdir":null,"wspd":null,"wgst":null,"visib":"","wxString":null,"clouds":[{"cover":"BKN","base":2500,"type":null}]},
            {"timeFrom":1791702000,"timeTo":1791730800,"timeBec":1791712800,"fcstChange":"BECMG","probability":null,"wdir":240,"wspd":12,"wgst":null,"visib":"6+","wxString":"NSW","clouds":[{"cover":"NSC","base":null,"type":null}]},
            {"timeFrom":1791745200,"timeTo":1791806400,"timeBec":1791756000,"fcstChange":"BECMG","probability":null,"wdir":"VRB","wspd":3,"wgst":null,"visib":"6+","wxString":"NSW","clouds":[{"cover":"NSC","base":null,"type":null}]},
            {"timeFrom":1791770400,"timeTo":1791788400,"timeBec":null,"fcstChange":"TEMPO","probability":30,"wdir":null,"wspd":null,"wgst":null,"visib":0.75,"wxString":"BCFG","clouds":[]}]},
          {"icaoId":"LGRP","issueTime":"2026-10-11T05:00:00.000Z","validTimeFrom":1791698400,"validTimeTo":1791784800,"rawTAF":"TAF LGRP 110500Z 1106/1206 25008KT 9999 FEW018 TEMPO 1200/1206 SCT010 PROB30 TEMPO 1204/1206 5000 RA FEW018TCU BKN020","fcsts":[
            {"timeFrom":1791777600,"timeTo":1791784800,"timeBec":null,"fcstChange":"TEMPO","probability":30,"wdir":null,"wspd":null,"wgst":null,"visib":3.11,"wxString":"RA","clouds":[{"cover":"FEW","base":1800,"type":"TCU"},{"cover":"BKN","base":2000,"type":null}]}]}]"#;
        let tafs = parse_taf_json(body, 7).unwrap();
        assert_eq!(tafs.len(), 2);
        let (icao, t) = &tafs[0];
        assert_eq!(
            (icao.as_str(), t.issued_at, t.fetched_at),
            ("EDDF", 1_791_700_260, 7)
        );
        assert_eq!(t.groups.len(), 5);
        let base = &t.groups[0];
        assert_eq!(
            (base.change, base.dir, base.wind, base.visibility_m),
            (TafChange::Base, Some(200.0), Some(5.0), Some(9999.0))
        );
        assert_eq!(base.clouds[0].cover, "NSC");
        assert_eq!(
            (
                t.groups[1].change,
                t.groups[1].wind,
                t.groups[1].visibility_m
            ),
            (TafChange::Tempo, None, None)
        );
        let becmg = &t.groups[3];
        assert_eq!(
            (becmg.change, becmg.variable, becmg.dir, becmg.by),
            (TafChange::Becoming, true, None, Some(1_791_756_000))
        );
        let fog = &t.groups[4];
        assert_eq!(
            (fog.probability, fog.visibility_m, fog.weather.as_deref()),
            (Some(30), Some(1200.0), Some("BCFG"))
        );
        // 30 hours of 1106/1212, with the BECMG to 24012KT done by 10 UTC.
        assert_eq!(t.hours.len(), 30);
        assert_eq!(
            (t.hours[0].wind, t.hours[5].wind, t.hours[5].dir),
            (Some(5.0), Some(12.0), Some(240.0))
        );
        let lgrp = &tafs[1].1;
        assert_eq!(lgrp.groups[0].visibility_m, Some(5000.0));
        assert_eq!(lgrp.groups[0].clouds[0].kind.as_deref(), Some("TCU"));
    }
}
