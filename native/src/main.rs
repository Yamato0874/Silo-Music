//! Silo Player — нативная версия: интерфейс на egui поверх общего ядра silo-core, без веб-движка для интерфейса.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod audio;
mod backend;
mod cache;
mod enhance;
mod exciter;
mod demo;
mod embed;
mod i18n;
mod import;
mod pages;
mod platform;
mod shell;
mod taste;
mod theme;
mod video;
mod widgets;
mod ytdlp;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// База основной (React) версии: %APPDATA%\app.silo.player\silo.db.
fn main_db() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("app.silo.player").join("silo.db")).filter(|p| p.exists())
}

/// Скопировать данные основной версии (её база не меняется: читаем снимок через VACUUM INTO).
/// force = false — только если своей базы ещё нет (первый запуск); true — положить копию, она подхватится при следующем запуске.
pub fn copy_main_db(dir: &Path, force: bool) -> Result<bool, String> {
    let Some(src) = main_db() else { return Ok(false) };
    let dst = if force { dir.join("pending-import.db") } else { dir.join("silo.db") };
    if !force && dst.exists() { return Ok(false); }
    let _ = std::fs::remove_file(&dst);
    let c = rusqlite::Connection::open_with_flags(&src, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|e| e.to_string())?;
    c.execute("VACUUM INTO ?1", [dst.to_string_lossy().to_string()]).map_err(|e| e.to_string())?;
    Ok(true)
}

fn copy_keys(db: &silo_core::db::Db) {
    let Some(src) = main_db() else { return };
    let Ok(c) = rusqlite::Connection::open_with_flags(&src, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) else { return };
    let keys = ["youtube_api_key", "soundcloud_client_id", "soundcloud_client_secret", "jamendo_client_id", "google_cse_key", "google_cse_cx", "listenbrainz_token", "mode_youtube", "mode_soundcloud"];
    for k in keys {
        if let Ok(v) = c.query_row("SELECT value FROM settings WHERE key=?", [k], |r| r.get::<_, String>(0)) {
            let _ = db.conn.execute("INSERT OR REPLACE INTO settings(key,value) VALUES(?,?)", [k, v.as_str()]);
        }
    }
}

/// Ключ YouTube, подставленный при сборке из переменной окружения SILO_EMBEDDED_YT_KEY (в репозитории и в обычных релизах его нет).
/// Ставится только если свой ключ в базе пуст; из exe такой ключ можно вытащить, поэтому раздавать такую сборку публично нельзя.
fn embed_keys(db: &silo_core::db::Db) {
    let Some(k) = option_env!("SILO_EMBEDDED_YT_KEY").map(str::trim).filter(|k| !k.is_empty()) else { return };
    let has: bool = db.conn.query_row("SELECT value FROM settings WHERE key=\"youtube_api_key\"", [], |r| r.get::<_, String>(0)).map(|v| !v.trim().is_empty()).unwrap_or(false);
    if !has { let _ = db.conn.execute("INSERT OR REPLACE INTO settings(key,value) VALUES(\"youtube_api_key\",?)", [k]); }
}

fn main() {
    platform::set_app_id();
    let testing = std::env::var_os("SILO_DEMO").is_some() || std::env::var_os("SILO_BENCH").is_some() || std::env::var_os("SILO_SELFTEST").is_some();
    if !testing && !platform::single_instance() { return; }
    // SILO_DATA_DIR — отдельная папка данных для самопроверок, чтобы тесты не трогали настоящую базу
    let dir = std::env::var_os("SILO_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("app.silo.player.native"));
    let _ = std::fs::create_dir_all(&dir);
    platform::register_app_id(&dir);
    // отложенный перенос из основной версии (кнопка в Настройках → Данные)
    let pending = dir.join("pending-import.db");
    if pending.exists() {
        let cur = dir.join("silo.db");
        let _ = std::fs::create_dir_all(dir.join("backups"));
        if cur.exists() { let _ = std::fs::copy(&cur, dir.join("backups").join("silo-before-import.db")); }
        for ext in ["-wal", "-shm"] { let _ = std::fs::remove_file(dir.join(format!("silo.db{ext}"))); }
        if std::fs::copy(&pending, &cur).is_ok() { let _ = std::fs::remove_file(&pending); }
    }
    let fresh = !dir.join("silo.db").exists();
    let db = match silo_core::db::Db::open(&dir.join("silo.db")) {
        Ok(d) => d,
        Err(e) => { let _ = rfd::MessageDialog::new().set_title("Silo").set_description(format!("Не удалось открыть базу: {e}")).show(); return; }
    };
    // первый запуск — с чистого листа: из основной версии переносятся только ключи сервисов
    if fresh { copy_keys(&db); }
    embed_keys(&db);
    let backend = Arc::new(backend::App { db: Mutex::new(db), music_dir: dir.join("Music") });
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().expect("tokio");
    let handle = rt.handle().clone();
    let hidden = std::env::args().any(|a| a == "--tray");
    let mut vp = eframe::egui::ViewportBuilder::default()
        .with_title("Silo Player")
        .with_inner_size([1180.0, 760.0])
        .with_min_inner_size([800.0, 560.0])
        .with_drag_and_drop(true)
        // своя тёмная шапка вместо системной рамки Windows
        .with_decorations(false)
        .with_visible(!hidden);
    if let Some(icon) = platform::window_icon() { vp = vp.with_icon(icon); }
    // самопроверка: окно за пределами экрана и без кнопки на панели задач, чтобы не мешать
    if std::env::var_os("SILO_DEMO").is_some() || std::env::var_os("SILO_BENCH").is_some() || std::env::var_os("SILO_SELFTEST").is_some() { vp = vp.with_position([-2600.0, 40.0]).with_taskbar(false); }
    let opts = eframe::NativeOptions { viewport: vp, ..Default::default() };
    let d2 = dir.clone();
    let _ = eframe::run_native("Silo Player", opts, Box::new(move |cc| Ok(Box::new(app::SiloApp::new(cc, handle, backend, d2)))));
    drop(rt);
}
