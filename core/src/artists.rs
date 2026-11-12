//! Подсказки исполнителей для выбора любимых. Источники: локальная библиотека (в приложении), Jamendo, SoundCloud и открытая база MusicBrainz.
//! Известного артиста может не быть в свободных источниках — тогда он служит только сигналом вкуса: его теги (жанры) берутся из MusicBrainz
//! и сопоставляются с таксономией, а треки подбираются по этим жанрам, а не по самому артисту.
use crate::{taxonomy, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ArtistHit {
    pub name: String,
    /// library | jamendo | soundcloud | musicbrainz
    pub source: String,
    #[serde(default)]
    pub image: String,
    /// id узлов таксономии (жанры исполнителя), только известные
    #[serde(default)]
    pub tags: Vec<String>,
    /// короткое пояснение (например, «британская группа»)
    #[serde(default)]
    pub note: String,
}

/// Теги → id таксономии: только настоящие жанры/настроения, без повторов, не более четырёх.
pub fn known_tags(raw: &[String]) -> Vec<String> {
    let tax = taxonomy::get();
    let mut out: Vec<String> = vec![];
    for t in tax.resolve(raw) {
        if tax.node(&t).map_or(false, |n| matches!(n.kind.as_str(), "genre" | "family" | "mood")) && !out.contains(&t) { out.push(t); }
        if out.len() == 4 { break; }
    }
    out
}

pub fn parse_musicbrainz(body: &str) -> Result<Vec<ArtistHit>> {
    let v: Value = serde_json::from_str(body)?;
    Ok(v["artists"].as_array().cloned().unwrap_or_default().iter().filter_map(|a| {
        let name = a["name"].as_str()?.to_string();
        let mut tags: Vec<(i64, String)> = a["tags"].as_array().cloned().unwrap_or_default().iter()
            .filter_map(|t| Some((t["count"].as_i64().unwrap_or(0), t["name"].as_str()?.to_string()))).collect();
        tags.sort_by(|x, y| y.0.cmp(&x.0));
        let raw: Vec<String> = tags.into_iter().map(|x| x.1).collect();
        Some(ArtistHit { name, source: "musicbrainz".into(), image: String::new(), tags: known_tags(&raw), note: a["disambiguation"].as_str().unwrap_or("").to_string() })
    }).collect())
}

pub fn parse_jamendo(body: &str) -> Result<Vec<ArtistHit>> {
    let v: Value = serde_json::from_str(body)?;
    Ok(v["results"].as_array().cloned().unwrap_or_default().iter().filter_map(|a| Some(ArtistHit {
        name: a["name"].as_str()?.to_string(), source: "jamendo".into(), image: a["image"].as_str().unwrap_or("").into(), ..Default::default()
    })).collect())
}

pub fn parse_soundcloud(body: &str) -> Result<Vec<ArtistHit>> {
    let v: Value = serde_json::from_str(body)?;
    let arr = v.as_array().or(v["collection"].as_array()).cloned().unwrap_or_default();
    Ok(arr.iter().filter_map(|a| Some(ArtistHit {
        name: a["username"].as_str()?.to_string(), source: "soundcloud".into(), image: a["avatar_url"].as_str().unwrap_or("").into(), ..Default::default()
    })).collect())
}

/// MusicBrainz (открытые данные, без ключа). Идентификатор клиента — общий, без личных данных. Не чаще одного запроса в секунду (вызывается по паузе ввода).
pub async fn musicbrainz(q: &str) -> Result<String> {
    Ok(crate::http()
        .get("https://musicbrainz.org/ws/2/artist/")
        .query(&[("query", q), ("fmt", "json"), ("limit", "6")])
        .send().await?.text().await?)
}

/// Склеивает подсказки: одно имя — одна запись; приоритет у той, что принесла жанры, затем у локальной и свободных источников.
pub fn merge(lists: Vec<Vec<ArtistHit>>, limit: usize) -> Vec<ArtistHit> {
    let rank = |s: &str| match s { "library" => 0, "jamendo" => 1, "soundcloud" => 2, _ => 3 };
    let mut out: Vec<ArtistHit> = vec![];
    for h in lists.into_iter().flatten() {
        let key = h.name.trim().to_lowercase();
        match out.iter_mut().find(|x| x.name.trim().to_lowercase() == key) {
            Some(x) => {
                if x.tags.is_empty() { x.tags = h.tags.clone(); }
                if x.image.is_empty() { x.image = h.image.clone(); }
                if x.note.is_empty() { x.note = h.note.clone(); }
                if rank(&h.source) < rank(&x.source) { x.source = h.source; }
            }
            None => out.push(h),
        }
    }
    out.truncate(limit);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn musicbrainz_tags_become_taxonomy_genres() {
        let body = r#"{"artists":[{"name":"Massive Attack","disambiguation":"UK trip hop group","tags":[{"name":"trip hop","count":30},{"name":"electronic","count":20},{"name":"seen live","count":50},{"name":"british","count":9}]}]}"#;
        let h = parse_musicbrainz(body).unwrap();
        assert_eq!(h[0].name, "Massive Attack");
        assert_eq!(h[0].tags, vec!["trip_hop".to_string(), "electronic".to_string()]);
        assert_eq!(h[0].source, "musicbrainz");
    }
    #[test]
    fn free_source_parsers() {
        let j = parse_jamendo(r#"{"results":[{"name":"Kai Engel","image":"http://x/i.jpg"}]}"#).unwrap();
        assert_eq!((j[0].name.as_str(), j[0].source.as_str()), ("Kai Engel", "jamendo"));
        let s = parse_soundcloud(r#"[{"username":"Lofi Girl","avatar_url":"http://a"}]"#).unwrap();
        assert_eq!(s[0].source, "soundcloud");
        assert!(parse_jamendo("{}").unwrap().is_empty());
    }
    #[test]
    fn merge_dedupes_and_keeps_genres_from_any_source() {
        let lib = vec![ArtistHit { name: "Kai Engel".into(), source: "library".into(), ..Default::default() }];
        let mb = vec![ArtistHit { name: "kai engel".into(), source: "musicbrainz".into(), tags: vec!["ambient_classic".into()], ..Default::default() },
                      ArtistHit { name: "Radiohead".into(), source: "musicbrainz".into(), tags: vec!["alt_rock".into()], ..Default::default() }];
        let m = merge(vec![lib, mb], 10);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].source, "library");
        assert_eq!(m[0].tags, vec!["ambient_classic".to_string()]);
    }
}
