//! Страницы (то же, что views.tsx, Onboarding.tsx, Import.tsx, Keys.tsx, LibBrowser.tsx) и список треков.
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::Arc;

use eframe::egui::{self, Align, Align2, Color32, CornerRadius, FontId, Layout, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};
use silo_core::echo::SeedArtist;
use silo_core::model::{PlaylistRef, SearchParams, Track};

use crate::app::{quality_format, Act, Ask, SiloApp, Tab, ToastKind, Tracks};
use crate::backend as be;
use crate::shell::src_name;
use crate::taste;
use crate::theme::{bold, mix, paint_icon, THEMES};
use crate::widgets::*;

#[derive(Default)]
pub struct ListState { pub filter: String, pub limit: usize }

#[derive(Default)]
pub struct Pages {
    pub acts: Vec<Act>,
    pub focus_search: bool,
    pub dirty_ratings: bool,
    pub pl_reload: bool,
    pub onb_redo: bool,
    pub artist_for: String,
    loading: HashSet<&'static str>,
    lists: HashMap<String, ListState>,
    // поиск
    q: String,
    q_changed_at: f64,
    q_auto_done: String,
    sug: Vec<Track>,
    sug_for: String,
    sug_sel: i32,
    res: Option<Tracks>,
    warn: Vec<String>,
    busy: bool,
    seq: u64,
    tags: Vec<String>,
    lang: String,
    show_filters: bool,
    recent: Option<Vec<String>>,
    usage: Option<(u32, u32)>,
    usage_at: f64,
    // подборки
    bq: String,
    hits: Option<Vec<PlaylistRef>>,
    bwarn: Vec<String>,
    bbusy: bool,
    open: Option<PlaylistRef>,
    otracks: Option<Tracks>,
    osaved: bool,
    // главная
    hist: Option<Tracks>,
    liked_shelf: Option<Tracks>,
    daily: Option<Tracks>,
    disc: Option<Tracks>,
    more_d: bool,
    more_w: bool,
    daily_busy: bool,
    stats: Option<serde_json::Value>,
    stats_all: bool,
    missing: Option<Vec<String>>,
    // списки
    liked: Option<Tracks>,
    banned: Option<Tracks>,
    history: Option<Tracks>,
    downloads: Option<Tracks>,
    // плейлисты
    pl_sel: Option<i64>,
    pl_virt: String,
    pl_tracks: Option<Tracks>,
    // исполнитель
    artist_tracks: Option<Tracks>,
    // настройки
    set_tab: String,
    keys: HashMap<String, String>,
    keys_loaded: bool,
    keys_saved: bool,
    dl_dir: Option<String>,
    dl_info: (u64, String),
    folder: Option<String>,
    scan_busy: bool,
    lib_q: String,
    lib_res: Vec<Track>,
    lib_got: HashMap<String, String>,
    lib_busy: bool,
    taste_q: String,
    taste_open: Option<String>,
    taste_more: bool,
    art_q: String,
    art_hits: Vec<silo_core::artists::ArtistHit>,
    art_for: String,
    art_busy: bool,
    art_changed: f64,
    // импорт
    imp_text: String,
    imp_name: String,
    imp_rows: Vec<(crate::import::Wanted, Option<Track>)>,
    imp_urls: Vec<Track>,
    imp_busy: bool,
    imp_prog: String,
    imp_saved: bool,
    imp_stop: Arc<AtomicBool>,
    // знакомство
    onb_step: i32,
    onb_keys: Option<bool>,
    onb_tip: usize,
    onb_tastes: Vec<String>,
    onb_artists: Vec<SeedArtist>,
}

const LANGS: [(&str, &str); 5] = [("ru", "Русский"), ("en", "English"), ("es", "Español"), ("fr", "Français"), ("de", "Deutsch")];

fn summary(en: bool, v: &[Track]) -> String {
    let sec: u64 = v.iter().map(|x| x.duration as u64).sum();
    let (h, m) = (sec / 3600, ((sec % 3600) as f64 / 60.0).round() as u64);
    let tn = crate::i18n::tr("tracksN", en);
    let (hs, ms) = (crate::i18n::tr("hShort", en), crate::i18n::tr("mShort", en));
    if sec == 0 { format!("{} {tn}", v.len()) } else if h > 0 { format!("{} {tn} · {h} {hs} {m} {ms}", v.len()) } else { format!("{} {tn} · {m} {ms}", v.len()) }
}
fn greeting(en: bool) -> &'static str {
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) + 3 * 3600;
    let h = (s % 86400) / 3600;
    crate::i18n::tr(if h < 5 { "gNight" } else if h < 12 { "gMorning" } else if h < 18 { "gDay" } else { "gEvening" }, en)
}
fn day_key(off_days: i64) -> String { let d = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64 / 86400).unwrap_or(0); (d - off_days).to_string() }

impl SiloApp {
    pub fn demo_query(&mut self, q: &str) { self.pages.q = q.into(); self.pages.q_changed_at = self.ctx.input(|i| i.time); }
    pub fn demo_set_tab(&mut self, t: &str) { self.pages.set_tab = t.into(); }
    pub fn demo_first_result(&self) -> Option<Track> { self.pages.res.as_ref().and_then(|r| r.first().cloned()) }
    fn push(&mut self, a: Act) { self.pages.acts.push(a); }

    /// Загрузить данные страницы один раз (ключ — что грузим).
    fn load_once<T: Send + 'static>(&mut self, key: &'static str, have: bool, fut: impl std::future::Future<Output = T> + Send + 'static, done: impl FnOnce(&mut SiloApp, T) + Send + 'static) {
        if have || self.pages.loading.contains(key) { return; }
        self.pages.loading.insert(key);
        self.spawn(fut, move |a, r| { a.pages.loading.remove(key); done(a, r); });
    }

    // ---------- список треков ----------
    pub fn track_list(&mut self, ui: &mut Ui, key: &str, tracks: &Tracks, numbered: bool, no_filter: bool, pl: Option<i64>) {
        let pal = self.pal;
        let en = self.en();
        if tracks.is_empty() { return; }
        let st = self.pages.lists.entry(key.to_string()).or_insert(ListState { filter: String::new(), limit: 100 });
        if st.limit == 0 { st.limit = 100; }
        let mut filter = st.filter.clone();
        if tracks.len() > 15 && !no_filter {
            ui.horizontal(|ui| {
                field(ui, &pal, &mut filter, crate::i18n::tr("filter", en), Some("search"), 280.0, &format!("flt:{key}"));
                if !filter.trim().is_empty() { ui.label(RichText::new(format!("{} / {}", tracks.iter().filter(|t| matches(t, &filter)).count(), tracks.len())).color(pal.mut_)); }
            });
            ui.add_space(6.0);
        }
        let low = filter.trim().to_lowercase();
        let shown: Vec<usize> = (0..tracks.len()).filter(|i| low.is_empty() || matches(&tracks[*i], &low)).collect();
        let limit = self.pages.lists.get(key).map(|s| s.limit).unwrap_or(100);
        if let Some(s) = self.pages.lists.get_mut(key) { if s.filter != filter { s.filter = filter.clone(); s.limit = 100; } }
        if shown.is_empty() { empty(ui, &pal, "search", crate::i18n::tr("noResults", en), &format!("«{filter}»"), |_| {}); return; }
        let cur_id = self.cur().map(|c| c.id.clone()).unwrap_or_default();
        let playing = self.ps.playing;
        let ratings = self.ratings.clone();
        let pls = self.playlists.clone();
        let w = ui.available_width();
        let mut act: Option<Act> = None;
        let mut rate: Option<(Track, i32)> = None;
        for (k, &i) in shown.iter().take(limit).enumerate() {
            let t = &tracks[i];
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 56.0), Sense::click_and_drag());
            if !ui.is_rect_visible(rect) { continue; }
            let hov = resp.hovered() || resp.context_menu_opened();
            let active = t.id == cur_id;
            let r = ratings.get(&t.id).copied().unwrap_or(0);
            let p = ui.painter().clone();
            if active { p.rect_filled(rect, CornerRadius::same(10), pal.acc_soft()); } else if hov { p.rect_filled(rect, CornerRadius::same(10), pal.hover()); }
            if let (Some(_), true) = (pl, low.is_empty()) {
                resp.dnd_set_drag_payload((key.to_string(), i));
                if let Some(from) = resp.dnd_release_payload::<(String, usize)>() { if from.0 == key && from.1 != i { if let Some(id) = pl { act = Some(Act::MovePl(id, from.1, i)); } } }
                if resp.dnd_hover_payload::<(String, usize)>().is_some() { p.hline(rect.x_range(), rect.top(), Stroke::new(2.0, pal.acc)); }
            }
            let mut x = rect.left() + 8.0;
            if numbered {
                p.text(Pos2::new(x + 12.0, rect.center().y), Align2::CENTER_CENTER, (k + 1).to_string(), FontId::proportional(12.5), pal.mut_);
                x += 30.0;
            }
            let cr = Rect::from_min_size(Pos2::new(x, rect.top() + 8.0), Vec2::splat(40.0));
            paint_cover(ui, &pal, &self.imgs, &t.image, cr, 6);
            if hov || active {
                p.rect_filled(cr, CornerRadius::same(6), Color32::from_black_alpha(if hov { 110 } else { 70 }));
                if active && !hov { bars(ui, &pal, cr.shrink(11.0), playing); } else { paint_icon(&p, cr.shrink(11.0), "play", Color32::WHITE, true); }
            }
            x += 52.0;
            let acts_w = if hov { 4.0 * 30.0 + 8.0 } else { 0.0 };
            let tw = (rect.right() - x - 64.0 - acts_w).max(60.0);
            let dim = r == -2;
            paint_trunc(ui, Pos2::new(x, rect.top() + 9.0), &t.name, bold(14.0), if dim { pal.mut_ } else if active { pal.acc } else { pal.fg }, tw, Align2::LEFT_TOP);
            // бейдж источника и исполнитель
            let src = src_name(&t.source, en);
            let g = p.layout_no_wrap(src.clone(), FontId::proportional(10.5), pal.mut_);
            let br = Rect::from_min_size(Pos2::new(x, rect.top() + 31.0), g.size() + Vec2::new(10.0, 4.0));
            p.rect(br, CornerRadius::same(4), Color32::TRANSPARENT, Stroke::new(1.0, mix(pal.line, src_color(&t.source), 0.6)), egui::StrokeKind::Inside);
            p.galley(br.min + Vec2::new(5.0, 2.0), g, src_color(&t.source));
            let ar = paint_trunc(ui, Pos2::new(br.right() + 8.0, rect.top() + 30.0), &t.artist, FontId::proportional(13.0), pal.mut_, tw - br.width() - 8.0, Align2::LEFT_TOP);
            let art_resp = ui.interact(ar, resp.id.with("art"), Sense::click());
            if art_resp.hovered() { p.hline(ar.x_range(), ar.bottom(), Stroke::new(1.0, pal.mut_)); }
            if art_resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !t.artist.is_empty() { act = Some(Act::Artist(t.artist.clone())); }
            // длительность
            p.text(Pos2::new(rect.right() - 14.0, rect.center().y), Align2::RIGHT_CENTER, fmt_dur(t.duration as f64), FontId::proportional(12.5), pal.mut_);
            // оценки и меню при наведении
            if hov {
                let mut c = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(Pos2::new(rect.right() - 60.0 - acts_w, rect.top()), Pos2::new(rect.right() - 56.0, rect.bottom()))).layout(Layout::left_to_right(Align::Center)));
                for (ic, val, tip) in [("like", 1, "like"), ("dislike", -1, "dislike"), ("ban", -2, "ban")] {
                    if icon_btn(&mut c, &pal, ic, 28.0, r == val, crate::i18n::tr(tip, en)).clicked() { rate = Some((t.clone(), if r == val { 0 } else { val })); }
                }
                let mr = icon_btn(&mut c, &pal, "more", 28.0, false, crate::i18n::tr("more", en));
                egui::Popup::menu(&mr).show(|ui| { if let Some(a) = track_menu(ui, &pal, en, t, r, &pls, pl) { act = Some(a); } });
            } else if r != 0 {
                let ic = match r { 1 => "like", -1 => "dislike", _ => "ban" };
                paint_icon(&p, Rect::from_center_size(Pos2::new(rect.right() - 74.0, rect.center().y), Vec2::splat(15.0)), ic, if r > 0 { pal.acc } else { pal.danger() }, r > 0);
            }
            resp.context_menu(|ui| { if let Some(a) = track_menu(ui, &pal, en, t, r, &pls, pl) { act = Some(a); } });
            if resp.clicked() && act.is_none() && rate.is_none() { act = Some(Act::Play(Arc::new(tracks.to_vec()), i)); }
            if resp.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
        }
        if shown.len() > limit {
            ui.add_space(6.0);
            if btn_sm(ui, &pal, V::Solid, None, &format!("{} ({})", crate::i18n::tr("showMore", en), shown.len() - limit), false).clicked() {
                if let Some(s) = self.pages.lists.get_mut(key) { s.limit += 100; }
            }
        }
        if let Some((t, v)) = rate { self.push(Act::Rate(t, v)); }
        if let Some(a) = act { self.push(a); }
    }

    fn play_buttons(&mut self, ui: &mut Ui, list: &Option<Tracks>) {
        let pal = self.pal;
        let has = list.as_ref().is_some_and(|l| !l.is_empty());
        if btn_off(ui, &pal, V::Primary, Some("play"), self.t("playAll"), has).clicked() { if let Some(l) = list { self.push(Act::Play(l.clone(), 0)); } }
        if btn_off(ui, &pal, V::Solid, Some("shuffle"), "", has).on_hover_text(self.t("shufflePlay")).clicked() { if let Some(l) = list { self.push(Act::Play(Arc::new(SiloApp::shuffle_list(l)), 0)); } }
    }

    // ---------- главная ----------
    pub fn page_home(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let en = self.en();
        let b = self.be.clone();
        self.load_once("hist", self.pages.hist.is_some(), async move { be::history(&b) }, |a, r| { let mut seen = HashSet::new(); a.pages.hist = Some(Arc::new(r.unwrap_or_default().into_iter().filter(|x| seen.insert(x.id.clone())).take(18).collect())); });
        let b = self.be.clone();
        self.load_once("likedshelf", self.pages.liked_shelf.is_some() && !self.pages.dirty_ratings, async move { be::liked_tracks(&b) }, |a, r| { a.pages.dirty_ratings = false; a.pages.liked_shelf = Some(Arc::new(r.unwrap_or_default().into_iter().take(18).collect())); });
        self.load_missing();
        self.echo_hero(ui, greeting(en));
        if self.pages.missing.as_ref().is_some_and(|m| m.len() == 3) {
            notice(ui, &pal, "key", self.t("addKeysHint"), |ui| { if btn_sm(ui, &pal, V::Solid, None, crate::i18n::tr("openSettings", en), false).clicked() { return true; } false }).then(|| self.tab = Tab::Settings);
        }
        // под настроение
        section_head(ui, &pal, self.t("moods"), |_| {});
        let moods: [(&str, &str, &str, &str, f32); 8] = [("work_focus", "Работа", "Focus", "edit", 210.0), ("workout", "Спорт", "Workout", "speed", 12.0), ("night", "Вечер", "Evening", "moon", 262.0), ("sleep", "Сон", "Sleep", "clock", 232.0), ("energetic", "Бодрое", "Upbeat", "sun", 42.0), ("sad", "Грустное", "Sad", "volumeLow", 200.0), ("romantic", "Романтика", "Romantic", "heart", 340.0), ("driving", "В дорогу", "Driving", "radio", 150.0)];
        let cols = ((ui.available_width() + 10.0) / 170.0).floor().clamp(2.0, 8.0) as usize;
        let cw = (ui.available_width() - 10.0 * (cols - 1) as f32) / cols as f32;
        for row in moods.chunks(cols) {
            ui.horizontal(|ui| {
                for (id, ru, en_, ic, hue) in row {
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(cw, 64.0), Sense::click());
                    let c = egui::ecolor::Hsva::new(hue / 360.0, if pal.dark { 0.45 } else { 0.35 }, if pal.dark { 0.42 } else { 0.85 }, 1.0);
                    let col: Color32 = c.into();
                    let h = ui.ctx().animate_bool(resp.id, resp.hovered());
                    ui.painter().rect_filled(r, CornerRadius::same(12), mix(mix(pal.panel, col, 0.55), col, 0.25 * h));
                    paint_icon(ui.painter(), Rect::from_min_size(r.min + Vec2::new(14.0, 21.0), Vec2::splat(22.0)), ic, pal.fg, false);
                    ui.painter().text(r.min + Vec2::new(46.0, 32.0), Align2::LEFT_CENTER, if en { *en_ } else { *ru }, bold(14.5), pal.fg);
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        let mut seed = vec![id.to_string()];
                        seed.extend(self.prefs.tastes.iter().take(2).cloned());
                        self.push(Act::Wave(Some(seed), None));
                    }
                }
            });
            ui.add_space(6.0);
        }
        self.genre_chips(ui, 12);
        let h = self.pages.hist.clone();
        self.shelf(ui, self.t("recent"), h, Some(Tab::History));
        let l = self.pages.liked_shelf.clone();
        self.shelf(ui, self.t("likedShelf"), l, Some(Tab::Liked));
        if !self.playlists.is_empty() {
            section_head(ui, &pal, self.t("myPls"), |ui| { if btn_sm(ui, &pal, V::Ghost, Some("chevR"), crate::i18n::tr("showAll", en), false).clicked() { self.pages.acts.push(Act::Go(Tab::Playlists)); } });
            let pls = self.playlists.clone();
            let cols = ((ui.available_width() + 10.0) / 230.0).floor().clamp(1.0, 4.0) as usize;
            let cw = (ui.available_width() - 10.0 * (cols - 1) as f32) / cols as f32;
            for row in pls.iter().take(8).collect::<Vec<_>>().chunks(cols) {
                ui.horizontal(|ui| {
                    for p in row {
                        let (r, resp) = ui.allocate_exact_size(Vec2::new(cw, 56.0), Sense::click());
                        ui.painter().rect(r, CornerRadius::same(10), if resp.hovered() { pal.hover() } else { pal.raise() }, Stroke::new(1.0, pal.line), egui::StrokeKind::Inside);
                        let ir = Rect::from_min_size(r.min + Vec2::new(10.0, 10.0), Vec2::splat(36.0));
                        ui.painter().rect_filled(ir, CornerRadius::same(8), pal.acc_soft());
                        paint_icon(ui.painter(), ir.shrink(8.0), "playlist", pal.acc, false);
                        paint_trunc(ui, r.min + Vec2::new(56.0, 10.0), &p.name, bold(14.0), pal.fg, cw - 66.0, Align2::LEFT_TOP);
                        ui.painter().text(r.min + Vec2::new(56.0, 32.0), Align2::LEFT_TOP, format!("{} {}", p.count, self.t("tracksN")), FontId::proportional(12.0), pal.mut_);
                        if resp.clicked() { self.pages.pl_sel = Some(p.id); self.pages.pl_tracks = None; self.pages.acts.push(Act::Go(Tab::Playlists)); }
                    }
                });
                ui.add_space(6.0);
            }
        }
        let empty_home = self.pages.hist.as_ref().is_some_and(|x| x.is_empty()) && self.pages.liked_shelf.as_ref().is_some_and(|x| x.is_empty()) && self.playlists.is_empty();
        if empty_home {
            let mut go = None;
            empty(ui, &pal, "music", self.t("homeEmptyT"), self.t("homeEmpty"), |ui| {
                if btn(ui, &pal, V::Primary, Some("search"), crate::i18n::tr("search", en), false).clicked() { go = Some(Tab::Search); }
                if btn(ui, &pal, V::Solid, Some("browse"), crate::i18n::tr("browse", en), false).clicked() { go = Some(Tab::Browse); }
            });
            if let Some(t) = go { self.tab = t; }
        }
        self.home_extras(ui);
        self.stats_section(ui);
    }

    fn load_missing(&mut self) {
        let b = self.be.clone();
        self.load_once("missing", self.pages.missing.is_some(), async move {
            let g = |k: &str| be::get_setting(&b, k.into()).unwrap_or_default();
            let mut m = vec![];
            if g("jamendo_client_id").trim().is_empty() { m.push("jamendo".to_string()); }
            if g("youtube_api_key").trim().is_empty() { m.push("youtube".into()); }
            if g("soundcloud_client_id").trim().is_empty() || g("soundcloud_client_secret").trim().is_empty() { m.push("soundcloud".into()); }
            m
        }, |a, m| a.pages.missing = Some(m));
    }

    fn genre_chips(&mut self, ui: &mut Ui, n: usize) {
        if self.prefs.tastes.is_empty() { return; }
        let pal = self.pal;
        section_head(ui, &pal, self.t("byGenre"), |_| {});
        let en = self.en();
        let tastes = self.prefs.tastes.clone();
        ui.horizontal_wrapped(|ui| {
            for id in tastes.iter().take(n) {
                if let Some(nm) = taste::taste_name(id, en) {
                    if btn_sm(ui, &pal, V::Chip, Some("echo"), &nm, false).clicked() { self.pages.acts.push(Act::Wave(Some(vec![id.clone()]), None)); }
                }
            }
        });
    }

    fn echo_hero(&mut self, ui: &mut Ui, kicker: &str) {
        let pal = self.pal;
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(Vec2::new(w, 132.0), Sense::hover());
        // мягкий градиент от акцента
        let mut m = egui::Mesh::default();
        let (a, b) = (mix(pal.panel, pal.acc, if self.wave { 0.32 } else { 0.18 }), pal.panel);
        m.colored_vertex(r.left_top(), a); m.colored_vertex(r.right_top(), b); m.colored_vertex(r.right_bottom(), b); m.colored_vertex(r.left_bottom(), a);
        m.add_triangle(0, 1, 2); m.add_triangle(0, 2, 3);
        ui.painter().add(egui::Shape::Mesh(m.into()));
        ui.painter().rect_stroke(r, CornerRadius::same(16), Stroke::new(1.0, pal.line), egui::StrokeKind::Inside);
        let pc = Pos2::new(r.left() + 66.0, r.center().y);
        let pr = Rect::from_center_size(pc, Vec2::splat(76.0));
        let resp = ui.interact(pr, ui.id().with("hero"), Sense::click());
        let h = ui.ctx().animate_bool(resp.id, resp.hovered());
        ui.painter().circle_filled(pc, 38.0 + 2.0 * h, pal.acc);
        if self.wave_starting { paint_spinner(ui, &pal, pr.shrink(22.0)); } else { paint_icon(ui.painter(), pr.shrink(24.0), if self.wave { "pause" } else { "play" }, pal.on_acc, true); }
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !self.wave_starting { if self.wave { self.stop_wave(); } else { self.start_wave(None, None); } }
        let x = r.left() + 124.0;
        ui.painter().text(Pos2::new(x, r.top() + 26.0), Align2::LEFT_CENTER, kicker.to_uppercase(), FontId::proportional(12.0), pal.acc);
        let title = if self.wave_starting { self.t("echoBuilding") } else if self.wave { self.t("echoPlaying") } else { self.t("startEcho") };
        ui.painter().text(Pos2::new(x, r.top() + 54.0), Align2::LEFT_CENTER, title, bold(22.0), pal.fg);
        let sub = match (self.wave, self.cur()) { (true, Some(c)) => format!("{} · {}", c.name, c.artist), _ => self.t("echoPitch").to_string() };
        let g = ui.painter().layout(sub, FontId::proportional(13.0), pal.mut_, r.right() - x - 20.0);
        ui.painter().galley(Pos2::new(x, r.top() + 72.0), g, pal.mut_);
    }

    fn shelf(&mut self, ui: &mut Ui, title: &str, tracks: Option<Tracks>, more: Option<Tab>) {
        let pal = self.pal;
        if tracks.as_ref().is_some_and(|t| t.is_empty()) { return; }
        let en = self.en();
        section_head(ui, &pal, title, |ui| { if let Some(t) = more { if btn_sm(ui, &pal, V::Ghost, Some("chevR"), crate::i18n::tr("showAll", en), false).clicked() { self.pages.acts.push(Act::Go(t)); } } });
        let Some(list) = tracks else { skeleton_rows(ui, &pal, 1); return };
        let cur = self.cur().map(|c| c.id.clone()).unwrap_or_default();
        egui::ScrollArea::horizontal().id_salt(title).auto_shrink([false, true]).show(ui, |ui| {
            ui.horizontal(|ui| {
                for (i, t) in list.iter().enumerate() {
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(150.0, 196.0), Sense::click());
                    let h = ui.ctx().animate_bool(resp.id, resp.hovered());
                    if h > 0.0 { ui.painter().rect_filled(r.expand(4.0), CornerRadius::same(12), mix(Color32::TRANSPARENT, pal.hover(), h)); }
                    let cr = Rect::from_min_size(r.min, Vec2::splat(150.0));
                    paint_cover(ui, &pal, &self.imgs, &t.image, cr, 10);
                    if t.id == cur { ui.painter().rect_stroke(cr, CornerRadius::same(10), Stroke::new(2.0, pal.acc), egui::StrokeKind::Inside); }
                    if h > 0.05 {
                        let pc = cr.right_bottom() - Vec2::new(26.0, 26.0);
                        ui.painter().circle_filled(pc, 18.0 * h, pal.acc);
                        paint_icon(ui.painter(), Rect::from_center_size(pc, Vec2::splat(16.0 * h)), "play", pal.on_acc, true);
                    }
                    paint_trunc(ui, Pos2::new(r.left(), cr.bottom() + 8.0), &t.name, bold(13.5), pal.fg, 150.0, Align2::LEFT_TOP);
                    paint_trunc(ui, Pos2::new(r.left(), cr.bottom() + 26.0), &t.artist, FontId::proportional(12.5), pal.mut_, 150.0, Align2::LEFT_TOP);
                    if resp.on_hover_text(format!("{} · {}", t.name, t.artist)).clicked() { self.pages.acts.push(Act::Play(list.clone(), i)); }
                    ui.add_space(6.0);
                }
            });
        });
    }

    fn home_extras(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let en = self.en();
        if self.pages.daily.is_none() && !self.pages.loading.contains("daily") {
            self.pages.loading.insert("daily");
            self.build_daily(false);
        }
        if let Some(d) = self.pages.daily.clone().filter(|d| !d.is_empty()) {
            let busy = self.pages.daily_busy;
            let mut refresh = false;
            let mut play = false;
            section_head(ui, &pal, self.t("daily"), |ui| {
                if btn_sm(ui, &pal, V::Solid, Some("play"), crate::i18n::tr("playAll", en), false).clicked() { play = true; }
                if busy { spinner(ui, &pal, 18.0); } else if icon_btn(ui, &pal, "refresh", 28.0, false, crate::i18n::tr("refresh", en)).clicked() { refresh = true; }
            });
            if play { self.push(Act::Play(d.clone(), 0)); }
            if refresh { self.pages.daily_busy = true; self.build_daily(true); }
            let shown: Tracks = if self.pages.more_d { d.clone() } else { Arc::new(d.iter().take(5).cloned().collect()) };
            self.track_list(ui, "daily", &shown, false, true, None);
            if d.len() > 5 && btn_sm(ui, &pal, V::Ghost, None, &if self.pages.more_d { self.t("less").to_string() } else { format!("{} ({})", self.t("showAll"), d.len()) }, false).clicked() { self.pages.more_d = !self.pages.more_d; }
        }
        if let Some(d) = self.pages.disc.clone().filter(|d| !d.is_empty()) {
            let mut play = false;
            section_head(ui, &pal, self.t("discover"), |ui| { if btn_sm(ui, &pal, V::Solid, Some("play"), crate::i18n::tr("playAll", en), false).clicked() { play = true; } });
            if play { self.push(Act::Play(d.clone(), 0)); }
            let shown: Tracks = if self.pages.more_w { d.clone() } else { Arc::new(d.iter().take(5).cloned().collect()) };
            self.track_list(ui, "disc", &shown, false, true, None);
            if d.len() > 5 && btn_sm(ui, &pal, V::Ghost, None, &if self.pages.more_w { self.t("less").to_string() } else { format!("{} ({})", self.t("showAll"), d.len()) }, false).clicked() { self.pages.more_w = !self.pages.more_w; }
        }
    }
    /// Плейлист дня и открытия недели (хранятся в настройке daily, как в React-версии).
    fn build_daily(&mut self, force: bool) {
        let saved: serde_json::Value = be::get_setting(&self.be, "daily".into()).ok().and_then(|x| serde_json::from_str(&x).ok()).unwrap_or(serde_json::json!({}));
        let day = day_key(0);
        let week = { let d: i64 = day.parse().unwrap_or(0); ((d + 3) / 7).to_string() };
        let fd = self.echo_fut(&self.prefs.tastes.clone(), &self.prefs.artists.clone(), &self.prefs.diversity.clone());
        let fw = self.echo_fut(&self.prefs.tastes.clone(), &[], "discovery");
        let b = self.be.clone();
        self.spawn(async move {
            let mut s = saved;
            let daily: Vec<Track> = if !force && s["day"].as_str() == Some(&day) && s["tracks"].as_array().is_some_and(|a| !a.is_empty()) { serde_json::from_value(s["tracks"].clone()).unwrap_or_default() } else {
                let v = fd.await.map(|o| o.tracks).unwrap_or_default();
                s["day"] = day.into(); s["tracks"] = serde_json::to_value(&v).unwrap_or_default(); v
            };
            let disc: Vec<Track> = if !force && s["week"].as_str() == Some(&week) && s["disc"].as_array().is_some_and(|a| !a.is_empty()) { serde_json::from_value(s["disc"].clone()).unwrap_or_default() } else {
                // открытия: только исполнители, которых ещё не было в истории
                let heard: HashSet<String> = be::history(&b).unwrap_or_default().iter().map(|x| x.artist.to_lowercase()).collect();
                let v: Vec<Track> = fw.await.map(|o| o.tracks).unwrap_or_default().into_iter().filter(|x| !heard.contains(&x.artist.to_lowercase())).collect();
                s["week"] = week.into(); s["disc"] = serde_json::to_value(&v).unwrap_or_default(); v
            };
            let _ = be::set_setting(&b, "daily".into(), s.to_string());
            (daily, disc)
        }, |a, (d, w)| { a.pages.loading.remove("daily"); a.pages.daily_busy = false; a.pages.daily = Some(Arc::new(d)); a.pages.disc = Some(Arc::new(w)); });
    }

    fn stats_section(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let en = self.en();
        let b = self.be.clone();
        self.load_once("stats", self.pages.stats.is_some(), async move { be::stats(&b) }, |a, r| a.pages.stats = Some(r.unwrap_or(serde_json::json!({}))));
        let Some(s) = self.pages.stats.clone() else { return };
        if s["all"]["tracks"].as_array().is_none_or(|v| v.is_empty()) && s["all"]["artists"].as_array().is_none_or(|v| v.is_empty()) { return; }
        let all = self.pages.stats_all;
        let mut sw = None;
        section_head(ui, &pal, self.t("stats"), |ui| {
            if btn_sm(ui, &pal, V::Chip, None, crate::i18n::tr("allTime", en), all).clicked() { sw = Some(true); }
            if btn_sm(ui, &pal, V::Chip, None, crate::i18n::tr("month", en), !all).clicked() { sw = Some(false); }
        });
        if let Some(v) = sw { self.pages.stats_all = v; }
        let x = &s[if all { "all" } else { "month" }];
        let cw = (ui.available_width() - 20.0) / 3.0;
        ui.horizontal_top(|ui| {
            card(ui, &pal, cw, |ui| {
                ui.label(RichText::new(crate::i18n::tr("listened", en)).color(pal.mut_));
                ui.horizontal(|ui| { ui.label(RichText::new(format!("{}", x["hours"].as_f64().unwrap_or(0.0))).font(bold(34.0)).color(pal.acc)); ui.label(RichText::new(crate::i18n::tr("hours", en)).color(pal.mut_)); });
            });
            let arts: Vec<(String, u64)> = x["artists"].as_array().cloned().unwrap_or_default().iter().filter_map(|a| Some((a[0].as_str()?.to_string(), a[1].as_u64()?))).collect();
            let mut go_art = None;
            card(ui, &pal, cw, |ui| {
                ui.label(RichText::new(crate::i18n::tr("topArtists", en)).color(pal.mut_));
                let mx = arts.iter().map(|a| a.1).max().unwrap_or(1).max(1);
                for (i, (a, n)) in arts.iter().take(6).enumerate() { if stat_row(ui, &pal, i, a, "", *n, mx).clicked() { go_art = Some(a.clone()); } }
                if arts.is_empty() { ui.label(RichText::new(crate::i18n::tr("statsNone", en)).color(pal.mut_)); }
            });
            if let Some(a) = go_art { self.pages.acts.push(Act::Artist(a)); }
            let trs: Vec<(Track, u64)> = x["tracks"].as_array().cloned().unwrap_or_default().iter().filter_map(|t| Some((serde_json::from_value(t["track"].clone()).ok()?, t["plays"].as_u64()?))).collect();
            let mut play = None;
            card(ui, &pal, cw, |ui| {
                ui.label(RichText::new(crate::i18n::tr("topTracks", en)).color(pal.mut_));
                let mx = trs.iter().map(|a| a.1).max().unwrap_or(1).max(1);
                for (i, (t, n)) in trs.iter().take(6).enumerate() { if stat_row(ui, &pal, i, &t.name, &t.artist, *n, mx).clicked() { play = Some(i); } }
                if trs.is_empty() { ui.label(RichText::new(crate::i18n::tr("statsNone", en)).color(pal.mut_)); }
            });
            if let Some(i) = play { self.pages.acts.push(Act::Play(Arc::new(trs.into_iter().map(|x| x.0).collect()), i)); }
        });
    }

    // ---------- поиск ----------
    pub fn page_search(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let en = self.en();
        let now = ui.input(|i| i.time);
        if self.pages.recent.is_none() { self.pages.recent = Some(be::get_setting(&self.be, "search_history".into()).ok().and_then(|x| serde_json::from_str(&x).ok()).unwrap_or_default()); }
        if now - self.pages.usage_at > 15.0 { self.pages.usage_at = now; self.pages.usage = Some(be::yt_usage(&self.be)); }
        let usage = self.pages.usage;
        page_head(ui, &pal, self.t("search"), "", |ui| {
            if let Some((u, l)) = usage { ui.label(RichText::new(format!("{}: {u}/{l}", crate::i18n::tr("ytToday", en))).color(pal.mut_).size(12.5)).on_hover_text(crate::i18n::tr("ytTodayHint", en)); }
        });
        let before = self.pages.q.clone();
        let mut submit = false;
        let field_rect;
        {
            let nf = self.pages.tags.len() + usize::from(!self.pages.lang.is_empty());
            let row = ui.horizontal(|ui| {
                let w = (ui.available_width() - 250.0).max(200.0);
                let r = field(ui, &pal, &mut self.pages.q, crate::i18n::tr("searchPlaceholder", en), Some(if self.pages.busy { "refresh" } else { "search" }), w, "search-input");
                if self.pages.focus_search { r.request_focus(); self.pages.focus_search = false; }
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && self.pages.sug_sel < 0 { submit = true; }
                let flt = if nf > 0 { format!("{} · {nf}", crate::i18n::tr("filters", en)) } else { crate::i18n::tr("filters", en).to_string() };
                if btn(ui, &pal, V::Solid, Some("settings"), &flt, self.pages.show_filters || nf > 0).clicked() { self.pages.show_filters = !self.pages.show_filters; }
                if btn_off(ui, &pal, V::Primary, None, crate::i18n::tr("find", en), !self.pages.q.trim().is_empty() || nf > 0).clicked() { submit = true; }
                r
            });
            field_rect = row.inner.rect;
        }
        let q = self.pages.q.trim().to_string();
        if self.pages.q != before { self.pages.q_changed_at = now; self.pages.sug_sel = -1; }
        // подсказки (Deezer) через 0,25 с и поиск при наборе через 0,7 с
        let is_link = q.starts_with("http://") || q.starts_with("https://");
        if q.chars().count() >= 2 && !is_link && now - self.pages.q_changed_at > 0.25 && self.pages.sug_for != q {
            self.pages.sug_for = q.clone();
            let b = self.be.clone();
            let qq = q.clone();
            self.spawn(async move { be::suggest_tracks(&b, qq).await }, |a, r| { a.pages.sug = r.unwrap_or_default(); });
        }
        if q.chars().count() < 2 { self.pages.sug.clear(); self.pages.sug_for.clear(); }
        if q.chars().count() >= 3 && now - self.pages.q_changed_at > 0.7 && self.pages.q_auto_done != q && !is_link { self.pages.q_auto_done = q.clone(); self.search_go(None, true); }
        if self.pages.q_changed_at > 0.0 && now - self.pages.q_changed_at < 1.0 { ui.ctx().request_repaint_after(std::time::Duration::from_millis(120)); }
        // список подсказок под полем
        let mut picked: Option<usize> = None;
        if !self.pages.sug.is_empty() && ui.memory(|m| m.has_focus(egui::Id::new("search-input"))) {
            let n = self.pages.sug.len().min(6) as i32;
            ui.input(|i| {
                if i.key_pressed(egui::Key::ArrowDown) { self.pages.sug_sel = (self.pages.sug_sel + 1) % n; }
                if i.key_pressed(egui::Key::ArrowUp) { self.pages.sug_sel = if self.pages.sug_sel <= 0 { n - 1 } else { self.pages.sug_sel - 1 }; }
                if i.key_pressed(egui::Key::Escape) { self.pages.sug.clear(); }
            });
            if self.pages.sug_sel >= 0 && ui.input(|i| i.key_pressed(egui::Key::Enter)) { picked = Some(self.pages.sug_sel as usize); }
            let sug = self.pages.sug.clone();
            let sel = self.pages.sug_sel;
            egui::Area::new(egui::Id::new("sug")).order(egui::Order::Foreground).fixed_pos(field_rect.left_bottom() + Vec2::new(0.0, 4.0)).show(ui.ctx(), |ui| {
                egui::Frame::new().fill(pal.panel).stroke(Stroke::new(1.0, pal.line)).corner_radius(10).inner_margin(6).shadow(ui.visuals().popup_shadow).show(ui, |ui| {
                    ui.set_width(field_rect.width() - 12.0);
                    for (k, t) in sug.iter().take(6).enumerate() {
                        let (r, resp) = ui.allocate_exact_size(Vec2::new(field_rect.width() - 12.0, 48.0), Sense::click());
                        if resp.hovered() || k as i32 == sel { ui.painter().rect_filled(r, CornerRadius::same(8), pal.hover()); }
                        paint_cover(ui, &pal, &self.imgs, &t.image, Rect::from_min_size(r.min + Vec2::new(6.0, 6.0), Vec2::splat(36.0)), 6);
                        paint_trunc(ui, r.min + Vec2::new(52.0, 7.0), &t.name, bold(13.5), pal.fg, r.width() - 90.0, Align2::LEFT_TOP);
                        let sub = if t.album.is_empty() { t.artist.clone() } else { format!("{} · {}", t.artist, t.album) };
                        paint_trunc(ui, r.min + Vec2::new(52.0, 26.0), &sub, FontId::proportional(12.0), pal.mut_, r.width() - 90.0, Align2::LEFT_TOP);
                        paint_icon(ui.painter(), Rect::from_center_size(Pos2::new(r.right() - 20.0, r.center().y), Vec2::splat(14.0)), "play", pal.mut_, true);
                        if resp.clicked() { picked = Some(k); }
                    }
                });
            });
        }
        if let Some(k) = picked {
            let sug = std::mem::take(&mut self.pages.sug);
            if let Some(t) = sug.get(k) { self.remember(&format!("{} {}", t.artist, t.name)); }
            self.pages.sug_sel = -1;
            self.push(Act::Play(Arc::new(sug), k));
        }
        if submit { self.pages.sug.clear(); let q2 = self.pages.q.clone(); self.remember(&q2); self.search_go(None, false); }
        if self.pages.show_filters {
            card(ui, &pal, ui.available_width(), |ui| {
                ui.horizontal(|ui| {
                    let cur = LANGS.iter().find(|l| l.0 == self.pages.lang).map(|l| l.1.to_string()).unwrap_or_else(|| format!("{}: {}", crate::i18n::tr("language", en), crate::i18n::tr("anyLang", en)));
                    egui::ComboBox::from_id_salt("lang").selected_text(cur).show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.pages.lang, String::new(), crate::i18n::tr("anyLang", en));
                        for (k, n) in LANGS { ui.selectable_value(&mut self.pages.lang, k.to_string(), n); }
                    });
                    if (!self.pages.tags.is_empty() || !self.pages.lang.is_empty()) && btn_sm(ui, &pal, V::Ghost, Some("close"), crate::i18n::tr("resetFilters", en), false).clicked() { self.pages.tags.clear(); self.pages.lang.clear(); }
                });
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    for f in taste::families() {
                        let tag = taste::query_tags(&f.id).first().cloned().unwrap_or_default();
                        let on = self.pages.tags.contains(&tag);
                        if btn_sm(ui, &pal, V::Chip, None, taste::name(f, en), on).clicked() { if on { self.pages.tags.retain(|x| *x != tag); } else { self.pages.tags.push(tag); } }
                    }
                });
            });
        }
        for w in self.pages.warn.clone() { notice(ui, &pal, "alert", &w, |_| false); }
        if self.pages.res.is_none() && !self.pages.busy {
            let recent = self.pages.recent.clone().unwrap_or_default();
            if !recent.is_empty() {
                let mut clear = false;
                section_head(ui, &pal, self.t("recentQ"), |ui| { if btn_sm(ui, &pal, V::Ghost, None, crate::i18n::tr("clear", en), false).clicked() { clear = true; } });
                if clear { self.save_recent(vec![]); }
                ui.horizontal_wrapped(|ui| { for r in &recent { if btn_sm(ui, &pal, V::Chip, Some("clock"), r, false).clicked() { self.pages.q = r.clone(); self.pages.q_changed_at = now; } } });
            }
            if !self.prefs.tastes.is_empty() {
                section_head(ui, &pal, self.t("forYou"), |_| {});
                let tastes = self.prefs.tastes.clone();
                ui.horizontal_wrapped(|ui| {
                    for id in &tastes {
                        if let Some(n) = taste::taste_name(id, en) {
                            if btn_sm(ui, &pal, V::Chip, None, &n, false).clicked() { self.search_go(Some((taste::query_tags(id).into_iter().take(1).collect(), taste::taste_lang(id))), false); }
                        }
                    }
                });
            }
            if recent.is_empty() && self.prefs.tastes.is_empty() { empty(ui, &pal, "search", self.t("searchEmptyT"), self.t("searchEmpty"), |_| {}); }
        }
        if self.pages.busy && self.pages.res.is_none() { skeleton_rows(ui, &pal, 8); }
        if let Some(r) = self.pages.res.clone() {
            if r.is_empty() { empty(ui, &pal, "search", self.t("noResults"), self.t("noResultsHint"), |_| {}); } else {
                self.track_list(ui, "search", &r, false, true, None);
                ui.add_space(14.0);
                ui.label(RichText::new(self.t("searchYtHint")).color(pal.mut_).size(12.5));
            }
        }
    }
    fn remember(&mut self, x: &str) {
        let v = x.trim().to_string();
        if v.is_empty() || v.starts_with("http") { return; }
        let mut r = self.pages.recent.clone().unwrap_or_default();
        r.retain(|y| !y.eq_ignore_ascii_case(&v));
        r.insert(0, v);
        r.truncate(10);
        self.save_recent(r);
    }
    fn save_recent(&mut self, r: Vec<String>) {
        let _ = be::set_setting(&self.be, "search_history".into(), serde_json::to_string(&r).unwrap_or_default());
        self.pages.recent = Some(r);
    }
    fn search_go(&mut self, by_tags: Option<(Vec<String>, Option<String>)>, auto: bool) {
        self.pages.seq += 1;
        let my = self.pages.seq;
        self.pages.busy = true;
        let q = self.pages.q.trim().to_string();
        let b = self.be.clone();
        if by_tags.is_none() && (q.starts_with("https://") || q.starts_with("http://")) && ["soundcloud.com", "youtube.com", "youtu.be"].iter().any(|h| q.contains(h)) {
            self.spawn(async move { be::link_track(q).await }, move |a, r| {
                if my == a.pages.seq { a.pages.busy = false; }
                match r { Ok(t) => { a.pages.res = Some(Arc::new(vec![t.clone()])); a.pages.warn.clear(); a.play(vec![t], 0, false); } Err(e) => a.err(e) }
            });
            return;
        }
        let online = self.online;
        let (tags, lang, sources) = match &by_tags {
            Some((t, l)) => (t.clone(), l.clone(), vec!["jamendo".to_string()]),
            None => (self.pages.tags.clone(), (!self.pages.lang.is_empty()).then(|| self.pages.lang.clone()), if !online { vec!["local".into()] } else if auto { vec!["local".into(), "jamendo".into(), "deezer".into()] } else { vec![] }),
        };
        let p = SearchParams { query: Some(if by_tags.is_some() { String::new() } else { q }), tags, lang, order: None, offset: 0, limit: 20, sources, audio_format: Some(quality_format(&self.prefs.quality)), lang_strict: Some(self.prefs.lang_strict) };
        self.spawn(async move { be::search_all(&b, p).await }, move |a, r| {
            if my != a.pages.seq { return; }
            a.pages.busy = false;
            match r { Ok(r) => { a.pages.res = Some(Arc::new(r.tracks)); a.pages.warn = r.errors; if let Some(l) = a.pages.lists.get_mut("search") { l.limit = 100; } } Err(e) => a.err(e) }
        });
    }

    // ---------- подборки ----------
    pub fn page_browse(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let en = self.en();
        if let Some(open) = self.pages.open.clone() {
            let tracks = self.pages.otracks.clone();
            let sub = format!("{}{}{}", src_name(&open.source, en), if open.owner.is_empty() { String::new() } else { format!(" · {}", open.owner) }, tracks.as_ref().filter(|t| !t.is_empty()).map(|t| format!(" · {}", summary(en, t))).unwrap_or_default());
            let mut back = false;
            ui.horizontal(|ui| { if icon_btn(ui, &pal, "back", 34.0, false, crate::i18n::tr("back", en)).clicked() { back = true; } });
            let saved = self.pages.osaved;
            let mut save = false;
            page_head(ui, &pal, &open.title, &sub, |ui| {
                self.play_buttons(ui, &tracks);
                if btn_off(ui, &pal, V::Solid, Some("save"), crate::i18n::tr(if saved { "saved" } else { "saveToMine" }, en), tracks.as_ref().is_some_and(|t| !t.is_empty()) && !saved).clicked() { save = true; }
            });
            if back { self.pages.open = None; return; }
            if save {
                if let Some(t) = &tracks { match be::save_playlist(&self.be, open.title.clone(), t.to_vec()) { Ok(_) => { self.pages.osaved = true; self.refresh_playlists(); self.toast(self.t("savedToMine"), ToastKind::Ok); } Err(e) => self.err(e) } }
            }
            match tracks {
                None => skeleton_rows(ui, &pal, 6),
                Some(t) if t.is_empty() => empty(ui, &pal, "playlist", self.t("plEmptyT"), self.t("plOpenEmpty"), |_| {}),
                Some(t) => self.track_list(ui, "browse-open", &t, true, false, None),
            }
            return;
        }
        page_head(ui, &pal, self.t("browse"), self.t("browseHint"), |_| {});
        let mut go = false;
        ui.horizontal(|ui| {
            let r = field(ui, &pal, &mut self.pages.bq, crate::i18n::tr("playlistSearch", en), Some("search"), (ui.available_width() - 120.0).max(200.0), "browse-q");
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) { go = true; }
            if btn_off(ui, &pal, V::Primary, None, crate::i18n::tr("find", en), !self.pages.bq.trim().is_empty()).clicked() { go = true; }
        });
        if go { let q = self.pages.bq.clone(); self.browse_find(q); }
        for w in self.pages.bwarn.clone() { notice(ui, &pal, "alert", &w, |_| false); }
        if self.pages.hits.is_none() && !self.pages.bbusy {
            let tastes = self.prefs.tastes.clone();
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| { for id in tastes.iter().take(10) { if let Some(n) = taste::taste_name(id, en) { if btn_sm(ui, &pal, V::Chip, None, &n, false).clicked() { self.pages.bq = n.clone(); self.browse_find(n); } } } });
            empty(ui, &pal, "browse", self.t("browseEmptyT"), self.t("browseEmpty"), |_| {});
        }
        if self.pages.bbusy && self.pages.hits.is_none() { skeleton_rows(ui, &pal, 6); }
        if let Some(h) = self.pages.hits.clone() {
            if h.is_empty() { empty(ui, &pal, "search", self.t("noResults"), self.t("noResultsHint"), |_| {}); }
            for p in h {
                let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 60.0), Sense::click());
                if resp.hovered() { ui.painter().rect_filled(r, CornerRadius::same(10), pal.hover()); }
                paint_cover(ui, &pal, &self.imgs, &p.image, Rect::from_min_size(r.min + Vec2::new(8.0, 8.0), Vec2::splat(44.0)), 6);
                paint_trunc(ui, r.min + Vec2::new(64.0, 10.0), &p.title, bold(14.0), pal.fg, r.width() - 200.0, Align2::LEFT_TOP);
                paint_trunc(ui, r.min + Vec2::new(64.0, 32.0), &format!("{} · {}", src_name(&p.source, en), p.owner), FontId::proportional(12.5), pal.mut_, r.width() - 200.0, Align2::LEFT_TOP);
                if p.count > 0 { ui.painter().text(Pos2::new(r.right() - 40.0, r.center().y), Align2::RIGHT_CENTER, format!("{} {}", p.count, self.t("tracksN")), FontId::proportional(12.5), pal.mut_); }
                paint_icon(ui.painter(), Rect::from_center_size(Pos2::new(r.right() - 18.0, r.center().y), Vec2::splat(16.0)), "chevR", pal.mut_, false);
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.pages.open = Some(p.clone()); self.pages.osaved = false; self.pages.otracks = None;
                    let b = self.be.clone();
                    let lang = if self.prefs.lang_strict { taste::seed_for(&self.prefs.tastes, &[]).lang } else { None };
                    self.spawn(async move { be::open_playlist(&b, p.id, lang).await }, |a, r| match r { Ok(v) => a.pages.otracks = Some(Arc::new(v)), Err(e) => { a.pages.otracks = Some(Arc::new(vec![])); a.err(e); } });
                }
            }
        }
    }
    fn browse_find(&mut self, q: String) {
        if q.trim().is_empty() { return; }
        self.pages.bbusy = true;
        let b = self.be.clone();
        self.spawn(async move { be::search_playlists(&b, q, vec![]).await }, |a, r| { a.pages.bbusy = false; match r { Ok(h) => { a.pages.hits = Some(h.playlists); a.pages.bwarn = h.errors; } Err(e) => a.err(e) } });
    }

    // ---------- Эхо ----------
    pub fn page_echo(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let en = self.en();
        self.load_missing();
        page_head(ui, &pal, self.t("echo"), self.t("echoHelp"), |_| {});
        self.echo_hero(ui, crate::i18n::tr("echo", en));
        if let Some(m) = self.pages.missing.clone().filter(|m| !m.is_empty()) {
            ui.add_space(6.0);
            ui.label(RichText::new(format!("{}: {}. {}", self.t("echoNoKeys"), m.iter().map(|x| src_name(x, en)).collect::<Vec<_>>().join(", "), self.t("echoNoKeysHint"))).color(pal.mut_));
        }
        section_head(ui, &pal, self.t("diversityTitle"), |_| {});
        self.diversity_chips(ui);
        ui.label(RichText::new(self.t("diversityNote")).color(pal.mut_).size(12.5));
        self.genre_chips(ui, 16);
        let up = self.upcoming();
        if self.wave && !up.is_empty() {
            section_head(ui, &pal, self.t("upNext"), |_| {});
            let l: Tracks = Arc::new(up.into_iter().take(20).collect());
            self.track_list(ui, "echo-up", &l, false, true, None);
        }
        if !self.wave && self.cur().is_none() {
            let mut go = false;
            empty(ui, &pal, "echo", self.t("echoIdleT"), self.t("echoIdle"), |ui| { if btn(ui, &pal, V::Solid, None, crate::i18n::tr("playLiked", en), false).clicked() { go = true; } });
            if go {
                let l = be::liked_tracks(&self.be).unwrap_or_default();
                if l.is_empty() { self.toast(self.t("noLikedYet"), ToastKind::Info); } else { self.play(SiloApp::shuffle_list(&l), 0, false); }
            }
        }
    }
    fn diversity_chips(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let en = self.en();
        ui.horizontal(|ui| {
            for (m, k) in [("familiar", "dFamiliar"), ("balanced", "dBalanced"), ("discovery", "dDiscovery")] {
                if btn_sm(ui, &pal, V::Chip, None, crate::i18n::tr(k, en), self.prefs.diversity == m).clicked() { self.prefs.diversity = m.into(); }
            }
        });
    }

    // ---------- простые списки ----------
    pub fn page_liked(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let b = self.be.clone();
        let dirty = self.pages.dirty_ratings;
        self.load_once("liked", self.pages.liked.is_some() && !dirty, async move { be::liked_tracks(&b) }, |a, r| { a.pages.dirty_ratings = false; a.pages.liked = Some(Arc::new(r.unwrap_or_default())); a.pages.banned = None; });
        let d = self.pages.liked.clone();
        let sub = d.as_ref().filter(|x| !x.is_empty()).map(|x| summary(self.en(), x)).unwrap_or_else(|| self.t("likedHint").into());
        page_head(ui, &pal, self.t("liked"), &sub, |ui| self.play_buttons(ui, &d));
        match d {
            None => skeleton_rows(ui, &pal, 6),
            Some(l) if l.is_empty() => { let mut go = false; empty(ui, &pal, "heart", self.t("likedEmptyT"), self.t("likedEmpty"), |ui| { go = btn(ui, &pal, V::Primary, Some("search"), crate::i18n::tr("search", self.en()), false).clicked(); }); if go { self.tab = Tab::Search; } }
            Some(l) => self.track_list(ui, "liked", &l, false, false, None),
        }
    }
    pub fn page_history(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let b = self.be.clone();
        self.load_once("history", self.pages.history.is_some(), async move { be::history(&b) }, |a, r| a.pages.history = Some(Arc::new(r.unwrap_or_default())));
        let d = self.pages.history.clone();
        let sub = d.as_ref().filter(|x| !x.is_empty()).map(|x| summary(self.en(), x)).unwrap_or_else(|| self.t("historyHint").into());
        let mut clear = false;
        let has = d.as_ref().is_some_and(|x| !x.is_empty());
        page_head(ui, &pal, self.t("history"), &sub, |ui| { if btn_off(ui, &pal, V::Solid, Some("trash"), crate::i18n::tr("clearHistory", self.en()), has).clicked() { clear = true; } });
        if clear {
            self.asks.push(Ask::Confirm { title: format!("{}?", self.t("clearHistory")), body: self.t("clearHistoryBody").into(), ok: self.t("clear").into(), danger: true, check: None, on_ok: Box::new(|a, _| {
                match be::clear_history(&a.be) { Ok(_) => { a.pages.history = None; a.pages.hist = None; a.toast(a.t("historyCleared"), ToastKind::Ok); } Err(e) => a.err(e) }
            }) });
        }
        match d {
            None => skeleton_rows(ui, &pal, 6),
            Some(l) if l.is_empty() => { let mut go = false; empty(ui, &pal, "history", self.t("historyEmptyT"), self.t("historyEmpty"), |ui| { go = btn(ui, &pal, V::Primary, None, crate::i18n::tr("home", self.en()), false).clicked(); }); if go { self.tab = Tab::Home; } }
            Some(l) => self.track_list(ui, "history", &l, false, false, None),
        }
    }
    pub fn page_downloads(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let b = self.be.clone();
        self.load_once("downloads", self.pages.downloads.is_some(), async move { be::list_downloads(&b) }, |a, r| a.pages.downloads = Some(Arc::new(r.unwrap_or_default().into_iter().map(|x| x.0).collect())));
        let d = self.pages.downloads.clone();
        let sub = d.as_ref().filter(|x| !x.is_empty()).map(|x| summary(self.en(), x)).unwrap_or_else(|| self.t("downloadsHint").into());
        page_head(ui, &pal, self.t("downloads"), &sub, |ui| self.play_buttons(ui, &d));
        match d {
            None => skeleton_rows(ui, &pal, 6),
            Some(l) if l.is_empty() => { let mut go = false; empty(ui, &pal, "download", self.t("dlEmptyT"), self.t("dlEmpty"), |ui| { go = btn(ui, &pal, V::Solid, Some("folder"), crate::i18n::tr("folder", self.en()), false).clicked(); }); if go { self.pages.set_tab = "data".into(); self.tab = Tab::Settings; } }
            Some(l) => self.track_list(ui, "downloads", &l, false, false, None),
        }
    }
    pub fn page_banned(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let en = self.en();
        let b = self.be.clone();
        let dirty = self.pages.dirty_ratings;
        self.load_once("banned", self.pages.banned.is_some() && !dirty, async move { be::banned_tracks(&b) }, |a, r| { a.pages.dirty_ratings = false; a.pages.banned = Some(Arc::new(r.unwrap_or_default())); a.pages.liked = None; });
        page_head(ui, &pal, self.t("banned"), self.t("bannedHint"), |_| {});
        let blocked = self.blocked.clone();
        if !blocked.is_empty() {
            section_head(ui, &pal, self.t("blockedArtists"), |_| {});
            ui.horizontal_wrapped(|ui| {
                for a in &blocked {
                    if btn_sm(ui, &pal, V::Chip, Some("close"), a, true).on_hover_text(crate::i18n::tr("unblockArtist", en)).clicked() {
                        self.set_blocked(a, false);
                        let a2 = a.clone();
                        self.toast_act(format!("{}: {a}", self.t("unblocked")), ToastKind::Ok, Some((self.t("undo").into(), Box::new(move |s: &mut SiloApp| s.set_blocked(&a2, true)))));
                    }
                }
            });
        }
        let Some(d) = self.pages.banned.clone() else { skeleton_rows(ui, &pal, 3); return };
        if d.is_empty() { if blocked.is_empty() { empty(ui, &pal, "ban", self.t("bannedEmptyT"), self.t("bannedEmpty"), |_| {}); } return; }
        if !blocked.is_empty() { section_head(ui, &pal, self.t("bannedTracks"), |_| {}); }
        for t in d.iter() {
            let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 56.0), Sense::hover());
            paint_cover(ui, &pal, &self.imgs, &t.image, Rect::from_min_size(r.min + Vec2::new(8.0, 8.0), Vec2::splat(40.0)), 6);
            paint_trunc(ui, r.min + Vec2::new(60.0, 9.0), &t.name, bold(14.0), pal.fg, r.width() - 200.0, Align2::LEFT_TOP);
            paint_trunc(ui, r.min + Vec2::new(60.0, 30.0), &format!("{} · {}", src_name(&t.source, en), t.artist), FontId::proportional(12.5), pal.mut_, r.width() - 200.0, Align2::LEFT_TOP);
            let mut c = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(Pos2::new(r.right() - 130.0, r.top()), r.max)).layout(Layout::right_to_left(Align::Center)));
            if btn_sm(&mut c, &pal, V::Solid, Some("refresh"), crate::i18n::tr("restore", en), false).clicked() {
                self.push(Act::Rate(t.clone(), 0));
                let t2 = t.clone();
                self.toast_act(format!("{}: {}", self.t("restored"), t.name), ToastKind::Ok, Some((self.t("undo").into(), Box::new(move |a: &mut SiloApp| a.rate(t2, -2)))));
            }
        }
    }

    // ---------- мои плейлисты ----------
    pub fn page_playlists(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let en = self.en();
        if self.pages.pl_reload { self.pages.pl_reload = false; self.pages.pl_tracks = None; }
        let mut create = false;
        page_head(ui, &pal, self.t("playlists"), "", |ui| { if btn(ui, &pal, V::Primary, Some("plus"), crate::i18n::tr("newPlaylist", en), false).clicked() { create = true; } });
        if create { self.ask_create_playlist(); }
        // загрузка выбранного
        if self.pages.pl_tracks.is_none() && !self.pages.loading.contains("pl") {
            if let Some(id) = self.pages.pl_sel {
                self.pages.loading.insert("pl");
                let b = self.be.clone();
                self.spawn(async move { be::playlist_tracks(&b, id) }, |a, r| { a.pages.loading.remove("pl"); a.pages.pl_tracks = Some(Arc::new(r.unwrap_or_default())); });
            } else if !self.pages.pl_virt.is_empty() {
                self.pages.loading.insert("pl");
                let (b, k) = (self.be.clone(), self.pages.pl_virt.clone());
                self.spawn(async move {
                    match k.as_str() {
                        "top" => be::stats(&b).map(|s| s["all"]["tracks"].as_array().cloned().unwrap_or_default().iter().filter_map(|x| serde_json::from_value(x["track"].clone()).ok()).collect()).unwrap_or_default(),
                        "liked" => be::liked_tracks(&b).unwrap_or_default(),
                        "dl" => be::list_downloads(&b).unwrap_or_default().into_iter().map(|x| x.0).collect(),
                        _ => { let recent: HashSet<String> = be::history(&b).unwrap_or_default().into_iter().take(300).map(|x| x.id).collect(); be::liked_tracks(&b).unwrap_or_default().into_iter().filter(|x| !recent.contains(&x.id)).collect() }
                    }
                }, |a, r: Vec<Track>| { a.pages.loading.remove("pl"); a.pages.pl_tracks = Some(Arc::new(r)); });
            }
        }
        let total = ui.available_width();
        let lw = 240.0f32.min(total * 0.3);
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(Vec2::new(lw, 10.0), Layout::top_down(Align::Min), |ui| {
                ui.set_width(lw);
                ui.label(RichText::new(self.t("smart").to_uppercase()).size(11.0).color(pal.mut_));
                for (k, key, ic) in [("top", "smartTop", "speed"), ("forgot", "smartForgot", "clock"), ("liked", "smartLiked", "heart"), ("dl", "smartDl", "download")] {
                    let on = self.pages.pl_virt == k && self.pages.pl_sel.is_none();
                    if btn_sized(ui, &pal, V::Nav, Some(ic), crate::i18n::tr(key, en), on, 34.0, true).clicked() { self.pages.pl_virt = k.into(); self.pages.pl_sel = None; self.pages.pl_tracks = None; }
                }
                ui.add_space(10.0);
                ui.label(RichText::new(self.t("myPls").to_uppercase()).size(11.0).color(pal.mut_));
                if self.playlists.is_empty() { ui.label(RichText::new(self.t("plNone")).color(pal.mut_)); }
                for p in self.playlists.clone() {
                    let on = self.pages.pl_sel == Some(p.id);
                    let r = btn_sized(ui, &pal, V::Nav, Some("playlist"), &p.name, on, 34.0, true);
                    ui.painter().text(Pos2::new(r.rect.right() - 10.0, r.rect.center().y), Align2::RIGHT_CENTER, p.count.to_string(), FontId::proportional(12.0), pal.mut_);
                    if r.clicked() { self.pages.pl_sel = Some(p.id); self.pages.pl_virt.clear(); self.pages.pl_tracks = None; }
                    if r.double_clicked() { let l = be::playlist_tracks(&self.be, p.id).unwrap_or_default(); if !l.is_empty() { self.play(l, 0, false); } }
                    let mut m = None;
                    r.context_menu(|ui| m = pl_menu(ui, &pal, en));
                    if let Some(x) = m { self.pl_action(x, &p); }
                }
            });
            ui.add_space(16.0);
            ui.vertical(|ui| {
                let sel = self.pages.pl_sel.and_then(|id| self.playlists.iter().find(|p| p.id == id).cloned());
                if sel.is_none() && self.pages.pl_virt.is_empty() {
                    let mut c = false;
                    empty(ui, &pal, "playlist", self.t("plPickT"), self.t("plPick"), |ui| { c = btn(ui, &pal, V::Solid, Some("plus"), crate::i18n::tr("newPlaylist", en), false).clicked(); });
                    if c { self.ask_create_playlist(); }
                    return;
                }
                let title = match &sel { Some(p) => p.name.clone(), None => crate::i18n::tr(match self.pages.pl_virt.as_str() { "top" => "smartTop", "liked" => "smartLiked", "dl" => "smartDl", _ => "smartForgot" }, en).to_string() };
                let tracks = self.pages.pl_tracks.clone();
                let sub = tracks.as_ref().filter(|t| !t.is_empty()).map(|t| summary(en, t)).unwrap_or_default();
                let mut menu_act = None;
                page_head(ui, &pal, &title, &sub, |ui| {
                    self.play_buttons(ui, &tracks);
                    if sel.is_some() {
                        let r = icon_btn(ui, &pal, "more", 34.0, false, crate::i18n::tr("more", en));
                        egui::Popup::menu(&r).show(|ui| { menu_act = pl_menu(ui, &pal, en); });
                    }
                });
                if let (Some(x), Some(p)) = (menu_act, &sel) { self.pl_action(x, p); }
                match tracks {
                    None => skeleton_rows(ui, &pal, 6),
                    Some(t) if t.is_empty() => empty(ui, &pal, "playlist", self.t("plEmptyT"), self.t(if sel.is_some() { "plEmpty" } else { "smartEmpty" }), |_| {}),
                    Some(t) => self.track_list(ui, &format!("pl:{}:{}", self.pages.pl_sel.unwrap_or(-1), self.pages.pl_virt), &t, true, false, sel.as_ref().map(|p| p.id)),
                }
            });
        });
    }
    fn ask_create_playlist(&mut self) {
        self.asks.push(Ask::Prompt { title: self.t("newPlaylist").into(), value: String::new(), placeholder: self.t("plNamePh").into(), ok: self.t("create").into(), on_ok: Box::new(|a, name| {
            match be::create_playlist(&a.be, name.clone()) { Ok(id) => { a.refresh_playlists(); a.pages.pl_sel = Some(id); a.pages.pl_virt.clear(); a.pages.pl_tracks = None; a.toast(format!("{}: {name}", a.t("plCreated")), ToastKind::Ok); } Err(e) => a.err(e) }
        }) });
    }
    fn pl_action(&mut self, x: u8, p: &silo_core::model::Playlist) {
        let (id, name) = (p.id, p.name.clone());
        match x {
            0 => self.asks.push(Ask::Prompt { title: self.t("rename").into(), value: name.clone(), placeholder: String::new(), ok: self.t("save").into(), on_ok: Box::new(move |a, n| { if n != name { if let Err(e) = be::rename_playlist(&a.be, id, n) { a.err(e); } a.refresh_playlists(); } }) }),
            1 => { let l = be::playlist_tracks(&self.be, id).unwrap_or_default(); let s = l.iter().map(|x| format!("{} - {}", x.artist, x.name)).collect::<Vec<_>>().join("\n"); self.push(Act::Copy(s, self.t("copied").into())); }
            2 => { let l = be::playlist_tracks(&self.be, id).unwrap_or_default(); let fname = name.chars().map(|c| if "\\/:*?\"<>|".contains(c) { '_' } else { c }).collect::<String>() + ".m3u8"; self.save_file(&fname, "m3u8", crate::import::m3u(&l)); }
            3 => {
                let body = format!("«{}» · {} {}. {}", p.name, p.count, self.t("tracksN"), self.t("cantUndo"));
                self.asks.push(Ask::Confirm { title: format!("{}?", self.t("deletePlaylist")), body, ok: self.t("del").into(), danger: true, check: None, on_ok: Box::new(move |a, _| {
                    match be::delete_playlist(&a.be, id) { Ok(_) => { if a.pages.pl_sel == Some(id) { a.pages.pl_sel = None; a.pages.pl_tracks = None; } a.refresh_playlists(); a.toast(format!("{}: {name}", a.t("plDeleted")), ToastKind::Ok); } Err(e) => a.err(e) }
                }) });
            }
            _ => {}
        }
    }

    // ---------- исполнитель ----------
    pub fn page_artist(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let en = self.en();
        let artist = self.artist.clone();
        if self.pages.artist_for != artist {
            self.pages.artist_for = artist.clone();
            self.pages.artist_tracks = None;
            let b = self.be.clone();
            let a2 = artist.clone();
            self.spawn(async move {
                let p = SearchParams { query: Some(a2.clone()), tags: vec![], lang: None, order: None, offset: 0, limit: 30, sources: vec!["local".into(), "jamendo".into()], audio_format: None, lang_strict: Some(false) };
                let a = be::search_all(&b, p).await.map(|r| r.tracks).unwrap_or_default();
                let yt = be::artist_tracks(&b, a2.clone()).await.map(|r| r.0).unwrap_or_default();
                let low = a2.to_lowercase();
                let mut seen = HashSet::new();
                a.into_iter().filter(|x| x.artist.to_lowercase().contains(&low)).chain(yt).filter(|x| seen.insert(x.id.clone())).collect::<Vec<_>>()
            }, move |s, v| { if s.artist == artist { s.pages.artist_tracks = Some(Arc::new(v)); } });
        }
        let blocked = self.blocked.iter().any(|x| x.eq_ignore_ascii_case(&self.artist));
        let tracks = self.pages.artist_tracks.clone();
        let mut back = false;
        ui.horizontal(|ui| { if icon_btn(ui, &pal, "back", 34.0, false, crate::i18n::tr("back", en)).clicked() { back = true; } });
        if back { self.tab = Tab::Home; return; }
        let sub = match &tracks { None => self.t("artistLoading").to_string(), Some(t) if !t.is_empty() => summary(en, t), _ => String::new() };
        let mut radio = false;
        let mut toggle_block = false;
        let name = self.artist.clone();
        page_head(ui, &pal, &name, &sub, |ui| {
            if btn_off(ui, &pal, V::Primary, Some("play"), crate::i18n::tr("playAll", en), tracks.as_ref().is_some_and(|t| !t.is_empty())).clicked() { if let Some(t) = &tracks { self.pages.acts.push(Act::Play(t.clone(), 0)); } }
            if btn(ui, &pal, V::Solid, Some("radio"), crate::i18n::tr("artistRadio", en), false).clicked() { radio = true; }
            let r = icon_btn(ui, &pal, "more", 34.0, false, crate::i18n::tr("more", en));
            egui::Popup::menu(&r).show(|ui| { if ui.button(crate::i18n::tr(if blocked { "unblockArtist" } else { "blockArtist" }, en)).clicked() { toggle_block = true; } });
        });
        if radio { self.start_wave(Some(self.prefs.tastes.clone()), Some(vec![SeedArtist { name: name.clone(), tags: vec![] }])); }
        if toggle_block { self.set_blocked(&name, !blocked); self.toast(format!("{}: {name}", self.t(if blocked { "unblocked" } else { "blockedT" })), ToastKind::Ok); }
        if blocked { if notice(ui, &pal, "ban", self.t("blockedNote"), |ui| btn_sm(ui, &pal, V::Solid, None, crate::i18n::tr("unblockArtist", en), false).clicked()) { self.set_blocked(&name, false); } }
        match tracks {
            None => skeleton_rows(ui, &pal, 6),
            Some(t) if t.is_empty() => empty(ui, &pal, "user", self.t("noResults"), self.t("artistNoKey"), |_| {}),
            Some(t) => self.track_list(ui, "artist", &t, false, false, None),
        }
    }

    // ---------- импорт ----------
    pub fn page_import(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let en = self.en();
        page_head(ui, &pal, self.t("importT"), self.t("importHint"), |_| {});
        let busy = self.pages.imp_busy;
        ui.add_enabled(!busy, egui::TextEdit::multiline(&mut self.pages.imp_text).desired_rows(7).desired_width(f32::INFINITY).hint_text(crate::i18n::tr("importPlaceholder", en)));
        ui.add_space(8.0);
        let mut run: Option<(String, String)> = None;
        ui.horizontal(|ui| {
            field(ui, &pal, &mut self.pages.imp_name, crate::i18n::tr("importName", en), None, 280.0, "imp-name");
            if btn_off(ui, &pal, V::Solid, Some("folder"), crate::i18n::tr("importFile", en), !busy).clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("playlist", &["m3u", "m3u8", "pls", "csv", "json", "txt"]).pick_file() {
                    let body = std::fs::read(&p).map(|b| String::from_utf8_lossy(&b).to_string()).unwrap_or_default();
                    let fname = p.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
                    if self.pages.imp_name.is_empty() { self.pages.imp_name = p.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default(); }
                    run = Some((fname, body));
                }
            }
            if btn_off(ui, &pal, V::Primary, None, crate::i18n::tr("importFind", en), !busy && !self.pages.imp_text.trim().is_empty()).clicked() { run = Some(("paste.txt".into(), self.pages.imp_text.clone())); }
            if busy && btn(ui, &pal, V::Ghost, None, crate::i18n::tr("importStop", en), false).clicked() { self.pages.imp_stop.store(true, Relaxed); }
        });
        ui.label(RichText::new(self.t("importFormats")).color(pal.mut_).size(12.5));
        if let Some((n, b)) = run { self.import_run(n, b); }
        let found: Vec<Track> = self.pages.imp_urls.iter().cloned().chain(self.pages.imp_rows.iter().filter_map(|r| r.1.clone())).collect();
        let missed = self.pages.imp_rows.iter().filter(|r| r.1.is_none()).count();
        if busy || !self.pages.imp_rows.is_empty() || !self.pages.imp_urls.is_empty() {
            ui.add_space(10.0);
            card(ui, &pal, ui.available_width(), |ui| {
                ui.horizontal(|ui| {
                    if busy { spinner(ui, &pal, 16.0); ui.label(RichText::new(format!("{} {}", crate::i18n::tr("importSearching", en), self.pages.imp_prog)).strong()); }
                    else { ui.label(RichText::new(format!("{}: {} · {}: {missed}", crate::i18n::tr("importFound", en), found.len(), crate::i18n::tr("importMissed", en))).strong()); }
                    if !busy && !found.is_empty() {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let saved = self.pages.imp_saved;
                            if btn_off(ui, &pal, V::Solid, Some("save"), crate::i18n::tr(if saved { "saved" } else { "importSave" }, en), !saved).clicked() {
                                let name = if self.pages.imp_name.trim().is_empty() { crate::i18n::tr("importDefaultName", en).to_string() } else { self.pages.imp_name.trim().to_string() };
                                match be::save_playlist(&self.be, name, found.clone()) { Ok(_) => { self.pages.imp_saved = true; self.refresh_playlists(); } Err(e) => self.err(e) }
                            }
                            if btn(ui, &pal, V::Primary, Some("play"), crate::i18n::tr("playAll", en), false).clicked() { self.pages.acts.push(Act::Play(Arc::new(found.clone()), 0)); }
                        });
                    }
                });
            });
            for (w, hit) in &self.pages.imp_rows {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(if hit.is_some() { "✓" } else { "✗" }).color(if hit.is_some() { pal.ok() } else { pal.danger() }));
                    ui.label(&w.raw);
                    ui.label(RichText::new(match hit { Some(t) => format!("{} · {} — {}", src_name(&t.source, en), t.artist, t.name), None => crate::i18n::tr("importNoMatch", en).into() }).color(pal.mut_));
                });
            }
        }
    }
    fn import_run(&mut self, name: String, content: String) {
        let (wanted, urls) = crate::import::parse(&name, &content);
        if wanted.is_empty() && urls.is_empty() { self.err(self.t("importNothing")); return; }
        self.pages.imp_stop = Arc::new(AtomicBool::new(false));
        self.pages.imp_busy = true; self.pages.imp_saved = false; self.pages.imp_rows.clear(); self.pages.imp_urls.clear();
        let (b, stop, tx, ctx) = (self.be.clone(), self.pages.imp_stop.clone(), self.tx.clone(), self.ctx.clone());
        self.rt.spawn(async move {
            let send = |j: crate::app::Job| { let _ = tx.send(j); ctx.request_repaint(); };
            let mut url_tracks = vec![];
            for u in urls {
                match async { let id = be::resolve_playlist(&b, u.clone()).await?; be::open_playlist(&b, id, None).await }.await {
                    Ok(v) => url_tracks.extend(v),
                    Err(e) => { let m = format!("{u}: {e}"); send(Box::new(move |a: &mut SiloApp| a.err(m))); }
                }
            }
            let ut = url_tracks.clone();
            send(Box::new(move |a: &mut SiloApp| a.pages.imp_urls = ut));
            let mut known = be::history(&b).unwrap_or_default();
            known.extend(be::liked_tracks(&b).unwrap_or_default());
            let mut artist_cache: HashMap<String, Vec<Track>> = HashMap::new();
            let mut yt = 0;
            let n = wanted.len();
            for (i, w) in wanted.into_iter().enumerate() {
                if stop.load(Relaxed) { break; }
                let prog = format!("{} / {n}", i + 1);
                let q = format!("{} {}", w.artist, w.title).trim().to_string();
                let mut best: Option<Track> = None;
                let mut bs = 0.0;
                let pick = |list: &[Track], bs: &mut f64, best: &mut Option<Track>| for t in list { let s = crate::import::score(&w, t); if s > *bs { *bs = s; *best = Some(t.clone()); } };
                pick(&known, &mut bs, &mut best);
                if bs < 0.6 {
                    let p = SearchParams { query: Some(q.clone()), tags: vec![], lang: None, order: None, offset: 0, limit: 6, sources: vec!["local".into(), "jamendo".into(), "soundcloud".into()], audio_format: None, lang_strict: Some(false) };
                    pick(&be::search_all(&b, p).await.map(|r| r.tracks).unwrap_or_default(), &mut bs, &mut best);
                }
                if bs < 0.6 && !w.artist.trim().is_empty() {
                    let ak = w.artist.trim().to_lowercase();
                    if !artist_cache.contains_key(&ak) && yt < 15 { let (v, searched) = be::artist_tracks(&b, w.artist.clone()).await.unwrap_or_default(); if searched { yt += 1; } artist_cache.insert(ak.clone(), v); }
                    pick(artist_cache.get(&ak).map(|v| v.as_slice()).unwrap_or(&[]), &mut bs, &mut best);
                }
                if bs < 0.6 && yt < 15 {
                    yt += 1;
                    let p = SearchParams { query: Some(q.clone()), tags: vec![], lang: None, order: None, offset: 0, limit: 5, sources: vec!["youtube".into()], audio_format: None, lang_strict: Some(false) };
                    pick(&be::search_all(&b, p).await.map(|r| r.tracks).unwrap_or_default(), &mut bs, &mut best);
                }
                let hit = if bs >= 0.6 { best } else { None };
                send(Box::new(move |a: &mut SiloApp| { a.pages.imp_rows.push((w, hit)); a.pages.imp_prog = prog; }));
            }
            send(Box::new(|a: &mut SiloApp| { a.pages.imp_busy = false; a.pages.imp_prog.clear(); }));
        });
    }

    // ---------- настройки ----------
    pub fn page_settings(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let en = self.en();
        page_head(ui, &pal, self.t("settings"), "", |_| {});
        if self.pages.set_tab.is_empty() { self.pages.set_tab = "look".into(); }
        ui.horizontal_wrapped(|ui| {
            for (k, key, ic) in [("look", "setLook", "sun"), ("taste", "setTaste", "heart"), ("play", "playbackTitle", "play"), ("sound", "setSound", "eq"), ("yt", "ytMode", "video"), ("keys", "keysTitle", "key"), ("data", "setData", "database"), ("help", "setHelp", "help")] {
                if btn_sm(ui, &pal, V::Chip, Some(ic), crate::i18n::tr(key, en), self.pages.set_tab == k).clicked() { self.pages.set_tab = k.into(); }
            }
        });
        ui.add_space(12.0);
        match self.pages.set_tab.clone().as_str() {
            "look" => {
                h3(ui, &pal, self.t("theme"));
                let cols = ((ui.available_width() + 10.0) / 190.0).floor().clamp(1.0, 7.0) as usize;
                let cw = (ui.available_width() - 10.0 * (cols - 1) as f32) / cols as f32;
                for row in THEMES.chunks(cols) {
                    ui.horizontal(|ui| {
                        for t in row {
                            let (r, resp) = ui.allocate_exact_size(Vec2::new(cw, 92.0), Sense::click());
                            let on = self.prefs.theme == t.0;
                            let p = crate::theme::Pal::get(t.0);
                            ui.painter().rect(r, CornerRadius::same(12), pal.raise(), Stroke::new(if on { 2.0 } else { 1.0 }, if on { pal.acc } else { pal.line }), egui::StrokeKind::Inside);
                            let pv = Rect::from_min_size(r.min + Vec2::new(10.0, 10.0), Vec2::new(r.width() - 20.0, 44.0));
                            ui.painter().rect_filled(pv, CornerRadius::same(8), p.bg);
                            for k in 0..4 { ui.painter().rect_filled(Rect::from_min_size(pv.min + Vec2::new(10.0 + k as f32 * 22.0, 12.0 + (k % 2) as f32 * 6.0), Vec2::new(14.0, 14.0)), CornerRadius::same(7), p.acc); }
                            ui.painter().text(Pos2::new(r.left() + 12.0, r.bottom() - 18.0), Align2::LEFT_CENTER, if en { t.2 } else { t.1 }, FontId::proportional(13.5), pal.fg);
                            if on { paint_icon(ui.painter(), Rect::from_center_size(Pos2::new(r.right() - 20.0, r.bottom() - 18.0), Vec2::splat(16.0)), "check", pal.acc, false); }
                            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() { self.prefs.theme = t.0.into(); }
                        }
                    });
                    ui.add_space(8.0);
                }
                h3(ui, &pal, self.t("interfaceTitle"));
                card(ui, &pal, ui.available_width(), |ui| {
                    set_row(ui, &pal, crate::i18n::tr("lang", en), "", |ui| {
                        egui::ComboBox::from_id_salt("uilang").selected_text(if en { "English" } else { "Русский" }).show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.prefs.lang, "ru".into(), "Русский");
                            ui.selectable_value(&mut self.prefs.lang, "en".into(), "English");
                        });
                    });
                    sw(ui, &pal, &mut self.prefs.show_video, crate::i18n::tr("showVideo", en), "");
                    sw(ui, &pal, &mut self.prefs.notify, crate::i18n::tr("notifyOpt", en), "");
                    sw(ui, &pal, &mut self.prefs.autostart, crate::i18n::tr("autostartOpt", en), "");
                    sw(ui, &pal, &mut self.prefs.tray, crate::i18n::tr("tray", en), "");
                });
            }
            "taste" => {
                h3(ui, &pal, self.t("tastesTitle"));
                let mut tastes = self.prefs.tastes.clone();
                if self.taste_picker(ui, &mut tastes) { self.prefs.eq = taste::eq_for(&tastes); self.prefs.eq_on = !tastes.is_empty(); self.prefs.tastes = tastes; }
                ui.label(RichText::new(self.t("tastesNote")).color(pal.mut_).size(12.5));
                h3(ui, &pal, self.t("artistsTitle"));
                let mut arts = self.prefs.artists.clone();
                if self.artist_picker(ui, &mut arts) { self.prefs.artists = arts; }
                h3(ui, &pal, self.t("diversityTitle"));
                self.diversity_chips(ui);
                ui.label(RichText::new(self.t("diversityNote")).color(pal.mut_).size(12.5));
                ui.add_space(8.0);
                let mut reset = false;
                card(ui, &pal, ui.available_width(), |ui| {
                    sw(ui, &pal, &mut self.prefs.lang_strict, crate::i18n::tr("langStrict", en), "");
                    set_row(ui, &pal, crate::i18n::tr("resetTaste", en), crate::i18n::tr("resetBody", en), |ui| { if btn(ui, &pal, V::Solid, Some("refresh"), crate::i18n::tr("resetTaste", en), false).clicked() { reset = true; } });
                });
                if reset {
                    self.asks.push(Ask::Confirm { title: self.t("resetTitle").into(), body: self.t("resetBody").into(), ok: self.t("confirm").into(), danger: true, check: Some((self.t("resetGroups").into(), false)), on_ok: Box::new(|a, groups| a.reset_taste(groups)) });
                }
            }
            "play" => {
                card(ui, &pal, ui.available_width(), |ui| {
                    sw(ui, &pal, &mut self.prefs.autoplay, crate::i18n::tr("autoplay", en), "");
                    sw(ui, &pal, &mut self.prefs.gapless, crate::i18n::tr("gapless", en), "");
                    set_row(ui, &pal, crate::i18n::tr("crossfadeT", en), crate::i18n::tr("crossfadeHint", en), |ui| {
                        let mut v = self.prefs.crossfade;
                        let lbl = if v >= 0.5 { format!("{} {}", v.round(), crate::i18n::tr("sec", en)) } else { crate::i18n::tr("off", en).into() };
                        ui.label(RichText::new(lbl).color(pal.mut_));
                        if slider(ui, &pal, &mut v, 0.0, 12.0, 160.0, 0.0).1 { self.prefs.crossfade = v.round(); }
                    });
                    set_row(ui, &pal, crate::i18n::tr("speed", en), "", |ui| {
                        for r in [2.0f32, 1.5, 1.25, 1.0, 0.75] { if btn_sm(ui, &pal, V::Chip, None, &format!("{r}×"), (self.prefs.rate - r).abs() < 0.01).clicked() { self.prefs.rate = r; } }
                    });
                    set_row(ui, &pal, crate::i18n::tr("quality", en), crate::i18n::tr("qualityNote", en), |ui| {
                        let lbl = |q: &str| crate::i18n::tr(match q { "low" => "qLow", "high" => "qHigh", _ => "qNormal" }, en);
                        egui::ComboBox::from_id_salt("q").selected_text(lbl(&self.prefs.quality)).show_ui(ui, |ui| { for q in ["low", "normal", "high"] { ui.selectable_value(&mut self.prefs.quality, q.into(), lbl(q)); } });
                    });
                });
                ui.label(RichText::new(self.t("fxOnly")).color(pal.mut_).size(12.5));
            }
            "sound" => {
                card(ui, &pal, ui.available_width(), |ui| {
                    sw(ui, &pal, &mut self.prefs.normalize, crate::i18n::tr("normalize", en), crate::i18n::tr("normalizeHint", en));
                    sw(ui, &pal, &mut self.prefs.eq_on, crate::i18n::tr("eqOn", en), "");
                    let on = self.prefs.eq_on;
                    set_row(ui, &pal, crate::i18n::tr("eqAutoOpt", en), "", |ui| { switch(ui, &pal, &mut self.prefs.eq_auto, on); });
                });
                if self.prefs.eq_on {
                    ui.add_space(8.0);
                    card(ui, &pal, ui.available_width(), |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for (k, v) in [("flat", [0.0; 10]), ("bass", [6., 5., 4., 2., 0., 0., 0., 0., 0., 0.]), ("vocal", [-2., -1., 0., 2., 4., 4., 3., 1., 0., -1.]), ("rock", [4., 3., 2., -1., -2., -1., 2., 3., 4., 4.]), ("treble", [0., 0., 0., 0., 0., 1., 3., 5., 6., 6.])] {
                                if btn_sm(ui, &pal, V::Chip, None, crate::i18n::tr(k, en), self.prefs.eq == v.to_vec()).clicked() { self.prefs.eq = v.to_vec(); }
                            }
                        });
                        ui.add_space(10.0);
                        if self.prefs.eq.len() < 10 { self.prefs.eq.resize(10, 0.0); }
                        ui.horizontal(|ui| {
                            for (i, f) in crate::audio::EQ_FREQS.iter().enumerate() {
                                ui.vertical(|ui| {
                                    let v = self.prefs.eq[i];
                                    ui.label(RichText::new(format!("{}{}", if v > 0.0 { "+" } else { "" }, v)).size(11.5).color(pal.mut_));
                                    let mut x = v;
                                    if vslider(ui, &pal, &mut x, -12.0, 12.0, 150.0).1 { self.prefs.eq[i] = x; }
                                    ui.label(RichText::new(if *f >= 1000.0 { format!("{}k", f / 1000.0) } else { format!("{f}") }).size(11.0).color(pal.mut_));
                                });
                                ui.add_space(6.0);
                            }
                        });
                    });
                }
                ui.label(RichText::new(self.t("fxOnly")).color(pal.mut_).size(12.5));
            }
            "yt" => self.settings_yt(ui),
            "keys" => self.keys_ui(ui),
            "data" => self.settings_data(ui),
            _ => {
                h3(ui, &pal, self.t("hotkeysTitle"));
                card(ui, &pal, ui.available_width(), |ui| {
                    let keys: [(&str, &str); 21] = [("hkPlay", "Space"), ("hkNext", "Ctrl + →"), ("hkPrev", "Ctrl + ←"), ("hkSeek", "← →"), ("hkVol", "↑ ↓"), ("hkMute", "M"), ("hkLike", "L"), ("shuffle", "S"), ("repeat", "R"), ("queue", "Q"), ("lyrics", "T"), ("karaoke", "K"), ("video", "V"), ("search", "Ctrl + F"), ("search", "/"), ("settings", "Ctrl + ,"), ("hkTabs", "Ctrl + 1…9"), ("hkGlobal", "Ctrl + Alt + Space"), ("hkGlobalSkip", "Ctrl + Alt + ← →"), ("hkMove", "перетаскивание"), ("hkMenu", "ПКМ")];
                    egui::Grid::new("keys").num_columns(2).spacing([40.0, 8.0]).show(ui, |ui| {
                        for (k, v) in keys { ui.label(crate::i18n::tr(k, en)); ui.label(RichText::new(v).monospace().color(pal.acc)); ui.end_row(); }
                    });
                });
                ui.label(RichText::new(self.t("globalKeys")).color(pal.mut_).size(12.5));
                h3(ui, &pal, self.t("limitsTitle"));
                ui.label(RichText::new(self.t("limits")).color(pal.mut_));
                h3(ui, &pal, self.t("aboutApp"));
                ui.label(RichText::new(format!("Silo Player {} · {} · {}", env!("CARGO_PKG_VERSION"), self.t("nativeBadge"), self.t("aboutText"))).color(pal.mut_));
                ui.label(RichText::new("egui · symphonia · cpal · Media Foundation · WebView2 (только для официальных плееров)").color(pal.mut_).size(12.0));
            }
        }
    }

    fn settings_yt(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let en = self.en();
        h3(ui, &pal, self.t("ytMode"));
        card(ui, &pal, ui.available_width(), |ui| {
            for (m, k, h) in [("embed", "ytModeEmbed", "ytModeEmbedHint"), ("ytdlp", "ytModeDlp", "ytModeDlpHint")] {
                let on = self.prefs.yt_mode == m;
                ui.horizontal(|ui| {
                    if ui.radio(on, RichText::new(crate::i18n::tr(k, en)).strong()).clicked() { self.prefs.yt_mode = m.into(); }
                });
                ui.label(RichText::new(crate::i18n::tr(h, en)).color(pal.mut_).size(12.5));
                ui.add_space(8.0);
            }
        });
        if self.prefs.yt_mode == "ytdlp" {
            ui.add_space(8.0);
            notice(ui, &pal, "alert", self.t("ytDlpWarn"), |_| false);
            card(ui, &pal, ui.available_width(), |ui| {
                ui.label(RichText::new(crate::i18n::tr("ytDlpPath", en)).color(pal.mut_));
                let mut check = false;
                ui.horizontal(|ui| {
                    field(ui, &pal, &mut self.prefs.ytdlp_path, "C:\\Tools\\yt-dlp.exe", Some("folder"), (ui.available_width() - 260.0).max(200.0), "ytdlp-path");
                    if btn(ui, &pal, V::Solid, None, "…", false).clicked() { if let Some(p) = rfd::FileDialog::new().add_filter("yt-dlp", &["exe"]).pick_file() { self.prefs.ytdlp_path = p.display().to_string(); } }
                    if btn(ui, &pal, V::Solid, None, crate::i18n::tr("ytDlpCheck", en), false).clicked() { check = true; }
                });
                if check || self.ytdlp_ok.is_none() {
                    self.ytdlp_ok = Some(match crate::ytdlp::find(&self.prefs.ytdlp_path) { Some(p) => crate::ytdlp::version(&p.to_string_lossy()).map(|v| format!("{} {v} · {}", crate::i18n::tr("ytDlpFound", en), p.display())), None => Err(crate::i18n::tr("ytDlpMissing", en).into()) });
                }
                match &self.ytdlp_ok { Some(Ok(s)) => { ui.label(RichText::new(s).color(pal.ok())); } Some(Err(e)) => { ui.label(RichText::new(e).color(pal.danger())); } None => {} }
                ui.add_space(6.0);
                sw(ui, &pal, &mut self.prefs.ytdlp_video, crate::i18n::tr("ytDlpVideo", en), "");
            });
        }
    }

    fn settings_data(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let en = self.en();
        if self.pages.dl_dir.is_none() { self.pages.dl_dir = Some(be::get_setting(&self.be, "download_dir".into()).unwrap_or_default()); self.pages.dl_info = be::downloads_info(&self.be).unwrap_or_default(); }
        if self.pages.folder.is_none() { self.pages.folder = Some(be::get_setting(&self.be, "music_folder".into()).unwrap_or_default()); }
        h3(ui, &pal, self.t("storageTitle"));
        card(ui, &pal, ui.available_width(), |ui| {
            ui.label(RichText::new(crate::i18n::tr("dlFolderT", en)).strong());
            ui.label(RichText::new(format!("{}: {:.1} MB · {}", crate::i18n::tr("dlInfo", en), self.pages.dl_info.0 as f64 / 1048576.0, self.pages.dl_info.1)).color(pal.mut_).size(12.5));
            ui.horizontal(|ui| {
                let mut d = self.pages.dl_dir.clone().unwrap_or_default();
                field(ui, &pal, &mut d, crate::i18n::tr("dlDir", en), Some("folder"), (ui.available_width() - 200.0).max(200.0), "dl-dir");
                if btn(ui, &pal, V::Solid, None, "…", false).clicked() { if let Some(p) = rfd::FileDialog::new().pick_folder() { d = p.display().to_string(); } }
                if btn(ui, &pal, V::Solid, None, crate::i18n::tr("save", en), false).clicked() {
                    let _ = be::set_setting(&self.be, "download_dir".into(), d.clone());
                    self.pages.dl_info = be::downloads_info(&self.be).unwrap_or_default();
                    self.toast(self.t("saved"), ToastKind::Ok);
                }
                self.pages.dl_dir = Some(d);
            });
            ui.add_space(10.0);
            ui.label(RichText::new(crate::i18n::tr("folder", en)).strong());
            ui.label(RichText::new(crate::i18n::tr("folderHint", en)).color(pal.mut_).size(12.5));
            ui.horizontal(|ui| {
                let mut f = self.pages.folder.clone().unwrap_or_default();
                field(ui, &pal, &mut f, "C:\\Users\\...\\Music", Some("music"), (ui.available_width() - 200.0).max(200.0), "music-folder");
                if btn(ui, &pal, V::Solid, None, "…", false).clicked() { if let Some(p) = rfd::FileDialog::new().pick_folder() { f = p.display().to_string(); } }
                if self.pages.scan_busy { spinner(ui, &pal, 18.0); } else if btn_off(ui, &pal, V::Solid, None, crate::i18n::tr("scan", en), !f.trim().is_empty()).clicked() {
                    self.pages.scan_busy = true;
                    let (b, p) = (self.be.clone(), f.clone());
                    self.spawn(async move { be::scan_folder(&b, p).await }, |a, r| { a.pages.scan_busy = false; a.pages.downloads = None; match r { Ok(n) => a.toast(format!("{}: {n}", a.t("scanned")), ToastKind::Ok), Err(e) => a.err(e) } });
                }
                self.pages.folder = Some(f);
            });
        });
        ui.label(RichText::new(self.t("cacheNote")).color(pal.mut_).size(12.5));
        // перенос из основной версии
        h3(ui, &pal, self.t("dataImport"));
        let mut copy = false;
        card(ui, &pal, ui.available_width(), |ui| {
            set_row(ui, &pal, crate::i18n::tr("dataImport", en), crate::i18n::tr("dataImportHint", en), |ui| { if btn(ui, &pal, V::Solid, Some("import"), crate::i18n::tr("dataImportBtn", en), false).clicked() { copy = true; } });
        });
        if copy {
            match crate::copy_main_db(&self.data_dir, true) { Ok(true) => self.toast(self.t("dataImported"), ToastKind::Ok), Ok(false) => self.err(self.t("noMainDb")), Err(e) => self.err(e) }
        }
        // резервная копия
        h3(ui, &pal, self.t("backup"));
        let (mut exp, mut imp) = (false, false);
        card(ui, &pal, ui.available_width(), |ui| {
            set_row(ui, &pal, crate::i18n::tr("backupExport", en), crate::i18n::tr("backupHint", en), |ui| { if btn(ui, &pal, V::Solid, Some("download"), crate::i18n::tr("backupExport", en), false).clicked() { exp = true; } });
            set_row(ui, &pal, crate::i18n::tr("backupImport", en), crate::i18n::tr("backupImportHint", en), |ui| { if btn(ui, &pal, V::Solid, Some("import"), crate::i18n::tr("chooseFile", en), false).clicked() { imp = true; } });
        });
        if exp {
            let b = &self.be;
            let pls: Vec<serde_json::Value> = be::list_playlists(b).unwrap_or_default().iter().map(|p| serde_json::json!({ "name": p.name, "tracks": be::playlist_tracks(b, p.id).unwrap_or_default() })).collect();
            let data = serde_json::json!({ "app": "silo", "v": 1, "prefs": serde_json::to_value(&self.prefs).unwrap_or_default(), "liked": be::liked_tracks(b).unwrap_or_default(), "banned": be::banned_tracks(b).unwrap_or_default(), "playlists": pls });
            let name = format!("silo-backup-{}.json", day_key(0));
            self.save_file(&name, "json", data.to_string());
        }
        if imp {
            if let Some(p) = rfd::FileDialog::new().add_filter("json", &["json"]).pick_file() {
                match std::fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).filter(|d| d["app"] == "silo") {
                    None => self.err(self.t("backupBad")),
                    Some(d) => {
                        let body = format!("{}: {} {}, {} {}.", self.t("backupWill"), d["liked"].as_array().map_or(0, |a| a.len()), self.t("backupLikes"), d["playlists"].as_array().map_or(0, |a| a.len()), self.t("backupPls"));
                        self.asks.push(Ask::Confirm { title: format!("{}?", self.t("backupImport")), body, ok: self.t("backupImport").into(), danger: false, check: None, on_ok: Box::new(move |a, _| {
                            for t in serde_json::from_value::<Vec<Track>>(d["liked"].clone()).unwrap_or_default() { let _ = be::rate_track(&a.be, t, 1); }
                            for t in serde_json::from_value::<Vec<Track>>(d["banned"].clone()).unwrap_or_default() { let _ = be::rate_track(&a.be, t, -2); }
                            for pl in d["playlists"].as_array().cloned().unwrap_or_default() { let _ = be::save_playlist(&a.be, pl["name"].as_str().unwrap_or("").into(), serde_json::from_value(pl["tracks"].clone()).unwrap_or_default()); }
                            if let Ok(p) = serde_json::from_value::<crate::app::Prefs>(d["prefs"].clone()) { a.prefs = p; }
                            a.ratings = be::all_ratings(&a.be).unwrap_or_default();
                            a.refresh_playlists();
                            a.toast(a.t("backupDone"), ToastKind::Ok);
                        }) });
                    }
                }
            }
        }
    }

    /// Ключи сервисов и браузер бесплатных библиотек (Keys.tsx + LibBrowser.tsx).
    pub fn keys_ui(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let en = self.en();
        let all = ["jamendo_client_id", "google_cse_key", "google_cse_cx", "listenbrainz_token", "youtube_api_key", "soundcloud_client_id", "soundcloud_client_secret", "mode_youtube", "mode_soundcloud"];
        if !self.pages.keys_loaded { for k in all { self.pages.keys.insert(k.into(), be::get_setting(&self.be, k.into()).unwrap_or_default()); } self.pages.keys_loaded = true; }
        // бесплатные библиотеки
        card(ui, &pal, ui.available_width(), |ui| {
            ui.label(RichText::new(if en { "Free music libraries" } else { "Бесплатные библиотеки музыки" }).font(bold(15.0)));
            ui.label(RichText::new(if en { "Internet Archive and ccMixter: music under open licenses. Find and download tracks; they will play offline from Downloads." } else { "Internet Archive и ccMixter: музыка под открытыми лицензиями. Найдите и скачайте треки, они будут играть без интернета из раздела «Скачано»." }).color(pal.mut_).size(12.5));
            let mut find = false;
            ui.horizontal(|ui| {
                let r = field(ui, &pal, &mut self.pages.lib_q, if en { "Genre, artist, album…" } else { "Жанр, исполнитель, альбом…" }, Some("search"), 320.0, "lib-q");
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) { find = true; }
                if self.pages.lib_busy { spinner(ui, &pal, 18.0); } else if btn_sm(ui, &pal, V::Solid, None, if en { "Find" } else { "Найти" }, false).clicked() { find = true; }
            });
            if find && !self.pages.lib_q.trim().is_empty() {
                self.pages.lib_busy = true;
                let p = SearchParams { query: Some(self.pages.lib_q.clone()), tags: vec![], lang: None, order: None, offset: 0, limit: 20, sources: vec!["ccmixter".into(), "archive".into()], audio_format: None, lang_strict: None };
                let b = self.be.clone();
                self.spawn(async move { be::search_all(&b, p).await }, |a, r| { a.pages.lib_busy = false; match r { Ok(r) => { if r.tracks.is_empty() { a.toast(a.t("noResults"), ToastKind::Info); } a.pages.lib_res = r.tracks; } Err(e) => a.err(e) } });
            }
            for t in self.pages.lib_res.clone() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&t.name).strong());
                    ui.label(RichText::new(format!("{} · {}", t.artist, src_name(&t.source, en))).color(pal.mut_));
                    let st = self.pages.lib_got.get(&t.id).cloned().unwrap_or_default();
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if btn_off(ui, &pal, V::Ghost, None, if st.is_empty() { if en { "Download" } else { "Скачать" } } else { &st }, st.is_empty()).clicked() {
                            self.pages.lib_got.insert(t.id.clone(), "…".into());
                            let (b, id) = (self.be.clone(), t.id.clone());
                            self.spawn(async move { be::download_track(&b, t).await }, move |a, r| match r { Ok(_) => { a.pages.lib_got.insert(id, "✓".into()); a.pages.downloads = None; } Err(e) => { a.pages.lib_got.remove(&id); a.err(e); } });
                        }
                    });
                });
            }
        });
        type Svc = (&'static str, &'static str, &'static [(&'static str, &'static str, bool)], [&'static str; 2], [&'static [&'static str]; 2]);
        let svcs: [Svc; 5] = [
            ("Jamendo", "https://devportal.jamendo.com", &[("jamendo_client_id", "Jamendo client_id", false)],
                ["Jamendo — главный источник музыки: свободные треки с лицензиями Creative Commons, их можно слушать, скачивать и обрабатывать эквалайзером. Без ключа поиск Jamendo, «Эхо» и скачивание не работают.", "Jamendo is the main music source: free Creative Commons tracks you can stream, download and run through the equalizer. Without a key Jamendo search, Echo and downloads do not work."],
                [&["Откройте devportal.jamendo.com и войдите (или зарегистрируйтесь, это бесплатно).", "Создайте приложение (Create an application), название любое, например «Silo Player».", "Скопируйте Client ID и вставьте ниже."], &["Open devportal.jamendo.com and sign in (or sign up, it is free).", "Create an application, any name, e.g. “Silo Player”.", "Copy the Client ID and paste it below."]]),
            ("Google Custom Search", "https://programmablesearchengine.google.com/controlpanel/create", &[("google_cse_key", "Custom Search API key", true), ("google_cse_cx", "Search engine ID (cx)", false)],
                ["Для режима «по ссылке»: поиск по названию находит треки на YouTube и SoundCloud без их ключей, и они играют внутри приложения. 100 поисков в день бесплатно.", "For link mode: searching by name finds tracks on YouTube and SoundCloud without their keys, and they play inside the app. 100 searches a day free."],
                [&["Откройте programmablesearchengine.google.com и создайте поисковую систему по сайтам youtube.com и soundcloud.com.", "Скопируйте её ID (cx).", "В console.cloud.google.com включите «Custom Search API» и создайте ключ API."], &["Open programmablesearchengine.google.com and create a search engine for youtube.com and soundcloud.com.", "Copy its ID (cx).", "In console.cloud.google.com enable “Custom Search API” and create an API key."]]),
            ("ListenBrainz", "https://listenbrainz.org/settings/", &[("listenbrainz_token", "ListenBrainz token", true)],
                ["Необязательно. Silo будет отправлять в ваш профиль ListenBrainz, что вы дослушали (скробблинг). Без токена ничего никуда не уходит.", "Optional. Silo will send what you finished listening to your ListenBrainz profile (scrobbling). Without a token nothing is sent."],
                [&["Войдите на listenbrainz.org.", "Откройте Настройки и скопируйте User token."], &["Sign in at listenbrainz.org.", "Open Settings and copy the User token."]]),
            ("YouTube", "https://console.cloud.google.com/apis/library/youtube.googleapis.com", &[("youtube_api_key", "YouTube Data API key", true)],
                ["Нужен, чтобы искать видео и плейлисты на YouTube. По умолчанию они играют внутри окна официальным плеером YouTube.", "Needed to search YouTube videos and playlists. By default they play inside the window in the official YouTube player."],
                [&["Откройте console.cloud.google.com и войдите в аккаунт Google.", "Создайте проект, затем на странице «YouTube Data API v3» нажмите «Включить».", "Откройте «Учётные данные» → «Создать учётные данные» → «Ключ API» и скопируйте ключ."], &["Open console.cloud.google.com and sign in with Google.", "Create a project, then on the “YouTube Data API v3” page click Enable.", "Go to Credentials → Create credentials → API key and copy the key."]]),
            ("SoundCloud", "https://soundcloud.com/you/apps", &[("soundcloud_client_id", "SoundCloud client_id", false), ("soundcloud_client_secret", "SoundCloud client_secret", true)],
                ["Нужен для поиска треков и подборок SoundCloud. Играют во встроенном плеере SoundCloud, без скачивания и эквалайзера. Необязателен.", "Needed to search SoundCloud tracks and playlists. They play in the embedded SoundCloud player, without downloads or the equalizer. Optional."],
                [&["Откройте soundcloud.com/you/apps и войдите.", "Зарегистрируйте приложение. SoundCloud выдаёт доступ по заявке, ответ может прийти не сразу.", "Скопируйте Client ID и Client Secret и вставьте ниже."], &["Open soundcloud.com/you/apps and sign in.", "Register an app. SoundCloud grants access on request and may take a while to reply.", "Copy the Client ID and Client Secret and paste them below."]]),
        ];
        let li = usize::from(en);
        for (name, url, fields, why, steps) in svcs {
            ui.add_space(8.0);
            card(ui, &pal, ui.available_width(), |ui| {
                ui.label(RichText::new(name).font(bold(15.0)));
                ui.label(RichText::new(why[li]).color(pal.mut_).size(12.5));
                for (i, s) in steps[li].iter().enumerate() { ui.label(RichText::new(format!("{}. {s}", i + 1)).size(12.5)); }
                if name == "YouTube" || name == "SoundCloud" {
                    let mk = format!("mode_{}", name.to_lowercase());
                    let filled = fields.iter().all(|f| !self.pages.keys.get(f.0).map(|x| x.trim().is_empty()).unwrap_or(true));
                    let cur = self.pages.keys.get(&mk).cloned().filter(|x| !x.is_empty()).unwrap_or_else(|| if filled { "api".into() } else { "link".into() });
                    ui.horizontal(|ui| {
                        for (m, ru, enl) in [("link", "По ссылке (без ключа)", "By link (no key)"), ("api", "Через API (поиск)", "Via API (search)")] {
                            if btn_sm(ui, &pal, V::Chip, None, if en { enl } else { ru }, cur == m).clicked() { self.pages.keys.insert(mk.clone(), m.into()); let _ = be::set_setting(&self.be, mk.clone(), m.into()); }
                        }
                    });
                }
                if btn_sm(ui, &pal, V::Ghost, Some("external"), &if en { format!("Open {name} page") } else { format!("Открыть страницу {name}") }, false).clicked() { self.pages.acts.push(Act::OpenUrl(url.into())); }
                for (k, label, secret) in fields.iter() {
                    let v = self.pages.keys.entry(k.to_string()).or_default();
                    if ui.add(egui::TextEdit::singleline(v).hint_text(*label).password(*secret).desired_width(420.0)).changed() { self.pages.keys_saved = false; }
                }
            });
        }
        ui.add_space(8.0);
        ui.label(RichText::new(if en { "Keys are stored only on this computer, in the app's local database, and are sent only to the matching service." } else { "Ключи хранятся только на этом компьютере, в локальной базе приложения, и отправляются только в соответствующий сервис." }).color(pal.mut_).size(12.5));
        if btn(ui, &pal, V::Solid, None, if self.pages.keys_saved { if en { "Saved" } else { "Сохранено" } } else if en { "Save" } else { "Сохранить" }, self.pages.keys_saved).clicked() {
            for k in all.iter().take(7) { let _ = be::set_setting(&self.be, k.to_string(), self.pages.keys.get(*k).cloned().unwrap_or_default()); }
            self.pages.keys_saved = true;
            self.pages.missing = None;
        }
    }

    /// Выбор вкусов: плитки семейств с поджанрами, поиск по всем узлам, настроения/эпохи/языки.
    fn taste_picker(&mut self, ui: &mut Ui, value: &mut Vec<String>) -> bool {
        let pal = self.pal;
        let en = self.en();
        let mut changed = false;
        let mut toggle = |v: &mut Vec<String>, id: &str| { if let Some(i) = v.iter().position(|x| x == id) { v.remove(i); } else { v.push(id.to_string()); } };
        field(ui, &pal, &mut self.pages.taste_q, crate::i18n::tr("genresSearch", en), Some("search"), 360.0, "taste-q");
        ui.add_space(6.0);
        if !value.is_empty() {
            ui.horizontal_wrapped(|ui| {
                for id in value.clone() { if let Some(n) = taste::node(&id) { if btn_sm(ui, &pal, V::Chip, Some("close"), taste::name(n, en), true).clicked() { toggle(value, &id); changed = true; } } }
            });
            ui.add_space(6.0);
        }
        let q = self.pages.taste_q.trim().to_string();
        if !q.is_empty() {
            let hits = taste::search(&q, 14);
            ui.horizontal_wrapped(|ui| {
                for n in &hits {
                    let fam = taste::family_name(&n.id, en);
                    let label = if fam.is_empty() || fam == taste::name(n, en) { taste::name(n, en).to_string() } else { format!("{} · {fam}", taste::name(n, en)) };
                    if btn_sm(ui, &pal, V::Chip, None, &label, value.contains(&n.id)).clicked() { toggle(value, &n.id); changed = true; }
                }
                if hits.is_empty() { ui.label(RichText::new(crate::i18n::tr("noResults", en)).color(pal.mut_)); }
            });
            return changed;
        }
        ui.horizontal_wrapped(|ui| {
            for f in taste::families() {
                let on = value.contains(&f.id);
                let cnt = taste::selected_in(&f.id, value);
                let label = if cnt > 0 { format!("{} · {cnt}", taste::name(f, en)) } else { taste::name(f, en).to_string() };
                if btn(ui, &pal, V::Chip, None, &label, on).clicked() { toggle(value, &f.id); changed = true; }
                let open = self.pages.taste_open.as_deref() == Some(f.id.as_str());
                if icon_btn(ui, &pal, if open { "chevD" } else { "chevR" }, 26.0, open, crate::i18n::tr("subgenres", en)).clicked() { self.pages.taste_open = if open { None } else { Some(f.id.clone()) }; }
                ui.add_space(6.0);
            }
        });
        if let Some(open) = self.pages.taste_open.clone() {
            ui.add_space(6.0);
            card(ui, &pal, ui.available_width(), |ui| {
                ui.label(RichText::new(format!("{}: {}", crate::i18n::tr("subgenres", en), taste::taste_name(&open, en).unwrap_or_default())).color(pal.mut_));
                ui.horizontal_wrapped(|ui| { for n in taste::descendants(&open) { if btn_sm(ui, &pal, V::Chip, None, taste::name(n, en), value.contains(&n.id)).clicked() { toggle(value, &n.id); changed = true; } } });
            });
        }
        ui.add_space(6.0);
        if btn_sm(ui, &pal, V::Ghost, None, crate::i18n::tr(if self.pages.taste_more { "less" } else { "moreGroups" }, en), false).clicked() { self.pages.taste_more = !self.pages.taste_more; }
        if self.pages.taste_more {
            for (k, title) in [("mood", "moodsTitle"), ("context", "contextTitle"), ("era", "eraTitle"), ("lang", "langTitle")] {
                ui.label(RichText::new(crate::i18n::tr(title, en)).strong());
                ui.horizontal_wrapped(|ui| { for n in taste::of_kind(k) { if btn_sm(ui, &pal, V::Chip, None, taste::name(n, en), value.contains(&n.id)).clicked() { toggle(value, &n.id); changed = true; } } });
                ui.add_space(4.0);
            }
        }
        changed
    }

    /// Выбор 5–10 любимых исполнителей с подсказками (библиотека, Jamendo, SoundCloud, MusicBrainz).
    fn artist_picker(&mut self, ui: &mut Ui, value: &mut Vec<SeedArtist>) -> bool {
        let pal = self.pal;
        let en = self.en();
        let now = ui.input(|i| i.time);
        let full = value.len() >= 10;
        let before = self.pages.art_q.clone();
        ui.add_enabled_ui(!full, |ui| { field(ui, &pal, &mut self.pages.art_q, crate::i18n::tr("artistsSearch", en), Some("user"), 360.0, "art-q"); });
        if self.pages.art_q != before { self.pages.art_changed = now; }
        let q = self.pages.art_q.trim().to_string();
        if q.chars().count() >= 2 && now - self.pages.art_changed > 0.35 && self.pages.art_for != q {
            self.pages.art_for = q.clone();
            self.pages.art_busy = true;
            let b = self.be.clone();
            let q2 = q.clone();
            self.spawn(async move { be::suggest_artists(&b, q2).await }, { let q = q.clone(); move |a, r| { if a.pages.art_for == q { a.pages.art_busy = false; a.pages.art_hits = r.unwrap_or_default(); } } });
        }
        if now - self.pages.art_changed < 0.5 { ui.ctx().request_repaint_after(std::time::Duration::from_millis(120)); }
        let mut changed = false;
        let has = |v: &Vec<SeedArtist>, n: &str| v.iter().any(|a| a.name.eq_ignore_ascii_case(n));
        if q.chars().count() >= 2 && !full {
            card(ui, &pal, 360.0, |ui| {
                for h in self.pages.art_hits.clone() {
                    if has(value, &h.name) { continue; }
                    let src = match h.source.as_str() { "library" => if en { "Your library" } else { "Ваша библиотека" }.to_string(), "musicbrainz" => "MusicBrainz".into(), s => src_name(s, en) };
                    let tags = h.tags.iter().filter_map(|i| taste::taste_name(i, en)).take(3).collect::<Vec<_>>().join(", ");
                    let sub = [src, h.note.clone(), tags].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ");
                    let r = ui.add(egui::Button::new(RichText::new(format!("{}\n{sub}", h.name))).frame(false));
                    if r.clicked() { value.push(SeedArtist { name: h.name.clone(), tags: h.tags.clone() }); self.pages.art_q.clear(); self.pages.art_hits.clear(); changed = true; }
                }
                if self.pages.art_busy { ui.label(RichText::new(crate::i18n::tr("artistLoading", en)).color(pal.mut_)); }
                if !self.pages.art_busy && !has(value, &q) && !self.pages.art_hits.iter().any(|h| h.name.eq_ignore_ascii_case(&q)) {
                    if ui.add(egui::Button::new(RichText::new(format!("{}: {q}\n{}", crate::i18n::tr("artistAdd", en), crate::i18n::tr("artistNoFree", en)))).frame(false)).clicked() { value.push(SeedArtist { name: q.clone(), tags: vec![] }); self.pages.art_q.clear(); changed = true; }
                }
            });
        }
        ui.horizontal_wrapped(|ui| {
            for a in value.clone() { if btn_sm(ui, &pal, V::Chip, Some("close"), &a.name, true).clicked() { value.retain(|x| x.name != a.name); changed = true; } }
        });
        let n = value.len();
        ui.label(RichText::new(format!("{}: {n} / 10{}", crate::i18n::tr("artistsCount", en), if n < 5 { format!(" · {}", crate::i18n::tr("artistsMin", en)) } else if n >= 10 { format!(" · {}", crate::i18n::tr("artistsMax", en)) } else { String::new() })).color(pal.mut_).size(12.5));
        changed
    }

    // ---------- знакомство (первый запуск) ----------
    pub fn onboarding(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let redo = self.pages.onb_redo;
        if self.pages.onb_keys.is_none() { self.pages.onb_keys = Some(!redo); if redo { self.pages.onb_step = 1; } }
        let en = self.en();
        let area = ui.available_rect_before_wrap();
        let w = 760.0f32.min(area.width() - 40.0);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(30.0);
            ui.horizontal(|ui| {
                ui.add_space(((area.width() - w) / 2.0).max(0.0));
                egui::Frame::new().fill(pal.panel).stroke(Stroke::new(1.0, pal.line)).corner_radius(18).inner_margin(28).show(ui, |ui| {
                    ui.set_width(w - 56.0);
                    if self.pages.onb_keys == Some(true) {
                        ui.label(RichText::new(if en { "Connect music services" } else { "Подключите музыкальные сервисы" }).font(bold(24.0)));
                        ui.horizontal(|ui| {
                            if btn_sm(ui, &pal, V::Chip, None, "Русский", !en).clicked() { self.prefs.lang = "ru".into(); }
                            if btn_sm(ui, &pal, V::Chip, None, "English", en).clicked() { self.prefs.lang = "en".into(); }
                        });
                        ui.label(RichText::new(if en { "Below you can download free music from open libraries. YouTube and SoundCloud play by pasted link without keys; for search inside them add a free key. Everything plays inside the app. You can change this later in Settings." } else { "Ниже можно скачать бесплатную музыку из открытых библиотек. YouTube и SoundCloud играют по вставленной ссылке без ключей; для поиска внутри них добавьте бесплатный ключ. Вся музыка играет внутри приложения. Позже всё можно изменить в Настройках." }).color(pal.mut_));
                        ui.add_space(8.0);
                        self.keys_ui(ui);
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            if btn(ui, &pal, V::Ghost, None, crate::i18n::tr("skip", en), false).clicked() { self.pages.onb_keys = Some(false); }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| { if btn(ui, &pal, V::Primary, None, crate::i18n::tr("continue", en), false).clicked() { self.pages.onb_keys = Some(false); } });
                        });
                        return;
                    }
                    match self.pages.onb_step {
                        0 => {
                            ui.label(RichText::new("Выберите язык · Choose language").font(bold(24.0)));
                            ui.add_space(10.0);
                            ui.horizontal(|ui| {
                                if btn_sized(ui, &pal, V::Chip, None, "Русский", !en, 48.0, true).clicked() { self.prefs.lang = "ru".into(); }
                                if btn_sized(ui, &pal, V::Chip, None, "English", en, 48.0, true).clicked() { self.prefs.lang = "en".into(); }
                            });
                            ui.add_space(14.0);
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| { if btn(ui, &pal, V::Primary, None, crate::i18n::tr("continue", en), false).clicked() { self.pages.onb_step = 1; } });
                        }
                        1 => {
                            ui.label(RichText::new(crate::i18n::tr("tastesQ", en)).font(bold(24.0)));
                            ui.label(RichText::new(crate::i18n::tr("tastesHint", en)).color(pal.mut_));
                            ui.add_space(8.0);
                            let mut v = self.pages.onb_tastes.clone();
                            if self.taste_picker(ui, &mut v) { self.pages.onb_tastes = v; }
                            ui.add_space(12.0);
                            ui.horizontal(|ui| {
                                if btn(ui, &pal, V::Ghost, None, crate::i18n::tr("skip", en), false).clicked() { self.pages.onb_tastes.clear(); self.pages.onb_step = 2; }
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if btn_off(ui, &pal, V::Primary, None, crate::i18n::tr("onbNext", en), !self.pages.onb_tastes.is_empty()).clicked() { self.pages.onb_step = 2; }
                                    if icon_btn(ui, &pal, "back", 32.0, false, crate::i18n::tr("back", en)).clicked() { self.pages.onb_step = 0; }
                                });
                            });
                        }
                        2 => {
                            ui.label(RichText::new(crate::i18n::tr("artistsQ", en)).font(bold(24.0)));
                            ui.label(RichText::new(crate::i18n::tr("artistsHint", en)).color(pal.mut_));
                            ui.add_space(8.0);
                            let mut v = self.pages.onb_artists.clone();
                            if self.artist_picker(ui, &mut v) { self.pages.onb_artists = v; }
                            ui.add_space(12.0);
                            let mut next: Option<bool> = None;
                            ui.horizontal(|ui| {
                                if btn(ui, &pal, V::Ghost, None, crate::i18n::tr("skip", en), false).clicked() { next = Some(false); }
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if btn_off(ui, &pal, V::Primary, None, crate::i18n::tr(if redo { "done" } else { "onbNext" }, en), !self.pages.onb_artists.is_empty()).clicked() { next = Some(true); }
                                    if icon_btn(ui, &pal, "back", 32.0, false, crate::i18n::tr("back", en)).clicked() { self.pages.onb_step = 1; }
                                });
                            });
                            if let Some(keep) = next { if !keep { self.pages.onb_artists.clear(); } if redo { self.onb_finish(); } else { self.pages.onb_step = 3; } }
                        }
                        _ => {
                            let tips = [("tip1t", "tip1"), ("tip2t", "tip2"), ("tip3t", "tip3"), ("tip4t", "tip4")];
                            let k = self.pages.onb_tip.min(3);
                            if k == 0 { ui.horizontal(|ui| { for ic in ["like", "dislike", "ban"] { let (r, _) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::hover()); ui.painter().circle_filled(r.center(), 20.0, pal.acc_soft()); paint_icon(ui.painter(), r.shrink(9.0), ic, pal.acc, false); } }); }
                            ui.label(RichText::new(crate::i18n::tr(tips[k].0, en)).font(bold(24.0)));
                            ui.label(RichText::new(crate::i18n::tr(tips[k].1, en)).color(pal.mut_));
                            ui.add_space(14.0);
                            ui.horizontal(|ui| {
                                if btn(ui, &pal, V::Ghost, None, crate::i18n::tr("skip", en), false).clicked() { self.onb_finish(); }
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if btn(ui, &pal, V::Primary, None, crate::i18n::tr(if k < 3 { "onbNext" } else { "done" }, en), false).clicked() { if k < 3 { self.pages.onb_tip += 1; } else { self.onb_finish(); } }
                                });
                            });
                        }
                    }
                });
            });
        });
    }
    fn onb_finish(&mut self) {
        let ts = self.pages.onb_tastes.clone();
        self.prefs.eq = taste::eq_for(&ts);
        self.prefs.eq_on = !ts.is_empty();
        self.prefs.tastes = ts;
        self.prefs.artists = self.pages.onb_artists.clone();
        self.prefs.onboarded = true;
        self.pages.onb_redo = false;
        self.pages.onb_step = 0;
        self.pages.onb_tip = 0;
        self.pages.onb_keys = None;
        self.pages.missing = None;
    }
}

fn matches(t: &Track, low: &str) -> bool { format!("{} {} {}", t.name, t.artist, t.album).to_lowercase().contains(&low.to_lowercase()) }

fn src_color(s: &str) -> Color32 {
    match s { "youtube" => Color32::from_rgb(0xe5, 0x48, 0x4d), "soundcloud" => Color32::from_rgb(0xff, 0x77, 0x33), "jamendo" => Color32::from_rgb(0x3f, 0xb2, 0x7f), "deezer" => Color32::from_rgb(0xa2, 0x38, 0xff), "local" => Color32::from_rgb(0x5a, 0xa2, 0xff), _ => Color32::from_rgb(0x99, 0x99, 0x99) }
}

/// Меню трека (кнопка «⋯» и правая кнопка мыши).
fn track_menu(ui: &mut Ui, pal: &crate::theme::Pal, en: bool, t: &Track, r: i32, pls: &[silo_core::model::Playlist], pl: Option<i64>) -> Option<Act> {
    let tr = |k| crate::i18n::tr(k, en);
    let mut out = None;
    let mut mi = |ui: &mut Ui, ic: &str, label: &str, enabled: bool| -> bool {
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(230.0, 30.0), if enabled { Sense::click() } else { Sense::hover() });
        if resp.hovered() && enabled { ui.painter().rect_filled(rect, CornerRadius::same(6), pal.hover()); }
        let col = if enabled { pal.fg } else { pal.mut_ };
        paint_icon(ui.painter(), Rect::from_min_size(rect.min + Vec2::new(8.0, 7.0), Vec2::splat(16.0)), ic, col, false);
        ui.painter().text(rect.min + Vec2::new(34.0, 15.0), Align2::LEFT_CENTER, label, FontId::proportional(13.5), col);
        resp.clicked()
    };
    if mi(ui, "playNext", tr("playNext"), true) { out = Some(Act::PlayNext(t.clone())); }
    if mi(ui, "addQueue", tr("addQueue"), true) { out = Some(Act::AddQueue(t.clone())); }
    if mi(ui, "radio", tr("trackRadio"), true) { out = Some(Act::Radio(t.clone())); }
    ui.separator();
    if mi(ui, "like", tr(if r == 1 { "unlike" } else { "like" }), true) { out = Some(Act::Rate(t.clone(), if r == 1 { 0 } else { 1 })); }
    if !t.artist.is_empty() && mi(ui, "user", tr("artistPage"), true) { out = Some(Act::Artist(t.artist.clone())); }
    if mi(ui, "info", tr("about"), true) { out = Some(Act::Info(t.clone())); }
    if mi(ui, "copy", tr("copyName"), true) { out = Some(Act::Copy(if t.artist.is_empty() { t.name.clone() } else { format!("{} - {}", t.artist, t.name) }, tr("copiedName").into())); }
    if ["jamendo", "ccmixter", "archive"].contains(&t.source.as_str()) && mi(ui, "download", tr(if t.download_allowed { "download" } else { "noDownload" }), t.download_allowed) { out = Some(Act::Download(t.clone())); }
    if let Some(id) = pl { ui.separator(); if mi(ui, "trash", tr("removeFromPlaylist"), true) { out = Some(Act::RemoveFromPl(id, t.clone())); } }
    if !pls.is_empty() {
        ui.separator();
        ui.label(RichText::new(tr("addToPlaylist")).size(12.0).color(pal.mut_));
        for p in pls { if mi(ui, "playlist", &p.name, true) { out = Some(Act::AddToPl(p.id, p.name.clone(), t.clone())); } }
    }
    if out.is_some() { ui.close(); }
    out
}

fn pl_menu(ui: &mut Ui, pal: &crate::theme::Pal, en: bool) -> Option<u8> {
    let tr = |k| crate::i18n::tr(k, en);
    let mut out = None;
    for (i, (ic, k)) in [("edit", "rename"), ("copy", "exportPl"), ("download", "exportM3u"), ("trash", "deletePlaylist")].iter().enumerate() {
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(220.0, 30.0), Sense::click());
        if resp.hovered() { ui.painter().rect_filled(rect, CornerRadius::same(6), pal.hover()); }
        let col = if i == 3 { pal.danger() } else { pal.fg };
        paint_icon(ui.painter(), Rect::from_min_size(rect.min + Vec2::new(8.0, 7.0), Vec2::splat(16.0)), ic, col, false);
        ui.painter().text(rect.min + Vec2::new(34.0, 15.0), Align2::LEFT_CENTER, tr(k), FontId::proportional(13.5), col);
        if resp.clicked() { out = Some(i as u8); }
        if i == 2 { ui.separator(); }
    }
    if out.is_some() { ui.close(); }
    out
}

pub fn card<R>(ui: &mut Ui, pal: &crate::theme::Pal, w: f32, f: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new().fill(pal.raise()).stroke(Stroke::new(1.0, pal.line)).corner_radius(12).inner_margin(egui::Margin::same(14)).show(ui, |ui| { ui.set_width(w - 30.0); f(ui) }).inner
}
fn h3(ui: &mut Ui, pal: &crate::theme::Pal, t: &str) { ui.add_space(14.0); ui.label(RichText::new(t).font(bold(16.0)).color(pal.fg)); ui.add_space(6.0); }
fn set_row(ui: &mut Ui, pal: &crate::theme::Pal, label: &str, hint: &str, f: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        let w = (ui.available_width() - 300.0).max(200.0);
        ui.vertical(|ui| {
            ui.set_width(w);
            ui.label(RichText::new(label).strong());
            if !hint.is_empty() { ui.add(egui::Label::new(RichText::new(hint).color(pal.mut_).size(12.0)).wrap()); }
        });
        ui.with_layout(Layout::right_to_left(Align::Center), f);
    });
    ui.add_space(8.0);
}
fn sw(ui: &mut Ui, pal: &crate::theme::Pal, v: &mut bool, label: &str, hint: &str) {
    // длинные подписи вида «Текст (пояснение)» делим на заголовок и пояснение
    let (l, h) = match label.find(" (").or_else(|| label.find(": ")) { Some(i) if label.len() - i > 14 && hint.is_empty() => (label[..i].to_string(), label[i + 2..].trim_end_matches(')').to_string()), _ => (label.to_string(), hint.to_string()) };
    set_row(ui, pal, &l, &h, |ui| { switch(ui, pal, v, true); });
}
/// Плашка-уведомление с иконкой; действие справа (true — нажато).
fn notice(ui: &mut Ui, pal: &crate::theme::Pal, icon: &str, text: &str, action: impl FnOnce(&mut Ui) -> bool) -> bool {
    let mut hit = false;
    egui::Frame::new().fill(pal.acc_soft()).stroke(Stroke::new(1.0, mix(pal.line, pal.acc, 0.5))).corner_radius(10).inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
            paint_icon(ui.painter(), r, icon, pal.acc, false);
            ui.add(egui::Label::new(RichText::new(text).color(pal.fg)).wrap());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| { hit = action(ui); });
        });
    });
    ui.add_space(8.0);
    hit
}
fn stat_row(ui: &mut Ui, pal: &crate::theme::Pal, i: usize, name: &str, sub: &str, n: u64, max: u64) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), if sub.is_empty() { 26.0 } else { 34.0 }), Sense::click());
    let w = r.width() * n as f32 / max as f32;
    ui.painter().rect_filled(Rect::from_min_size(r.min, Vec2::new(w, r.height())), CornerRadius::same(6), mix(Color32::TRANSPARENT, pal.acc, if resp.hovered() { 0.25 } else { 0.14 }));
    ui.painter().text(r.left_center() + Vec2::new(8.0, 0.0), Align2::LEFT_CENTER, (i + 1).to_string(), FontId::proportional(12.0), pal.mut_);
    paint_trunc(ui, r.min + Vec2::new(26.0, if sub.is_empty() { 5.0 } else { 2.0 }), name, FontId::proportional(13.0), pal.fg, r.width() - 60.0, Align2::LEFT_TOP);
    if !sub.is_empty() { paint_trunc(ui, r.min + Vec2::new(26.0, 18.0), sub, FontId::proportional(11.5), pal.mut_, r.width() - 60.0, Align2::LEFT_TOP); }
    ui.painter().text(r.right_center() - Vec2::new(8.0, 0.0), Align2::RIGHT_CENTER, n.to_string(), FontId::proportional(12.0), pal.mut_);
    resp
}
