//! What an alarm says, in the account's language, time zone and wind unit. Push messages and
//! e-mails are messages to people, not API answers: the server writes them, in German and
//! English as the app (`web/messages`), with the same compass points.

use chrono::{DateTime, Datelike, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use wf_core::alarm::{Notice, Window};
use wf_core::gear::{GearItem, GearKind};
use wf_core::units::WindUnit;

/// A message: its title (the e-mail's subject) and text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Text {
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    De,
    En,
}

impl Lang {
    pub fn of(locale: &str) -> Self {
        if locale == "en" { Lang::En } else { Lang::De }
    }
}

/// How the account reads things, and about which spot.
#[derive(Debug, Clone, Copy)]
pub struct Ctx<'a> {
    pub lang: Lang,
    pub tz: Tz,
    pub unit: WindUnit,
    pub spot: &'a str,
}

const COMPASS_DE: [&str; 16] = [
    "N", "NNO", "NO", "ONO", "O", "OSO", "SO", "SSO", "S", "SSW", "SW", "WSW", "W", "WNW", "NW",
    "NNW",
];
const COMPASS_EN: [&str; 16] = [
    "N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSW", "SW", "WSW", "W", "WNW", "NW",
    "NNW",
];

impl Ctx<'_> {
    fn local(&self, at: i64) -> DateTime<Tz> {
        self.tz.from_utc_datetime(
            &DateTime::<Utc>::from_timestamp(at, 0)
                .unwrap_or_default()
                .naive_utc(),
        )
    }

    /// The local day as yyyymmdd, the key of what was told
    pub fn day(&self, at: i64) -> i64 {
        let d = self.local(at);
        i64::from(d.year()) * 10_000 + i64::from(d.month()) * 100 + i64::from(d.day())
    }

    pub fn hour(&self, at: i64) -> u32 {
        self.local(at).hour()
    }

    /// "heute", "morgen" or the weekday
    fn day_word(&self, at: i64, now: i64) -> String {
        let (d, today) = (self.local(at).date_naive(), self.local(now).date_naive());
        let days = (d - today).num_days();
        let weekday = d.weekday().num_days_from_monday() as usize;
        match (self.lang, days) {
            (Lang::De, 0) => "heute".into(),
            (Lang::De, 1) => "morgen".into(),
            (Lang::De, _) => [
                "Montag",
                "Dienstag",
                "Mittwoch",
                "Donnerstag",
                "Freitag",
                "Samstag",
                "Sonntag",
            ][weekday]
                .into(),
            (Lang::En, 0) => "today".into(),
            (Lang::En, 1) => "tomorrow".into(),
            (Lang::En, _) => [
                "Monday",
                "Tuesday",
                "Wednesday",
                "Thursday",
                "Friday",
                "Saturday",
                "Sunday",
            ][weekday]
                .into(),
        }
    }

    /// "14–18 Uhr" / "14:00–18:00"
    fn span(&self, w: &Window) -> String {
        let (a, b) = (self.local(w.start), self.local(w.end));
        match self.lang {
            Lang::De => format!("{}–{} Uhr", a.format("%-H"), b.format("%-H")),
            Lang::En => format!("{}–{}", a.format("%H:%M"), b.format("%H:%M")),
        }
    }

    /// "19 kn"
    fn wind(&self, kn: f64) -> String {
        let (value, label) = match self.unit {
            WindUnit::Kn => (kn, "kn"),
            WindUnit::Ms => (WindUnit::Ms.from_kn(kn), "m/s"),
            WindUnit::Kmh => (WindUnit::Kmh.from_kn(kn), "km/h"),
        };
        format!("{value:.0} {label}")
    }

    fn compass(&self, deg: f64) -> &'static str {
        let i = ((deg.rem_euclid(360.0) / 22.5).round() as usize) % 16;
        match self.lang {
            Lang::De => COMPASS_DE[i],
            Lang::En => COMPASS_EN[i],
        }
    }

    /// " · Wing 4", or nothing
    fn gear(&self, g: Option<&GearItem>) -> String {
        let Some(g) = g else { return String::new() };
        let kind = match (g.kind, self.lang) {
            (GearKind::Kite, _) => "Kite",
            (GearKind::Wing, _) => "Wing",
            (GearKind::Windsurf, Lang::De) => "Segel",
            (GearKind::Windsurf, Lang::En) => "Sail",
            (GearKind::Parawing, _) => "Parawing",
        };
        let size = if g.size.fract() == 0.0 {
            format!("{:.0}", g.size)
        } else {
            format!("{:.1}", g.size)
        };
        format!(" · {kind} {size}")
    }

    /// What there is to tell about a day's window; `gear` the piece for its strongest hour.
    pub fn notice(&self, n: &Notice, gear: Option<&GearItem>, now: i64) -> Text {
        let spot = self.spot;
        match (n, self.lang) {
            (Notice::New(w), Lang::De) => Text {
                title: format!("{spot}: Wind {}", self.day_word(w.start, now)),
                body: format!(
                    "{}, bis {} aus {}{}",
                    self.span(w),
                    self.wind(w.peak_kn),
                    self.compass(w.dir),
                    self.gear(gear)
                ),
            },
            (Notice::New(w), Lang::En) => Text {
                title: format!("{spot}: wind {}", self.day_word(w.start, now)),
                body: format!(
                    "{}, up to {} from {}{}",
                    self.span(w),
                    self.wind(w.peak_kn),
                    self.compass(w.dir),
                    self.gear(gear)
                ),
            },
            (Notice::Changed { before, now: w }, Lang::De) => Text {
                title: format!("{spot}: Wind {}, geändert", self.day_word(w.start, now)),
                body: format!(
                    "Jetzt {}, bis {} aus {}{} (vorher {}, bis {})",
                    self.span(w),
                    self.wind(w.peak_kn),
                    self.compass(w.dir),
                    self.gear(gear),
                    self.span(before),
                    self.wind(before.peak_kn)
                ),
            },
            (Notice::Changed { before, now: w }, Lang::En) => Text {
                title: format!("{spot}: wind {}, changed", self.day_word(w.start, now)),
                body: format!(
                    "Now {}, up to {} from {}{} (was {}, up to {})",
                    self.span(w),
                    self.wind(w.peak_kn),
                    self.compass(w.dir),
                    self.gear(gear),
                    self.span(before),
                    self.wind(before.peak_kn)
                ),
            },
            (Notice::Gone(w), Lang::De) => Text {
                title: format!("{spot}: doch kein Wind {}", self.day_word(w.start, now)),
                body: format!(
                    "Das Fenster {} ist aus der Vorhersage verschwunden.",
                    self.span(w)
                ),
            },
            (Notice::Gone(w), Lang::En) => Text {
                title: format!("{spot}: no wind {} after all", self.day_word(w.start, now)),
                body: format!("The window {} has gone from the forecast.", self.span(w)),
            },
        }
    }

    /// The station measures it now.
    pub fn live(
        &self,
        wind: f64,
        gust: Option<f64>,
        dir: Option<f64>,
        gear: Option<&GearItem>,
    ) -> Text {
        let spot = self.spot;
        let (title, measured, gusts, from) = match self.lang {
            Lang::De => (format!("{spot}: Es weht!"), "Gemessen", "Böen", "aus"),
            Lang::En => (
                format!("{spot}: it's blowing!"),
                "Measured",
                "gusts",
                "from",
            ),
        };
        let mut body = format!("{measured} {}", self.wind(wind));
        if let Some(g) = gust {
            body.push_str(&format!(", {gusts} {}", self.wind(g)));
        }
        if let Some(d) = dir {
            body.push_str(&format!(" {from} {}", self.compass(d)));
        }
        body.push_str(&self.gear(gear));
        Text { title, body }
    }
}

/// The message a user sends themselves to see that it arrives.
pub fn test(lang: Lang) -> Text {
    match lang {
        Lang::De => Text {
            title: "Windfenster".into(),
            body: "So kommen deine Alarme an.".into(),
        },
        Lang::En => Text {
            title: "Windfenster".into(),
            body: "This is how your alarms arrive.".into(),
        },
    }
}

/// The e-mail's text: the message, where to look and where to change the alarms.
pub fn email_body(lang: Lang, text: &Text, link: Option<&str>, account: Option<&str>) -> String {
    let mut out = format!("{}\n", text.body);
    if let Some(l) = link {
        out.push_str(&format!("\n{l}\n"));
    }
    if let Some(a) = account {
        out.push_str(&match lang {
            Lang::De => format!("\n-- \nAlarme ändern oder abschalten: {a}\n"),
            Lang::En => format!("\n-- \nChange or turn off alarms: {a}\n"),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-10-12 10:00 UTC, a Monday: 12:00 in Berlin
    const NOW: i64 = 1_791_799_200;
    const H: i64 = 3_600;

    fn ctx(lang: Lang) -> Ctx<'static> {
        Ctx {
            lang,
            tz: chrono_tz::Europe::Berlin,
            unit: WindUnit::Kn,
            spot: "Baggersee",
        }
    }

    fn window(start: i64) -> Window {
        Window {
            start,
            end: start + 4 * H,
            peak_kn: 18.6,
            peak_at: start + H,
            dir: 240.0,
        }
    }

    #[test]
    fn writes_the_window_in_local_time() {
        let wing = GearItem {
            kind: GearKind::Wing,
            size: 4.0,
            min_kn: 17.0,
            max_kn: 27.0,
        };
        // Tomorrow 12–16 UTC is 14–18 in Berlin (summer time)
        let w = window(NOW + 26 * H);
        let de = ctx(Lang::De).notice(&Notice::New(w), Some(&wing), NOW);
        assert_eq!(de.title, "Baggersee: Wind morgen");
        assert_eq!(de.body, "14–18 Uhr, bis 19 kn aus WSW · Wing 4");
        let en = ctx(Lang::En).notice(&Notice::Gone(w), None, NOW);
        assert_eq!(en.title, "Baggersee: no wind tomorrow after all");
        assert_eq!(
            en.body,
            "The window 14:00–18:00 has gone from the forecast."
        );
        let later = Window {
            start: w.start + 2 * H,
            end: w.end + 2 * H,
            ..w
        };
        let changed = ctx(Lang::De).notice(
            &Notice::Changed {
                before: w,
                now: later,
            },
            None,
            NOW,
        );
        assert_eq!(
            changed.body,
            "Jetzt 16–20 Uhr, bis 19 kn aus WSW (vorher 14–18 Uhr, bis 19 kn)"
        );
        // In three days: the weekday; in m/s
        let ms = Ctx {
            unit: WindUnit::Ms,
            ..ctx(Lang::De)
        };
        let thu = ms.notice(&Notice::New(window(NOW + 74 * H)), None, NOW);
        assert_eq!(
            (thu.title.as_str(), thu.body.as_str()),
            (
                "Baggersee: Wind Donnerstag",
                "14–18 Uhr, bis 10 m/s aus WSW"
            )
        );
        assert_eq!(ctx(Lang::De).day(NOW + 11 * H), 20261012);
        assert_eq!(ctx(Lang::De).day(NOW + 12 * H), 20261013);
    }

    #[test]
    fn writes_the_live_alarm() {
        let t = ctx(Lang::En).live(16.2, Some(23.0), Some(90.0), None);
        assert_eq!(
            (t.title.as_str(), t.body.as_str()),
            (
                "Baggersee: it's blowing!",
                "Measured 16 kn, gusts 23 kn from E"
            )
        );
        assert_eq!(
            ctx(Lang::De).live(16.2, None, None, None).body,
            "Gemessen 16 kn"
        );
    }
}
