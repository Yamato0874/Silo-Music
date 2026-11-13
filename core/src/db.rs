use crate::{model::*, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub struct Db {
    pub conn: Connection,
}

impl Db {
    /// Версия схемы. Данные пользователя (вкусы, история, оценки, плейлисты, ключи) живут в этом файле в AppData
    /// и переживают обновление программы; перед миграцией на новую версию делается копия.
    pub const SCHEMA_VERSION: i32 = 2;

    pub fn open(path: &Path) -> Result<Self> {
        if path.exists() {
            let old: i32 = Connection::open(path)
                .and_then(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))
                .unwrap_or(0);
            if old < Self::SCHEMA_VERSION { Self::backup(path, &format!("v{old}")); }
            Self::backup(path, &format!("daily-{}", now_secs() / 86400));
        }
        let db = Self::init(Connection::open(path)?)?;
        db.conn.execute_batch(&format!("PRAGMA user_version={};", Self::SCHEMA_VERSION))?;
        Ok(db)
    }
    /// Копия базы в `backups/` рядом с ней (одна на тег; остаются 10 самых свежих). Ошибки копирования не мешают запуску.
    fn backup(path: &Path, tag: &str) {
        let Some(dir) = path.parent().map(|p| p.join("backups")) else { return };
        if std::fs::create_dir_all(&dir).is_err() { return; }
        let to = dir.join(format!("silo-{tag}.db"));
        if to.exists() { return; }
        let _ = std::fs::copy(path, &to);
        // ключи сервисов в резервные копии не попадают
        if let Ok(c) = Connection::open(&to) {
            let _ = c.execute_batch("DELETE FROM settings WHERE key IN ('youtube_api_key','soundcloud_client_id','soundcloud_client_secret','jamendo_client_id','google_cse_key','google_cse_cx','listenbrainz_token'); VACUUM;");
        }
        if let Ok(rd) = std::fs::read_dir(&dir) {
            let mut v: Vec<_> = rd.filter_map(|e| e.ok()).filter_map(|e| { let m = e.metadata().ok()?.modified().ok()?; Some((m, e.path())) }).collect();
            v.sort_by(|a, b| b.0.cmp(&a.0));
            for (_, p) in v.into_iter().skip(10) { let _ = std::fs::remove_file(p); }
        }
    }
    pub fn memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }
    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS plays(
               id INTEGER PRIMARY KEY, track_id TEXT NOT NULL, tags TEXT NOT NULL,
               completed INTEGER NOT NULL, ts INTEGER NOT NULL DEFAULT (strftime('%s','now')));
             CREATE TABLE IF NOT EXISTS playlists(id INTEGER PRIMARY KEY, name TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS playlist_tracks(
               playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
               pos INTEGER NOT NULL, track TEXT NOT NULL, PRIMARY KEY(playlist_id, pos));
             CREATE TABLE IF NOT EXISTS downloads(
               track_id TEXT PRIMARY KEY, path TEXT NOT NULL, track TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS seen(track_id TEXT NOT NULL, key TEXT NOT NULL, pop REAL NOT NULL, PRIMARY KEY(track_id, key));
             CREATE INDEX IF NOT EXISTS seen_key_pop ON seen(key, pop);
             CREATE TABLE IF NOT EXISTS cache(key TEXT PRIMARY KEY, ts INTEGER NOT NULL, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS yt_match(track_id TEXT PRIMARY KEY, video_id TEXT NOT NULL, via TEXT NOT NULL, ts INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS ratings(
               track_id TEXT PRIMARY KEY, value INTEGER NOT NULL, tags TEXT NOT NULL,
               ts INTEGER NOT NULL DEFAULT (strftime('%s','now')));",
        )?;
        // миграции для баз, созданных на прошлых этапах (ошибка «колонка уже есть» игнорируется)
        let _ = conn.execute("ALTER TABLE plays ADD COLUMN track TEXT", []);
        let _ = conn.execute("ALTER TABLE plays ADD COLUMN fraction REAL", []);
        let _ = conn.execute("ALTER TABLE ratings ADD COLUMN track TEXT", []);
        // индексы под Эхо, историю и итоги; кэш старше 31 дня не нужен (правила YouTube API — 30 дней)
        let _ = conn.execute_batch("CREATE INDEX IF NOT EXISTS plays_ts ON plays(ts); CREATE INDEX IF NOT EXISTS plays_track ON plays(track_id); CREATE INDEX IF NOT EXISTS ratings_ts ON ratings(ts);
            DELETE FROM cache WHERE ts < strftime('%s','now') - 31*86400; PRAGMA optimize;");
        let db = Self { conn };
        db.migrate_secrets();
        Ok(db)
    }

    /// Кэш ответов внешних API (экономия квоты YouTube). max_age — секунды.
    pub fn cache_get(&self, key: &str, max_age: i64) -> Result<Option<String>> {
        let min_ts = now_secs() as i64 - max_age;
        Ok(self.conn.query_row("SELECT value FROM cache WHERE key=? AND ts>=?", params![key, min_ts], |r| r.get(0)).optional()?)
    }
    /// Найденное видео YouTube для трека Deezer: повторное воспроизведение не тратит квоту.
    /// Найденное через YouTube Data API живёт 30 дней (правила API), через Piped/Invidious — без срока.
    pub fn yt_match_get(&self, track_id: &str) -> Result<Option<String>> {
        let min_ts = now_secs() as i64 - 30 * 86400;
        Ok(self.conn.query_row("SELECT video_id FROM yt_match WHERE track_id=? AND (via<>'api' OR ts>=?)", params![track_id, min_ts], |r| r.get(0)).optional()?)
    }
    pub fn yt_match_put(&self, track_id: &str, video_id: &str, via: &str) -> Result<()> {
        self.conn.execute("INSERT OR REPLACE INTO yt_match(track_id,video_id,via,ts) VALUES(?,?,?,?)", params![track_id, video_id, via, now_secs() as i64])?;
        Ok(())
    }
    pub fn cache_put(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO cache(key,ts,value) VALUES(?,?,?) ON CONFLICT(key) DO UPDATE SET ts=excluded.ts, value=excluded.value",
            params![key, now_secs() as i64, value],
        )?;
        // не даём кэшу расти бесконечно
        self.conn.execute("DELETE FROM cache WHERE ts < ?", [now_secs() as i64 - 31 * 86400])?;
        Ok(())
    }

    /// Ключи сервисов хранятся зашифрованными (DPAPI) и расшифровываются при чтении.
    /// Если расшифровать не удалось (база перенесена на другой ПК или учётную запись), ключ считается пустым, запись в базе не трогается.
    /// Миграция: открытые ключи из старых версий перешифровываются при первом запуске.
    /// Каждый ключ проверяется обратной расшифровкой; при сбое остаётся как был, данные не теряются.
    fn migrate_secrets(&self) {
        for k in crate::secret::SECRET_KEYS {
            let raw: Option<String> = self.conn.query_row("SELECT value FROM settings WHERE key=?", [k], |r| r.get(0)).optional().ok().flatten();
            let Some(v) = raw.filter(|v| !v.is_empty() && !crate::secret::is_protected(v)) else { continue };
            if let Some(p) = crate::secret::protect(&v).filter(|p| crate::secret::unprotect(p).as_deref() == Some(v.as_str())) {
                let _ = self.conn.execute("UPDATE settings SET value=? WHERE key=?", [p.as_str(), k]);
            }
        }
    }
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let v: Option<String> = self.conn.query_row("SELECT value FROM settings WHERE key=?", [key], |r| r.get(0)).optional()?;
        Ok(match v {
            Some(x) if crate::secret::is_secret(key) => Some(crate::secret::unprotect(&x).unwrap_or_default()),
            other => other,
        })
    }
    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let enc;
        let value = if crate::secret::is_secret(key) {
            // шифруем, только если обратная расшифровка даёт то же самое; иначе храним как есть
            enc = crate::secret::protect(value).filter(|p| crate::secret::unprotect(p).as_deref() == Some(value));
            enc.as_deref().unwrap_or(value)
        } else { value };
        self.conn.execute(
            "INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [key, value],
        )?;
        Ok(())
    }

    /// fraction — доля прослушанного трека (0..1)
    pub fn record_play(&self, t: &Track, fraction: f64) -> Result<()> {
        let f = fraction.clamp(0.0, 1.0);
        self.conn.execute(
            "INSERT INTO plays(track_id,tags,completed,track,fraction) VALUES(?,?,?,?,?)",
            params![t.id, serde_json::to_string(&t.tags)?, f >= 0.8, serde_json::to_string(t)?, f],
        )?;
        Ok(())
    }
    /// (трек, доля прослушанного, возраст в секундах); для старых записей без доли — 1.0 если дослушан, иначе 0.3
    pub fn play_events(&self, limit: u32) -> Result<Vec<(Track, f64, i64)>> {
        let mut s = self.conn.prepare(
            "SELECT track_id, tags, track, COALESCE(fraction, CASE completed WHEN 1 THEN 1.0 ELSE 0.3 END), strftime('%s','now')-ts FROM plays ORDER BY id DESC LIMIT ?",
        )?;
        let rows = s.query_map([limit], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, f64>(3)?, r.get::<_, i64>(4)?)))?;
        let mut out = vec![];
        for r in rows {
            let (id, tags, track, f, a) = r?;
            let t = match track.and_then(|x| serde_json::from_str::<Track>(&x).ok()) {
                Some(t) => t,
                None => Track::stub(&id, serde_json::from_str(&tags)?),
            };
            out.push((t, f, a));
        }
        Ok(out)
    }
    pub fn played_ids(&self, limit: u32) -> Result<Vec<String>> {
        let mut s = self.conn.prepare("SELECT DISTINCT track_id FROM plays ORDER BY id DESC LIMIT ?")?;
        let v = s.query_map([limit], |r| r.get(0))?.collect::<std::result::Result<_, _>>()?;
        Ok(v)
    }

    /// value: 1 нравится, -1 не нравится, 0 снять оценку
    pub fn set_rating(&self, t: &Track, value: i32) -> Result<()> {
        if value == 0 {
            self.conn.execute("DELETE FROM ratings WHERE track_id=?", [&t.id])?;
        } else {
            self.conn.execute(
                "INSERT OR REPLACE INTO ratings(track_id,value,tags,track) VALUES(?,?,?,?)",
                params![t.id, value, serde_json::to_string(&t.tags)?, serde_json::to_string(t)?],
            )?;
        }
        Ok(())
    }
    pub fn ratings(&self) -> Result<std::collections::HashMap<String, i32>> {
        let mut s = self.conn.prepare("SELECT track_id,value FROM ratings")?;
        let v = s.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<std::result::Result<_, _>>()?;
        Ok(v)
    }
    /// (трек, оценка, возраст в секундах)
    pub fn rating_events(&self) -> Result<Vec<(Track, i32, i64)>> {
        let mut s = self.conn.prepare("SELECT track_id,tags,track,value,strftime('%s','now')-ts FROM ratings")?;
        let rows = s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, i32>(3)?, r.get::<_, i64>(4)?)))?;
        let mut v = vec![];
        for r in rows {
            let (id, tags, track, x, a) = r?;
            let t = match track.and_then(|y| serde_json::from_str::<Track>(&y).ok()) {
                Some(t) => t,
                None => Track::stub(&id, serde_json::from_str(&tags)?),
            };
            v.push((t, x, a));
        }
        Ok(v)
    }
    /// Группа по оценке: 1 — «Понравившиеся», -1 — дизлайк (мягкий сигнал), -2 — «Бан» (трек исключается из «Эхо»). Новые сверху.
    pub fn rated_tracks(&self, value: i32) -> Result<Vec<Track>> {
        let mut s = self.conn.prepare("SELECT track FROM ratings WHERE value=? AND track IS NOT NULL ORDER BY ts DESC, rowid DESC")?;
        let rows = s.query_map([value], |r| r.get::<_, String>(0))?;
        let mut v = vec![];
        for r in rows { v.push(serde_json::from_str(&r?)?); }
        Ok(v)
    }

    /// Запоминает популярность увиденных треков: из этого строится «жанр-относительная» оценка качественности.
    /// Ключи: "<источник>:<тег>" для каждого тега и "<источник>:*" для всего источника.
    pub fn note_seen(&self, tracks: &[Track]) -> Result<()> {
        for t in tracks.iter().filter(|t| t.pop > 0.0) {
            let mut keys: Vec<String> = t.tags.iter().map(|g| format!("{}:{}", t.source, g)).collect();
            keys.push(format!("{}:*", t.source));
            for k in keys {
                self.conn.execute("INSERT OR REPLACE INTO seen(track_id,key,pop) VALUES(?,?,?)", params![t.id, k, t.pop])?;
            }
        }
        Ok(())
    }
    /// Процентиль популярности трека среди увиденных треков того же жанра (0..1; 1 — топ жанра).
    /// Берётся жанр с достаточной выборкой (≥10), иначе вся площадка; None, если данных нет.
    pub fn quality_of(&self, t: &Track) -> Result<Option<f64>> {
        if t.pop <= 0.0 { return Ok(None); }
        let genre_keys: Vec<String> = t.tags.iter().map(|g| format!("{}:{}", t.source, g)).collect();
        let pct = |k: &str| -> Result<Option<f64>> {
            let (n, below, equal): (f64, f64, f64) = self.conn.query_row(
                "SELECT COUNT(*), COALESCE(SUM(pop<?1),0), COALESCE(SUM(pop=?1),0) FROM seen WHERE key=?2",
                params![t.pop, k], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            Ok(if n >= 10.0 { Some((below + 0.5 * equal) / n) } else { None })
        };
        // сначала жанры с достаточной выборкой (их оценки усредняются); вся площадка — только если жанровых данных нет
        let mut ps = vec![];
        for k in &genre_keys { if let Some(p) = pct(k)? { ps.push(p); } }
        if ps.is_empty() { if let Some(p) = pct(&format!("{}:*", t.source))? { ps.push(p); } }
        Ok(if ps.is_empty() { None } else { Some(ps.iter().sum::<f64>() / ps.len() as f64) })
    }

    /// Момент (unix, секунды) последнего сброса вкусов: события старше него модель не учитывает.
    pub fn taste_epoch(&self) -> Result<i64> {
        Ok(self.get_setting("taste_epoch")?.and_then(|x| x.parse().ok()).unwrap_or(0))
    }
    /// «Сбросить вкусы»: очищает историю и обнуляет влияние прошлых оценок на модель.
    /// Группы «Понравившиеся/Не нравится» остаются, пока clear_groups = false.
    pub fn reset_taste(&self, clear_groups: bool) -> Result<()> {
        self.conn.execute("DELETE FROM plays", [])?;
        let now: i64 = self.conn.query_row("SELECT CAST(strftime('%s','now') AS INTEGER)", [], |r| r.get(0))?;
        self.set_setting("taste_epoch", &now.to_string())?;
        if clear_groups {
            self.conn.execute("DELETE FROM ratings", [])?;
        }
        Ok(())
    }

    pub fn search_downloads(&self, q: &str) -> Result<Vec<Track>> {
        let q = q.to_lowercase();
        Ok(self
            .list_downloads()?
            .into_iter()
            .map(|(t, _)| t)
            .filter(|t| format!("{} {} {}", t.name, t.artist, t.album).to_lowercase().contains(&q))
            .collect())
    }

    /// История прослушиваний без повторов, новые сверху.
    pub fn history(&self, limit: u32) -> Result<Vec<Track>> {
        let mut s = self.conn.prepare("SELECT track FROM plays WHERE track IS NOT NULL ORDER BY id DESC LIMIT 500")?;
        let rows = s.query_map([], |r| r.get::<_, String>(0))?;
        let mut v: Vec<Track> = vec![];
        for r in rows {
            let t: Track = serde_json::from_str(&r?)?;
            if !v.iter().any(|x| x.id == t.id) { v.push(t); }
            if v.len() as u32 >= limit { break; }
        }
        Ok(v)
    }
    pub fn clear_history(&self) -> Result<()> {
        self.conn.execute("DELETE FROM plays", [])?;
        Ok(())
    }
    /// Сохранить подборку к себе (плейлист + все треки).
    pub fn save_playlist(&self, name: &str, tracks: &[Track]) -> Result<i64> {
        let id = self.create_playlist(name)?;
        for t in tracks { self.add_to_playlist(id, t)?; }
        Ok(id)
    }

    pub fn create_playlist(&self, name: &str) -> Result<i64> {
        self.conn.execute("INSERT INTO playlists(name) VALUES(?)", [name])?;
        Ok(self.conn.last_insert_rowid())
    }
    pub fn rename_playlist(&self, id: i64, name: &str) -> Result<()> {
        self.conn.execute("UPDATE playlists SET name=? WHERE id=?", rusqlite::params![name, id])?;
        Ok(())
    }
    pub fn delete_playlist(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM playlists WHERE id=?", [id])?;
        Ok(())
    }
    pub fn list_playlists(&self) -> Result<Vec<Playlist>> {
        let mut s = self.conn.prepare(
            "SELECT p.id,p.name,(SELECT COUNT(*) FROM playlist_tracks WHERE playlist_id=p.id) FROM playlists p ORDER BY p.id",
        )?;
        let v = s
            .query_map([], |r| Ok(Playlist { id: r.get(0)?, name: r.get(1)?, count: r.get(2)? }))?
            .collect::<std::result::Result<_, _>>()?;
        Ok(v)
    }
    pub fn add_to_playlist(&self, id: i64, t: &Track) -> Result<()> {
        let pos: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(pos),-1)+1 FROM playlist_tracks WHERE playlist_id=?",
            [id],
            |r| r.get(0),
        )?;
        self.conn.execute(
            "INSERT INTO playlist_tracks(playlist_id,pos,track) VALUES(?,?,?)",
            params![id, pos, serde_json::to_string(t)?],
        )?;
        Ok(())
    }
    pub fn reorder_playlist(&self, id: i64, from: usize, to: usize) -> Result<()> {
        let mut v = self.playlist_tracks(id)?;
        if from >= v.len() || to >= v.len() { return Ok(()); }
        let t = v.remove(from); v.insert(to, t);
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM playlist_tracks WHERE playlist_id=?", [id])?;
        for (i, t) in v.iter().enumerate() { tx.execute("INSERT INTO playlist_tracks(playlist_id,pos,track) VALUES(?,?,?)", params![id, i as i64, serde_json::to_string(t)?])?; }
        tx.commit()?;
        Ok(())
    }
    pub fn playlist_tracks(&self, id: i64) -> Result<Vec<Track>> {
        let mut s = self.conn.prepare("SELECT track FROM playlist_tracks WHERE playlist_id=? ORDER BY pos")?;
        let rows = s.query_map([id], |r| r.get::<_, String>(0))?;
        let mut v = vec![];
        for r in rows {
            v.push(serde_json::from_str(&r?)?);
        }
        Ok(v)
    }
    pub fn remove_from_playlist(&self, id: i64, track_id: &str) -> Result<()> {
        let tracks = self.playlist_tracks(id)?;
        self.conn.execute("DELETE FROM playlist_tracks WHERE playlist_id=?", [id])?;
        for t in tracks.iter().filter(|t| t.id != track_id) {
            self.add_to_playlist(id, t)?;
        }
        Ok(())
    }

    pub fn save_download(&self, t: &Track, path: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO downloads(track_id,path,track) VALUES(?,?,?)",
            params![t.id, path, serde_json::to_string(t)?],
        )?;
        Ok(())
    }
    pub fn list_downloads(&self) -> Result<Vec<(Track, String)>> {
        let mut s = self.conn.prepare("SELECT track,path FROM downloads ORDER BY rowid DESC")?;
        let rows = s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut v = vec![];
        for r in rows {
            let (t, p) = r?;
            v.push((serde_json::from_str(&t)?, p));
        }
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn secrets_migrate_and_read() {
        let d = Db::memory().unwrap();
        d.conn.execute("INSERT INTO settings(key,value) VALUES('youtube_api_key','plain-key')", []).unwrap();
        d.migrate_secrets();
        assert_eq!(d.get_setting("youtube_api_key").unwrap().as_deref(), Some("plain-key"));
        d.set_setting("youtube_api_key", "new-key").unwrap();
        assert_eq!(d.get_setting("youtube_api_key").unwrap().as_deref(), Some("new-key"));
        let raw: String = d.conn.query_row("SELECT value FROM settings WHERE key='youtube_api_key'", [], |r| r.get(0)).unwrap();
        if cfg!(windows) { assert!(raw.starts_with("dpapi:")); }
    }
    #[test]
    fn yt_match_roundtrip() {
        let d = Db::memory().unwrap();
        d.yt_match_put("deezer:1", "vid", "api").unwrap();
        assert_eq!(d.yt_match_get("deezer:1").unwrap().as_deref(), Some("vid"));
    }
    pub fn track(id: &str, tags: &[&str]) -> Track {
        Track {
            id: id.into(), source: "jamendo".into(), kind: "music".into(), url: "".into(), name: "n".into(), artist: "a".into(), album: "".into(), image: "".into(),
            audio: "".into(), download_url: "".into(), download_allowed: true, duration: 1,
            tags: tags.iter().map(|s| s.to_string()).collect(), license_url: "".into(), pop: 0.0,
        }
    }
    #[test]
    fn playlists_roundtrip() {
        let db = Db::memory().unwrap();
        let id = db.create_playlist("Мой").unwrap();
        db.add_to_playlist(id, &track("1", &["rock"])).unwrap();
        db.add_to_playlist(id, &track("2", &[])).unwrap();
        assert_eq!(db.list_playlists().unwrap()[0].count, 2);
        db.remove_from_playlist(id, "1").unwrap();
        assert_eq!(db.playlist_tracks(id).unwrap()[0].id, "2");
        db.delete_playlist(id).unwrap();
        assert!(db.list_playlists().unwrap().is_empty());
    }
    #[test]
    fn ratings_and_local_search() {
        let db = Db::memory().unwrap();
        let t = track("jamendo:1", &["rock"]);
        db.set_rating(&t, 1).unwrap();
        db.set_rating(&t, -1).unwrap();
        assert_eq!(db.ratings().unwrap()["jamendo:1"], -1);
        db.set_rating(&t, -2).unwrap();
        assert_eq!(db.rated_tracks(-2).unwrap().len(), 1);
        assert!(db.rated_tracks(-1).unwrap().is_empty());
        assert!(db.rated_tracks(1).unwrap().is_empty());
        db.set_rating(&t, 0).unwrap();
        assert!(db.ratings().unwrap().is_empty());
        db.save_download(&t, "/x.mp3").unwrap();
        assert_eq!(db.search_downloads("N").unwrap().len(), 1);
        assert!(db.search_downloads("zzz").unwrap().is_empty());
    }
    #[test]
    fn history_and_save() {
        let db = Db::memory().unwrap();
        let (a, b) = (track("1", &[]), track("2", &[]));
        db.record_play(&a, 1.0).unwrap();
        db.record_play(&a, 0.9).unwrap();
        db.record_play(&b, 0.1).unwrap();
        let ev = db.play_events(10).unwrap();
        assert_eq!(ev.len(), 3);
        assert_eq!(ev[0].0.id, "2");
        assert!((ev[0].1 - 0.1).abs() < 1e-9);
        assert_eq!(db.history(10).unwrap().len(), 2);
        db.clear_history().unwrap();
        assert!(db.history(10).unwrap().is_empty());
        let id = db.save_playlist("S", &[a, b]).unwrap();
        assert_eq!(db.playlist_tracks(id).unwrap().len(), 2);
    }
    #[test]
    fn quality_is_relative_to_genre() {
        let db = Db::memory().unwrap();
        let mk = |i: usize, tag: &str, pop: f64| { let mut t = track(&format!("{tag}{i}"), &[tag]); t.pop = pop; t };
        // рок: популярность 100..2000, джаз: 1..20 — одинаковая позиция в жанре даёт одинаковое качество
        let rock: Vec<Track> = (1..=20).map(|i| mk(i, "rock", i as f64 * 100.0)).collect();
        let jazz: Vec<Track> = (1..=20).map(|i| mk(i, "jazz", i as f64)).collect();
        db.note_seen(&rock).unwrap();
        db.note_seen(&jazz).unwrap();
        let (qr, qj) = (db.quality_of(&rock[18]).unwrap().unwrap(), db.quality_of(&jazz[18]).unwrap().unwrap());
        assert!(qr > 0.85 && (qr - qj).abs() < 0.2, "{qr} {qj}");
        assert!(db.quality_of(&rock[1]).unwrap().unwrap() < 0.35);
        // мало данных или неизвестная популярность — нет оценки
        assert!(db.quality_of(&track("x", &["rock"])).unwrap().is_none());
        assert!(Db::memory().unwrap().quality_of(&rock[5]).unwrap().is_none());
    }
    #[test]
    fn reset_taste_keeps_groups_unless_asked() {
        let db = Db::memory().unwrap();
        let t = track("1", &["rock"]);
        db.record_play(&t, 1.0).unwrap();
        db.set_rating(&t, 1).unwrap();
        db.reset_taste(false).unwrap();
        assert!(db.play_events(10).unwrap().is_empty());
        assert_eq!(db.rated_tracks(1).unwrap().len(), 1);
        assert!(db.taste_epoch().unwrap() > 0);
        db.reset_taste(true).unwrap();
        assert!(db.rated_tracks(1).unwrap().is_empty());
    }
    #[test]
    fn old_track_json_without_source() {
        let j = r#"{"id":"1","name":"n","artist":"a","album":"","image":"","audio":"","downloadUrl":"","downloadAllowed":false,"duration":1,"tags":[],"licenseUrl":""}"#;
        let t: Track = serde_json::from_str(j).unwrap();
        assert_eq!(t.source, "jamendo");
    }
    #[test]
    fn settings() {
        let db = Db::memory().unwrap();
        assert_eq!(db.get_setting("k").unwrap(), None);
        db.set_setting("k", "v").unwrap();
        db.set_setting("k", "w").unwrap();
        assert_eq!(db.get_setting("k").unwrap().as_deref(), Some("w"));
    }
}
