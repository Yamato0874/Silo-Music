//! Логика приложения, перенесённая из src-tauri/src/lib.rs без изменения поведения: те же команды, но вызываются напрямую из нативного интерфейса.
#![allow(dead_code, clippy::all)]
use silo_core::{db::Db, deezer, ytmatch, echo, jamendo, lbradio, lrclib, model::*, opencat, soundcloud, youtube};
use std::collections::HashMap;
use std::{path::PathBuf, sync::Mutex};

pub struct App {
    pub db: Mutex<Db>,
    pub music_dir: PathBuf,
}
type R<T> = Result<T, String>;
pub fn e<E: std::fmt::Display>(x: E) -> String {
    x.to_string()
}
pub fn db<'a>(s: &'a App) -> std::sync::MutexGuard<'a, Db> {
    s.db.lock().unwrap()
}
pub fn setting(s: &App, key: &str) -> R<String> {
    // режим yt-dlp: ключ API не нужен и не используется (запросы идут через yt-dlp)
    if key == "youtube_api_key" && ytdlp_mode().is_some() { return Ok("yt-dlp".into()); }
    Ok(db(s).get_setting(key).map_err(e)?.unwrap_or_default())
}

/// Режим сервиса: "api" (поиск по ключам) или "link" (только вставка ссылок). По умолчанию API, если ключи заданы.
pub fn api_on(s: &App, svc: &str, keys: &[&str]) -> bool {
    if svc == "youtube" && ytdlp_mode().is_some() { return true; }
    match setting(s, &format!("mode_{svc}")).unwrap_or_default().as_str() {
        "api" => true,
        "link" => false,
        _ => keys.iter().all(|k| !setting(s, k).unwrap_or_default().trim().is_empty()),
    }
}
/// Исполнители, которых человек попросил не рекомендовать (в нижнем регистре).
pub fn blocked_artists(s: &App) -> std::collections::HashSet<String> {
    serde_json::from_str::<Vec<String>>(&setting(s, "blocked_artists").unwrap_or_default()).unwrap_or_default().into_iter().map(|x| x.to_lowercase()).collect()
}
/// Убрать забаненные треки и заблокированных исполнителей.
pub fn drop_banned(s: &App, v: &mut Vec<Track>) {
    let r = db(s).ratings().unwrap_or_default();
    let ba = blocked_artists(s);
    v.retain(|t| r.get(&t.id) != Some(&-2) && !ba.contains(&t.artist.trim_end_matches(" - Topic").to_lowercase()));
}
pub fn download_dir(s: &App) -> R<PathBuf> {
    let d = setting(s, "download_dir")?;
    Ok(if d.trim().is_empty() { s.music_dir.clone() } else { PathBuf::from(d) })
}

const KEYS: [&str; 16] = ["blocked_artists", "music_folder", "daily", "listenbrainz_token", "prefs", "download_dir", "jamendo_client_id", "youtube_api_key", "soundcloud_client_id", "soundcloud_client_secret", "mode_youtube", "mode_soundcloud", "google_cse_key", "google_cse_cx", "session", "search_history"];

pub fn get_setting(s: &App, key: String) -> R<String> {
    if !KEYS.contains(&key.as_str()) {
        return Err("неизвестная настройка".into());
    }
    Ok(db(s).get_setting(&key).map_err(e)?.unwrap_or_default())
}
pub fn set_setting(s: &App, key: String, value: String) -> R<()> {
    if !KEYS.contains(&key.as_str()) {
        return Err("неизвестная настройка".into());
    }
    db(&s).set_setting(&key, value.trim()).map_err(e)
}
// ---- YouTube: кэш и защита от исчерпания квоты (поиск стоит 100 из 10 000 единиц в сутки) ----
static YT_BLOCK_UNTIL: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
const YT_QUOTA_MSG: &str = "YouTube: лимит запросов на сегодня исчерпан, сбрасывается в 11:00 по Москве. Пока работаю с другими источниками.";
const DAY: i64 = 86400;
pub fn now_s() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}
/// Квота сбрасывается в полночь по тихоокеанскому времени = 08:00 UTC = 11:00 МСК.
pub fn next_quota_reset(now: i64) -> i64 {
    let t = now - now.rem_euclid(DAY) + 8 * 3600;
    if t > now { t } else { t + DAY }
}
pub fn yt_blocked() -> bool {
    if ytdlp_mode().is_some() { return false; }
    YT_BLOCK_UNTIL.load(std::sync::atomic::Ordering::Relaxed) > now_s() || YT_USED.load(std::sync::atomic::Ordering::Relaxed) >= YT_SEARCH_LIMIT
}
/// Лимит поисков YouTube в сутки (search.list стоит 100 из 10 000 единиц).
const YT_SEARCH_LIMIT: u32 = 100;
static YT_USED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// Сутки квоты по тихоокеанскому времени: сброс в 08:00 UTC = 11:00 МСК.
pub fn pt_day(now: i64) -> i64 { (now - 8 * 3600).div_euclid(DAY) }
/// Поисков YouTube за текущие сутки квоты (хранится в базе, переживает перезапуск).
pub fn yt_used(s: &App) -> u32 {
    let n = db(s).cache_get(&format!("yt:used:{}", pt_day(now_s())), 2 * DAY).ok().flatten().and_then(|x| x.parse().ok()).unwrap_or(0);
    YT_USED.store(n, std::sync::atomic::Ordering::Relaxed);
    n
}
/// Отметить один потраченный поиск YouTube.
pub fn yt_note(s: &App) {
    let n = yt_used(s) + 1;
    let _ = db(s).cache_put(&format!("yt:used:{}", pt_day(now_s())), &n.to_string());
    YT_USED.store(n, std::sync::atomic::Ordering::Relaxed);
}
pub fn yt_usage(s: &App) -> (u32, u32) { (yt_used(&s), YT_SEARCH_LIMIT) }
pub fn yt_err(x: silo_core::Error) -> String {
    let m = x.to_string();
    if m.to_lowercase().contains("quota") {
        YT_BLOCK_UNTIL.store(next_quota_reset(now_s()), std::sync::atomic::Ordering::Relaxed);
        YT_QUOTA_MSG.into()
    } else { m }
}
/// Поиск видео YouTube: сперва кэш (ttl секунд), затем сеть; при исчерпанной квоте сразу понятное сообщение без запроса.
/// Треки жанра для «Эхо» почти без квоты: плейлист жанра ищется раз в 30 дней (100 ед.), его треки берутся за 1 ед. и кэшируются на 7 дней.
/// Волна: похожие исполнители + жанр (или вкус, если поиски кончились).
pub async fn yt_genre_tracks(s: &App, key: &str, text: &str, region: &str, fav: &[String]) -> R<Vec<Track>> {
    // лимит поисков берегём, а обычных запросов (10 000 в день) хватает: каналы из лайков подмешиваем всегда
    // порядок важен: сначала любимые и похожие, затем каналы из лайков, жанр и только потом популярное
    let mut v = yt_similar_tracks(s, key, fav).await;
    v.extend(yt_taste_tracks(s, key, region, false).await.unwrap_or_default());
    if v.len() < 30 { v.extend(yt_genre_core(s, key, text, region).await?); }
    Ok(v)
}
/// Как LB Radio: исполнители из лайков и дослушанного → похожие по ListenBrainz → их треки на YouTube.
/// Поиск YouTube — один на нового исполнителя, кэш 30 дней; MBID и похожие — в кэше на 30 и 7 дней.
pub async fn yt_similar_tracks(s: &App, key: &str, fav: &[String]) -> Vec<Track> {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as usize).unwrap_or(0);
    let seeds: Vec<String> = {
        let d = db(s);
        let mut v: Vec<Track> = d.rated_tracks(1).unwrap_or_default();
        v.extend(d.play_events(300).unwrap_or_default().into_iter().filter(|(_, f, _)| *f >= 0.7).map(|(t, _, _)| t));
        // любимые исполнители из знакомства и Настроек — первыми
        let mut names: Vec<String> = fav.iter().map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect();
        for t in v { let a = t.artist.trim().trim_end_matches(" - Topic").to_string(); if !a.is_empty() && !names.iter().any(|x| x.eq_ignore_ascii_case(&a)) { names.push(a); } }
        names
    };
    if seeds.is_empty() { return vec![]; }
    let nfav = fav.len().min(seeds.len());
    let mut fresh = 0;
    // сами любимые исполнители: их топ на YouTube (поиск один раз на исполнителя, кэш 30 дней)
    let mut out: Vec<Track> = vec![];
    for a in seeds.iter().take(nfav.min(10)) {
        let ck = format!("yt:art:{}", a.to_lowercase());
        let hit: Option<Vec<Track>> = db(s).cache_get(&ck, 30 * DAY).ok().flatten().and_then(|j| serde_json::from_str(&j).ok());
        let v = match hit {
            Some(v) => v,
            None => {
                if fresh >= 4 || yt_blocked() { continue; }
                fresh += 1;
                yt_note(s);
                let Ok(v) = ytw::search(key, a, 10).await.map_err(yt_err) else { continue };
                let v = ytw::keep_music(key, v).await.unwrap_or_default();
                let _ = db(s).cache_put(&ck, &serde_json::to_string(&v).unwrap_or_default());
                v
            }
        };
        // не всегда первые три из десяти: иначе волна по любимому исполнителю крутит одни и те же песни
        let rot = if v.is_empty() { 0 } else { (n / 11 + a.len()) % v.len() };
        out.extend(v.iter().cycle().skip(rot).take(3.min(v.len())).cloned());
    }
    let mut sim: Vec<String> = vec![];
    let mut mb_calls = 0;
    // для похожих: все любимые плюс два случайных исполнителя из лайков
    let picks: Vec<&String> = seeds.iter().take(nfav).chain((0..2.min(seeds.len() - nfav)).map(|k| &seeds[nfav + (n / 7 + k) % (seeds.len() - nfav)])).take(8).collect();
    for a in picks {
        let mk = format!("mb:{}", a.to_lowercase());
        let cached = db(s).cache_get(&mk, 30 * DAY).ok().flatten();
        let id = match cached {
            Some(x) => x,
            None => {
                if mb_calls > 0 { std::thread::sleep(std::time::Duration::from_millis(1100)); } // MusicBrainz: не чаще 1 запроса в секунду
                mb_calls += 1;
                let x = lbradio::mbid(a).await.ok().flatten().unwrap_or_default();
                let _ = db(s).cache_put(&mk, &x);
                x
            }
        };
        if id.is_empty() { continue; }
        let lk = format!("lb:{id}");
        let hit: Option<Vec<String>> = db(s).cache_get(&lk, 7 * DAY).ok().flatten().and_then(|j| serde_json::from_str(&j).ok());
        let list = match hit { Some(v) => v, None => { let v = lbradio::similar(&id, 20).await.unwrap_or_default(); let _ = db(s).cache_put(&lk, &serde_json::to_string(&v).unwrap_or_default()); v } };
        for x in list { if !seeds.iter().any(|y| y.eq_ignore_ascii_case(&x)) && !sim.iter().any(|y| y.eq_ignore_ascii_case(&x)) { sim.push(x); } }
    }
    // похожих берём по кругу; новых (без кэша) ищем не больше 3 сверх любимых, чтобы беречь лимит поисков
    for k in 0..sim.len().min(20) {
        let a = &sim[(n / 13 + k * 3) % sim.len()];
        let ck = format!("yt:art:{}", a.to_lowercase());
        let hit: Option<Vec<Track>> = db(s).cache_get(&ck, 30 * DAY).ok().flatten().and_then(|j| serde_json::from_str(&j).ok());
        let v = match hit {
            Some(v) => v,
            None => {
                if fresh >= 7 || yt_blocked() { continue; }
                fresh += 1;
                yt_note(s);
                let Ok(v) = ytw::search(key, a, 10).await.map_err(yt_err) else { continue };
                let v = ytw::keep_music(key, v).await.unwrap_or_default();
                let _ = db(s).cache_put(&ck, &serde_json::to_string(&v).unwrap_or_default());
                v
            }
        };
        // не всегда первые три из десяти: иначе волна по любимому исполнителю крутит одни и те же песни
        let rot = if v.is_empty() { 0 } else { (n / 11 + a.len()) % v.len() };
        out.extend(v.iter().cycle().skip(rot).take(3.min(v.len())).cloned());
    }
    out
}
pub async fn yt_genre_core(s: &App, key: &str, text: &str, region: &str) -> R<Vec<Track>> {
    // лимит поисков исчерпан или плейлист не нашёлся: берём популярную музыку (1 ед., другой лимит)
    let pl = yt_search_playlists(s, key, text, 30 * DAY).await.unwrap_or_default();
    let Some(first) = pl.first() else { return yt_taste_tracks(s, key, region, false).await };
    let id = first.id.trim_start_matches("youtube:").to_string();
    let ck = format!("yt:pm:{id}");
    let hit = db(s).cache_get(&ck, 7 * DAY).map_err(e)?.and_then(|j| serde_json::from_str::<Vec<Track>>(&j).ok());
    let v = match hit {
        Some(v) => v,
        None => {
            let v = ytw::playlist_items(key, &id).await.unwrap_or_default();
            let mut v = ytw::keep_music(key, v).await.unwrap_or_default();
            youtube::cap_artist(&mut v, 3);
            let _ = db(s).cache_put(&ck, &serde_json::to_string(&v).unwrap_or_default());
            v
        }
    };
    // в плейлисте одни часовые миксы или он недоступен: берём по вкусу и популярное
    if v.len() < 5 { let mut t = yt_taste_tracks(s, key, region, false).await?; t.extend(v); return Ok(t); }
    Ok(v)
}
/// Популярная музыка на YouTube в стране языка, кэш на сутки.
pub async fn yt_chart(s: &App, key: &str, region: &str) -> R<Vec<Track>> {
    let ck = format!("yt:chart2:{region}");
    if let Some(v) = db(s).cache_get(&ck, DAY).map_err(e)?.and_then(|j| serde_json::from_str::<Vec<Track>>(&j).ok()) { return Ok(v); }
    let mut v = ytw::music_chart(key, region).await.map_err(e)?;
    v.retain(|t| (60..=720).contains(&t.duration));
    let _ = db(s).cache_put(&ck, &serde_json::to_string(&v).unwrap_or_default());
    Ok(v)
}
/// Без лимита поисков, по вкусу: свежие треки с каналов, которые человек лайкал или дослушивал (по 1 ед.), плюс популярное в стране языка.
pub async fn yt_taste_tracks(s: &App, key: &str, region: &str, with_chart: bool) -> R<Vec<Track>> {
    let ids: Vec<String> = {
        let d = db(s);
        let mut v: Vec<Track> = d.rated_tracks(1).map_err(e)?;
        v.extend(d.play_events(300).map_err(e)?.into_iter().filter(|(_, f, _)| *f >= 0.7).map(|(t, _, _)| t));
        let mut ids: Vec<String> = vec![];
        for t in v { if let Some(id) = t.id.strip_prefix("youtube:") { if !ids.iter().any(|x| x == id) && ids.len() < 50 { ids.push(id.to_string()); } } }
        ids
    };
    let mut out: Vec<Track> = vec![];
    if !ids.is_empty() {
        let ck = format!("yt:ch:{}", ids.join(","));
        let hit: Option<Vec<String>> = db(s).cache_get(&ck, 7 * DAY).map_err(e)?.and_then(|j| serde_json::from_str(&j).ok());
        let chans: Vec<String> = match hit {
            Some(c) => c,
            None => { let c = ytw::channels_of(key, &ids).await.map_err(e)?; let _ = db(s).cache_put(&ck, &serde_json::to_string(&c).unwrap_or_default()); c }
        };
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as usize).unwrap_or(0);
        let mut picked: Vec<Track> = vec![];
        for k in 0..chans.len().min(25) {
            let c = &chans[(n + k) % chans.len()];
            let Some(rest) = c.strip_prefix("UC") else { continue };
            let pl = format!("UU{rest}");
            let pk = format!("yt:pm:{pl}");
            let hit = db(s).cache_get(&pk, 7 * DAY).map_err(e)?.and_then(|j| serde_json::from_str::<Vec<Track>>(&j).ok());
            let items = match hit {
                Some(v) => v,
                // только музыка: проверка категории и длины, результат в кэше на 7 дней
                None => match ytw::playlist_items(key, &pl).await {
                    Ok(v) => { let m = ytw::keep_music(key, v).await.unwrap_or_default(); let _ = db(s).cache_put(&pk, &serde_json::to_string(&m).unwrap_or_default()); m }
                    Err(_) => vec![],
                },
            };
            picked.extend(items.into_iter().take(10));
        }
        youtube::cap_artist(&mut picked, 3);
        out.extend(picked);
    }
    // популярное — только если по вкусу набралось мало
    if with_chart { if let Ok(mut v) = yt_chart(s, key, region).await { youtube::cap_artist(&mut v, 2); out.extend(v); } }
    Ok(out)
}
pub async fn yt_search(s: &App, key: &str, q: &str, limit: u32, ttl: i64) -> R<Vec<Track>> {
    let ck = format!("yt:s:{limit}:{}", q.trim().to_lowercase());
    let hit = db(s).cache_get(&ck, ttl).map_err(e)?;
    if let Some(v) = hit.and_then(|j| serde_json::from_str::<Vec<Track>>(&j).ok()) { return Ok(v); }
    if yt_blocked() { return Err(YT_QUOTA_MSG.into()); }
    yt_note(s);
    let v = ytw::search(key, q, limit).await.map_err(yt_err)?;
    let _ = db(s).cache_put(&ck, &serde_json::to_string(&v).unwrap_or_default());
    Ok(v)
}
pub async fn yt_search_playlists(s: &App, key: &str, q: &str, ttl: i64) -> R<Vec<PlaylistRef>> {
    let ck = format!("yt:p:{}", q.trim().to_lowercase());
    let hit = db(s).cache_get(&ck, ttl).map_err(e)?;
    if let Some(v) = hit.and_then(|j| serde_json::from_str::<Vec<PlaylistRef>>(&j).ok()) { return Ok(v); }
    if yt_blocked() { return Err(YT_QUOTA_MSG.into()); }
    yt_note(s);
    let v = ytw::search_playlists(key, q).await.map_err(yt_err)?;
    let _ = db(s).cache_put(&ck, &serde_json::to_string(&v).unwrap_or_default());
    Ok(v)
}

/// Открыть https-ссылку в системном браузере (только для справки о получении ключей).
pub fn open_url(url: String) -> R<()> {
    const OK: [&str; 5] = ["https://devportal.jamendo.com", "https://console.cloud.google.com", "https://developers.google.com", "https://soundcloud.com", "https://developers.soundcloud.com"];
    if !OK.iter().any(|p| url == *p || url.starts_with(&format!("{p}/"))) {
        return Err("ссылка не разрешена".into());
    }
    #[cfg(target_os = "windows")]
    let r = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", &url]).spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(&url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(&url).spawn();
    r.map(|_| ()).map_err(|x| x.to_string())
}
/// Обзор Jamendo по жанрам/языку.
pub async fn search_tracks(s: &App, params: SearchParams) -> R<Vec<Track>> {
    let id = setting(&s, "jamendo_client_id")?;
    let v = jamendo::search(&id, &params).await.map_err(e)?;
    db(&s).note_seen(&v).map_err(e)?;
    Ok(v)
}
/// Поиск ссылок YouTube и SoundCloud через официальный Google Custom Search JSON API (режим «по ссылке»).
pub async fn cse_search(key: &str, cx: &str, q: &str, yt: bool, sc: bool) -> R<Vec<Track>> {
    let sites = [(yt, "site:youtube.com"), (sc, "site:soundcloud.com")].iter().filter(|x| x.0).map(|x| x.1).collect::<Vec<_>>().join(" OR ");
    let query = format!("{q} ({sites})");
    let r = silo_core::http().get("https://www.googleapis.com/customsearch/v1")
        .query(&[("key", key), ("cx", cx), ("q", query.as_str()), ("num", "10")]).send().await.map_err(e)?;
    let ok = r.status().is_success();
    let v: serde_json::Value = r.json().await.map_err(e)?;
    if !ok { return Err(format!("Google Custom Search: {}", v["error"]["message"].as_str().unwrap_or("ошибка"))); }
    let mut out = vec![];
    for it in v["items"].as_array().cloned().unwrap_or_default() {
        let link = it["link"].as_str().unwrap_or("").to_string();
        let raw = it["title"].as_str().unwrap_or("").to_string();
        let img = it["pagemap"]["cse_thumbnail"][0]["src"].as_str().unwrap_or("").to_string();
        let mut name = raw.trim_end_matches(" - YouTube").split(" | ").next().unwrap_or("").to_string();
        let (id, source, url, mut artist) = if let Some(vid) = link.split(['?', '&']).find_map(|p| p.strip_prefix("v=")).filter(|_| link.contains("youtube.com/watch")) {
            (format!("youtube:{vid}"), "youtube", format!("https://www.youtube.com/watch?v={vid}"), String::new())
        } else if let Some(path) = link.strip_prefix("https://soundcloud.com/").or_else(|| link.strip_prefix("https://m.soundcloud.com/")) {
            let segs: Vec<&str> = path.split(['?', '#']).next().unwrap_or("").trim_end_matches('/').split('/').collect();
            let track = segs.len() == 2 && !["sets", "tracks", "albums", "likes", "reposts", "followers", "following", "popular-tracks", "spotlight"].contains(&segs[1]);
            if !(track || (segs.len() == 3 && segs[1] == "sets")) { continue; }
            (format!("soundcloud:link:{}", segs.join("/")), "soundcloud", format!("https://soundcloud.com/{}", segs.join("/")), segs[0].to_string())
        } else { continue };
        // «Stream Song by Artist» → название и исполнитель
        if let Some(x) = name.strip_prefix("Stream ") { name = x.to_string(); }
        if let Some((n, a)) = name.rsplit_once(" by ").map(|(n, a)| (n.to_string(), a.to_string())) { if source == "soundcloud" { name = n; artist = a; } }
        if source == "youtube" { if let Some((a, n)) = name.split_once(" - ").map(|(a, n)| (a.to_string(), n.to_string())) { artist = a; name = n; } }
        out.push(Track { id, source: source.into(), kind: "music".into(), url: url.clone(), name, artist, album: String::new(), image: img,
            audio: String::new(), download_url: String::new(), download_allowed: false, duration: 0, tags: vec![], license_url: url, pop: 0.0 });
    }
    Ok(out)
}
/// Единый поиск: local, jamendo, youtube, soundcloud. Ошибка одного источника не мешает остальным.
pub async fn search_all(s: &App, params: SearchParams) -> R<SearchAll> {
    let q = params.query.clone().unwrap_or_default();
    let want = |n: &str| params.sources.is_empty() || params.sources.iter().any(|x| x == n);
    let mut out = SearchAll::default();
    if want("local") {
        out.tracks.extend(db(&s).search_downloads(&q).map_err(e)?);
    }
    if want("jamendo") && !setting(&s, "jamendo_client_id")?.trim().is_empty() {
        match jamendo::search(&setting(&s, "jamendo_client_id")?, &params).await {
            Ok(v) => out.tracks.extend(v),
            Err(x) => out.errors.push(format!("Jamendo: {x}")),
        }
    }
    // текстовый поиск — только YouTube (в режиме yt-dlp через yt-dlp: без ключа API и квоты); Deezer в нативной версии не используется
    if want("youtube") && !q.trim().is_empty() && api_on(&s, "youtube", &["youtube_api_key"]) {
        let key = setting(&s, "youtube_api_key")?;
        match yt_search(&s, &key, &q, 15, 30 * DAY).await { Ok(v) => out.tracks.extend(v), Err(x) => out.errors.push(x) }
    }
    if want("soundcloud") && !q.trim().is_empty() && api_on(&s, "soundcloud", &["soundcloud_client_id", "soundcloud_client_secret"]) {
        let (id, sec) = (setting(&s, "soundcloud_client_id")?, setting(&s, "soundcloud_client_secret")?);
        match soundcloud::search(&id, &sec, &q, 15).await {
            Ok(v) => out.tracks.extend(v),
            Err(x) => out.errors.push(x.to_string()),
        }
    }
    // режим «по ссылке» + ключ Google Custom Search: ищем ссылки YouTube/SoundCloud по названию
    let yt_link = want("youtube") && !api_on(&s, "youtube", &["youtube_api_key"]);
    let sc_link = want("soundcloud") && !api_on(&s, "soundcloud", &["soundcloud_client_id", "soundcloud_client_secret"]);
    let (gk, gcx) = (setting(&s, "google_cse_key")?, setting(&s, "google_cse_cx")?);
    if (yt_link || sc_link) && !q.trim().is_empty() && !gk.trim().is_empty() && !gcx.trim().is_empty() {
        match cse_search(gk.trim(), gcx.trim(), q.trim(), yt_link, sc_link).await { Ok(v) => out.tracks.extend(v), Err(x) => out.errors.push(x) }
    }
    let qq = if q.trim().is_empty() { params.tags.join(" ") } else { q.clone() };
    if params.sources.iter().any(|x| x == "ccmixter") && !qq.trim().is_empty() {
        match opencat::ccmixter(&qq, 10).await { Ok(v) => out.tracks.extend(v), Err(x) => out.errors.push(format!("ccMixter: {x}")) }
    }
    if params.sources.iter().any(|x| x == "archive") && !qq.trim().is_empty() {
        match opencat::archive(&qq, 6).await { Ok(v) => out.tracks.extend(v), Err(x) => out.errors.push(format!("Internet Archive: {x}")) }
    }
    drop_banned(&s, &mut out.tracks);
    // строгий язык: по умолчанию включён; отбрасываем треки других языков из всех источников
    if let Some(l) = params.lang.as_deref().filter(|l| !l.is_empty()) {
        if params.lang_strict != Some(false) { silo_core::langfilter::retain(&mut out.tracks, l); }
    }
    Ok(out)
}
/// Поиск Deezer с кэшем на сутки: подсказки и повторные запросы не упираются в лимит 50 запросов за 5 с.
pub async fn deezer_search(s: &App, q: &str, limit: u32) -> R<Vec<Track>> {
    let ck = format!("dz:{limit}:{}", q.trim().to_lowercase());
    if let Some(v) = db(s).cache_get(&ck, DAY).map_err(e)?.and_then(|j| serde_json::from_str::<Vec<Track>>(&j).ok()) { return Ok(v); }
    let v = deezer::search(q, limit).await.map_err(|x| format!("Deezer: {}", x.to_string().trim_start_matches("Deezer: ")))?;
    let _ = db(s).cache_put(&ck, &serde_json::to_string(&v).unwrap_or_default());
    Ok(v)
}
/// Подсказки при вводе: несколько треков Deezer.
pub async fn suggest_tracks(s: &App, query: String) -> R<Vec<Track>> {
    if query.trim().chars().count() < 2 { return Ok(vec![]); }
    let _ = query; return Ok(vec![]);
    #[allow(unreachable_code)]
    let mut v = deezer_search(&s, &query, 25).await?;
    drop_banned(&s, &mut v);
    v.truncate(6);
    Ok(v)
}
static FREE_START: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// Трек Deezer → видео YouTube при нажатии play. Порядок: кэш в базе → YouTube Data API (1 поиск) → Piped/Invidious.
/// Метаданные (название, исполнитель, обложка, длительность) остаются от Deezer. Не нашлось — играет 30-секундное превью.
pub async fn resolve_track(s: &App, track: Track) -> R<Track> {
    if track.source != "deezer" { return Ok(track); }
    let as_yt = |vid: &str| { let mut t = track.clone(); t.id = format!("youtube:{vid}"); t.source = "youtube".into(); t.url = format!("https://www.youtube.com/watch?v={vid}"); t.audio = String::new(); t };
    let hit = db(&s).yt_match_get(&track.id).map_err(e)?;
    if let Some(vid) = hit { return Ok(as_yt(&vid)); }
    let q = ytmatch::query(&track);
    let key = setting(&s, "youtube_api_key")?;
    yt_used(&s);
    if !key.trim().is_empty() && api_on(&s, "youtube", &["youtube_api_key"]) && !yt_blocked() {
        yt_note(&s);
        match ytw::search(&key, &q, 10).await {
            Ok(v) => if let Some(c) = ytmatch::best(&track, &ytmatch::from_tracks(&v)) {
                let _ = db(&s).yt_match_put(&track.id, &c.vid, "api");
                return Ok(as_yt(&c.vid));
            },
            Err(x) => { let _ = yt_err(x); }
        }
    }
    // квота кончилась, ключа нет или ничего похожего: бесплатный запасной путь
    if let Some((i, v)) = ytmatch::free_search(&q, FREE_START.load(std::sync::atomic::Ordering::Relaxed)).await {
        FREE_START.store(i, std::sync::atomic::Ordering::Relaxed);
        if let Some(c) = ytmatch::best(&track, &v) {
            let _ = db(&s).yt_match_put(&track.id, &c.vid, "free");
            return Ok(as_yt(&c.vid));
        }
    }
    let mut t = track;
    t.kind = "preview".into();
    Ok(t)
}
/// Ссылка на публичный плейлист (YouTube, SoundCloud, Jamendo) → id для open_playlist.
pub async fn resolve_playlist(s: &App, url: String) -> R<String> {
    let u = url.trim();
    let rest = u.strip_prefix("https://").or_else(|| u.strip_prefix("http://")).ok_or("нужна ссылка, начинающаяся с https://")?;
    let host = rest.split('/').next().unwrap_or("").trim_start_matches("www.").trim_start_matches("m.");
    if host == "youtube.com" || host == "music.youtube.com" || host == "youtu.be" {
        let list = u.split(['?', '&']).find_map(|p| p.strip_prefix("list=")).ok_or("в ссылке YouTube нет плейлиста (list=…)")?;
        return Ok(format!("youtube:{list}"));
    }
    if host == "soundcloud.com" {
        let (i, sec) = (setting(&s, "soundcloud_client_id")?, setting(&s, "soundcloud_client_secret")?);
        let id = soundcloud::resolve_playlist_id(&i, &sec, u).await.map_err(e)?;
        return Ok(format!("soundcloud:{id}"));
    }
    if host == "jamendo.com" {
        let id = rest.split('/').skip_while(|p| *p != "playlist").nth(1).filter(|p| p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty()).ok_or("не нашёл номер плейлиста Jamendo в ссылке")?;
        return Ok(format!("jamendo:{id}"));
    }
    Err("поддерживаются ссылки YouTube, SoundCloud и Jamendo".into())
}
/// Поиск готовых плейлистов: Jamendo, YouTube, SoundCloud.
pub async fn search_playlists(s: &App, query: String, sources: Vec<String>) -> R<PlaylistHits> {
    let want = |n: &str| sources.is_empty() || sources.iter().any(|x| x == n);
    let mut out = PlaylistHits::default();
    fn push(out: &mut PlaylistHits, r: silo_core::Result<Vec<PlaylistRef>>) {
        match r {
            Ok(v) => out.playlists.extend(v),
            Err(x) => out.errors.push(x.to_string()),
        }
    }
    if want("jamendo") {
        push(&mut out, jamendo::search_playlists(&setting(&s, "jamendo_client_id")?, &query).await);
    }
    if want("youtube") {
        let key = setting(&s, "youtube_api_key")?;
        match yt_search_playlists(&s, &key, &query, 30 * DAY).await {
            Ok(v) => out.playlists.extend(v),
            Err(x) => out.errors.push(x),
        }
    }
    if want("soundcloud") {
        let (id, sec) = (setting(&s, "soundcloud_client_id")?, setting(&s, "soundcloud_client_secret")?);
        push(&mut out, soundcloud::search_playlists(&id, &sec, &query).await);
    }
    Ok(out)
}
/// Ссылка SoundCloud (трек или плейлист) без ключей API: название и обложка из oEmbed, играет встроенный плеер.
pub async fn link_track(url: String) -> R<Track> {
    let u = url.trim().split('?').next().unwrap_or("").trim_end_matches('/').to_string();
    let rest = u.strip_prefix("https://").or_else(|| u.strip_prefix("http://")).ok_or("нужна ссылка, начинающаяся с https://")?;
    let host = rest.split('/').next().unwrap_or("").trim_start_matches("www.").trim_start_matches("m.");
    if ["youtube.com", "music.youtube.com", "youtu.be"].contains(&host) {
        let full = url.trim();
        let vid = full.split(['?', '&']).find_map(|p| p.strip_prefix("v=")).map(String::from)
            .or_else(|| (host == "youtu.be").then(|| rest.split('/').nth(1).unwrap_or("").to_string()))
            .or_else(|| rest.split("/shorts/").nth(1).map(|x| x.split('/').next().unwrap_or("").to_string()))
            .filter(|v| !v.is_empty()).ok_or("в ссылке YouTube нет видео")?;
        let page = format!("https://www.youtube.com/watch?v={vid}");
        let v: serde_json::Value = silo_core::http().get("https://www.youtube.com/oembed").query(&[("format", "json"), ("url", page.as_str())])
            .send().await.map_err(e)?.json().await.map_err(|_| "YouTube не отдаёт это видео (закрыто или удалено)".to_string())?;
        let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
        return Ok(Track {
            id: format!("youtube:{vid}"), source: "youtube".into(), kind: "music".into(), url: page.clone(), name: s("title"), artist: s("author_name"),
            album: String::new(), image: s("thumbnail_url"), audio: String::new(), download_url: String::new(), download_allowed: false,
            duration: 0, tags: vec![], license_url: page, pop: 0.0,
        });
    }
    if host != "soundcloud.com" && host != "on.soundcloud.com" { return Err("поддерживаются ссылки YouTube и SoundCloud".into()); }
    let r = silo_core::http().get("https://soundcloud.com/oembed").query(&[("format", "json"), ("url", u.as_str())]).send().await.map_err(e)?;
    if !r.status().is_success() { return Err("SoundCloud не нашёл трек по этой ссылке (он удалён или закрыт)".into()); }
    let v: serde_json::Value = r.json().await.map_err(e)?;
    let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
    let (title, artist) = (s("title"), s("author_name"));
    let name = title.strip_suffix(&format!(" by {artist}")).unwrap_or(&title).to_string();
    // короткая ссылка on.soundcloud.com: настоящий адрес берём из html виджета
    let real = s("html").split("url=").nth(1).and_then(|x| x.split('&').next()).map(|x| x.replace("%3A", ":").replace("%2F", "/"))
        .filter(|x| x.starts_with("https://api.soundcloud.com/")).unwrap_or(u.clone());
    Ok(Track {
        id: format!("soundcloud:link:{}", rest.trim_start_matches(host).trim_start_matches('/')),
        source: "soundcloud".into(), kind: "music".into(), url: real, name, artist, album: String::new(),
        image: s("thumbnail_url"), audio: String::new(), download_url: String::new(), download_allowed: false,
        duration: 0, tags: vec![], license_url: u, pop: 0.0,
    })
}
/// Топ-треки исполнителя на YouTube для переноса плейлистов: общий кэш с волной (30 дней), поиск — один на исполнителя.
/// Возвращает треки и признак, был ли потрачен поиск.
pub async fn artist_tracks(s: &App, artist: String) -> R<(Vec<Track>, bool)> {
    let a = artist.trim().to_string();
    if a.is_empty() { return Ok((vec![], false)); }
    let ck = format!("yt:art:{}", a.to_lowercase());
    let hit: Option<Vec<Track>> = db(&s).cache_get(&ck, 30 * DAY).map_err(e)?.and_then(|j| serde_json::from_str(&j).ok());
    if let Some(v) = hit { return Ok((v, false)); }
    let key = setting(&s, "youtube_api_key")?;
    if key.trim().is_empty() || yt_blocked() { return Ok((vec![], false)); }
    yt_note(&s);
    let v = ytw::search(&key, &a, 10).await.map_err(yt_err)?;
    let v = ytw::keep_music(&key, v).await.unwrap_or_default();
    let _ = db(&s).cache_put(&ck, &serde_json::to_string(&v).unwrap_or_default());
    Ok((v, true))
}
/// Текст песни (LRCLIB). Для YouTube название часто «Исполнитель - Песня (Official Video)»: чистим.
pub async fn lyrics(artist: String, title: String, duration: u32) -> R<(String, String)> {
    let mut a = artist.trim_end_matches(" - Topic").trim().to_string();
    let mut t = title.clone();
    for cut in ["(", "[", "|", " ft.", " feat."] { if let Some(i) = t.find(cut) { if i > 0 { t.truncate(i); } } }
    if let Some((x, y)) = t.split_once(" - ") { a = x.trim().to_string(); t = y.to_string(); }
    lrclib::lyrics(&a, t.trim(), duration).await.map_err(e)
}
pub fn rename_playlist(s: &App, id: i64, name: String) -> R<()> { db(&s).rename_playlist(id, name.trim()).map_err(e) }
/// Итоги: топ исполнителей и треков и часы прослушивания (за 30 дней и за всё время).
pub fn stats(s: &App) -> R<serde_json::Value> {
    let ev = db(&s).play_events(100000).map_err(e)?;
    let mut out = serde_json::Map::new();
    for (key, max_age) in [("month", 30 * DAY), ("all", i64::MAX)] {
        let mut secs = 0f64;
        let mut arts: HashMap<String, u32> = HashMap::new();
        let mut trs: HashMap<String, (Track, u32)> = HashMap::new();
        for (t, f, age) in ev.iter().filter(|(_, _, a)| *a <= max_age) {
            secs += t.duration as f64 * f.clamp(0.0, 1.0);
            if *f >= 0.5 {
                *arts.entry(t.artist.trim_end_matches(" - Topic").to_string()).or_default() += 1;
                trs.entry(t.id.clone()).or_insert((t.clone(), 0)).1 += 1;
                let _ = age;
            }
        }
        let mut a: Vec<_> = arts.into_iter().filter(|(k, _)| !k.is_empty()).collect(); a.sort_by(|x, y| y.1.cmp(&x.1)); a.truncate(10);
        let mut t: Vec<_> = trs.into_values().collect(); t.sort_by(|x, y| y.1.cmp(&x.1)); t.truncate(10);
        out.insert(key.into(), serde_json::json!({ "hours": (secs / 360.0).round() / 10.0, "artists": a, "tracks": t.into_iter().map(|(t, n)| serde_json::json!({ "track": t, "plays": n })).collect::<Vec<_>>() }));
    }
    Ok(serde_json::Value::Object(out))
}
/// Скробблинг в ListenBrainz: только если человек сам вписал свой токен.
pub async fn scrobble(s: &App, track: Track) -> R<()> {
    let token = setting(&s, "listenbrainz_token")?;
    if token.trim().is_empty() { return Ok(()); }
    let body = serde_json::json!({ "listen_type": "single", "payload": [{ "listened_at": now_s(),
        "track_metadata": { "artist_name": track.artist.trim_end_matches(" - Topic"), "track_name": track.name, "additional_info": { "origin_url": track.url, "media_player": "Silo Player" } } }] });
    let r = silo_core::http().post("https://api.listenbrainz.org/1/submit-listens").header("Authorization", format!("Token {}", token.trim())).json(&body).send().await.map_err(e)?;
    if r.status().is_success() { Ok(()) } else { Err(format!("ListenBrainz: {}", r.status())) }
}
/// Порядок трека в плейлисте (перетаскивание).
pub fn reorder_playlist(s: &App, id: i64, from: usize, to: usize) -> R<()> { db(&s).reorder_playlist(id, from, to).map_err(e) }
/// Своя музыкальная папка: mp3/flac/ogg/m4a/wav с тегами (lofty) попадают в библиотеку («Скачано»), поиск и «Эхо».
pub async fn scan_folder(s: &App, path: String) -> R<usize> {
    let root = PathBuf::from(path.trim());
    if !root.is_dir() { return Err("папка не найдена".into()); }
    db(&s).set_setting("music_folder", &root.to_string_lossy()).map_err(e)?;
    let files = tokio::task::spawn_blocking(move || {
        use lofty::prelude::*;
        let mut out = vec![];
        let mut stack = vec![root];
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else { continue };
            for en in rd.flatten() {
                let p = en.path();
                if p.is_dir() { stack.push(p); continue; }
                let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_lowercase();
                if !["mp3", "flac", "ogg", "opus", "m4a", "wav", "aac"].contains(&ext.as_str()) { continue; }
                let stem = p.file_stem().and_then(|x| x.to_str()).unwrap_or("").to_string();
                let (mut name, mut artist, mut album, mut genre, mut dur) = (stem.clone(), String::new(), String::new(), String::new(), 0u32);
                if let Some((a, b)) = stem.split_once(" - ") { artist = a.trim().into(); name = b.trim().into(); }
                if let Ok(tf) = lofty::read_from_path(&p) {
                    dur = tf.properties().duration().as_secs() as u32;
                    if let Some(t) = tf.primary_tag().or_else(|| tf.first_tag()) {
                        if let Some(x) = t.title() { name = x.to_string(); }
                        if let Some(x) = t.artist() { artist = x.to_string(); }
                        if let Some(x) = t.album() { album = x.to_string(); }
                        if let Some(x) = t.genre() { genre = x.to_lowercase(); }
                    }
                }
                let ps = p.to_string_lossy().to_string();
                out.push(Track { id: format!("local:file:{ps}"), source: "local".into(), kind: "music".into(), url: String::new(), name, artist, album,
                    image: String::new(), audio: ps, download_url: String::new(), download_allowed: false, duration: dur,
                    tags: if genre.is_empty() { vec![] } else { vec![genre] }, license_url: String::new(), pop: 0.0 });
            }
        }
        out
    }).await.map_err(e)?;
    let d = db(&s);
    for t in &files { d.save_download(t, &t.audio).map_err(e)?; }
    Ok(files.len())
}
/// Открыть подборку: id вида "youtube:PL…", "soundcloud:123", "jamendo:45".
pub async fn open_playlist(s: &App, id: String, lang: Option<String>) -> R<Vec<Track>> {
    let (src, raw) = id.split_once(':').ok_or("плохой id")?;
    let mut v = match src {
        "jamendo" => jamendo::playlist_tracks(&setting(&s, "jamendo_client_id")?, raw).await,
        "youtube" => ytw::playlist_items(&setting(&s, "youtube_api_key")?, raw).await.map_err(|x| silo_core::Error::Msg(yt_err(x))),
        "soundcloud" => {
            let (i, sec) = (setting(&s, "soundcloud_client_id")?, setting(&s, "soundcloud_client_secret")?);
            soundcloud::playlist_tracks(&i, &sec, raw).await
        }
        _ => Err("неизвестный источник".into()),
    }
    .map_err(e)?;
    if let Some(l) = lang.as_deref().filter(|l| !l.is_empty()) { silo_core::langfilter::retain(&mut v, l); }
    Ok(v)
}
pub fn save_playlist(s: &App, name: String, tracks: Vec<Track>) -> R<i64> {
    db(&s).save_playlist(&name, &tracks).map_err(e)
}
pub fn history(s: &App) -> R<Vec<Track>> {
    db(&s).history(200).map_err(e)
}
pub fn clear_history(s: &App) -> R<()> {
    db(&s).clear_history().map_err(e)
}
/// Размер папки загрузок в байтах и сама папка.
pub fn downloads_info(s: &App) -> R<(u64, String)> {
    let dir = download_dir(&s)?;
    let size = std::fs::read_dir(&dir).map(|d| d.flatten().filter_map(|f| f.metadata().ok()).map(|m| m.len()).sum()).unwrap_or(0);
    Ok((size, dir.to_string_lossy().into()))
}

pub fn rate_track(s: &App, track: Track, value: i32) -> R<()> {
    // -2 = бан трека (раньше ошибочно обрезался до -1 и становился дизлайком)
    db(&s).set_rating(&track, value.clamp(-2, 1)).map_err(e)
}
pub fn liked_tracks(s: &App) -> R<Vec<Track>> {
    db(&s).rated_tracks(1).map_err(e)
}
pub fn banned_tracks(s: &App) -> R<Vec<Track>> {
    db(&s).rated_tracks(-2).map_err(e)
}
/// «Сбросить вкусы»: очистить историю и обнулить влияние прошлых оценок на «Эхо».
/// Группы «Понравившиеся/Бан» трогаются только при clear_groups = true.
pub fn reset_taste(s: &App, clear_groups: bool) -> R<()> {
    db(&s).reset_taste(clear_groups).map_err(e)
}
pub fn all_ratings(s: &App) -> R<HashMap<String, i32>> {
    db(&s).ratings().map_err(e)
}
pub fn record_play(s: &App, track: Track, fraction: f64) -> R<()> {
    db(&s).record_play(&track, fraction).map_err(e)
}
/// «Эхо»: адаптивная модель вкуса (бета-распределение «понравится» по жанрам), которая строится из журнала
/// прослушиваний (доля прослушанного, пропуски, 👍/👎, давность). Подборка выбирается выборкой Томпсона: исследование
/// возникает из неопределённости и склонности самого человека, а не из фиксированного процента.
/// `diversity` (familiar | balanced | discovery) лишь сдвигает стартовое ожидание по незнакомым жанрам.
pub async fn echo_mix(
    s: &App,
    _offset: u32,
    audio_format: Option<String>,
    seed_tags: Option<Vec<String>>,
    seed_lang: Option<String>,
    diversity: Option<String>,
    seed_artists: Option<Vec<echo::SeedArtist>>,
    lang_strict: Option<bool>,
) -> R<EchoOut> {
    let fav: Vec<String> = seed_artists.as_ref().map(|v| v.iter().map(|a| a.name.clone()).collect()).unwrap_or_default();
    use std::collections::HashSet;
    let id = setting(&s, "jamendo_client_id")?;
    let yt_key = setting(&s, "youtube_api_key")?;
    let (sc_id, sc_sec) = (setting(&s, "soundcloud_client_id")?, setting(&s, "soundcloud_client_secret")?);
    let has_jam = !id.trim().is_empty();
    let has_yt = !yt_key.trim().is_empty();
    let has_sc = !sc_id.trim().is_empty() && !sc_sec.trim().is_empty();
    let mut out_info = EchoOut::default();
    if !has_jam { out_info.missing.push("jamendo".into()); }
    if !has_yt { out_info.missing.push("youtube".into()); }
    if !has_sc { out_info.missing.push("soundcloud".into()); }
    let local = db(&s).search_downloads("").map_err(e)?;
    let mode = echo::Mode::parse(diversity.as_deref().unwrap_or("balanced"));
    let seed = seed_tags.unwrap_or_default();
    let (model, played, rated, has_history) = {
        let d = db(&s);
        let epoch = d.taste_epoch().map_err(e)?;
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|x| x.as_secs() as i64).unwrap_or(0);
        let fresh = |age: i64| now - age >= epoch; // события до «Сбросить вкусы» модель не учитывает
        let mut memo: HashMap<String, Option<f64>> = HashMap::new();
        let mut events: Vec<echo::Event> = vec![];
        for (t, f, age) in d.play_events(1500).map_err(e)? {
            if !fresh(age) { continue; }
            let q = *memo.entry(t.id.clone()).or_insert_with(|| d.quality_of(&t).ok().flatten());
            events.push(echo::play_event(t.tags, f, age).with_quality(q).with_artist(&t.artist));
        }
        for (t, v, age) in d.rating_events().map_err(e)? {
            // бан (-2) исключает только сам трек и на модель вкуса не влияет; лайк и дизлайк лишь сдвигают оценки
            if !fresh(age) || v == -2 { continue; }
            let q = *memo.entry(t.id.clone()).or_insert_with(|| d.quality_of(&t).ok().flatten());
            events.push(echo::rating_event(t.tags, v, age).with_quality(q).with_artist(&t.artist));
        }
        let has = !events.is_empty();
        (
            echo::Model::build(&events, mode, &seed).with_seed_artists(&seed_artists.unwrap_or_default()),
            d.played_ids(1000).map_err(e)?.into_iter().collect::<HashSet<_>>(),
            d.ratings().map_err(e)?,
            has,
        )
    };
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
    let mut rng = echo::Rng::new(nanos);
    let mut slots = echo::plan(&model, 24, &mut rng);
    // не больше 6 запросов к Jamendo за раз
    let mut tags: Vec<String> = vec![];
    for sl in &slots {
        if !tags.contains(&sl.tag) && tags.len() < 6 {
            tags.push(sl.tag.clone());
        }
    }
    slots.retain(|sl| tags.contains(&sl.tag));
    let mut pool: HashMap<String, Vec<Track>> = HashMap::new();
    let mut last_err = None;
    for tag in &tags {
        let explore = !slots.iter().any(|sl| &sl.tag == tag && !sl.explore);
        let p = SearchParams {
            tags: silo_core::taxonomy::get().query_tags(tag),
            // язык стартового вкуса — только пока нет истории и только для «своих» жанров
            lang: if !has_history && !explore { seed_lang.clone() } else { None },
            order: Some(if explore { "popularity_month" } else { "popularity_week" }.into()),
            offset: rng.below(if explore { 60 } else { 25 }) as u32,
            limit: 30,
            audio_format: audio_format.clone(),
            ..Default::default()
        };
        let qt = silo_core::taxonomy::get().query_tags(tag);
        let mut cand: Vec<Track> = vec![];
        // скачанные треки с подходящими тегами
        cand.extend(local.iter().filter(|t| t.tags.iter().any(|x| qt.contains(x))).cloned());
        // YouTube и SoundCloud: не больше двух жанров за подборку (экономия квоты YouTube)
        let pos = tags.iter().position(|x| x == tag);
        let ext = pos.map_or(false, |i| i < 2);
        let text = format!("{} music", qt.first().cloned().unwrap_or_else(|| tag.clone()));
        // YouTube дорогой (100 ед. за поиск): только один жанр за подборку, результаты кэшируются на 3 суток;
        // при исчерпанной квоте тихо пропускаем (сообщение один раз)
        // источники опрашиваются параллельно: подборка собирается быстрее
        let region = match seed_lang.as_deref() { Some("en") => "US", Some("de") => "DE", Some("fr") => "FR", Some("es") => "ES", _ => "RU" };
        let yt_f = async { if pos == Some(0) && has_yt { Some(yt_genre_tracks(&s, &yt_key, &text, region, &fav).await) } else { None } };
        let sc_f = async { if ext && has_sc { Some(soundcloud::search(&sc_id, &sc_sec, &text, 10).await) } else { None } };
        let jam_f = async { if has_jam { jamendo::search(&id, &p).await } else { Ok(vec![]) } };
        let (ytr, scr, jam) = futures::join!(yt_f, sc_f, jam_f);
        match ytr {
            Some(Ok(mut v)) => { for t in &mut v { if t.tags.is_empty() { t.tags = qt.clone(); } } cand.extend(v); }
            Some(Err(x)) => { if !out_info.errors.contains(&x) { out_info.errors.push(x); } }
            None => {}
        }
        match scr {
            Some(Ok(mut v)) => { for t in &mut v { if t.tags.is_empty() { t.tags = qt.clone(); } } cand.extend(v); }
            Some(Err(x)) => out_info.errors.push(format!("SoundCloud: {x}")),
            None => {}
        }
        // строгий язык: если выбран язык, чужих треков в «Эхо» нет в любом источнике
        let strict_lang = seed_lang.clone().filter(|l| !l.is_empty() && lang_strict != Some(false));
        if let Some(l) = &strict_lang { silo_core::langfilter::retain(&mut cand, l); }
        match jam {
            Ok(mut v) => {
                if let Some(l) = &strict_lang { silo_core::langfilter::retain(&mut v, l); }
                v.extend(cand);
                // мягкая подстройка порядка под привычную для человека «топовость» внутри жанра
                let qs: HashMap<String, Option<f64>> = {
                    let d = db(&s);
                    d.note_seen(&v.iter().filter(|t| t.source == "jamendo").cloned().collect::<Vec<_>>()).map_err(e)?;
                    v.iter().map(|t| (t.id.clone(), d.quality_of(t).ok().flatten())).collect()
                };
                // порядок внутри жанра: слегка к привычной «топовости» и к исполнителям, которые нравятся (и без запретов)
                let (target, wq) = model.quality_target();
                echo::rerank(
                    &mut v,
                    &|t| qs.get(&t.id).copied().flatten(),
                    target,
                    wq,
                    &|t| model.artist_affinity(&t.artist, &t.tags),
                    model.artist_weight(),
                    &mut rng,
                );
                pool.insert(tag.clone(), v);
            }
            Err(x) => {
                last_err = Some(e(&x));
                out_info.errors.push(format!("Jamendo: {x}"));
                if !cand.is_empty() { pool.insert(tag.clone(), cand); }
            }
        }
    }
    if pool.is_empty() {
        return Err(if has_jam || has_yt || has_sc {
            last_err.unwrap_or_else(|| "не удалось получить подборку".into())
        } else {
            "Для «Эхо» нужен хотя бы один ключ (Jamendo, YouTube или SoundCloud): Настройки → Ключи сервисов".into()
        });
    }
    // версии одного трека (клип, lyrics, Topic) считаются повтором: ключи недавно игравших (до 1000 последних)
    let played_keys: HashSet<String> = db(&s).history(1000).unwrap_or_default().iter().map(echo::track_key).collect();
    out_info.tracks = echo::compose(&slots, &pool, &played, &played_keys, &rated);
    // шире: не больше двух треков одного исполнителя за подборку
    youtube::cap_artist(&mut out_info.tracks, 2);
    drop_banned(&s, &mut out_info.tracks);
    if out_info.tracks.is_empty() && yt_blocked() {
        return Err("Поиски YouTube на сегодня кончились, поэтому волна не может найти твои жанры и исполнителей. Соберётся после 11:00 по Москве, а уже найденное запоминается на месяц.".into());
    }
    Ok(out_info)
}
#[derive(serde::Serialize, serde::Deserialize, Default, Clone)]
pub struct EchoOut {
    pub tracks: Vec<Track>,
    /// источники без ключа: jamendo | youtube | soundcloud
    pub missing: Vec<String>,
    pub errors: Vec<String>,
}
/// Подсказки исполнителей при выборе любимых: библиотека, Jamendo, SoundCloud (если есть ключи) и MusicBrainz (жанры для известных артистов).
/// Исполнитель, которого нет в свободных источниках, всё равно годится как сигнал вкуса: берутся его жанры.
pub async fn suggest_artists(s: &App, query: String) -> R<Vec<silo_core::artists::ArtistHit>> {
    use silo_core::artists as ar;
    let q = query.trim().to_string();
    if q.chars().count() < 2 { return Ok(vec![]); }
    let ql = q.to_lowercase();
    let mut lists: Vec<Vec<ar::ArtistHit>> = vec![];
    {
        let d = db(&s);
        let mut names: Vec<String> = vec![];
        let mut add = |a: &str| { if a.to_lowercase().contains(&ql) && !names.iter().any(|x| x == a) { names.push(a.to_string()); } };
        for (t, _, _) in d.play_events(500).map_err(e)? { add(&t.artist); }
        for t in d.rated_tracks(1).map_err(e)? { add(&t.artist); }
        lists.push(names.into_iter().take(4).map(|n| ar::ArtistHit { name: n, source: "library".into(), ..Default::default() }).collect());
    }
    let (jid, scid, scsec) = (setting(&s, "jamendo_client_id").unwrap_or_default(), setting(&s, "soundcloud_client_id").unwrap_or_default(), setting(&s, "soundcloud_client_secret").unwrap_or_default());
    if !jid.trim().is_empty() {
        if let Ok(b) = silo_core::jamendo::artists(&jid, &q).await { if let Ok(v) = ar::parse_jamendo(&b) { lists.push(v); } }
    }
    if !scid.trim().is_empty() && !scsec.trim().is_empty() {
        if let Ok(b) = silo_core::soundcloud::users(&scid, &scsec, &q).await { if let Ok(v) = ar::parse_soundcloud(&b) { lists.push(v); } }
    }
    if let Ok(b) = ar::musicbrainz(&q).await { if let Ok(v) = ar::parse_musicbrainz(&b) { lists.push(v); } }
    Ok(ar::merge(lists, 8))
}
pub fn list_playlists(s: &App) -> R<Vec<Playlist>> {
    db(&s).list_playlists().map_err(e)
}
pub fn create_playlist(s: &App, name: String) -> R<i64> {
    db(&s).create_playlist(&name).map_err(e)
}
pub fn delete_playlist(s: &App, id: i64) -> R<()> {
    db(&s).delete_playlist(id).map_err(e)
}
pub fn playlist_tracks(s: &App, id: i64) -> R<Vec<Track>> {
    db(&s).playlist_tracks(id).map_err(e)
}
pub fn add_to_playlist(s: &App, id: i64, track: Track) -> R<()> {
    db(&s).add_to_playlist(id, &track).map_err(e)
}
pub fn remove_from_playlist(s: &App, id: i64, track_id: String) -> R<()> {
    db(&s).remove_from_playlist(id, &track_id).map_err(e)
}
pub fn list_downloads(s: &App) -> R<Vec<(Track, String)>> {
    db(&s).list_downloads().map_err(e)
}
/// Скачивание только если лицензия трека это разрешает (audiodownload_allowed).
pub async fn download_track(s: &App, track: Track) -> R<String> {
    if !["jamendo", "ccmixter", "archive"].contains(&track.source.as_str()) {
        return Err("Скачивать можно только треки Jamendo, ccMixter и Internet Archive с открытой лицензией".into());
    }
    if !track.download_allowed || track.download_url.is_empty() {
        return Err("Автор не разрешил скачивание этого трека".into());
    }
    // скачиваем потоком: большой FLAC не держится целиком в памяти
    let mut resp = silo_core::http().get(&track.download_url).send().await.map_err(e)?.error_for_status().map_err(e)?;
    let safe = |x: &str| x.chars().map(|c| if r#"\/:*?"<>|"#.contains(c) || c.is_control() { '_' } else { c }).collect::<String>();
    let dir = download_dir(&s)?;
    std::fs::create_dir_all(&dir).map_err(e)?;
    let path = dir.join(format!("{} - {}.mp3", safe(&track.artist), safe(&track.name)));
    {
        use std::io::Write;
        let mut f = std::io::BufWriter::new(std::fs::File::create(&path).map_err(e)?);
        while let Some(c) = resp.chunk().await.map_err(e)? { f.write_all(&c).map_err(e)?; }
        f.flush().map_err(e)?;
    }
    let p = path.to_string_lossy().to_string();
    let local = Track { id: format!("local:{}", track.id), source: "local".into(), audio: p.clone(), ..track };
    db(&s).save_download(&local, &p).map_err(e)?;
    Ok(p)
}


/// Режим «Библиотека yt-dlp»: все запросы к YouTube идут через yt-dlp, ключ API и квота не тратятся.
/// Some(путь) — режим включён (пустой путь: yt-dlp не найден, тогда YouTube-запросы просто пусты).
pub static YTDLP: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
pub fn ytdlp_mode() -> Option<String> { YTDLP.lock().unwrap().clone() }

/// Обёртки над youtube::*: в режиме yt-dlp — yt-dlp, иначе YouTube Data API.
pub mod ytw {
    use silo_core::{model::*, youtube, Result};
    fn dlp() -> Option<String> { super::ytdlp_mode() }
    async fn run(exe: String, args: Vec<String>) -> Result<serde_json::Value> {
        if exe.is_empty() { return Err("yt-dlp не найден".into()); }
        tokio::task::spawn_blocking(move || crate::ytdlp::json(&exe, &args)).await.map_err(|e| silo_core::Error::Msg(e.to_string()))?.map_err(silo_core::Error::Msg)
    }
    pub async fn search(key: &str, q: &str, limit: u32) -> Result<Vec<Track>> {
        match dlp() { Some(exe) => Ok(crate::ytdlp::tracks_of(&run(exe, vec!["--flat-playlist".into(), "-J".into(), format!("ytsearch{limit}:{q}")]).await?)), None => youtube::search(key, q, limit).await }
    }
    pub async fn keep_music(key: &str, v: Vec<Track>) -> Result<Vec<Track>> {
        // без API: музыка — то, что по длине похоже на трек
        match dlp() { Some(_) => Ok(v.into_iter().filter(|t| t.duration == 0 || (60..=720).contains(&t.duration)).collect()), None => youtube::keep_music(key, v).await }
    }
    pub async fn playlist_items(key: &str, id: &str) -> Result<Vec<Track>> {
        match dlp() { Some(exe) => Ok(crate::ytdlp::tracks_of(&run(exe, vec!["--flat-playlist".into(), "-J".into(), "--playlist-end".into(), "200".into(), format!("https://www.youtube.com/playlist?list={id}")]).await?)), None => youtube::playlist_items(key, id).await }
    }
    pub async fn search_playlists(key: &str, q: &str) -> Result<Vec<PlaylistRef>> {
        match dlp() {
            Some(exe) => {
                let url = format!("https://www.youtube.com/results?search_query={}&sp=EgIQAw%3D%3D", crate::ytdlp::urlenc(q));
                let v = run(exe, vec!["--flat-playlist".into(), "-J".into(), "--playlist-end".into(), "15".into(), url]).await?;
                Ok(v["entries"].as_array().cloned().unwrap_or_default().iter().filter_map(|e| {
                    let id = e["id"].as_str()?;
                    if !(id.starts_with("PL") || id.starts_with("OL") || id.starts_with("RD")) { return None; }
                    Some(PlaylistRef { id: format!("youtube:{id}"), source: "youtube".into(), title: e["title"].as_str().unwrap_or("").into(), owner: e["channel"].as_str().or(e["uploader"].as_str()).unwrap_or("").into(), image: e["thumbnails"][0]["url"].as_str().unwrap_or("").into(), count: e["playlist_count"].as_u64().unwrap_or(0) as u32 })
                }).collect())
            }
            None => youtube::search_playlists(key, q).await,
        }
    }
    pub async fn channels_of(key: &str, ids: &[String]) -> Result<Vec<String>> {
        match dlp() { Some(_) => Ok(vec![]), None => youtube::channels_of(key, ids).await }
    }
    pub async fn music_chart(key: &str, region: &str) -> Result<Vec<Track>> {
        match dlp() { Some(_) => Ok(vec![]), None => youtube::music_chart(key, region).await }
    }
}
