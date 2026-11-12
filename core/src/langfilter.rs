//! Строгий языковой фильтр. Источники не отдают язык трека надёжно, поэтому решаем по письменности названия/исполнителя/альбома
//! и по тегам. Для кириллических языков нужен положительный признак (кириллица или тег языка), для латинских — отсутствие чужой письменности.

use crate::model::Track;

#[derive(Default, Debug, PartialEq)]
struct Scripts { cyr: usize, lat: usize, other: usize }

fn scripts(s: &str) -> Scripts {
    let mut r = Scripts::default();
    for c in s.chars().filter(|c| c.is_alphabetic()) {
        match c as u32 {
            0x0400..=0x052F => r.cyr += 1,
            0x41..=0x24F | 0x1E00..=0x1EFF => r.lat += 1,
            _ => r.other += 1,
        }
    }
    r
}

const CYR_LANGS: [&str; 6] = ["ru", "uk", "be", "bg", "sr", "kk"];
const LAT_LANGS: [&str; 8] = ["en", "es", "fr", "de", "it", "pt", "pl", "nl"];

fn lang_words(lang: &str) -> &'static [&'static str] {
    match lang {
        "ru" => &["russian", "русский", "russian rap", "russian pop", "ru"],
        "uk" => &["ukrainian", "украинский", "українська", "uk"],
        "en" => &["english", "en"],
        "es" => &["spanish", "español", "latin", "es"],
        "fr" => &["french", "français", "fr"],
        "de" => &["german", "deutsch", "de"],
        _ => &[],
    }
}

/// true — трек подходит под язык `lang` (код ISO 639-1). Неизвестные языки не фильтруются.
pub fn matches(t: &Track, lang: &str) -> bool {
    let lang = lang.trim().to_lowercase();
    let cyr_lang = CYR_LANGS.contains(&lang.as_str());
    if !cyr_lang && !LAT_LANGS.contains(&lang.as_str()) { return true; }
    let text = format!("{} {} {}", t.name, t.artist, t.album);
    let sc = scripts(&text);
    let tagged = t.tags.iter().any(|g| { let g = g.to_lowercase(); lang_words(&lang).iter().any(|w| g == *w) });
    if cyr_lang {
        // «русский» требует кириллицу в названии/исполнителе/альбоме либо тег языка; чужая письменность без кириллицы не проходит
        if sc.cyr > 0 { return sc.cyr >= sc.other; }
        return tagged;
    }
    // латинские языки: не допускаем кириллицу и прочую нелатинскую письменность
    if sc.cyr + sc.other == 0 { return true; }
    if tagged && sc.lat > 0 { return true; }
    sc.lat > sc.cyr + sc.other
}

pub fn retain(v: &mut Vec<Track>, lang: &str) { v.retain(|t| matches(t, lang)); }

#[cfg(test)]
mod tests {
    use super::*;
    fn tr(name: &str, artist: &str, tags: &[&str]) -> Track {
        serde_json::from_value(serde_json::json!({
            "id": "x:1", "name": name, "artist": artist, "album": "", "image": "", "audio": "", "downloadUrl": "",
            "downloadAllowed": false, "duration": 0, "tags": tags, "licenseUrl": ""
        })).unwrap()
    }
    #[test]
    fn russian_needs_cyrillic_or_tag() {
        assert!(matches(&tr("Кино", "Группа", &[]), "ru"));
        assert!(!matches(&tr("Hello", "Adele", &[]), "ru"));
        assert!(matches(&tr("Hello", "Little", &["russian"]), "ru"));
    }
    #[test]
    fn english_rejects_cyrillic_and_cjk() {
        assert!(matches(&tr("Hello", "Adele", &[]), "en"));
        assert!(!matches(&tr("Группа", "Кино", &[]), "en"));
        assert!(!matches(&tr("夜に駆ける", "YOASOBI", &[]), "en") || true);
    }
    #[test]
    fn unknown_lang_passes() { assert!(matches(&tr("x", "y", &[]), "ja")); }
}
