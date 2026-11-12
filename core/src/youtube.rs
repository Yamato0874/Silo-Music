//! Поиск через официальный YouTube Data API v3. Воспроизведение — только через IFrame Player API (в интерфейсе).
use crate::{model::*, Error, Result};
use serde_json::Value;

pub fn unescape(s: &str) -> String {
    s.replace("&amp;", "&").replace("&#39;", "'").replace("&quot;", "\"").replace("&lt;", "<").replace("&gt;", ">")
}

pub fn parse(body: &str) -> Result<Vec<Track>> {
    let v: Value = serde_json::from_str(body)?;
    if let Some(m) = v["error"]["message"].as_str() {
        return Err(Error::Msg(format!("YouTube: {m}")));
    }
    let s = |x: &Value| x.as_str().unwrap_or("").to_string();
    Ok(v["items"]
        .as_array()
        .map(|a| a.as_slice())
        .unwrap_or(&[])
        .iter()
        .filter_map(|it| {
            let vid = it["id"]["videoId"].as_str()?.to_string();
            let sn = &it["snippet"];
            Some(Track {
                id: format!("youtube:{vid}"),
                source: "youtube".into(),
                kind: "music".into(),
                url: format!("https://www.youtube.com/watch?v={vid}"),
                name: unescape(&s(&sn["title"])),
                artist: unescape(&s(&sn["channelTitle"])),
                album: String::new(),
                image: s(&sn["thumbnails"]["medium"]["url"]),
                audio: String::new(),
                download_url: String::new(),
                download_allowed: false,
                duration: 0,
                tags: vec![],
                license_url: String::new(),
                pop: 0.0,
            })
        })
        .collect())
}

/// ISO 8601 (PT1H2M3S) → секунды
pub fn iso_duration(s: &str) -> u32 {
    let (mut n, mut tot) = (0u32, 0u32);
    for c in s.trim_start_matches("PT").chars() {
        match c {
            '0'..='9' => n = n * 10 + c.to_digit(10).unwrap(),
            'H' => { tot += n * 3600; n = 0 }
            'M' => { tot += n * 60; n = 0 }
            'S' => { tot += n; n = 0 }
            _ => {}
        }
    }
    tot
}

/// Добавляет просмотры и длительность из videos.list (statistics, contentDetails).
pub fn apply_stats(tracks: &mut [Track], body: &str) {
    let Ok(v) = serde_json::from_str::<Value>(body) else { return };
    for it in v["items"].as_array().cloned().unwrap_or_default() {
        let Some(id) = it["id"].as_str() else { continue };
        if let Some(t) = tracks.iter_mut().find(|t| t.id == format!("youtube:{id}")) {
            t.pop = it["statistics"]["viewCount"].as_str().and_then(|x| x.parse().ok()).unwrap_or(0.0);
            t.duration = iso_duration(it["contentDetails"]["duration"].as_str().unwrap_or(""));
        }
    }
}

pub async fn search(api_key: &str, query: &str, limit: u32) -> Result<Vec<Track>> {
    if api_key.trim().is_empty() {
        return Err(Error::Msg("не указан ключ YouTube Data API (Настройки)".into()));
    }
    let q = [
        ("part", "snippet"), ("type", "video"), ("videoCategoryId", "10"), ("videoEmbeddable", "true"),
        ("maxResults", &limit.clamp(1, 50).to_string()), ("q", query), ("key", api_key.trim()),
    ];
    let body = reqwest::Client::new()
        .get("https://www.googleapis.com/youtube/v3/search")
        .query(&q)
        .send()
        .await?
        .text()
        .await?;
    let mut tracks = parse(&body)?;
    if !tracks.is_empty() {
        let ids = tracks.iter().map(|t| t.id.replace("youtube:", "")).collect::<Vec<_>>().join(",");
        if let Ok(b) = get("videos", &[("part", "statistics,contentDetails"), ("id", &ids), ("key", api_key.trim())]).await {
            apply_stats(&mut tracks, &b);
        }
    }
    Ok(tracks)
}

pub fn parse_playlists(body: &str) -> Result<Vec<PlaylistRef>> {
    let v: Value = serde_json::from_str(body)?;
    if let Some(m) = v["error"]["message"].as_str() {
        return Err(Error::Msg(format!("YouTube: {m}")));
    }
    let s = |x: &Value| x.as_str().unwrap_or("").to_string();
    Ok(v["items"].as_array().cloned().unwrap_or_default().iter().filter_map(|it| {
        let id = it["id"]["playlistId"].as_str()?;
        let sn = &it["snippet"];
        Some(PlaylistRef {
            id: format!("youtube:{id}"), source: "youtube".into(), title: unescape(&s(&sn["title"])),
            owner: unescape(&s(&sn["channelTitle"])), image: s(&sn["thumbnails"]["medium"]["url"]), count: 0,
        })
    }).collect())
}

pub fn parse_items(body: &str) -> Result<Vec<Track>> {
    let v: Value = serde_json::from_str(body)?;
    if let Some(m) = v["error"]["message"].as_str() {
        return Err(Error::Msg(format!("YouTube: {m}")));
    }
    let s = |x: &Value| x.as_str().unwrap_or("").to_string();
    Ok(v["items"].as_array().cloned().unwrap_or_default().iter().filter_map(|it| {
        let sn = &it["snippet"];
        let vid = sn["resourceId"]["videoId"].as_str()?;
        let title = unescape(&s(&sn["title"]));
        if title == "Private video" || title == "Deleted video" { return None; }
        Some(Track {
            id: format!("youtube:{vid}"), source: "youtube".into(), kind: "music".into(),
            url: format!("https://www.youtube.com/watch?v={vid}"), name: title,
            artist: unescape(&s(&sn["videoOwnerChannelTitle"])).trim_end_matches(" - Topic").to_string(),
            album: String::new(), image: s(&sn["thumbnails"]["medium"]["url"]), audio: String::new(),
            download_url: String::new(), download_allowed: false, duration: 0, tags: vec![], license_url: String::new(), pop: 0.0,
        })
    }).collect())
}

async fn get(path: &str, q: &[(&str, &str)]) -> Result<String> {
    Ok(reqwest::Client::new().get(format!("https://www.googleapis.com/youtube/v3/{path}")).query(q).send().await?.text().await?)
}
fn need_key(k: &str) -> Result<()> {
    if k.trim().is_empty() { return Err(Error::Msg("не указан ключ YouTube Data API (Настройки)".into())); }
    Ok(())
}
pub async fn search_playlists(key: &str, q: &str) -> Result<Vec<PlaylistRef>> {
    need_key(key)?;
    parse_playlists(&get("search", &[("part", "snippet"), ("type", "playlist"), ("maxResults", "20"), ("q", q), ("key", key.trim())]).await?)
}
pub async fn playlist_items(key: &str, id: &str) -> Result<Vec<Track>> {
    need_key(key)?;
    parse_items(&get("playlistItems", &[("part", "snippet"), ("maxResults", "50"), ("playlistId", id), ("key", key.trim())]).await?)
}

#[cfg(test)]
mod tests {
    #[test]
    fn stats_add_views_and_duration() {
        assert_eq!(super::iso_duration("PT1H2M3S"), 3723);
        assert_eq!(super::iso_duration("PT4M"), 240);
        let mut t = super::parse(r#"{"items":[{"id":{"videoId":"abc"},"snippet":{"title":"A"}}]}"#).unwrap();
        super::apply_stats(&mut t, r#"{"items":[{"id":"abc","statistics":{"viewCount":"9000"},"contentDetails":{"duration":"PT3M30S"}}]}"#);
        assert_eq!((t[0].pop, t[0].duration), (9000.0, 210));
    }
    #[test]
    fn parses_playlists() {
        let j = r#"{"items":[{"id":{"playlistId":"PL1"},"snippet":{"title":"X","channelTitle":"C","thumbnails":{"medium":{"url":"u"}}}}]}"#;
        assert_eq!(super::parse_playlists(j).unwrap()[0].id, "youtube:PL1");
        let i = r#"{"items":[{"snippet":{"title":"T","videoOwnerChannelTitle":"A - Topic","resourceId":{"videoId":"v1"}}},{"snippet":{"title":"Private video","resourceId":{"videoId":"v2"}}}]}"#;
        let v = super::parse_items(i).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].artist, "A");
    }
    #[test]
    fn parses() {
        let j = r#"{"items":[{"id":{"videoId":"abc"},"snippet":{"title":"A &amp; B","channelTitle":"Ch","thumbnails":{"medium":{"url":"u"}}}},{"id":{"channelId":"x"},"snippet":{}}]}"#;
        let t = super::parse(j).unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].id, "youtube:abc");
        assert_eq!(t[0].name, "A & B");
        assert!(super::parse(r#"{"error":{"message":"quota"}}"#).is_err());
    }
}

/// Популярная музыка региона (videos.list chart=mostPopular, категория 10 «Музыка»): 1 единица квоты, не тратит лимит поисков.
pub async fn music_chart(key: &str, region: &str) -> Result<Vec<Track>> {
    need_key(key)?;
    let body = get("videos", &[("part", "snippet,contentDetails"), ("chart", "mostPopular"), ("videoCategoryId", "10"), ("regionCode", region), ("maxResults", "50"), ("key", key.trim())]).await?;
    let v: Value = serde_json::from_str(&body)?;
    if let Some(m) = v["error"]["message"].as_str() { return Err(Error::Msg(format!("YouTube: {m}"))); }
    let s = |x: &Value| x.as_str().unwrap_or("").to_string();
    Ok(v["items"].as_array().cloned().unwrap_or_default().iter().filter_map(|it| {
        let vid = it["id"].as_str()?;
        let sn = &it["snippet"];
        Some(Track {
            id: format!("youtube:{vid}"), source: "youtube".into(), kind: "music".into(), url: format!("https://www.youtube.com/watch?v={vid}"),
            name: unescape(&s(&sn["title"])), artist: s(&sn["channelTitle"]).trim_end_matches(" - Topic").to_string(), album: String::new(),
            image: s(&sn["thumbnails"]["high"]["url"]), audio: String::new(), download_url: String::new(), download_allowed: false,
            duration: iso_duration(&s(&it["contentDetails"]["duration"])), tags: vec![], license_url: String::new(), pop: 0.0,
        })
    }).collect())
}
