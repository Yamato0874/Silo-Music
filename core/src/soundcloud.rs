//! Поиск через официальный SoundCloud API (client_credentials). Воспроизведение — только через Widget API (в интерфейсе).
use crate::{model::*, Error, Result};
use serde_json::Value;

pub fn parse(body: &str) -> Result<Vec<Track>> {
    let v: Value = serde_json::from_str(body)?;
    if let Some(m) = v["error"].as_str().or(v["errors"][0]["error_message"].as_str()) {
        return Err(Error::Msg(format!("SoundCloud: {m}")));
    }
    let arr = v.as_array().or(v["collection"].as_array()).cloned().unwrap_or_default();
    let s = |x: &Value| x.as_str().unwrap_or("").to_string();
    Ok(arr
        .iter()
        .filter(|t| t["access"].as_str() != Some("blocked") && t["permalink_url"].is_string())
        .map(|t| {
            let mut tags: Vec<String> = vec![];
            if let Some(g) = t["genre"].as_str().filter(|g| !g.is_empty()) {
                tags.push(g.to_lowercase());
            }
            tags.extend(
                s(&t["tag_list"]).split_whitespace().filter(|x| !x.starts_with('"')).take(5).map(|x| x.to_lowercase()),
            );
            Track {
                id: format!("soundcloud:{}", t["id"]),
                source: "soundcloud".into(),
                kind: "music".into(),
                url: s(&t["permalink_url"]),
                name: s(&t["title"]),
                artist: s(&t["user"]["username"]),
                album: String::new(),
                image: s(&t["artwork_url"]),
                audio: String::new(),
                download_url: String::new(),
                download_allowed: false,
                duration: (t["duration"].as_u64().unwrap_or(0) / 1000) as u32,
                tags,
                license_url: String::new(),
                pop: t["playback_count"].as_f64().unwrap_or(0.0),
            }
        })
        .collect())
}

async fn token(id: &str, secret: &str) -> Result<String> {
    let v: Value = crate::http()
        .post("https://secure.soundcloud.com/oauth/token")
        .form(&[("grant_type", "client_credentials"), ("client_id", id), ("client_secret", secret)])
        .send()
        .await?
        .json()
        .await?;
    v["access_token"].as_str().map(String::from).ok_or_else(|| Error::Msg("SoundCloud: не удалось получить токен (проверьте client_id/secret)".into()))
}

pub async fn search(client_id: &str, client_secret: &str, query: &str, limit: u32) -> Result<Vec<Track>> {
    if client_id.trim().is_empty() || client_secret.trim().is_empty() {
        return Err(Error::Msg("не указаны ключи SoundCloud (Настройки)".into()));
    }
    let tok = token(client_id.trim(), client_secret.trim()).await?;
    let body = crate::http()
        .get("https://api.soundcloud.com/tracks")
        .header("Authorization", format!("OAuth {tok}"))
        .query(&[("q", query), ("limit", &limit.clamp(1, 50).to_string()), ("access", "playable")])
        .send()
        .await?
        .text()
        .await?;
    parse(&body)
}

async fn api_get(client_id: &str, client_secret: &str, path: &str, q: &[(&str, &str)]) -> Result<String> {
    if client_id.trim().is_empty() || client_secret.trim().is_empty() {
        return Err(Error::Msg("не указаны ключи SoundCloud (Настройки)".into()));
    }
    let tok = token(client_id.trim(), client_secret.trim()).await?;
    Ok(crate::http()
        .get(format!("https://api.soundcloud.com/{path}"))
        .header("Authorization", format!("OAuth {tok}"))
        .query(q)
        .send().await?.text().await?)
}

pub fn parse_playlists(body: &str) -> Result<Vec<PlaylistRef>> {
    let v: Value = serde_json::from_str(body)?;
    if let Some(m) = v["error"].as_str() {
        return Err(Error::Msg(format!("SoundCloud: {m}")));
    }
    let arr = v.as_array().or(v["collection"].as_array()).cloned().unwrap_or_default();
    let s = |x: &Value| x.as_str().unwrap_or("").to_string();
    Ok(arr.iter().map(|p| PlaylistRef {
        id: format!("soundcloud:{}", p["id"]), source: "soundcloud".into(), title: s(&p["title"]),
        owner: s(&p["user"]["username"]), image: s(&p["artwork_url"]), count: p["track_count"].as_u64().unwrap_or(0) as u32,
    }).collect())
}
pub async fn search_playlists(id: &str, secret: &str, q: &str) -> Result<Vec<PlaylistRef>> {
    parse_playlists(&api_get(id, secret, "playlists", &[("q", q), ("limit", "20")]).await?)
}
/// Публичная ссылка на плейлист SoundCloud → его числовой id.
pub async fn resolve_playlist_id(id: &str, secret: &str, url: &str) -> Result<String> {
    let v: Value = serde_json::from_str(&api_get(id, secret, "resolve", &[("url", url)]).await?)?;
    if v["kind"].as_str() == Some("playlist") {
        if let Some(n) = v["id"].as_u64() { return Ok(n.to_string()); }
    }
    Err(Error::Msg("это не ссылка на плейлист SoundCloud".into()))
}
pub async fn playlist_tracks(id: &str, secret: &str, pl: &str) -> Result<Vec<Track>> {
    parse(&api_get(id, secret, &format!("playlists/{pl}/tracks"), &[("limit", "100")]).await?)
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_playlists() {
        let j = r#"[{"id":3,"title":"S","user":{"username":"U"},"artwork_url":null,"track_count":12}]"#;
        let p = super::parse_playlists(j).unwrap();
        assert_eq!((p[0].id.as_str(), p[0].count), ("soundcloud:3", 12));
    }
    #[test]
    fn parses() {
        let j = r#"[{"id":5,"title":"T","user":{"username":"U"},"artwork_url":null,"permalink_url":"https://soundcloud.com/u/t",
        "duration":61000,"playback_count":5000,"genre":"Hip-Hop","tag_list":"rap \"hard beat\" russian","access":"playable"},
        {"id":6,"title":"X","permalink_url":"https://soundcloud.com/u/x","access":"blocked"}]"#;
        let t = super::parse(j).unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].duration, 61);
        assert_eq!(t[0].tags[0], "hip-hop");
        assert_eq!(t[0].id, "soundcloud:5");
        assert_eq!(t[0].pop, 5000.0);
    }
}

/// Поиск исполнителей (пользователей SoundCloud) для подсказок при выборе любимых.
pub async fn users(client_id: &str, client_secret: &str, q: &str) -> Result<String> {
    api_get(client_id, client_secret, "users", &[("q", q), ("limit", "6")]).await
}
