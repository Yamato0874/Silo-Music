//! Самопроверка интерфейса: SILO_DEMO=<папка> проходит по экранам и сохраняет снимки окна (без доступа к экрану).
use eframe::egui;

use crate::app::{SiloApp, Tab};

pub struct Demo { dir: std::path::PathBuf, step: usize, at: f64, waiting: bool, done: std::sync::Arc<std::sync::atomic::AtomicBool>, saved: Option<String>, started: bool }

impl Demo {
    pub fn from_env() -> Option<Demo> {
        std::env::var_os("SILO_DEMO").map(|d| Demo { dir: d.into(), step: 0, at: 0.0, waiting: false, done: Default::default(), saved: None, started: false })
    }
}

const STEPS: [(&str, f64); 18] = [
    ("01-home", 6.0), ("02-search", 7.0), ("03-echo", 3.0), ("04-liked", 3.0), ("05-playlists", 3.0), ("06-history", 3.0),
    ("07-settings-look", 2.0), ("08-settings-yt", 2.0), ("09-settings-sound", 2.0), ("09b-ai-missing", 2.0), ("09c-ai-installing", 2.0), ("09d-ai-ready", 2.0), ("10-queue", 3.0), ("11-lyrics", 4.0), ("12-karaoke", 2.0), ("13-light", 3.0), ("14-preview", 8.0), ("15-youtube", 14.0),
];

/// Замеры: SILO_BENCH=preview|youtube — найти «кино группа крови» и включить превью (свой движок) или полный трек (официальный плеер), звук на нуле.
pub fn bench(app: &mut SiloApp, ctx: &egui::Context) {
    let Some(mode) = std::env::var("SILO_BENCH").ok() else { return };
    let t = ctx.input(|i| i.time);
    let step = app.bench_step;
    if step == 0 && t > 1.0 { app.prefs.volume = 0.0; app.set_volume_live(0.0); app.tab = Tab::Search; app.demo_query("кино группа крови"); app.bench_step = 1; }
    if step == 1 { if let Some(mut tr) = app.demo_first_result() { if mode == "preview" { tr.kind = "preview".into(); } if mode != "idle" { app.play(vec![tr], 0, false); } app.tab = Tab::Home; app.bench_step = 2; } }
    ctx.request_repaint_after(std::time::Duration::from_millis(200));
}

/// Самопроверка yt-dlp с поддельным yt-dlp.exe (SILO_SELFTEST=ytdlp, SILO_FAKE_YTDLP=<путь>): поиск → воспроизведение → перемотка. Журнал — в stderr.
pub fn selftest(app: &mut SiloApp, ctx: &egui::Context) {
    let Ok(mode) = std::env::var("SILO_SELFTEST") else { return };
    if mode == "enhance" { selftest_enhance(app, ctx); return; }
    let t = ctx.input(|i| i.time);
    let st = app.bench_step;
    let log = |app: &SiloApp, what: &str| eprintln!("[selftest t={:.1}] {what}: route={} playing={} pos={:.1} dur={:.1} results={} toasts={:?}", ctx.input(|i| i.time), app.route_name(), app.ps.playing, app.ps.pos, app.ps.dur, app.demo_result_count(), app.toasts.iter().map(|x| x.text.clone()).collect::<Vec<_>>());
    match st {
        0 if t > 1.0 => {
            app.prefs.onboarded = true;
            app.prefs.volume = 0.0; app.set_volume_live(0.0);
            app.prefs.yt_mode = if mode == "embed" { "embed".into() } else { "ytdlp".into() };
            app.prefs.ytdlp_path = std::env::var("SILO_FAKE_YTDLP").unwrap_or_default();
            if let Ok(u) = std::env::var("SILO_UPSCALE") { app.prefs.upscale = u.parse().unwrap_or(0); }
            if std::env::var("SILO_HOLD").is_ok() { app.prefs.repeat = "one".into(); app.prefs.gapless = false; }
            app.tab = Tab::Search; app.demo_query("кино группа крови");
            app.bench_step = 1; app.selftest_at = t; log(app, "search sent");
        }
        1 if app.demo_result_count() > 0 => { log(app, "results"); if let Some(tr) = app.demo_first_result() { eprintln!("[selftest] first = {} / {} / {}s / {}", tr.name, tr.artist, tr.duration, tr.id); app.play(vec![tr], 0, false); } app.bench_step = 2; app.selftest_at = t; }
        1 if t - app.selftest_at > 15.0 => { log(app, "NO RESULTS"); std::process::exit(2); }
        2 if std::env::var("SILO_HOLD").is_ok() => { if t - app.selftest_at > 3.0 { log(app, "playing (удержание для замера)"); app.bench_step = 99; } }
        2 if t - app.selftest_at > 3.0 => { log(app, "playing 3s"); app.seek(14.0); app.bench_step = 3; app.selftest_at = t; }
        3 if t - app.selftest_at > 1.5 => { log(app, "after seek (during download)"); app.bench_step = 4; app.selftest_at = t; }
        4 if t - app.selftest_at > 7.0 => { log(app, "after download"); app.seek(2.0); app.bench_step = 5; app.selftest_at = t; }
        5 if t - app.selftest_at > 2.0 => { log(app, "after seek back to 2s"); app.toggle(); app.bench_step = 6; app.selftest_at = t; }
        6 if t - app.selftest_at > 1.5 => { log(app, "paused"); std::process::exit(0); }
        _ => {}
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(100));
}

/// Самопроверка подготовки наперёд и «Среднего» апскейла с поддельными yt-dlp и Python (SILO_SELFTEST=enhance).
fn selftest_enhance(app: &mut SiloApp, ctx: &egui::Context) {
    let t = ctx.input(|i| i.time);
    let st = app.bench_step;
    let files = |d: &std::path::Path| -> String { let mut v: Vec<String> = std::fs::read_dir(d).map(|rd| rd.flatten().map(|e| format!("{}({}K)", e.file_name().to_string_lossy().chars().take(8).collect::<String>(), e.metadata().map(|m| m.len() / 1024).unwrap_or(0))).collect()).unwrap_or_default(); v.sort(); v.join(" ") };
    let log = |app: &SiloApp, what: &str| eprintln!("[enh t={:.1}] {what}: route={} playing={} pos={:.1} src={} | cache[{}] enh[{}] queue={:?} state={:?}", ctx.input(|i| i.time), app.route_name(), app.ps.playing, app.ps.pos, format!("{} fx={}", app.last_src, app.last_fx_up), files(&app.cache.dir), files(&app.cache.enh_dir), app.enh.queue_info(), app.enh.state());
    match st {
        0 if t > 1.0 => {
            app.prefs.onboarded = true;
            app.prefs.volume = 0.0; app.set_volume_live(0.0);
            app.prefs.yt_mode = "ytdlp".into();
            app.prefs.ytdlp_path = std::env::var("SILO_FAKE_YTDLP").unwrap_or_default();
            app.prefs.upscale = 2;
            app.tab = Tab::Search; app.demo_query("кино группа крови");
            app.bench_step = 1; app.selftest_at = t; log(app, "start");
        }
        1 if app.demo_result_count() > 0 => { let all = app.demo_results(); app.play(all, 0, false); app.bench_step = 2; app.selftest_at = t; log(app, "playing #1"); }
        2 if t - app.selftest_at > 5.0 => { log(app, "5s after start"); app.bench_step = 3; app.selftest_at = t; }
        3 if t - app.selftest_at > 8.0 => { log(app, "13s after start"); app.skip(1); app.bench_step = 4; app.selftest_at = t; }
        4 if t - app.selftest_at > 2.5 => { log(app, "track #2 playing"); app.skip(1); app.bench_step = 5; app.selftest_at = t; }
        5 if t - app.selftest_at > 2.5 => { log(app, "track #3 playing"); std::process::exit(0); }
        _ => {}
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(100));
}

pub fn frame(app: &mut SiloApp, ctx: &egui::Context) {
    selftest(app, ctx);
    bench(app, ctx);
    let Some(mut d) = app.demo.take() else { return };
    let now = ctx.input(|i| i.time);
    // готовый снимок
    if d.waiting && d.done.swap(false, std::sync::atomic::Ordering::SeqCst) {
        d.step += 1;
        d.waiting = false;
        d.at = now;
        if d.step >= STEPS.len() {
            // вернуть настройки как были до самопроверки
            if let Some(s) = &d.saved { let _ = crate::backend::set_setting(&app.be, "prefs".into(), s.clone()); }
            std::process::exit(0);
        }
    }
    if d.at == 0.0 { d.at = now; d.saved = serde_json::to_string(&app.prefs).ok(); app.prefs.onboarded = true; }
    if !d.waiting {
        // подготовка шага
        let step = STEPS[d.step].0;
        match step {
            "01-home" => app.tab = Tab::Home,
            "02-search" => { if app.tab != Tab::Search { app.tab = Tab::Search; app.demo_query("кино группа крови"); } }
            "03-echo" => app.tab = Tab::Echo,
            "04-liked" => app.tab = Tab::Liked,
            "05-playlists" => app.tab = Tab::Playlists,
            "06-history" => app.tab = Tab::History,
            "07-settings-look" => { app.tab = Tab::Settings; app.demo_set_tab("look"); }
            "08-settings-yt" => app.demo_set_tab("yt"),
            "09-settings-sound" => { app.demo_set_tab("sound"); app.prefs.eq_on = true; }
            "09b-ai-missing" => { app.pages.focus_ai = true; }
            "09c-ai-installing" => { app.enh.demo_state(crate::enhance::Install::Installing { step: "3/6 PyTorch (≈ 2,7 ГБ, самый долгий шаг)".into(), detail: "1234 МБ, прошло 3 мин".into(), frac: None }); app.pages.focus_ai = true; }
            "09d-ai-ready" => { app.enh.demo_state(crate::enhance::Install::Ready); app.pages.focus_ai = true; }
            "10-queue" => { app.tab = Tab::Home; app.show_q = true; }
            "11-lyrics" => { app.show_q = false; app.show_l = true; app.load_lyrics(); }
            "12-karaoke" => { app.show_l = false; app.stage = true; app.karaoke = true; app.load_lyrics(); }
            "13-light" => { app.stage = false; app.karaoke = false; app.prefs.theme = "sage".into(); }
            "14-preview" => if !d.started {
                d.started = true;
                app.prefs.theme = "amber".into();
                app.stage = true; app.karaoke = true;
                app.prefs.volume = 0.0; app.set_volume_live(0.0);
                if let Some(mut t) = app.demo_first_result() { t.kind = "preview".into(); app.play(vec![t], 0, false); }
            },
            "15-youtube" => if !d.started {
                d.started = true;
                app.stage = false; app.karaoke = false;
                app.prefs.show_video = true;
                app.tab = Tab::Search;
                if let Some(t) = app.demo_first_result() { app.play(vec![t], 0, false); }
            },
            _ => {}
        }
        if now - d.at > STEPS[d.step].1 {
            let log = format!("{}: route={:?} playing={} pos={:.1} dur={:.1} toasts={:?}
", STEPS[d.step].0, app.route_name(), app.ps.playing, app.ps.pos, app.ps.dur, app.toasts.iter().map(|t| t.text.clone()).collect::<Vec<_>>());
            { use std::io::Write; if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(d.dir.join("log.txt")) { let _ = f.write_all(log.as_bytes()); } }
            d.started = false;
            let (path, done, c2) = (d.dir.join(format!("{}.png", STEPS[d.step].0)), d.done.clone(), ctx.clone());
            let _ = std::fs::create_dir_all(&d.dir);
            ctx.request_screenshot(move |img| {
                let [w, h] = img.size;
                let raw: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
                if let Some(b) = image::RgbaImage::from_raw(w as u32, h as u32, raw) { let _ = b.save(&path); }
                done.store(true, std::sync::atomic::Ordering::SeqCst);
                c2.request_repaint();
            });
            d.waiting = true;
        }
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(100));
    app.demo = Some(d);
}
