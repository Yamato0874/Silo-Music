//! Самопроверка интерфейса: SILO_DEMO=<папка> проходит по экранам и сохраняет снимки окна (без доступа к экрану).
use eframe::egui;

use crate::app::{SiloApp, Tab};

pub struct Demo { dir: std::path::PathBuf, step: usize, at: f64, waiting: bool, done: std::sync::Arc<std::sync::atomic::AtomicBool>, saved: Option<String>, started: bool }

impl Demo {
    pub fn from_env() -> Option<Demo> {
        std::env::var_os("SILO_DEMO").map(|d| Demo { dir: d.into(), step: 0, at: 0.0, waiting: false, done: Default::default(), saved: None, started: false })
    }
}

const STEPS: [(&str, f64); 15] = [
    ("01-home", 6.0), ("02-search", 7.0), ("03-echo", 3.0), ("04-liked", 3.0), ("05-playlists", 3.0), ("06-history", 3.0),
    ("07-settings-look", 2.0), ("08-settings-yt", 2.0), ("09-settings-sound", 2.0), ("10-queue", 3.0), ("11-lyrics", 4.0), ("12-karaoke", 4.0), ("13-light", 3.0), ("14-preview", 8.0), ("15-youtube", 14.0),
];

pub fn frame(app: &mut SiloApp, ctx: &egui::Context) {
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
    if d.at == 0.0 { d.at = now; d.saved = serde_json::to_string(&app.prefs).ok(); }
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
            "10-queue" => { app.tab = Tab::Home; app.show_q = true; }
            "11-lyrics" => { app.show_q = false; app.show_l = true; app.load_lyrics(); }
            "12-karaoke" => { app.show_l = false; app.stage = true; app.karaoke = true; app.load_lyrics(); }
            "13-light" => { app.stage = false; app.karaoke = false; app.prefs.theme = "sage".into(); }
            "14-preview" => if !d.started {
                d.started = true;
                app.prefs.theme = "amber".into();
                app.prefs.volume = 0.0; app.set_volume_live(0.0);
                if let Some(mut t) = app.demo_first_result() { t.kind = "preview".into(); app.play(vec![t], 0, false); }
            },
            "15-youtube" => if !d.started {
                d.started = true;
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
