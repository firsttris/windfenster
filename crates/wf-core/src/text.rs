use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// Lower case without accents, for search: "Rheinmünster" → "rheinmunster", "Málaga" → "malaga".
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.nfd().filter(|c| !is_combining_mark(*c)) {
        match c {
            'ß' => out.push_str("ss"),
            'æ' | 'Æ' => out.push_str("ae"),
            'ø' | 'Ø' => out.push('o'),
            'ł' | 'Ł' => out.push('l'),
            'đ' | 'Đ' => out.push('d'),
            'þ' | 'Þ' => out.push_str("th"),
            c => out.extend(c.to_lowercase()),
        }
    }
    out
}

/// Short words that are names, not abbreviations.
const SHORT_WORDS: [&str; 7] = ["BAD", "NEU", "OST", "EMS", "AUE", "ALT", "WEST"];
/// Words written small inside a name.
const SMALL_WORDS: [&str; 8] = ["AM", "AN", "IM", "DER", "DIE", "DAS", "DEN", "BEI"];

/// Names as PEGELONLINE writes them, in upper case, made readable: "FRANKFURT AM MAIN" →
/// "Frankfurt am Main", "BERLIN-PLÖTZENSEE UP" → "Berlin-Plötzensee UP". Abbreviations of up
/// to three letters (UP, OW, DFH) stay as they are.
pub fn title_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut word = String::new();
    let mut first = true;
    let flush = |word: &mut String, out: &mut String, first: &mut bool| {
        if word.is_empty() {
            return;
        }
        let upper = word.chars().all(|c| !c.is_lowercase());
        if !upper {
            out.push_str(word);
        } else if !*first && SMALL_WORDS.contains(&word.as_str()) {
            out.push_str(&word.to_lowercase());
        } else if word.chars().count() <= 3 && !SHORT_WORDS.contains(&word.as_str()) {
            out.push_str(word);
        } else {
            let mut chars = word.chars();
            if let Some(c) = chars.next() {
                out.push(c);
                out.extend(chars.flat_map(char::to_lowercase));
            }
        }
        word.clear();
        *first = false;
    };
    for c in s.trim().chars() {
        if c.is_alphanumeric() {
            word.push(c);
        } else {
            flush(&mut word, &mut out, &mut first);
            out.push(c);
        }
    }
    flush(&mut word, &mut out, &mut first);
    out
}

#[cfg(test)]
mod tests {
    use super::{fold, title_case};

    #[test]
    fn title_cases_gauge_names() {
        // Names from PEGELONLINE's station list (2026-10-11).
        assert_eq!(title_case("IFFEZHEIM"), "Iffezheim");
        assert_eq!(title_case("BERLIN-PLÖTZENSEE UP"), "Berlin-Plötzensee UP");
        assert_eq!(
            title_case("PASSAU LUITPOLDBRÜCKE DFH"),
            "Passau Luitpoldbrücke DFH"
        );
        assert_eq!(title_case("BAD EMS"), "Bad Ems");
        assert_eq!(title_case("FRANKFURT AM MAIN"), "Frankfurt am Main");
        assert_eq!(
            title_case("UNTERE HAVEL-WASSERSTRASSE"),
            "Untere Havel-Wasserstrasse"
        );
        assert_eq!(title_case("GW-PWDRFP 02"), "GW-Pwdrfp 02");
        assert_eq!(title_case("Konstanz"), "Konstanz");
    }

    #[test]
    fn folds_accents_and_case() {
        assert_eq!(fold("Rheinmünster"), "rheinmunster");
        assert_eq!(fold("Málaga-Costa del Sol"), "malaga-costa del sol");
        assert_eq!(fold("Großenkneten"), "grossenkneten");
        assert_eq!(fold("Københavns Lufthavn"), "kobenhavns lufthavn");
        assert_eq!(fold("Łódź"), "lodz");
        assert_eq!(fold("EDSB"), "edsb");
    }
}
