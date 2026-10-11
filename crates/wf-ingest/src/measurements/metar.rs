//! METARs from aviationweather.gov: the cache file of all current reports (CSV) and the
//! API's history of one station (JSON).

use serde::Deserialize;
use wf_core::observation::Observation;

/// Parses aviationweather.gov's METAR cache CSV. Wind is in knots already (m/s reports are
/// converted); a direction of 0 with wind means variable (VRB), north is 360.
pub fn parse_metar_csv(text: &str) -> Vec<(String, Observation)> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(text.as_bytes());
    let Ok(headers) = reader.headers().cloned() else {
        return Vec::new();
    };
    let col = |name: &str| headers.iter().position(|h| h == name);
    let (Some(raw), Some(id), Some(time), Some(dir), Some(spd), Some(gust), Some(temp)) = (
        col("raw_text"),
        col("station_id"),
        col("observation_time"),
        col("wind_dir_degrees"),
        col("wind_speed_kt"),
        col("wind_gust_kt"),
        col("temp_c"),
    ) else {
        return Vec::new();
    };
    let num = |r: &csv::StringRecord, i: usize| r.get(i).and_then(|v| v.trim().parse::<f32>().ok());
    reader
        .records()
        .filter_map(Result::ok)
        .filter_map(|r| {
            let at = wf_core::time::parse_iso(r.get(time)?)?;
            let wind = num(&r, spd);
            let dir = num(&r, dir).filter(|d| *d > 0.0 || wind == Some(0.0));
            Some((
                r.get(id)?.to_owned(),
                Observation {
                    at,
                    wind,
                    gust: num(&r, gust),
                    dir: if wind == Some(0.0) { None } else { dir },
                    temp: num(&r, temp),
                    raw: r.get(raw).map(str::to_owned),
                },
            ))
        })
        .collect()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MetarJson {
    icao_id: String,
    obs_time: i64,
    temp: Option<f32>,
    /// Degrees, or "VRB"
    wdir: Option<serde_json::Value>,
    wspd: Option<f32>,
    wgst: Option<f32>,
    raw_ob: Option<String>,
}

/// Parses the METAR API's JSON (`/api/data/metar?format=json`). Wind in knots; the direction
/// is a number or "VRB", 0 with wind is variable as well.
pub fn parse_metar_json(body: &[u8]) -> serde_json::Result<Vec<(String, Observation)>> {
    let list: Vec<MetarJson> = serde_json::from_slice(body)?;
    Ok(list
        .into_iter()
        .map(|m| {
            let calm = m.wspd == Some(0.0);
            let dir = m
                .wdir
                .and_then(|d| d.as_f64())
                .map(|d| d as f32)
                .filter(|d| *d > 0.0 && !calm);
            (
                m.icao_id,
                Observation {
                    at: m.obs_time,
                    wind: m.wspd,
                    gust: m.wgst,
                    dir,
                    temp: m.temp,
                    raw: m.raw_ob,
                },
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metar_cache_rows() {
        // Header and rows from the real file (2026-10-11), columns after temp cut.
        let csv = "raw_text,station_id,observation_time,latitude,longitude,temp_c,dewpoint_c,wind_dir_degrees,wind_speed_kt,wind_gust_kt\n\
\"METAR EDSB 110850Z AUTO 21009KT CAVOK 13/10 Q1015\",EDSB,2026-10-11T08:50:00.000Z,48.7790,8.0810,13,10,210,9,\n\
\"METAR KDBN 110855Z AUTO VRB05KT 10SM 20/20 A2994 RMK AO2 LTG\",KDBN,2026-10-11T08:55:00.000Z,32.5,-82.8,20,20,0,5,\n\
\"METAR KFMH 110856Z 00000KT 10SM CLR 06/05 A3047\",KFMH,2026-10-11T08:56:00.000Z,41.6,-70.5,6,5,0,0,\n\
\"METAR KSEE 110855Z AUTO 17005G11KT 10SM SCT030 20/18 A2945 RMK AO2\",KSEE,2026-10-11T08:55:00.000Z,32.8,-116.9,20,18,170,5,11\n\
\"METAR LXXX 110850Z 36012KT 9999 15/10 Q1010\",LXXX,2026-10-11T08:50:00.000Z,48,8,15,10,360,12,\n";
        let rows = parse_metar_csv(csv);
        assert_eq!(rows.len(), 5);
        let (id, edsb) = &rows[0];
        assert_eq!(id, "EDSB");
        assert_eq!(edsb.at, 1_791_708_600);
        assert_eq!(
            (edsb.wind, edsb.dir, edsb.gust, edsb.temp),
            (Some(9.0), Some(210.0), None, Some(13.0))
        );
        assert_eq!(
            edsb.raw.as_deref(),
            Some("METAR EDSB 110850Z AUTO 21009KT CAVOK 13/10 Q1015")
        );
        assert_eq!((rows[1].1.wind, rows[1].1.dir), (Some(5.0), None)); // VRB
        assert_eq!((rows[2].1.wind, rows[2].1.dir), (Some(0.0), None)); // calm
        assert_eq!(rows[3].1.gust, Some(11.0));
        assert_eq!(rows[4].1.dir, Some(360.0)); // north
    }

    #[test]
    fn metar_api_rows() {
        // From the real answer of /api/data/metar?ids=EDSB,LGRP&format=json (2026-10-11), cut.
        let body = br#"[{"icaoId":"LGRP","receiptTime":"2026-10-11T08:54:50.248Z","obsTime":1791708600,"reportTime":"2026-10-11T09:00:00.000Z","temp":25,"dewp":19,"wdir":340,"wspd":4,"visib":"6+","altim":1012,"qcField":16,"metarType":"METAR","rawOb":"METAR LGRP 110850Z 34004KT 280V020 9999 FEW018 25/19 Q1012 NOSIG","lat":36.405,"lon":28.086,"elev":3,"name":"Rhodes/Diagoras Arpt, K, GR","cover":"FEW","clouds":[{"cover":"FEW","base":1800}],"fltCat":"VFR"},
            {"icaoId":"LGRP","obsTime":1791706800,"temp":24,"wdir":"VRB","wspd":4,"rawOb":"METAR LGRP 110820Z VRB04KT 9999 FEW018 24/19 Q1012 NOSIG"},
            {"icaoId":"EDSB","obsTime":1791708600,"temp":13,"wdir":210,"wspd":9,"wgst":null,"rawOb":"METAR EDSB 110850Z AUTO 21009KT CAVOK 13/10 Q1015"}]"#;
        let rows = parse_metar_json(body).unwrap();
        assert_eq!(rows.len(), 3);
        let (id, o) = &rows[0];
        assert_eq!(
            (id.as_str(), o.at, o.wind, o.dir, o.temp),
            ("LGRP", 1_791_708_600, Some(4.0), Some(340.0), Some(25.0))
        );
        assert_eq!((rows[1].1.wind, rows[1].1.dir), (Some(4.0), None)); // VRB
        assert_eq!(rows[2].1.gust, None);
        assert!(parse_metar_json(b"[]").unwrap().is_empty());
    }
}
