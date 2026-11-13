//! Поиск и подсказки через публичный Deezer API: без ключа, лимит около 50 запросов за 5 секунд.
//! Звук у Deezer — только 30-секундные превью, поэтому полный трек при нажатии play ищется на YouTube (см. `ytmatch`).
use crate::{model::*, Error, Result};
use serde_json::Value;

pub fn parse(body: &str) -> Result<Vec<Track>> {
    let v: Value = serde_json::from_str(body)?;
    if let Some(m) = v["error"]["message"].as_str() {
        return Err(Error::Msg(format!("Deezer: {m}")));
    }
    let s = |x: &Value| x.as_str().unwrap_or("").to_string();
    Ok(v["data"].as_array().cloned().unwrap_or_default().iter().filter_map(|it| {
        let id = it["id"].as_i64()?;
        if it["type"].as_str().is_some_and(|t| t != "track") { return None; }
        let link = s(&it["link"]);
        Some(Track {
            id: format!("deezer:{id}"),
            source: "deezer".into(),
            kind: "music".into(),
            url: link.clone(),
            name: s(&it["title"]),
            artist: s(&it["artist"]["name"]),
            album: s(&it["album"]["title"]),
            image: s(&it["album"]["cover_medium"]),
            // превью 30 с: играет, только если полный трек на YouTube не нашёлся
            audio: s(&it["preview"]),
            download_url: String::new(),
            download_allowed: false,
            duration: it["duration"].as_u64().unwrap_or(0) as u32,
            tags: vec![],
            license_url: link,
            pop: it["rank"].as_f64().unwrap_or(0.0),
        })
    }).collect())
}

pub async fn search(query: &str, limit: u32) -> Result<Vec<Track>> {
    let body = crate::http()
        .get("https://api.deezer.com/search")
        .query(&[("q", query.trim()), ("limit", &limit.clamp(1, 100).to_string())])
        .send().await?.text().await?;
    parse(&body)
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_tracks() {
        let b = r#"{"data":[{"id":3135556,"type":"track","title":"Numb","title_short":"Numb","link":"https://www.deezer.com/track/3135556","duration":185,"rank":900000,"preview":"https://cdn/p.mp3","artist":{"name":"Linkin Park"},"album":{"title":"Meteora","cover_medium":"https://cdn/c.jpg"}}],"total":1}"#;
        let v = super::parse(b).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].id, "deezer:3135556");
        assert_eq!((v[0].artist.as_str(), v[0].duration), ("Linkin Park", 185));
        assert!(super::parse(r#"{"error":{"type":"Exception","message":"Quota limit exceeded"}}"#).is_err());
    }
}
