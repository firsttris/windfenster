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

#[cfg(test)]
mod tests {
    use super::fold;

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
