//! Тексты песен из открытого LRCLIB (lrclib.net): бесплатно, без ключа. Синхронный текст (LRC) или обычный.
use crate::Result;
use serde_json::Value;

/// (синхронный LRC, обычный текст). Пустые строки — текста нет.
pub async fn lyrics(artist: &str, title: &str, duration: u32) -> Result<(String, String)> {
    let c = reqwest::Client::builder().user_agent("SiloPlayer/1.0 (open-source desktop music player)").build()?;
    let mut q: Vec<(&str, String)> = vec![("artist_name", artist.into()), ("track_name", title.into())];
    if duration > 0 { q.push(("duration", duration.to_string())); }
    let r = c.get("https://lrclib.net/api/get").query(&q).send().await?;
    let v: Value = if r.status().is_success() { r.json().await? } else {
        // точного совпадения нет: поиск, первый результат
        let list: Value = c.get("https://lrclib.net/api/search").query(&[("q", format!("{artist} {title}"))]).send().await?.json().await?;
        list.get(0).cloned().unwrap_or(Value::Null)
    };
    let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
    Ok((s("syncedLyrics"), s("plainLyrics")))
}
