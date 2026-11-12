use crate::{model::*, Error, Result};
use serde::Deserialize;

const BASE: &str = "https://api.jamendo.com/v3.0/tracks/";

#[derive(Deserialize)]
struct Resp {
    headers: Headers,
    results: Vec<Raw>,
}
#[derive(Deserialize)]
struct Headers {
    status: String,
    #[serde(default)]
    error_message: String,
}
#[derive(Deserialize)]
struct Raw {
    id: String,
    name: String,
    #[serde(default)]
    artist_name: String,
    #[serde(default)]
    album_name: String,
    #[serde(default)]
    image: String,
    #[serde(default)]
    audio: String,
    #[serde(default)]
    audiodownload: String,
    #[serde(default)]
    audiodownload_allowed: bool,
    #[serde(default)]
    duration: u32,
    #[serde(default)]
    license_ccurl: String,
    musicinfo: Option<MusicInfo>,
    stats: Option<serde_json::Value>,
}
#[derive(Deserialize, Default)]
struct MusicInfo {
    #[serde(default)]
    tags: Tags,
}
#[derive(Deserialize, Default)]
struct Tags {
    #[serde(default)]
    genres: Vec<String>,
    #[serde(default)]
    instruments: Vec<String>,
    #[serde(default)]
    vartags: Vec<String>,
}

/// число из JSON-значения (число или строка)
fn num(v: &serde_json::Value) -> f64 {
    v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or(0.0)
}

pub fn parse(body: &str) -> Result<Vec<Track>> {
    let r: Resp = serde_json::from_str(body)?;
    if r.headers.status != "success" {
        return Err(Error::Msg(format!("Jamendo: {}", r.headers.error_message)));
    }
    Ok(r.results
        .into_iter()
        .map(|t| {
            let mi = t.musicinfo.unwrap_or_default();
            let mut tags = mi.tags.genres;
            tags.extend(mi.tags.vartags);
            let _ = mi.tags.instruments;
            Track {
                id: format!("jamendo:{}", t.id),
                source: "jamendo".into(),
                kind: "music".into(),
                url: String::new(),
                name: t.name,
                artist: t.artist_name,
                album: t.album_name,
                image: t.image,
                audio: t.audio,
                download_url: t.audiodownload,
                download_allowed: t.audiodownload_allowed,
                duration: t.duration,
                tags,
                license_url: t.license_ccurl,
                pop: t.stats.as_ref().map_or(0.0, |s| num(&s["rate_listened_total"])),
            }
        })
        .collect())
}

pub async fn search(client_id: &str, p: &SearchParams) -> Result<Vec<Track>> {
    if client_id.trim().is_empty() {
        return Err(Error::Msg("Не указан Jamendo client_id (Настройки)".into()));
    }
    let mut q: Vec<(&str, String)> = vec![
        ("client_id", client_id.trim().into()),
        ("format", "json".into()),
        ("limit", p.limit.clamp(1, 50).to_string()),
        ("offset", p.offset.to_string()),
        ("include", "musicinfo stats".into()),
        ("imagesize", "200".into()),
        ("audioformat", p.audio_format.clone().unwrap_or_else(|| "mp32".into())),
        ("order", p.order.clone().unwrap_or_else(|| "popularity_total".into())),
    ];
    if let Some(s) = p.query.as_ref().filter(|s| !s.trim().is_empty()) {
        q.push(("search", s.trim().into()));
        q.retain(|(k, _)| *k != "order");
        q.push(("order", "relevance".into()));
    }
    if !p.tags.is_empty() {
        q.push(("fuzzytags", p.tags.join(" ")));
    }
    if let Some(l) = p.lang.as_ref().filter(|s| !s.is_empty()) {
        q.push(("lang", l.clone()));
    }
    let body = crate::http().get(BASE).query(&q).send().await?.text().await?;
    parse(&body)
}

fn key_check(client_id: &str) -> Result<()> {
    if client_id.trim().is_empty() {
        return Err(Error::Msg("Не указан Jamendo client_id (Настройки)".into()));
    }
    Ok(())
}

pub fn parse_playlists(body: &str) -> Result<Vec<PlaylistRef>> {
    let v: serde_json::Value = serde_json::from_str(body)?;
    if v["headers"]["status"] != "success" {
        return Err(Error::Msg(format!("Jamendo: {}", v["headers"]["error_message"].as_str().unwrap_or("ошибка"))));
    }
    let s = |x: &serde_json::Value| x.as_str().unwrap_or("").to_string();
    Ok(v["results"].as_array().cloned().unwrap_or_default().iter().map(|p| PlaylistRef {
        id: format!("jamendo:{}", s(&p["id"])),
        source: "jamendo".into(),
        title: s(&p["name"]),
        owner: s(&p["user_name"]),
        image: String::new(),
        count: 0,
    }).collect())
}

pub fn parse_playlist_tracks(body: &str) -> Result<Vec<Track>> {
    let v: serde_json::Value = serde_json::from_str(body)?;
    if v["headers"]["status"] != "success" {
        return Err(Error::Msg(format!("Jamendo: {}", v["headers"]["error_message"].as_str().unwrap_or("ошибка"))));
    }
    let s = |x: &serde_json::Value| x.as_str().unwrap_or("").to_string();
    let tracks = v["results"][0]["tracks"].as_array().cloned().unwrap_or_default();
    Ok(tracks.iter().map(|t| Track {
        id: format!("jamendo:{}", s(&t["id"])),
        source: "jamendo".into(),
        kind: "music".into(),
        url: String::new(),
        name: s(&t["name"]),
        artist: s(&t["artist_name"]),
        album: s(&t["album_name"]),
        image: if t["image"].is_string() { s(&t["image"]) } else { s(&t["album_image"]) },
        audio: s(&t["audio"]),
        download_url: s(&t["audiodownload"]),
        download_allowed: !s(&t["audiodownload"]).is_empty(),
        duration: t["duration"].as_u64().unwrap_or(0) as u32,
        tags: vec![],
        license_url: s(&t["license_ccurl"]),
        pop: 0.0,
    }).collect())
}

pub async fn search_playlists(client_id: &str, q: &str) -> Result<Vec<PlaylistRef>> {
    key_check(client_id)?;
    let body = crate::http()
        .get("https://api.jamendo.com/v3.0/playlists/")
        .query(&[("client_id", client_id.trim()), ("format", "json"), ("limit", "20"), ("namesearch", q)])
        .send().await?.text().await?;
    parse_playlists(&body)
}

pub async fn playlist_tracks(client_id: &str, id: &str) -> Result<Vec<Track>> {
    key_check(client_id)?;
    let body = crate::http()
        .get("https://api.jamendo.com/v3.0/playlists/tracks/")
        .query(&[("client_id", client_id.trim()), ("format", "json"), ("id", id), ("audioformat", "mp32")])
        .send().await?.text().await?;
    parse_playlist_tracks(&body)
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_playlists() {
        let j = r#"{"headers":{"status":"success"},"results":[{"id":"9","name":"P","user_name":"U"}]}"#;
        assert_eq!(super::parse_playlists(j).unwrap()[0].id, "jamendo:9");
        let t = r#"{"headers":{"status":"success"},"results":[{"tracks":[{"id":"1","name":"A","artist_name":"B","audio":"u","audiodownload":"d","duration":5}]}]}"#;
        let v = super::parse_playlist_tracks(t).unwrap();
        assert_eq!(v[0].id, "jamendo:1");
        assert!(v[0].download_allowed);
    }
    #[test]
    fn parses() {
        let j = r#"{"headers":{"status":"success"},"results":[{"id":"1","name":"A","artist_name":"B",
        "audio":"u","audiodownload_allowed":true,"duration":10,
        "musicinfo":{"tags":{"genres":["rock"],"instruments":[],"vartags":["happy"]}},"stats":{"rate_listened_total":"1234"}}]}"#;
        let t = super::parse(j).unwrap();
        assert_eq!(t[0].tags, vec!["rock", "happy"]);
        assert!(t[0].download_allowed);
        assert_eq!(t[0].pop, 1234.0);
    }
}

/// Поиск исполнителей Jamendo по имени (для подсказок при выборе любимых).
pub async fn artists(client_id: &str, q: &str) -> Result<String> {
    key_check(client_id)?;
    Ok(crate::http()
        .get("https://api.jamendo.com/v3.0/artists/")
        .query(&[("client_id", client_id.trim()), ("format", "json"), ("limit", "6"), ("namesearch", q), ("imagesize", "100")])
        .send().await?.text().await?)
}
