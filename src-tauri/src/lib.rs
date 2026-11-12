use silo_core::{db::Db, echo, jamendo, model::*, soundcloud, youtube};
use std::collections::HashMap;
use std::{path::PathBuf, sync::Mutex};
use tauri::{Manager, State};

struct App {
    db: Mutex<Db>,
    music_dir: PathBuf,
}
type R<T> = Result<T, String>;
fn e<E: std::fmt::Display>(x: E) -> String {
    x.to_string()
}
fn db<'a>(s: &'a State<App>) -> std::sync::MutexGuard<'a, Db> {
    s.db.lock().unwrap()
}
fn setting(s: &State<App>, key: &str) -> R<String> {
    Ok(db(s).get_setting(key).map_err(e)?.unwrap_or_default())
}

fn download_dir(s: &State<App>) -> R<PathBuf> {
    let d = setting(s, "download_dir")?;
    Ok(if d.trim().is_empty() { s.music_dir.clone() } else { PathBuf::from(d) })
}

const KEYS: [&str; 6] = ["prefs", "download_dir", "jamendo_client_id", "youtube_api_key", "soundcloud_client_id", "soundcloud_client_secret"];

#[tauri::command]
fn get_setting(s: State<App>, key: String) -> R<String> {
    if !KEYS.contains(&key.as_str()) {
        return Err("неизвестная настройка".into());
    }
    setting(&s, &key)
}
#[tauri::command]
fn set_setting(s: State<App>, key: String, value: String) -> R<()> {
    if !KEYS.contains(&key.as_str()) {
        return Err("неизвестная настройка".into());
    }
    db(&s).set_setting(&key, value.trim()).map_err(e)
}
// ---- YouTube: кэш и защита от исчерпания квоты (поиск стоит 100 из 10 000 единиц в сутки) ----
static YT_BLOCK_UNTIL: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
const YT_QUOTA_MSG: &str = "YouTube: лимит запросов на сегодня исчерпан, сбрасывается в 11:00 по Москве. Пока работаю с другими источниками.";
const DAY: i64 = 86400;
fn now_s() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}
/// Квота сбрасывается в полночь по тихоокеанскому времени = 08:00 UTC = 11:00 МСК.
fn next_quota_reset(now: i64) -> i64 {
    let t = now - now.rem_euclid(DAY) + 8 * 3600;
    if t > now { t } else { t + DAY }
}
fn yt_blocked() -> bool {
    YT_BLOCK_UNTIL.load(std::sync::atomic::Ordering::Relaxed) > now_s()
}
fn yt_err(x: silo_core::Error) -> String {
    let m = x.to_string();
    if m.to_lowercase().contains("quota") {
        YT_BLOCK_UNTIL.store(next_quota_reset(now_s()), std::sync::atomic::Ordering::Relaxed);
        YT_QUOTA_MSG.into()
    } else { m }
}
/// Поиск видео YouTube: сперва кэш (ttl секунд), затем сеть; при исчерпанной квоте сразу понятное сообщение без запроса.
async fn yt_search(s: &State<'_, App>, key: &str, q: &str, limit: u32, ttl: i64) -> R<Vec<Track>> {
    let ck = format!("yt:s:{limit}:{}", q.trim().to_lowercase());
    let hit = db(s).cache_get(&ck, ttl).map_err(e)?;
    if let Some(v) = hit.and_then(|j| serde_json::from_str::<Vec<Track>>(&j).ok()) { return Ok(v); }
    if yt_blocked() { return Err(YT_QUOTA_MSG.into()); }
    let v = youtube::search(key, q, limit).await.map_err(yt_err)?;
    let _ = db(s).cache_put(&ck, &serde_json::to_string(&v).unwrap_or_default());
    Ok(v)
}
async fn yt_search_playlists(s: &State<'_, App>, key: &str, q: &str, ttl: i64) -> R<Vec<PlaylistRef>> {
    let ck = format!("yt:p:{}", q.trim().to_lowercase());
    let hit = db(s).cache_get(&ck, ttl).map_err(e)?;
    if let Some(v) = hit.and_then(|j| serde_json::from_str::<Vec<PlaylistRef>>(&j).ok()) { return Ok(v); }
    if yt_blocked() { return Err(YT_QUOTA_MSG.into()); }
    let v = youtube::search_playlists(key, q).await.map_err(yt_err)?;
    let _ = db(s).cache_put(&ck, &serde_json::to_string(&v).unwrap_or_default());
    Ok(v)
}

/// Открыть https-ссылку в системном браузере (только для справки о получении ключей).
#[tauri::command]
fn open_url(url: String) -> R<()> {
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
#[tauri::command]
async fn search_tracks(s: State<'_, App>, params: SearchParams) -> R<Vec<Track>> {
    let id = setting(&s, "jamendo_client_id")?;
    let v = jamendo::search(&id, &params).await.map_err(e)?;
    db(&s).note_seen(&v).map_err(e)?;
    Ok(v)
}
/// Единый поиск: local, jamendo, youtube, soundcloud. Ошибка одного источника не мешает остальным.
#[tauri::command]
async fn search_all(s: State<'_, App>, params: SearchParams) -> R<SearchAll> {
    let q = params.query.clone().unwrap_or_default();
    let want = |n: &str| params.sources.is_empty() || params.sources.iter().any(|x| x == n);
    let mut out = SearchAll::default();
    if want("local") {
        out.tracks.extend(db(&s).search_downloads(&q).map_err(e)?);
    }
    if want("jamendo") {
        match jamendo::search(&setting(&s, "jamendo_client_id")?, &params).await {
            Ok(v) => out.tracks.extend(v),
            Err(x) => out.errors.push(format!("Jamendo: {x}")),
        }
    }
    if want("youtube") && !q.trim().is_empty() {
        let key = setting(&s, "youtube_api_key")?;
        match yt_search(&s, &key, &q, 15, 3 * DAY).await {
            Ok(v) => out.tracks.extend(v),
            Err(x) => out.errors.push(x),
        }
    }
    if want("soundcloud") && !q.trim().is_empty() {
        let (id, sec) = (setting(&s, "soundcloud_client_id")?, setting(&s, "soundcloud_client_secret")?);
        match soundcloud::search(&id, &sec, &q, 15).await {
            Ok(v) => out.tracks.extend(v),
            Err(x) => out.errors.push(x.to_string()),
        }
    }
    // строгий язык: по умолчанию включён; отбрасываем треки других языков из всех источников
    if let Some(l) = params.lang.as_deref().filter(|l| !l.is_empty()) {
        if params.lang_strict != Some(false) { silo_core::langfilter::retain(&mut out.tracks, l); }
    }
    Ok(out)
}
/// Ссылка на публичный плейлист (YouTube, SoundCloud, Jamendo) → id для open_playlist.
#[tauri::command]
async fn resolve_playlist(s: State<'_, App>, url: String) -> R<String> {
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
#[tauri::command]
async fn search_playlists(s: State<'_, App>, query: String, sources: Vec<String>) -> R<PlaylistHits> {
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
        match yt_search_playlists(&s, &key, &query, 3 * DAY).await {
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
/// Открыть подборку: id вида "youtube:PL…", "soundcloud:123", "jamendo:45".
#[tauri::command]
async fn open_playlist(s: State<'_, App>, id: String, lang: Option<String>) -> R<Vec<Track>> {
    let (src, raw) = id.split_once(':').ok_or("плохой id")?;
    let mut v = match src {
        "jamendo" => jamendo::playlist_tracks(&setting(&s, "jamendo_client_id")?, raw).await,
        "youtube" => youtube::playlist_items(&setting(&s, "youtube_api_key")?, raw).await.map_err(|x| silo_core::Error::Msg(yt_err(x))),
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
#[tauri::command]
fn save_playlist(s: State<App>, name: String, tracks: Vec<Track>) -> R<i64> {
    db(&s).save_playlist(&name, &tracks).map_err(e)
}
#[tauri::command]
fn history(s: State<App>) -> R<Vec<Track>> {
    db(&s).history(200).map_err(e)
}
#[tauri::command]
fn clear_history(s: State<App>) -> R<()> {
    db(&s).clear_history().map_err(e)
}
/// Размер папки загрузок в байтах и сама папка.
#[tauri::command]
fn downloads_info(s: State<App>) -> R<(u64, String)> {
    let dir = download_dir(&s)?;
    let size = std::fs::read_dir(&dir).map(|d| d.flatten().filter_map(|f| f.metadata().ok()).map(|m| m.len()).sum()).unwrap_or(0);
    Ok((size, dir.to_string_lossy().into()))
}

#[tauri::command]
fn rate_track(s: State<App>, track: Track, value: i32) -> R<()> {
    db(&s).set_rating(&track, value.clamp(-1, 1)).map_err(e)
}
#[tauri::command]
fn liked_tracks(s: State<App>) -> R<Vec<Track>> {
    db(&s).rated_tracks(1).map_err(e)
}
#[tauri::command]
fn banned_tracks(s: State<App>) -> R<Vec<Track>> {
    db(&s).rated_tracks(-2).map_err(e)
}
/// «Сбросить вкусы»: очистить историю и обнулить влияние прошлых оценок на «Эхо».
/// Группы «Понравившиеся/Бан» трогаются только при clear_groups = true.
#[tauri::command]
fn reset_taste(s: State<App>, clear_groups: bool) -> R<()> {
    db(&s).reset_taste(clear_groups).map_err(e)
}
#[tauri::command]
fn all_ratings(s: State<App>) -> R<HashMap<String, i32>> {
    db(&s).ratings().map_err(e)
}
#[tauri::command]
fn record_play(s: State<App>, track: Track, fraction: f64) -> R<()> {
    db(&s).record_play(&track, fraction).map_err(e)
}
/// «Эхо»: адаптивная модель вкуса (бета-распределение «понравится» по жанрам), которая строится из журнала
/// прослушиваний (доля прослушанного, пропуски, 👍/👎, давность). Подборка выбирается выборкой Томпсона: исследование
/// возникает из неопределённости и склонности самого человека, а не из фиксированного процента.
/// `diversity` (familiar | balanced | discovery) лишь сдвигает стартовое ожидание по незнакомым жанрам.
#[tauri::command]
async fn echo_mix(
    s: State<'_, App>,
    _offset: u32,
    audio_format: Option<String>,
    seed_tags: Option<Vec<String>>,
    seed_lang: Option<String>,
    diversity: Option<String>,
    seed_artists: Option<Vec<echo::SeedArtist>>,
    lang_strict: Option<bool>,
) -> R<EchoOut> {
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
            d.played_ids(60).map_err(e)?.into_iter().collect::<HashSet<_>>(),
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
        if pos == Some(0) && has_yt {
            match yt_search(&s, &yt_key, &text, 10, 3 * DAY).await {
                Ok(mut v) => { for t in &mut v { if t.tags.is_empty() { t.tags = qt.clone(); } } cand.extend(v); }
                Err(x) => { if !out_info.errors.contains(&x) { out_info.errors.push(x); } }
            }
        }
        if ext && has_sc {
            match soundcloud::search(&sc_id, &sc_sec, &text, 10).await {
                Ok(mut v) => { for t in &mut v { if t.tags.is_empty() { t.tags = qt.clone(); } } cand.extend(v); }
                Err(x) => out_info.errors.push(format!("SoundCloud: {x}")),
            }
        }
        let jam = if has_jam { jamendo::search(&id, &p).await } else { Ok(vec![]) };
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
    out_info.tracks = echo::compose(&slots, &pool, &played, &rated);
    Ok(out_info)
}
#[derive(serde::Serialize, Default)]
struct EchoOut {
    tracks: Vec<Track>,
    /// источники без ключа: jamendo | youtube | soundcloud
    missing: Vec<String>,
    errors: Vec<String>,
}
/// Подсказки исполнителей при выборе любимых: библиотека, Jamendo, SoundCloud (если есть ключи) и MusicBrainz (жанры для известных артистов).
/// Исполнитель, которого нет в свободных источниках, всё равно годится как сигнал вкуса: берутся его жанры.
#[tauri::command]
async fn suggest_artists(s: State<'_, App>, query: String) -> R<Vec<silo_core::artists::ArtistHit>> {
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
#[tauri::command]
fn list_playlists(s: State<App>) -> R<Vec<Playlist>> {
    db(&s).list_playlists().map_err(e)
}
#[tauri::command]
fn create_playlist(s: State<App>, name: String) -> R<i64> {
    db(&s).create_playlist(&name).map_err(e)
}
#[tauri::command]
fn delete_playlist(s: State<App>, id: i64) -> R<()> {
    db(&s).delete_playlist(id).map_err(e)
}
#[tauri::command]
fn playlist_tracks(s: State<App>, id: i64) -> R<Vec<Track>> {
    db(&s).playlist_tracks(id).map_err(e)
}
#[tauri::command]
fn add_to_playlist(s: State<App>, id: i64, track: Track) -> R<()> {
    db(&s).add_to_playlist(id, &track).map_err(e)
}
#[tauri::command]
fn remove_from_playlist(s: State<App>, id: i64, track_id: String) -> R<()> {
    db(&s).remove_from_playlist(id, &track_id).map_err(e)
}
#[tauri::command]
fn list_downloads(s: State<App>) -> R<Vec<(Track, String)>> {
    db(&s).list_downloads().map_err(e)
}
/// Скачивание только если лицензия трека это разрешает (audiodownload_allowed).
#[tauri::command]
async fn download_track(s: State<'_, App>, track: Track) -> R<String> {
    if track.source != "jamendo" {
        return Err("Скачивать можно только треки Jamendo с разрешения автора".into());
    }
    if !track.download_allowed || track.download_url.is_empty() {
        return Err("Автор не разрешил скачивание этого трека".into());
    }
    let bytes = reqwest::get(&track.download_url)
        .await
        .map_err(e)?
        .error_for_status()
        .map_err(e)?
        .bytes()
        .await
        .map_err(e)?;
    let safe = |x: &str| x.chars().map(|c| if r#"\/:*?"<>|"#.contains(c) || c.is_control() { '_' } else { c }).collect::<String>();
    let dir = download_dir(&s)?;
    std::fs::create_dir_all(&dir).map_err(e)?;
    let path = dir.join(format!("{} - {}.mp3", safe(&track.artist), safe(&track.name)));
    std::fs::write(&path, &bytes).map_err(e)?;
    let p = path.to_string_lossy().to_string();
    let local = Track { id: format!("local:{}", track.id), source: "local".into(), audio: p.clone(), ..track };
    db(&s).save_download(&local, &p).map_err(e)?;
    Ok(p)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let db = Db::open(&dir.join("silo.db")).map_err(|x| x.to_string())?;
            app.manage(App { db: Mutex::new(db), music_dir: dir.join("Music") });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_setting, set_setting, open_url, resolve_playlist, search_playlists, open_playlist, save_playlist, history, clear_history, downloads_info, search_tracks, search_all, rate_track, liked_tracks, banned_tracks, reset_taste, all_ratings, record_play, echo_mix, suggest_artists, list_playlists,
            create_playlist, delete_playlist, playlist_tracks, add_to_playlist,
            remove_from_playlist, list_downloads, download_track
        ])
        .run(tauri::generate_context!())
        .expect("ошибка запуска");
}
