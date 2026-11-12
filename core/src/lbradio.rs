//! Похожие исполнители по открытым данным MusicBrainz и ListenBrainz (как LB Radio в troi). Без ключей.
use crate::Result;
use serde_json::Value;

fn client() -> Result<reqwest::Client> {
    Ok(crate::http())
}

/// MBID исполнителя по имени (лучшее совпадение с оценкой не ниже 90).
pub async fn mbid(name: &str) -> Result<Option<String>> {
    let q = format!("artist:\"{}\"", name.replace('"', ""));
    let v: Value = client()?.get("https://musicbrainz.org/ws/2/artist/").query(&[("query", q.as_str()), ("fmt", "json"), ("limit", "1")]).send().await?.json().await?;
    let a = &v["artists"][0];
    Ok(if a["score"].as_u64().unwrap_or(0) >= 90 { a["id"].as_str().map(String::from) } else { None })
}

/// Похожие исполнители (имена), самые близкие первыми.
pub async fn similar(mbid: &str, limit: usize) -> Result<Vec<String>> {
    let v: Value = client()?.get("https://labs.api.listenbrainz.org/similar-artists/json")
        .query(&[("artist_mbids", mbid), ("algorithm", "session_based_days_7500_session_300_contribution_5_threshold_10_limit_100_filter_True_skip_30")])
        .send().await?.json().await?;
    Ok(v.as_array().cloned().unwrap_or_default().iter()
        .filter(|a| a["artist_mbid"].as_str() != Some(mbid))
        .filter_map(|a| a["name"].as_str().map(String::from)).take(limit).collect())
}
