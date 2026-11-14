//! Состояние приложения и логика плеера (то же, что App.tsx): очередь, перемешивание, повтор, «Эхо»,
//! сессия, настройки, оценки, тосты, диалоги, горячие клавиши, маршрутизация воспроизведения.
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

use eframe::egui;
use serde::{Deserialize, Serialize};
use silo_core::echo::SeedArtist;
use silo_core::model::{Playlist, Track};

use crate::audio::{Engine, Event, Fx, PState, Src};
use crate::backend::{self as be, App as Backend};
use crate::embed::Embed;
use crate::i18n::tr;
use crate::pages::Pages;
use crate::theme::Pal;
use crate::widgets::Images;

pub type Tracks = Arc<Vec<Track>>;

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct Prefs {
    pub shuffle: bool,
    pub repeat: String,
    pub autoplay: bool,
    pub crossfade: f32,
    pub gapless: bool,
    pub normalize: bool,
    pub eq_on: bool,
    pub eq: Vec<f32>,
    pub quality: String,
    pub theme: String,
    pub lang: String,
    pub volume: f32,
    pub onboarded: bool,
    pub diversity: String,
    pub tastes: Vec<String>,
    pub artists: Vec<SeedArtist>,
    pub show_video: bool,
    pub lang_strict: bool,
    pub rate: f32,
    pub notify: bool,
    pub autostart: bool,
    pub eq_auto: bool,
    // только нативная версия
    /// embed — официальный плеер YouTube; ytdlp — без плеера, через yt-dlp
    pub yt_mode: String,
    pub ytdlp_path: String,
    pub ytdlp_video: bool,
    /// браузер для cookies yt-dlp ("" — не использовать)
    pub ytdlp_cookies: String,
    /// вопрос про cookies браузера уже задавался (спрашиваем один раз)
    pub ytdlp_cookies_asked: bool,
    /// показывать плашки с ошибками (для разработчика; обычный пользователь их не видит, всё пишется в diag.log)
    pub show_errors: bool,
    pub tray: bool,
    /// режим YouTube уже переведён на yt-dlp по умолчанию (однократно)
    pub yt_default_v2: bool,
    /// апскейл звука: 0 нет, 1 слабый, 2 средний, 3 сильный
    pub upscale: u8,
}
impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            shuffle: false, repeat: "off".into(), autoplay: true, crossfade: 0.0, gapless: true, normalize: false, eq_on: false, eq: vec![0.0; 10],
            quality: "normal".into(), theme: "amber".into(), lang: "ru".into(), volume: 0.8, onboarded: false, diversity: "balanced".into(), tastes: vec![], artists: vec![],
            show_video: false, lang_strict: true, rate: 1.0, notify: true, autostart: false, eq_auto: false,
            yt_mode: "ytdlp".into(), ytdlp_path: String::new(), ytdlp_video: true, ytdlp_cookies: String::new(), ytdlp_cookies_asked: false, show_errors: false, tray: true, yt_default_v2: true, upscale: 0,
        }
    }
}
pub fn quality_format(q: &str) -> String { match q { "low" => "mp31", "high" => "flac", _ => "mp32" }.into() }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab { Home, Search, Browse, Echo, Liked, Playlists, Downloads, History, Import, Banned, Settings, Artist }
pub const NAV: [(&str, &[(Tab, &str, &str)]); 3] = [
    ("navListen", &[(Tab::Home, "home", "home"), (Tab::Search, "search", "search"), (Tab::Browse, "browse", "browse"), (Tab::Echo, "echo", "echo")]),
    ("navMine", &[(Tab::Liked, "liked", "heart"), (Tab::Playlists, "playlists", "playlist"), (Tab::Downloads, "downloads", "download"), (Tab::History, "history", "history")]),
    ("navMore", &[(Tab::Import, "import", "import"), (Tab::Banned, "banned", "ban"), (Tab::Settings, "settings", "settings")]),
];

#[derive(Clone, Copy, PartialEq)]
pub enum ToastKind { Ok, Error, Info }
pub type Job = Box<dyn FnOnce(&mut SiloApp) + Send>;
pub struct Toast { pub id: u64, pub kind: ToastKind, pub text: String, pub until: f64, pub action: Option<(String, Job)> }

pub enum Ask {
    Confirm { title: String, body: String, ok: String, danger: bool, check: Option<(String, bool)>, on_ok: Box<dyn FnOnce(&mut SiloApp, bool)> },
    Prompt { title: String, value: String, placeholder: String, ok: String, on_ok: Box<dyn FnOnce(&mut SiloApp, String)> },
    Info(Track),
}

/// Действия из списков и меню: применяются после отрисовки кадра.
pub enum Act {
    Play(Tracks, usize),
    PlayNext(Track),
    AddQueue(Track),
    Radio(Track),
    Rate(Track, i32),
    Artist(String),
    Info(Track),
    Copy(String, String),
    Download(Track),
    AddToPl(i64, String, Track),
    RemoveFromPl(i64, Track),
    MovePl(i64, usize, usize),
    Go(Tab),
    Wave(Option<Vec<String>>, Option<Vec<SeedArtist>>),
    OpenUrl(String),
}

pub struct Lyrics { pub id: String, pub lines: Vec<(f64, String)>, pub plain: String, pub loading: bool }

#[derive(Clone, Copy, PartialEq)]
pub enum Route { None, Engine, Embed, Waiting }

pub struct SiloApp {
    pub ctx: egui::Context,
    pub rt: tokio::runtime::Handle,
    pub be: Arc<Backend>,
    pub eng: Engine,
    ev_rx: Receiver<Event>,
    pub tx: Sender<Job>,
    rx: Receiver<Job>,
    pub data_dir: std::path::PathBuf,
    pub imgs: Images,
    pub pal: Pal,
    pub prefs: Prefs,
    prefs_saved: String,
    pub loaded: bool,
    pub tab: Tab,
    pub artist: String,
    pub queue: Vec<Track>,
    pub order: Vec<usize>,
    pub pos: isize,
    pub wave: bool,
    wave_seed: Option<Vec<String>>,
    wave_artists: Option<Vec<SeedArtist>>,
    wave_busy: bool,
    wave_retry: f64,
    pub wave_starting: bool,
    pub ratings: HashMap<String, i32>,
    pub playlists: Vec<Playlist>,
    pub blocked: Vec<String>,
    pub show_q: bool,
    pub show_l: bool,
    pub mini: bool,
    pub viz: bool,
    pub stage: bool,
    pub karaoke: bool,
    pub toasts: Vec<Toast>,
    toast_id: u64,
    pub asks: Vec<Ask>,
    pub pages: Pages,
    pub embed: Option<Embed>,
    pub embed_rect: Option<[f32; 4]>,
    pub video: Option<crate::video::Video>,
    pub lyrics: Option<Lyrics>,
    pub route: Route,
    pub token: u64,
    cur_key: String,
    preloaded: Option<(u64, String)>,
    hold: Option<(String, f64)>,
    pub ps: PState,
    pub online: bool,
    last_session_save: f64,
    session_dirty: bool,
    last_prefs_check: f64,
    pub plat: crate::platform::Platform,
    pub dropping: bool,
    pub seek_drag: Option<f32>,
    pub seek_pending: Option<(f64, f64)>,
    pub vol_drag: bool,
    pub quit: bool,
    pub ytdlp_ok: Option<Result<String, String>>,
    /// проверка версии yt-dlp уже идёт в фоне
    pub ytdlp_checking: bool,
    notified_for: String,
    resolving: String,
    embed_lyrics_for: String,
    pub demo: Option<crate::demo::Demo>,
    pub embed_pending: Option<(Track, f64)>,
    pub bench_step: u8,
    pub selftest_at: f64,
    fresh_previews: std::collections::HashSet<String>,
    /// звук на диске: следующие 2 трека скачиваются заранее, улучшенные версии лежат рядом
    pub cache: crate::cache::Cache,
    /// «Средний» апскейл (нейросеть Apollo): установка, очередь, статус
    pub enh: crate::enhance::Enhancer,
    prefetch_at: f64,
    trim_at: f64,
    preload_gen: u64,
    preloaded_enh: bool,
    cur_enhanced: bool,
    /// чем играет текущий трек (для проверок и журнала)
    pub last_src: String,
    /// какой уровень апскейла сейчас применён в движке (0 нет, 1 достройка верха)
    pub last_fx_up: u8,
    /// до какого момента (по часам окна) звук не берём у yt-dlp: YouTube просит подтвердить, что это не бот
    pub yt_blocked_until: f64,
    /// короткое сообщение про бот-проверку уже показывалось в этой сессии
    pub bot_toast_shown: bool,
    /// время последнего сообщения о другой ошибке yt-dlp
    pub yt_err_toast_at: f64,
    /// треки YouTube, которые yt-dlp не смог отдать: играют официальным плеером
    force_embed: HashSet<String>,
}

impl SiloApp {
    pub fn new(cc: &eframe::CreationContext<'_>, rt: tokio::runtime::Handle, be: Arc<Backend>, data_dir: std::path::PathBuf) -> SiloApp {
        let ctx = cc.egui_ctx.clone();
        crate::theme::fonts(&ctx);
        let (eng, ev_rx) = Engine::start(rt.clone());
        let c2 = ctx.clone();
        eng.set_repaint(Box::new(move || c2.request_repaint()));
        let (tx, rx) = std::sync::mpsc::channel();
        let prefs: Prefs = be::get_setting(&be, "prefs".into()).ok().and_then(|x| serde_json::from_str(&x).ok()).unwrap_or_default();
        let mut prefs = prefs;
        if !prefs.yt_default_v2 { prefs.yt_mode = "ytdlp".into(); prefs.yt_default_v2 = true; }
        let pal = Pal::get(&prefs.theme);
        pal.apply(&ctx);
        let plat = crate::platform::Platform::new(cc, &ctx);
        // кэш звука и нейросеть живут в локальных данных (не в роуминге): они большие и переживают переустановку
        let local = std::env::var_os("SILO_LOCAL_DIR").map(std::path::PathBuf::from).unwrap_or_else(|| std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir).join("app.silo.player.native"));
        let cache = crate::cache::Cache::new(&local.join("cache"));
        let enh = crate::enhance::Enhancer::new(local.join("ai"), rt.clone(), ctx.clone());
        let mut app = SiloApp {
            cache, enh, prefetch_at: 0.0, trim_at: 0.0, preload_gen: 0, preloaded_enh: false, cur_enhanced: false, last_src: String::new(), last_fx_up: 0, yt_blocked_until: 0.0, bot_toast_shown: false, yt_err_toast_at: -1000.0, force_embed: HashSet::new(),
            imgs: Images::new(rt.clone()),
            ctx, rt, be, eng, ev_rx, tx, rx, data_dir, pal,
            prefs_saved: serde_json::to_string(&prefs).unwrap_or_default(), prefs, loaded: true,
            tab: Tab::Home, artist: String::new(), queue: vec![], order: vec![], pos: -1,
            wave: false, wave_seed: None, wave_artists: None, wave_busy: false, wave_retry: 0.0, wave_starting: false,
            ratings: HashMap::new(), playlists: vec![], blocked: vec![],
            show_q: false, show_l: false, mini: false, viz: false, stage: false, karaoke: false,
            toasts: vec![], toast_id: 0, asks: vec![], pages: Pages::default(),
            embed: None, embed_rect: None, video: None, lyrics: None, route: Route::None, token: 0, cur_key: String::new(), preloaded: None, hold: None,
            ps: PState::default(), online: true, last_session_save: 0.0, session_dirty: false, last_prefs_check: 0.0, plat, dropping: false,
            seek_drag: None, seek_pending: None, vol_drag: false, quit: false, ytdlp_ok: None, ytdlp_checking: false, notified_for: String::new(), resolving: String::new(), embed_lyrics_for: String::new(), demo: crate::demo::Demo::from_env(), embed_pending: None, bench_step: 0, selftest_at: 0.0, fresh_previews: Default::default(),
        };
        app.ratings = be::all_ratings(&app.be).unwrap_or_default();
        app.blocked = be::get_setting(&app.be, "blocked_artists".into()).ok().and_then(|x| serde_json::from_str(&x).ok()).unwrap_or_default();
        app.refresh_playlists();
        // продолжить с того же места: очередь, трек и позиция — на паузе
        if let Some(ss) = be::get_setting(&app.be, "session".into()).ok().and_then(|x| serde_json::from_str::<serde_json::Value>(&x).ok()) {
            let q: Vec<Track> = serde_json::from_value(ss["queue"].clone()).unwrap_or_default();
            let o: Vec<usize> = serde_json::from_value(ss["order"].clone()).unwrap_or_default();
            let p = ss["pos"].as_i64().unwrap_or(-1);
            if !q.is_empty() && p >= 0 && (p as usize) < o.len() && o.iter().all(|i| *i < q.len()) {
                let tr = &q[o[p as usize]];
                app.hold = Some((tr.id.clone(), ss["at"].as_f64().unwrap_or(0.0)));
                app.queue = q; app.order = o; app.pos = p as isize;
            }
        }
        app.apply_engine_prefs();
        app.eng.set_volume(app.prefs.volume);
        app
    }

    pub fn route_name(&self) -> &str { match self.route { Route::None => "none", Route::Engine => "engine", Route::Embed => if self.embed.is_some() { "embed" } else { "embed-pending" }, Route::Waiting => "waiting" } }
    pub fn en(&self) -> bool { self.prefs.lang == "en" }
    pub fn t(&self, k: &str) -> &'static str { tr(k, self.en()) }

    /// Запустить фоновую задачу; результат применяется к приложению в UI-потоке.
    pub fn spawn<T: Send + 'static>(&self, fut: impl std::future::Future<Output = T> + Send + 'static, done: impl FnOnce(&mut SiloApp, T) + Send + 'static) {
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        self.rt.spawn(async move {
            let r = fut.await;
            let _ = tx.send(Box::new(move |a: &mut SiloApp| done(a, r)));
            ctx.request_repaint();
        });
    }
    pub fn toast(&mut self, text: impl Into<String>, kind: ToastKind) { self.toast_act(text, kind, None); }
    pub fn toast_act(&mut self, text: impl Into<String>, kind: ToastKind, action: Option<(String, Job)>) {
        let text = text.into();
        if text.is_empty() || self.toasts.iter().any(|x| x.text == text) { return; }
        self.toast_id += 1;
        let now = self.ctx.input(|i| i.time);
        let dur = match kind { ToastKind::Error => 8.0, _ if action.is_some() => 7.0, _ => 3.5 };
        if self.toasts.len() > 3 { self.toasts.remove(0); }
        self.toasts.push(Toast { id: self.toast_id, kind, text, until: now + dur, action });
    }
    /// Ошибка: всегда пишется в журнал diag.log, плашка показывается только с включённым «Показывать ошибки» (Настройки → Внешний вид).
    pub fn err(&mut self, e: impl std::fmt::Display) {
        let m = e.to_string();
        diag(&m);
        if self.prefs.show_errors { self.toast(m, ToastKind::Error); }
    }

    pub fn refresh_playlists(&mut self) {
        let b = self.be.clone();
        self.spawn(async move { be::list_playlists(&b) }, |a, r| { if let Ok(v) = r { a.playlists = v; } });
    }
    pub fn set_blocked(&mut self, name: &str, on: bool) {
        self.blocked.retain(|x| !x.eq_ignore_ascii_case(name));
        if on { self.blocked.push(name.to_string()); }
        let _ = be::set_setting(&self.be, "blocked_artists".into(), serde_json::to_string(&self.blocked).unwrap_or_default());
    }

    pub fn cur(&self) -> Option<&Track> {
        if self.pos < 0 { return None; }
        self.order.get(self.pos as usize).and_then(|i| self.queue.get(*i))
    }
    pub fn upcoming(&self) -> Vec<Track> {
        if self.pos < 0 { return vec![]; }
        self.order.iter().skip(self.pos as usize + 1).filter_map(|i| self.queue.get(*i).cloned()).collect()
    }

    fn shuffled(n: usize, first: Option<usize>) -> Vec<usize> {
        let mut r: Vec<usize> = (0..n).filter(|i| Some(*i) != first).collect();
        let mut seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(7) | 1;
        for i in (1..r.len()).rev() {
            seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17;
            let j = (seed % (i as u64 + 1)) as usize;
            r.swap(i, j);
        }
        match first { Some(f) => std::iter::once(f).chain(r).collect(), None => r }
    }
    pub fn shuffle_list(v: &[Track]) -> Vec<Track> { Self::shuffled(v.len(), None).into_iter().map(|i| v[i].clone()).collect() }

    pub fn play(&mut self, list: Vec<Track>, i: usize, keep_wave: bool) {
        if list.is_empty() { return; }
        if !keep_wave { self.wave = false; }
        let n = list.len();
        self.queue = list;
        if self.prefs.shuffle { self.order = Self::shuffled(n, Some(i)); self.pos = 0; } else { self.order = (0..n).collect(); self.pos = i as isize; }
        self.cur_key.clear(); // тот же трек ещё раз — начать заново
        self.session_dirty = true;
    }
    pub fn enqueue(&mut self, t: Track, next: bool) {
        if self.queue.is_empty() || self.pos < 0 { self.play(vec![t], 0, false); return; }
        self.queue.push(t);
        let at = if next { self.pos as usize + 1 } else { self.order.len() };
        self.order.insert(at.min(self.order.len()), self.queue.len() - 1);
        self.session_dirty = true;
    }
    fn next_pos(&self, manual: bool) -> isize {
        if self.prefs.repeat == "one" && !manual { return self.pos; }
        if (self.pos + 1) < self.order.len() as isize { return self.pos + 1; }
        if self.prefs.repeat == "all" { 0 } else { -1 }
    }
    pub fn next_track(&self) -> Option<&Track> {
        let n = self.next_pos(false);
        if self.pos < 0 || n < 0 || self.prefs.repeat == "one" { return None; }
        self.order.get(n as usize).and_then(|i| self.queue.get(*i))
    }
    fn record(&mut self, fraction: f64) {
        if let Some(c) = self.cur().cloned() {
            let b = self.be.clone();
            let f = fraction;
            self.spawn(async move { let _ = be::record_play(&b, c.clone(), f); if f >= 0.5 { let _ = be::scrobble(&b, c).await; } }, |_, _| {});
        }
    }
    pub fn skip(&mut self, d: i32) {
        let ps = self.ps;
        if ps.dur > 0.0 && ps.pos / ps.dur < 0.8 { self.record(ps.pos / ps.dur); }
        if d < 0 {
            if ps.pos > 3.0 { self.seek(0.0); } else { self.pos = (self.pos - 1).max(0); self.cur_key.clear(); }
            return;
        }
        let n = self.next_pos(true);
        if n >= 0 { self.pos = n; self.cur_key.clear(); }
    }
    fn ended(&mut self) {
        self.record(1.0);
        if self.prefs.repeat == "one" { self.seek(0.0); self.set_playing(true); return; }
        let n = self.next_pos(false);
        if n >= 0 { self.pos = n; self.cur_key.clear(); return; }
        if self.prefs.autoplay || self.wave {
            let at = self.order.len() as isize;
            self.extend_wave(Some(at));
        }
    }
    pub fn toggle(&mut self) {
        match self.route {
            Route::Embed => if self.embed_pending.is_some() { self.open_pending_embed(true); } else { let pl = self.ps.playing; if let Some(e) = self.embed.as_mut() { if pl { e.pause() } else { e.play() } } },
            Route::Engine => { self.eng.toggle(); if let Some(v) = &self.video { v.set_playing(self.eng.playing()); } }
            _ => { if self.cur().is_some() && self.route == Route::None { self.cur_key.clear(); } }
        }
    }
    pub fn set_playing(&mut self, p: bool) {
        match self.route { Route::Embed => if p && self.embed_pending.is_some() { self.open_pending_embed(true); } else if let Some(e) = self.embed.as_mut() { if p { e.play() } else { e.pause() } }, Route::Engine => self.eng.set_playing(p), _ => {} }
    }
    pub fn seek(&mut self, sec: f64) {
        let now = self.ctx.input(|i| i.time);
        self.seek_pending = Some((sec, now + 2.5));
        match self.route { Route::Embed => if let Some(p) = self.embed_pending.as_mut() { p.1 = sec; } else if let Some(e) = self.embed.as_mut() { e.seek(sec) }, Route::Engine => self.eng.seek(sec), _ => {} }
        if let Some(v) = &self.video { v.seek(sec); }
        self.ps.pos = sec;
    }
    pub fn set_volume_live(&mut self, v: f32) {
        self.eng.set_volume(v);
        if let Some(e) = self.embed.as_mut() { e.volume(v); }
    }

    pub fn rate(&mut self, t: Track, v: i32) {
        let b = self.be.clone();
        let t2 = t.clone();
        self.spawn(async move { be::rate_track(&b, t2, v) }, move |a, r| {
            if let Err(e) = r { a.err(e); return; }
            if v != 0 { a.ratings.insert(t.id.clone(), v); } else { a.ratings.remove(&t.id); }
            a.pages.dirty_ratings = true;
            if v == -2 {
                // забаненный трек сразу перестаёт играть и уходит из очереди
                if a.cur().is_some_and(|c| c.id == t.id) { a.skip(1); }
                let p = a.pos.max(0) as usize;
                let q = a.queue.clone();
                let mut k = 0;
                a.order.retain(|i| { let keep = k <= p || q.get(*i).is_none_or(|x| x.id != t.id); k += 1; keep });
                // 10+ забаненных треков исполнителя: предложить не рекомендовать его
                if !t.artist.is_empty() {
                    let n = be::banned_tracks(&a.be).unwrap_or_default().iter().filter(|x| x.artist.eq_ignore_ascii_case(&t.artist)).count();
                    if n >= 10 && !a.blocked.iter().any(|x| x.eq_ignore_ascii_case(&t.artist)) {
                        let body = a.t("blockAsk").replace("{n}", &n.to_string()).replace("{a}", &t.artist);
                        let artist = t.artist.clone();
                        a.asks.push(Ask::Confirm { title: a.t("blockArtist").into(), body, ok: a.t("blockArtist").into(), danger: false, check: None, on_ok: Box::new(move |a, _| a.set_blocked(&artist, true)) });
                    }
                }
            }
        });
    }

    pub fn reset_taste(&mut self, clear_groups: bool) {
        let _ = be::reset_taste(&self.be, clear_groups);
        if clear_groups { self.ratings.clear(); }
        self.pages.onb_redo = true;
        self.prefs.onboarded = false;
        self.prefs.tastes.clear();
        self.prefs.artists.clear();
        self.prefs.eq = vec![0.0; 10];
        self.prefs.eq_on = false;
    }

    fn echo_args(&self, tastes: &[String], artists: &[SeedArtist], diversity: &str) -> (Option<String>, Option<Vec<String>>, Option<String>, Option<String>, Option<Vec<SeedArtist>>, Option<bool>) {
        let s = crate::taste::seed_for(tastes, artists);
        (Some(quality_format(&self.prefs.quality)), Some(s.tags), s.lang, Some(diversity.to_string()), Some(s.artists), Some(self.prefs.lang_strict))
    }
    /// Подборка «Эхо» (как echoMixFull в React).
    pub fn echo_fut(&self, tastes: &[String], artists: &[SeedArtist], diversity: &str) -> impl std::future::Future<Output = Result<be::EchoOut, String>> + Send + 'static {
        let (af, tags, lang, div, arts, strict) = self.echo_args(tastes, artists, diversity);
        let b = self.be.clone();
        async move { be::echo_mix(&b, 0, af, tags, lang, div, arts, strict).await }
    }
    pub fn start_wave(&mut self, seed: Option<Vec<String>>, artists: Option<Vec<SeedArtist>>) {
        self.wave_seed = seed.clone();
        self.wave_artists = artists.clone();
        let tastes = seed.clone().unwrap_or_else(|| self.prefs.tastes.clone());
        let arts = artists.unwrap_or_else(|| if seed.is_some() { vec![] } else { self.prefs.artists.clone() });
        self.wave_starting = true;
        let f = self.echo_fut(&tastes, &arts, &self.prefs.diversity.clone());
        self.spawn(f, |a, r| {
            a.wave_starting = false;
            match r {
                Ok(o) if !o.tracks.is_empty() => { a.wave = true; a.play(o.tracks, 0, true); }
                Ok(_) => a.err(a.t("echoEmpty")),
                Err(e) => a.err(e),
            }
        });
    }
    pub fn stop_wave(&mut self) { self.wave = false; }
    pub fn radio(&mut self, t: &Track) {
        let arts = if t.artist.is_empty() { None } else { Some(vec![SeedArtist { name: t.artist.clone(), tags: t.tags.iter().take(3).cloned().collect() }]) };
        self.start_wave(Some(self.prefs.tastes.clone()), arts);
    }
    /// Подгрузить следующую порцию «Эхо» в конец очереди; then_pos — сразу перейти на первый новый трек.
    fn extend_wave(&mut self, then_pos: Option<isize>) {
        if self.wave_busy { return; }
        self.wave_busy = true;
        let tastes = self.wave_seed.clone().unwrap_or_else(|| self.prefs.tastes.clone());
        let arts = self.wave_artists.clone().unwrap_or_else(|| if self.wave_seed.is_some() { vec![] } else { self.prefs.artists.clone() });
        let f = self.echo_fut(&tastes, &arts, &self.prefs.diversity.clone());
        self.spawn(f, move |a, r| {
            a.wave_busy = false;
            match r {
                Ok(o) => {
                    let have: std::collections::HashSet<String> = a.queue.iter().map(|x| x.id.clone()).collect();
                    let more: Vec<Track> = o.tracks.into_iter().filter(|x| !have.contains(&x.id)).collect();
                    if more.is_empty() { a.wave_retry = a.ctx.input(|i| i.time) + 30.0; return; }
                    let base = a.queue.len();
                    let n = more.len();
                    a.queue.extend(more);
                    a.order.extend(base..base + n);
                    if let Some(p) = then_pos { a.pos = p; a.cur_key.clear(); }
                    a.session_dirty = true;
                }
                Err(e) => { a.err(e); a.wave_retry = a.ctx.input(|i| i.time) + 30.0; }
            }
        });
    }

    pub fn apply(&mut self, act: Act) {
        match act {
            Act::Play(l, i) => self.play(l.to_vec(), i, false),
            Act::PlayNext(t) => { self.enqueue(t, true); self.toast(self.t("addedNext"), ToastKind::Ok); }
            Act::AddQueue(t) => { self.enqueue(t, false); self.toast(self.t("addedQueue"), ToastKind::Ok); }
            Act::Radio(t) => self.radio(&t),
            Act::Rate(t, v) => self.rate(t, v),
            Act::Artist(n) => { self.artist = n; self.tab = Tab::Artist; self.pages.artist_for.clear(); }
            Act::Info(t) => self.asks.push(Ask::Info(t)),
            Act::Copy(s, msg) => { self.ctx.copy_text(s); self.toast(msg, ToastKind::Ok); }
            Act::Download(t) => {
                self.toast(format!("{}: {}", self.t("downloading"), t.name), ToastKind::Info);
                let b = self.be.clone();
                let name = t.name.clone();
                // YouTube-треки скачиваются через библиотеку yt-dlp, остальные (Jamendo и т. п.) обычным способом
                let exe = if t.source == "youtube" { self.ytdlp_exe() } else { None };
                self.spawn(async move { match exe { Some(exe) => be::download_youtube(&b, exe, t).await, None => be::download_track(&b, t).await } }, move |a, r| match r {
                    Ok(_) => { a.toast(format!("{}: {name}", a.t("downloaded")), ToastKind::Ok); a.pages_reset_downloads(); }
                    Err(e) => { diag(&e); a.toast(format!("Не удалось скачать: {name}"), ToastKind::Error); }
                });
            }
            Act::AddToPl(id, name, t) => {
                let b = self.be.clone();
                self.spawn(async move { be::add_to_playlist(&b, id, t) }, move |a, r| match r { Ok(_) => { a.refresh_playlists(); a.toast(format!("{} «{name}»", a.t("addedTo")), ToastKind::Ok); } Err(e) => a.err(e) });
            }
            Act::RemoveFromPl(id, t) => {
                let b = self.be.clone();
                let t2 = t.clone();
                self.spawn(async move { be::remove_from_playlist(&b, id, t2.id.clone()) }, move |a, r| match r {
                    Ok(_) => {
                        a.refresh_playlists(); a.pages.pl_reload = true;
                        let t3 = t.clone();
                        a.toast_act(a.t("removedFromPl"), ToastKind::Ok, Some((a.t("undo").into(), Box::new(move |a: &mut SiloApp| { let _ = be::add_to_playlist(&a.be, id, t3); a.refresh_playlists(); a.pages.pl_reload = true; }))));
                    }
                    Err(e) => a.err(e),
                });
            }
            Act::MovePl(id, from, to) => { if let Err(e) = be::reorder_playlist(&self.be, id, from, to) { self.err(e); } self.pages.pl_reload = true; }
            Act::Go(t) => self.tab = t,
            Act::Wave(s, a) => self.start_wave(s, a),
            Act::OpenUrl(u) => { if let Err(e) = crate::platform::open_url(&u) { self.err(e); } }
        }
    }

    pub fn apply_engine_prefs(&mut self) {
        crate::ytdlp::set_quality(&self.prefs.quality);
        let p = &self.prefs;
        let mut eq = [0f32; 10];
        for (i, v) in p.eq.iter().take(10).enumerate() { eq[i] = *v; }
        // «Средний»: если трек уже улучшен нейросетью, лёгкий эксайтер не нужен; пока не готов, играет «Слабый» вариант
        let up = match p.upscale { 0 => 0, 2 => if self.cur_enhanced { 0 } else { 1 }, _ => 1 };
        self.last_fx_up = up;
        self.eng.set_fx(Fx { eq_on: p.eq_on, eq, normalize: p.normalize, upscale: up });
        self.eng.set_crossfade(p.crossfade, p.gapless);
        self.eng.set_speed(if p.rate > 0.0 { p.rate } else { 1.0 });
    }

    pub fn ytdlp_exe(&self) -> Option<String> {
        if self.prefs.yt_mode != "ytdlp" { return None; }
        // поиск файла — раз в несколько секунд, а не каждый кадр
        thread_local! { static C: std::cell::RefCell<(f64, String, Option<String>)> = Default::default(); }
        let now = self.ctx.input(|i| i.time);
        C.with(|c| {
            let mut c = c.borrow_mut();
            if now - c.0 > 5.0 || c.1 != self.prefs.ytdlp_path || now < c.0 { *c = (now, self.prefs.ytdlp_path.clone(), crate::ytdlp::find(&self.prefs.ytdlp_path).map(|p| p.to_string_lossy().to_string())); }
            c.2.clone()
        })
    }
    /// Файл с оригиналом трека для улучшения: свой локальный файл или скачанный в кэш.
    fn orig_file(&self, t: &Track) -> Option<std::path::PathBuf> {
        if t.source == "local" { let p = std::path::PathBuf::from(&t.audio); return p.is_file().then_some(p); }
        self.cache.orig(&crate::cache::key(&t.id))
    }
    /// Как играть трек: улучшенный файл из кэша, скачанный оригинал, поток или встроенный официальный плеер. Второе значение: звучит улучшенная версия.
    fn src_for(&self, t: &Track) -> Option<(Src, bool)> {
        let key = crate::cache::key(&t.id);
        let file = |p: std::path::PathBuf| Src::File(p.to_string_lossy().to_string());
        let embed_only = t.source == "soundcloud" || (t.source == "youtube" && (self.ytdlp_exe().is_none() || self.force_embed.contains(&t.id)));
        if !embed_only {
            if self.prefs.upscale == 2 && self.enh.ready() { if let Some(p) = self.cache.enhanced(&key) { return Some((file(p), true)); } }
            if t.source != "local" { if let Some(p) = self.cache.orig(&key) { return Some((file(p), false)); } }
        }
        match t.source.as_str() {
            "youtube" if !self.force_embed.contains(&t.id) => self.ytdlp_exe().map(|exe| (Src::Ytdlp(t.id.trim_start_matches("youtube:").to_string(), exe), false)),
            "youtube" => None,
            "soundcloud" => None,
            "local" => Some((Src::File(t.audio.clone()), false)),
            _ if !t.audio.is_empty() => Some((Src::Url(t.audio.clone()), false)),
            _ => None,
        }
    }

    /// Скачать трек в кэш заранее (YouTube через yt-dlp или прямой адрес), если его там ещё нет.
    fn ensure_cached(&mut self, t: &Track, now: f64) {
        let key = crate::cache::key(&t.id);
        if matches!(t.source.as_str(), "local" | "soundcloud" | "deezer") || self.cache.orig(&key).is_some() { return; }
        let (cache, ctx) = (self.cache.clone(), self.ctx.clone());
        if t.source == "youtube" {
            let Some(exe) = self.ytdlp_exe() else { return };
            if !cache.begin(&key, now) { return; }
            let vid = t.id.trim_start_matches("youtube:").to_string();
            self.rt.spawn_blocking(move || {
                let (tmp, dst) = (cache.tmp_for(&key, "m4a"), cache.path_for(&key, "m4a"));
                let r = crate::ytdlp::audio_file(&exe, &vid, &tmp).and_then(|_| std::fs::rename(&tmp, &dst).map_err(|e| e.to_string()));
                cache.finish(&key, r.is_ok().then_some(dst), now);
                ctx.request_repaint();
            });
        } else if t.audio.starts_with("http") {
            if !cache.begin(&key, now) { return; }
            let url = t.audio.clone();
            self.rt.spawn(async move {
                let ext = crate::cache::ext_of_url(&url);
                let (tmp, dst) = (cache.tmp_for(&key, ext), cache.path_for(&key, ext));
                let r = crate::cache::download_http(&url, &tmp, &dst).await;
                cache.finish(&key, r.ok(), now);
                ctx.request_repaint();
            });
        }
    }

    /// Раз в секунду: готовим два следующих трека (скачать, улучшить), чистим кэш по размеру.
    fn prefetch_tick(&mut self, now: f64) {
        if now - self.prefetch_at < 1.0 { return; }
        self.prefetch_at = now;
        if self.pos < 0 || self.queue.is_empty() { return; }
        let p = self.pos as usize;
        let ahead: Vec<Track> = (1..=2).filter_map(|k| self.order.get(p + k).and_then(|i| self.queue.get(*i)).cloned()).collect();
        let mut keep: HashSet<String> = ahead.iter().map(|t| crate::cache::key(&t.id)).collect();
        if let Some(c) = self.cur() { keep.insert(crate::cache::key(&c.id)); }
        for t in &ahead { self.ensure_cached(t, now); }
        if self.prefs.upscale == 2 && self.enh.ready() {
            let jobs: Vec<crate::enhance::EJob> = ahead.iter().filter_map(|t| {
                let k = crate::cache::key(&t.id);
                Some(crate::enhance::EJob { orig: self.orig_file(t)?, out: self.cache.enhanced_path(&k), key: k, secs: t.duration as f64 })
            }).collect();
            self.enh.want(jobs);
        }
        if now - self.trim_at > 60.0 {
            self.trim_at = now;
            let c = self.cache.clone();
            self.rt.spawn_blocking(move || { c.trim(false, 1_500_000_000, &keep); c.trim(true, 2_500_000_000, &keep); });
        }
    }

    /// Каждый кадр: смена трека, подбор YouTube для Deezer, предзагрузка, события движка, состояние.
    pub fn tick(&mut self) {
        // раз в сутки тихо обновляем свой yt-dlp (YouTube часто меняется, старая версия перестаёт работать)
        if self.prefs.yt_mode == "ytdlp" { if let Some(exe) = self.ytdlp_exe() { crate::ytdlp::auto_update(&self.rt, exe); } }
        // yt-dlp не найден: один раз скачиваем официальный и сверяем контрольную сумму
        if self.prefs.yt_mode == "ytdlp" && !crate::ytdlp::download_attempted() && self.ytdlp_exe().is_none() { crate::ytdlp::start_download(&self.rt); }
        // «Средний» нельзя держать включённым без установленной нейросети
        if self.prefs.upscale == 2 && matches!(self.enh.state(), crate::enhance::Install::Missing | crate::enhance::Install::Failed(_)) { self.prefs.upscale = 1; }
        // режим yt-dlp: ядро ходит в YouTube только через yt-dlp, без ключа и квоты
        *be::YTDLP.lock().unwrap() = (self.prefs.yt_mode == "ytdlp").then(|| self.ytdlp_exe().unwrap_or_default());
        while let Ok(j) = self.rx.try_recv() { j(self); }
        if let Some(e) = self.embed.as_mut() { e.pump(); }
        let now = self.ctx.input(|i| i.time);
        // события движка
        while let Ok(ev) = self.ev_rx.try_recv() {
            match ev {
                Event::Ended(t) if t == self.token && self.route == Route::Engine => self.ended(),
                Event::Advanced(t) if Some(t) == self.preloaded.as_ref().map(|x| x.0) => {
                    // gapless/кроссфейд: следующий трек уже звучит
                    self.record(1.0);
                    let n = self.next_pos(false);
                    if n >= 0 { self.pos = n; }
                    self.token = t;
                    self.cur_key = format!("{}#{}", self.cur().map(|c| c.id.clone()).unwrap_or_default(), self.pos);
                    self.cur_enhanced = self.preloaded_enh;
                    self.last_src = if self.cur_enhanced { "улучшенный файл (предзагрузка)".into() } else { "предзагрузка".into() };
                    self.preloaded = None;
                    self.apply_engine_prefs();
                    self.on_track_changed();
                }
                Event::Error(t, m) if t == self.token => {
                    let cur = self.cur().cloned();
                    let yt = cur.as_ref().filter(|c| c.source == "youtube" && self.ytdlp_exe().is_some() && !self.force_embed.contains(&c.id));
                    let now = self.ctx.input(|i| i.time);
                    if let Some(c) = yt {
                        // yt-dlp не отдал звук: этот трек играет официальный плеер YouTube, битый файл из кэша удаляем
                        self.force_embed.insert(c.id.clone());
                        if let Some(p) = self.cache.orig(&crate::cache::key(&c.id)) { let _ = std::fs::remove_file(p); }
                        // пользователю ничего не показываем: трек просто играет через официальный плеер, причина в журнале
                        diag(&format!("yt-dlp не отдал звук ({}): {m}", c.id));
                        if self.prefs.show_errors && now - self.yt_err_toast_at > 120.0 {
                            self.yt_err_toast_at = now;
                            let short: String = m.chars().take(70).collect();
                            self.toast(format!("yt-dlp: {short}"), ToastKind::Info);
                        }
                        self.preloaded = None;
                        self.cur_key.clear();
                    } else {
                        self.err(format!("{}: {m}", cur.as_ref().map(|c| c.name.clone()).unwrap_or_default()));
                        if cur.as_ref().is_some_and(|c| c.source == "youtube") { self.ended(); }
                    }
                }
                _ => {}
            }
        }
        // смена текущего трека
        let key = self.cur().map(|c| format!("{}#{}", c.id, self.pos)).unwrap_or_default();
        if key != self.cur_key {
            self.cur_key = key;
            self.start_current();
        }
        // состояние воспроизведения
        let mut st = match self.route {
            Route::Engine => self.eng.state(),
            Route::Embed => match (&self.embed_pending, &self.embed) {
                (Some((t, at)), _) => PState { playing: false, pos: *at, dur: t.duration as f64, token: self.token },
                (None, Some(e)) => { let s = e.st.lock().unwrap(); PState { playing: s.playing, pos: s.pos, dur: s.dur, token: self.token } }
                _ => PState::default(),
            },
            _ => PState::default(),
        };
        if st.dur <= 0.0 { st.dur = self.cur().map(|c| c.duration as f64).unwrap_or(0.0); }
        if let Some((at, until)) = self.seek_pending { if (st.pos - at).abs() < 1.5 || now > until { self.seek_pending = None; } else { st.pos = at; } }
        self.ps = st;
        if self.route == Route::Embed {
            let (ended, error) = self.embed.as_ref().map(|e| { let mut s = e.st.lock().unwrap(); let r = (s.ended, s.error.take()); s.ended = false; r }).unwrap_or((false, None));
            if let Some(code) = error { self.err(format!("YouTube: видео нельзя воспроизвести во встроенном плеере ({code}), пропускаю")); self.ended(); }
            else if ended { self.ended(); }
        }
        // два следующих трека готовим заранее (скачивание, «Средний» апскейл)
        self.prefetch_tick(now);
        // предзагрузка следующего (gapless и кроссфейд)
        if self.route == Route::Engine && (self.prefs.gapless || self.prefs.crossfade > 0.0) {
            if let Some(n) = self.next_track().cloned() {
                let ok = n.source != "deezer" || n.kind == "preview";
                if let (true, Some((src, enh))) = (ok, self.src_for(&n)) {
                    // YouTube без файла: ждём, пока подготовка его скачает, и не запускаем второй yt-dlp
                    let waiting = matches!(src, Src::Ytdlp(..)) && self.cache.is_inflight(&crate::cache::key(&n.id));
                    let same = self.preloaded.as_ref().is_some_and(|p| p.1 == n.id);
                    // улучшенная версия появилась после предзагрузки оригинала: заменяем
                    let upgrade = same && enh && !self.preloaded_enh;
                    if !waiting && (!same || upgrade) {
                        self.preload_gen += 1;
                        let tok = self.token + 1_000_000 + self.preload_gen;
                        self.eng.preload(tok, src, n.duration as f64, self.prefs.rate.max(0.25));
                        self.preloaded = Some((tok, n.id.clone()));
                        self.preloaded_enh = enh;
                    }
                }
            }
        }
        // «Эхо»: когда впереди меньше 6 треков, заранее подгружаем следующую порцию
        if self.wave && self.pos >= 0 && (self.order.len() as isize - self.pos - 1) < 6 && !self.wave_busy && now > self.wave_retry { self.extend_wave(None); }
        // сессия и настройки
        if self.session_dirty && now - self.last_session_save > 0.5 || now - self.last_session_save > 10.0 { self.save_session(); }
        if now - self.last_prefs_check > 0.4 {
            self.last_prefs_check = now;
            let s = serde_json::to_string(&self.prefs).unwrap_or_default();
            if s != self.prefs_saved {
                let old: Prefs = serde_json::from_str(&self.prefs_saved).unwrap_or_default();
                self.prefs_saved = s.clone();
                let _ = be::set_setting(&self.be, "prefs".into(), s);
                if old.theme != self.prefs.theme { self.pal = Pal::get(&self.prefs.theme); self.pal.apply(&self.ctx); }
                if old.autostart != self.prefs.autostart { crate::platform::set_autostart(self.prefs.autostart); }
                if old.yt_mode != self.prefs.yt_mode || old.ytdlp_path != self.prefs.ytdlp_path { self.ytdlp_ok = None; }
                self.apply_engine_prefs();
            }
        }
        // тосты
        self.toasts.retain(|t| t.until > now);
        // видео: включено/выключено
        self.sync_video();
        if self.ps.playing { crate::widgets::repaint(&self.ctx, 500); }
    }

    fn save_session(&mut self) {
        self.last_session_save = self.ctx.input(|i| i.time);
        self.session_dirty = false;
        if self.queue.is_empty() { return; }
        let q: Vec<&Track> = self.queue.iter().take(500).collect();
        let o: Vec<usize> = self.order.iter().copied().filter(|i| *i < 500).collect();
        let v = serde_json::json!({ "queue": q, "order": o, "pos": self.pos, "at": self.ps.pos });
        let _ = be::set_setting(&self.be, "session".into(), v.to_string());
    }

    fn start_current(&mut self) {
        let Some(tr) = self.cur().cloned() else {
            self.eng.stop(); self.close_embed(); self.route = Route::None; return;
        };
        let hold = self.hold.take().filter(|h| h.0 == tr.id);
        let (start, autoplay) = match &hold { Some(h) => (h.1, false), None => (0.0, true) };
        self.seek_pending = None;
        // трек Deezer: видео YouTube ищется только сейчас (один поиск, повторно — из кэша в базе)
        if tr.source == "deezer" && tr.kind != "preview" {
            self.eng.stop(); self.close_embed(); self.route = Route::Waiting;
            if self.resolving == tr.id { return; }
            self.resolving = tr.id.clone();
            let b = self.be.clone();
            let id = tr.id.clone();
            if let Some(h) = hold { self.hold = Some(h); }
            self.spawn(async move { be::resolve_track(&b, tr).await }, move |a, r| {
                a.resolving.clear();
                match r {
                    Ok(t) => {
                        if t.kind == "preview" { a.err(a.t("previewOnly")); }
                        if let Some(h) = a.hold.as_mut() { if h.0 == id { h.0 = t.id.clone(); } }
                        for x in a.queue.iter_mut() { if x.id == id { *x = t.clone(); } }
                        a.cur_key.clear();
                    }
                    Err(e) => a.err(e),
                }
            });
            return;
        }
        // превью Deezer: ссылка в кэше поиска со временем протухает (403), берём свежую по id трека (без ключа и квоты)
        if tr.source == "deezer" && tr.kind == "preview" && !self.fresh_previews.contains(&tr.id) {
            self.eng.stop(); self.close_embed(); self.route = Route::Waiting;
            let id = tr.id.clone();
            let num = id.trim_start_matches("deezer:").to_string();
            if let Some(h) = hold { self.hold = Some(h); }
            self.spawn(async move {
                let v: serde_json::Value = silo_core::http().get(format!("https://api.deezer.com/track/{num}")).send().await.ok()?.json().await.ok()?;
                v["preview"].as_str().filter(|x| !x.is_empty()).map(String::from)
            }, move |a, r| {
                a.fresh_previews.insert(id.clone());
                if let Some(u) = r { for x in a.queue.iter_mut() { if x.id == id { x.audio = u.clone(); } } }
                a.cur_key.clear();
            });
            return;
        }
        self.token += 1;
        let speed = if self.prefs.rate > 0.0 { self.prefs.rate } else { 1.0 };
        match self.src_for(&tr) {
            Some((src, enh)) => {
                self.close_embed();
                let pre = matches!(&self.preloaded, Some((_, id)) if *id == tr.id);
                self.cur_enhanced = if pre { self.preloaded_enh } else { enh };
                self.last_src = format!("{}{:?}", if pre { "предзагрузка " } else { "" }, src);
                self.apply_engine_prefs();
                let tok = match &self.preloaded { Some((t, id)) if *id == tr.id => *t, _ => self.token };
                self.token = tok;
                self.preloaded = None;
                self.eng.play(tok, src, tr.duration as f64, start, autoplay, speed);
                self.route = Route::Engine;
            }
            None if tr.source == "youtube" || tr.source == "soundcloud" => {
                self.eng.stop();
                self.route = Route::Embed;
                // восстановленная сессия на паузе: веб-плеер создаётся только по нажатию «играть» (меньше памяти при старте)
                self.embed_pending = Some((tr.clone(), start));
                if autoplay { self.open_pending_embed(true); }
            }
            None => { self.err(format!("{}: нет звука", tr.name)); self.route = Route::None; }
        }
        self.video = None;
        self.on_track_changed();
    }

    /// Заголовок окна, уведомление, системная плашка, эквалайзер под жанр, текст песни.
    fn on_track_changed(&mut self) {
        let Some(c) = self.cur().cloned() else { return };
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!("{} — {}", c.name, c.artist)));
        self.plat.set_metadata(&c);
        let minimized = self.ctx.input(|i| i.viewport().minimized.unwrap_or(false) || !i.viewport().focused.unwrap_or(true));
        let _ = minimized; // уведомлений о треках нет (по просьбе)
        if false && self.prefs.eq_auto {
            for tg in &c.tags { if let Some(n) = crate::taste::search(tg, 1).first() { if let Some(eq) = crate::taste::eq_of(&n.id) { self.prefs.eq = eq.iter().map(|x| *x as f32).collect(); break; } } }
        }
        if self.show_l || self.karaoke { self.load_lyrics(); }
    }

    pub fn load_lyrics(&mut self) {
        let Some(c) = self.cur().cloned() else { return };
        if self.lyrics.as_ref().is_some_and(|l| l.id == c.id) { return; }
        self.lyrics = Some(Lyrics { id: c.id.clone(), lines: vec![], plain: String::new(), loading: true });
        let id = c.id.clone();
        self.spawn(async move { be::lyrics(c.artist, c.name, c.duration).await }, move |a, r| {
            let (synced, plain) = r.unwrap_or_default();
            let lines: Vec<(f64, String)> = synced.lines().filter_map(|l| {
                let l = l.trim();
                let rest = l.strip_prefix('[')?;
                let (ts, text) = rest.split_once(']')?;
                let (m, s) = ts.split_once(':')?;
                Some((m.parse::<f64>().ok()? * 60.0 + s.parse::<f64>().ok()?, text.trim().to_string()))
            }).collect();
            if a.lyrics.as_ref().is_some_and(|l| l.id == id) { a.lyrics = Some(Lyrics { id, lines, plain, loading: false }); a.embed_lyrics_for.clear(); }
        });
    }

    /// Создать официальный плеер для отложенного трека и запустить его.
    fn open_pending_embed(&mut self, autoplay: bool) {
        let Some((tr, start)) = self.embed_pending.take() else { return };
        self.ensure_embed();
        let (vol, speed) = (self.prefs.volume, if self.prefs.rate > 0.0 { self.prefs.rate } else { 1.0 });
        if let Some(e) = self.embed.as_mut() {
            if tr.source == "youtube" { e.load_youtube(tr.id.trim_start_matches("youtube:"), start, autoplay, vol); e.rate(speed); }
            else { e.load_soundcloud(&tr.url, vol); }
        }
        self.embed_rect = None;
        self.embed_lyrics_for.clear();
    }
    pub fn ensure_embed(&mut self) {
        if self.embed.is_some() { return; }
        let dir = self.data_dir.join("webview");
        match self.plat.make_embed(dir) {
            Ok(e) => self.embed = Some(e),
            Err(e) => self.err(format!("WebView2: {e}")),
        }
    }
    fn close_embed(&mut self) {
        self.embed_pending = None;
        // плеер YouTube держит в памяти видео и отдельный процесс: убираем его, когда играет не YouTube
        if self.embed.take().is_some() { self.embed_rect = None; }
    }
    /// Караоке в режиме официального плеера: строки передаются странице плеера.
    pub fn sync_embed_lyrics(&mut self) {
        if self.embed.is_none() { return; }
        let key = format!("{}:{}:{}", self.lyrics.as_ref().map(|l| l.id.as_str()).unwrap_or(""), self.karaoke, self.lyrics.as_ref().map(|l| l.loading).unwrap_or(true));
        if key == self.embed_lyrics_for { return; }
        self.embed_lyrics_for = key;
        let lines = self.lyrics.as_ref().map(|l| l.lines.clone()).unwrap_or_default();
        let on = self.karaoke && self.stage;
        if let Some(e) = self.embed.as_mut() { e.lyrics(&lines, on); }
    }
    /// Клип YouTube в режиме без плеера (yt-dlp + Media Foundation), только когда он нужен на экране.
    fn sync_video(&mut self) {
        let want = self.stage && self.route == Route::Engine && self.prefs.ytdlp_video && self.cur().is_some_and(|c| c.source == "youtube");
        if !want { if self.video.is_some() { self.video = None; } return; }
        if self.video.is_some() { return; }
        let (Some(exe), Some(c)) = (self.ytdlp_exe(), self.cur().cloned()) else { return };
        self.video = Some(crate::video::Video::open(exe, c.id.trim_start_matches("youtube:").to_string(), self.ctx.clone(), self.rt.clone(), self.ps.pos, self.eng.playing()));
    }

    /// Горячие клавиши (как в React-версии), когда фокус не в поле ввода и нет диалога.
    pub fn hotkeys(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() || !self.asks.is_empty() { return; }
        use egui::Key;
        let ev: Vec<(Key, egui::Modifiers)> = ctx.input(|i| i.events.iter().filter_map(|e| match e { egui::Event::Key { key, pressed: true, modifiers, .. } => Some((*key, *modifiers)), _ => None }).collect());
        for (k, m) in ev {
            if m.alt { continue; }
            match k {
                Key::Space => self.toggle(),
                Key::ArrowRight if m.ctrl => self.skip(1),
                Key::ArrowLeft if m.ctrl => self.skip(-1),
                Key::ArrowUp => { self.prefs.volume = (self.prefs.volume + 0.05).min(1.0); self.set_volume_live(self.prefs.volume); }
                Key::ArrowDown => { self.prefs.volume = (self.prefs.volume - 0.05).max(0.0); self.set_volume_live(self.prefs.volume); }
                Key::ArrowRight | Key::ArrowLeft => { if self.ps.dur > 0.0 { let d = if k == Key::ArrowRight { 5.0 } else { -5.0 }; self.seek((self.ps.pos + d).clamp(0.0, self.ps.dur)); } }
                Key::F if m.ctrl => { self.tab = Tab::Search; self.pages.focus_search = true; }
                Key::K if m.ctrl => { self.tab = Tab::Search; self.pages.focus_search = true; }
                Key::Slash => { self.tab = Tab::Search; self.pages.focus_search = true; }
                Key::Comma if m.ctrl => self.tab = Tab::Settings,
                Key::M => { self.prefs.volume = if self.prefs.volume == 0.0 { 0.8 } else { 0.0 }; self.set_volume_live(self.prefs.volume); }
                Key::L => { if let Some(c) = self.cur().cloned() { self.rate(c, 1); } }
                Key::S => { self.prefs.shuffle = !self.prefs.shuffle; self.shuffle_changed(); }
                Key::R => { self.prefs.repeat = match self.prefs.repeat.as_str() { "off" => "all", "all" => "one", _ => "off" }.into(); }
                Key::Q => { self.show_q = !self.show_q; self.show_l = false; }
                Key::T => { self.show_l = !self.show_l; self.show_q = false; if self.show_l { self.load_lyrics(); } }
                Key::K => { self.karaoke = !self.karaoke; self.stage = self.karaoke || self.stage && !self.karaoke; if self.karaoke { self.stage = true; self.load_lyrics(); } }
                Key::V => { self.stage = !self.stage; if !self.stage { self.karaoke = false; } }
                Key::Escape => { self.show_q = false; self.show_l = false; if self.stage { self.stage = false; self.karaoke = false; } }
                k if m.ctrl => {
                    let digits = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9];
                    if let Some(n) = digits.iter().position(|d| *d == k) {
                        let tabs: Vec<Tab> = NAV.iter().flat_map(|g| g.1.iter().map(|x| x.0)).collect();
                        if let Some(t) = tabs.get(n) { self.tab = *t; }
                    }
                }
                _ => {}
            }
        }
    }

    pub fn shuffle_changed(&mut self) {
        if self.queue.is_empty() || self.pos < 0 { return; }
        // смена shuffle на лету: текущий трек остаётся первым
        let c = self.order[self.pos as usize];
        if self.prefs.shuffle { self.order = Self::shuffled(self.queue.len(), Some(c)); self.pos = 0; } else { self.order = (0..self.queue.len()).collect(); self.pos = c as isize; }
        self.cur_key = self.cur().map(|c| format!("{}#{}", c.id, self.pos)).unwrap_or_default();
        self.preloaded = None;
        self.eng.cancel_preload();
        self.session_dirty = true;
    }
    pub fn move_queue(&mut self, from: usize, to: usize) {
        if from == to { return; }
        let base = self.pos as usize + 1;
        if base + from >= self.order.len() || base + to >= self.order.len() { return; }
        let m = self.order.remove(base + from);
        self.order.insert(base + to, m);
        self.session_dirty = true;
    }
    pub fn remove_from_queue(&mut self, i: usize) {
        let at = self.pos as usize + 1 + i;
        if at < self.order.len() { self.order.remove(at); self.session_dirty = true; }
    }
    pub fn mark_session(&mut self) { self.session_dirty = true; }

    /// Сохранить файл через системный диалог.
    pub fn save_file(&mut self, name: &str, ext: &str, body: String) {
        let ctx = self.ctx.clone();
        let tx = self.tx.clone();
        let name = name.to_string();
        let ext = ext.to_string();
        std::thread::spawn(move || {
            let f = rfd::FileDialog::new().set_file_name(&name).add_filter(&ext, &[ext.as_str()]).save_file();
            let r = f.map(|p| std::fs::write(&p, body).map(|_| p.display().to_string()));
            let _ = tx.send(Box::new(move |a: &mut SiloApp| match r { Some(Ok(p)) => a.toast(format!("{}: {p}", a.t("fileSaved")), ToastKind::Ok), Some(Err(e)) => a.err(e), None => {} }));
            ctx.request_repaint();
        });
    }
}


/// Тихий журнал диагностики: %LOCALAPPDATA%\app.silo.player.native\diag.log (до 1 МБ, потом начинается заново). Ключи и cookies сюда не пишутся.
pub fn diag(msg: &str) {
    use std::io::Write;
    let dir = std::env::var_os("SILO_LOCAL_DIR").map(std::path::PathBuf::from).unwrap_or_else(|| std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir).join("app.silo.player.native"));
    let _ = std::fs::create_dir_all(&dir);
    let p = dir.join("diag.log");
    if std::fs::metadata(&p).map(|m| m.len() > 1_000_000).unwrap_or(false) { let _ = std::fs::remove_file(&p); }
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&p) { let _ = writeln!(f, "[{secs}] {msg}"); }
}
