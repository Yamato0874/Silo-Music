//! Подбор видео YouTube для трека из Deezer: по длительности и совпадению исполнителя и названия,
//! чтобы реже попадали каверы, караоке и клипы с длинным вступлением.
//! Источник кандидатов — YouTube Data API (100 поисков в сутки), а когда квота кончилась — Piped или Invidious.
use crate::{model::Track, Result};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct Cand {
    pub vid: String,
    pub title: String,
    pub channel: String,
    pub duration: u32,
}

/// Слова в нижнем регистре, ё → е, без пунктуации.
fn words(s: &str) -> Vec<String> {
    s.to_lowercase().replace('ё', "е").split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(String::from).collect()
}
/// «Get Lucky (Radio Edit - feat. Pharrell)» → «Get Lucky»: скобки и feat. мешают и поиску, и сравнению.
pub fn short_title(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in s.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    let low = out.to_lowercase();
    let cut = [" feat.", " ft.", " feat ", " - "].iter().filter_map(|m| low.find(m)).min().unwrap_or(out.len());
    let r = out[..cut.min(out.len())].trim().to_string();
    if r.is_empty() { s.trim().to_string() } else { r }
}
/// Строка поиска: «исполнитель название».
pub fn query(t: &Track) -> String {
    format!("{} {}", t.artist.trim(), short_title(&t.name))
}

const BAD: [&str; 24] = ["cover", "karaoke", "originally", "tribute", "sunfly", "remix", "live", "reaction", "nightcore", "slowed", "sped", "instrumental", "8d", "tutorial",
    "lesson", "минус", "кавер", "караоке", "ремикс", "концерт", "acoustic", "piano", "mashup", "parody"];

/// Чем выше, тем вероятнее, что это сам трек. Отрицательное — скорее всего не он.
pub fn score(t: &Track, c: &Cand) -> i32 {
    let ct = words(&c.title);
    let ch = words(&c.channel.replace(" - Topic", ""));
    let topic = c.channel.ends_with(" - Topic");
    let art = words(&t.artist);
    let name = words(&short_title(&t.name));
    let full = words(&t.name);
    let mut sc = 0;
    // исполнитель в названии видео или в названии канала
    let in_title = !art.is_empty() && art.iter().all(|w| ct.contains(w));
    let in_chan = !art.is_empty() && art.iter().all(|w| ch.contains(w));
    if in_title || in_chan { sc += 25; } else { sc -= 15; }
    if topic && in_chan { sc += 10; } // официальный аудиоканал лейбла
    // все слова названия есть в названии видео
    let hit = name.iter().filter(|w| ct.contains(w)).count();
    if !name.is_empty() && hit == name.len() { sc += 25; } else if hit * 2 >= name.len().max(1) { sc += 5; } else { sc -= 25; }
    // каверы, караоке, ремиксы, концерты — если их нет в названии самого трека
    // ...и каналы караоке и трибьют-исполнителей («Originally Performed by …»)
    for b in BAD { if ct.iter().chain(ch.iter()).any(|w| w.starts_with(b)) && !full.iter().any(|w| w.starts_with(b)) { sc -= 40; break; } }
    // длительность — главный признак: у клипа вступление, у кавера своя длина
    if t.duration > 0 && c.duration > 0 {
        let d = (t.duration as i64 - c.duration as i64).unsigned_abs();
        sc += match d { 0..=3 => 40, 4..=8 => 30, 9..=15 => 15, 16..=30 => 0, 31..=60 => -20, _ => -45 };
    }
    sc
}
/// Лучший кандидат, если он хоть немного похож на трек.
pub fn best(t: &Track, cands: &[Cand]) -> Option<Cand> {
    cands.iter().map(|c| (score(t, c), c)).filter(|(s, _)| *s > 0).max_by_key(|(s, _)| *s).map(|(_, c)| c.clone())
}
pub fn from_tracks(v: &[Track]) -> Vec<Cand> {
    v.iter().map(|x| Cand { vid: x.id.trim_start_matches("youtube:").into(), title: x.name.clone(), channel: x.artist.clone(), duration: x.duration }).collect()
}

// ---- бесплатный запасной путь: Piped и Invidious (без ключа и квоты, инстансы часто лежат — перебираем) ----
pub const PIPED: [&str; 3] = ["https://pipedapi.ducks.party", "https://api.piped.private.coffee", "https://pipedapi.kavin.rocks"];
// проверено 13.11.2026: отвечают ducks.party, private.coffee, chocolatemoo53; остальные — запасные
pub const INVIDIOUS: [&str; 2] = ["https://yt.chocolatemoo53.com", "https://inv.nadeko.net"];

pub fn parse_piped(body: &str) -> Vec<Cand> {
    let v: Value = serde_json::from_str(body).unwrap_or_default();
    v["items"].as_array().cloned().unwrap_or_default().iter().filter_map(|it| {
        let vid = it["url"].as_str()?.split("v=").nth(1)?.split('&').next()?.to_string();
        Some(Cand { vid, title: it["title"].as_str().unwrap_or("").into(), channel: it["uploaderName"].as_str().unwrap_or("").into(),
            duration: it["duration"].as_i64().unwrap_or(0).max(0) as u32 })
    }).collect()
}
pub fn parse_invidious(body: &str) -> Vec<Cand> {
    let v: Value = serde_json::from_str(body).unwrap_or_default();
    v.as_array().cloned().unwrap_or_default().iter().filter_map(|it| {
        if it["type"].as_str().is_some_and(|t| t != "video") { return None; }
        Some(Cand { vid: it["videoId"].as_str()?.into(), title: it["title"].as_str().unwrap_or("").into(), channel: it["author"].as_str().unwrap_or("").into(),
            duration: it["lengthSeconds"].as_i64().unwrap_or(0).max(0) as u32 })
    }).collect()
}
async fn get(url: &str, q: &[(&str, &str)]) -> Result<String> {
    let r = crate::http().get(url).query(q).timeout(std::time::Duration::from_secs(6)).send().await?;
    if !r.status().is_success() { return Err(crate::Error::Msg(format!("HTTP {}", r.status()))); }
    Ok(r.text().await?)
}
/// Кандидаты из первого ответившего инстанса. `start` — с какого начинать (последний удачный).
pub async fn free_search(q: &str, start: usize) -> Option<(usize, Vec<Cand>)> {
    let n = PIPED.len() + INVIDIOUS.len();
    for k in 0..n {
        let i = (start + k) % n;
        let v = if i < PIPED.len() {
            let base = format!("{}/search", PIPED[i]);
            let mut v = get(&base, &[("q", q), ("filter", "music_songs")]).await.map(|b| parse_piped(&b)).unwrap_or_default();
            if v.is_empty() { v = get(&base, &[("q", q), ("filter", "videos")]).await.map(|b| parse_piped(&b)).unwrap_or_default(); }
            v
        } else {
            let base = format!("{}/api/v1/search", INVIDIOUS[i - PIPED.len()]);
            get(&base, &[("q", q), ("type", "video")]).await.map(|b| parse_invidious(&b)).unwrap_or_default()
        };
        if !v.is_empty() { return Some((i, v)); }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tr(name: &str, artist: &str, d: u32) -> Track {
        let mut t = Track::stub("deezer:1", vec![]);
        t.name = name.into(); t.artist = artist.into(); t.duration = d; t
    }
    fn c(title: &str, ch: &str, d: u32) -> Cand { Cand { vid: title.into(), title: title.into(), channel: ch.into(), duration: d } }
    #[test]
    fn short_titles() {
        assert_eq!(short_title("Get Lucky (Radio Edit - feat. Pharrell Williams)"), "Get Lucky");
        assert_eq!(short_title("bad guy (with Justin Bieber)"), "bad guy");
        assert_eq!(short_title("Numb"), "Numb");
        assert_eq!(short_title("(intro)"), "(intro)");
    }
    #[test]
    fn picks_original_over_cover_and_clip() {
        let t = tr("Numb", "Linkin Park", 185);
        let v = vec![
            c("Numb [Official Music Video] - Linkin Park", "Linkin Park", 207),
            c("Linkin Park - Numb (Piano Cover)", "SomePianist", 186),
            c("Numb", "Linkin Park - Topic", 186),
            c("Numb - Linkin Park karaoke", "Karaoke Hits", 185),
            c("Numb (Originally Performed by Linkin Park)", "Sunfly Karaoke", 186),
        ];
        assert_eq!(best(&t, &v).unwrap().channel, "Linkin Park - Topic");
    }
    #[test]
    fn cyrillic_and_live_in_title() {
        let t = tr("Группа крови", "Кино", 286);
        let v = vec![c("Кино - Группа крови (live)", "Кино", 300), c("Кино — Группа крови", "Кино", 287)];
        assert_eq!(best(&t, &v).unwrap().duration, 287);
        // live в названии трека — живое исполнение не штрафуем
        let l = tr("искала (live)", "Земфира", 240);
        assert!(score(&l, &c("Земфира - Искала (live)", "Земфира", 241)) > 0);
    }
    #[test]
    fn rejects_unrelated() {
        let t = tr("Believer", "Imagine Dragons", 204);
        assert!(best(&t, &[c("Top 10 songs 2017", "Charts", 1800)]).is_none());
    }
    #[test]
    fn parses_free_apis() {
        let p = r#"{"items":[{"url":"/watch?v=abc123","title":"Numb","uploaderName":"Linkin Park - Topic","duration":186},{"url":"/channel/x","title":"ch"}]}"#;
        assert_eq!(parse_piped(p), vec![c("Numb", "Linkin Park - Topic", 186)].into_iter().map(|mut x| { x.vid = "abc123".into(); x }).collect::<Vec<_>>());
        let i = r#"[{"type":"video","videoId":"xyz","title":"Numb","author":"Linkin Park","lengthSeconds":185},{"type":"channel","author":"A"}]"#;
        assert_eq!(parse_invidious(i).len(), 1);
    }
    #[tokio::test]
    #[ignore]
    async fn live_deezer_to_free() {
        for q in ["numb linkin park", "кино группа крови", "billie eilish bad guy", "daft punk get lucky", "земфира искала", "imagine dragons believer"] {
            let d = crate::deezer::search(q, 3).await.unwrap();
            let t = &d[0];
            let r = free_search(&query(t), 0).await;
            let b = r.as_ref().and_then(|(_, v)| best(t, v));
            println!("{q}: {} - {} ({}s) -> {:?}", t.artist, t.name, t.duration, b.map(|c| (c.title, c.channel, c.duration, c.vid)));
        }
    }
}
