//! Режим «без плеера»: звук и видео YouTube через внешний yt-dlp (включается в настройках, по умолчанию выключен).
//! yt-dlp не входит в установщик: пользователь ставит его сам и сам отвечает за соблюдение условий YouTube.
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
static LAST_ERR: Mutex<String> = Mutex::new(String::new());

pub fn last_error() -> String {
    let e = LAST_ERR.lock().unwrap().clone();
    if e.is_empty() { "yt-dlp завершился с ошибкой".into() } else { e }
}

/// Где лежит yt-dlp: путь из настроек, рядом с программой, затем PATH.
pub fn find(custom: &str) -> Option<PathBuf> {
    let c = custom.trim().trim_matches('"');
    if !c.is_empty() {
        let p = PathBuf::from(c);
        return p.is_file().then_some(p);
    }
    if let Some(d) = std::env::current_exe().ok().and_then(|x| x.parent().map(|p| p.to_path_buf())) {
        for n in ["yt-dlp.exe", "yt-dlp/yt-dlp.exe"] { let p = d.join(n); if p.is_file() { return Some(p); } }
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join("yt-dlp.exe")).find(|p| p.is_file())
}

fn cmd(exe: &str) -> Command {
    let mut c = Command::new(exe);
    c.creation_flags(CREATE_NO_WINDOW).stdin(Stdio::null());
    c
}

/// Версия yt-dlp (для проверки в настройках).
pub fn version(exe: &str) -> Result<String, String> {
    let o = cmd(exe).arg("--version").output().map_err(|e| e.to_string())?;
    if !o.status.success() { return Err(String::from_utf8_lossy(&o.stderr).trim().to_string()); }
    Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// Процесс, который пишет звуковую дорожку (m4a/AAC, её умеет наш декодер) в stdout.
pub fn audio_pipe(exe: &str, vid: &str) -> Result<Child, String> {
    let url = format!("https://www.youtube.com/watch?v={vid}");
    let mut c = cmd(exe);
    c.args(["-f", "bestaudio[ext=m4a]/140/bestaudio[acodec^=mp4a]", "--no-playlist", "--no-part", "--quiet", "--no-warnings", "-o", "-", &url])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = c.spawn().map_err(|e| format!("yt-dlp не запустился: {e}"))?;
    if let Some(mut err) = child.stderr.take() {
        std::thread::spawn(move || {
            use std::io::Read;
            let mut s = String::new();
            let _ = err.read_to_string(&mut s);
            let line = s.lines().filter(|l| l.contains("ERROR")).last().unwrap_or(s.lines().last().unwrap_or("")).trim().to_string();
            if !line.is_empty() { *LAST_ERR.lock().unwrap() = line; }
        });
    }
    Ok(child)
}

/// Прямая ссылка на видеодорожку H.264 (без звука, до 720p): её открывает Media Foundation.
pub fn video_url(exe: &str, vid: &str) -> Result<String, String> {
    let url = format!("https://www.youtube.com/watch?v={vid}");
    let o = cmd(exe)
        .args(["-f", "bestvideo[vcodec^=avc1][height<=720][ext=mp4]/best[vcodec^=avc1][height<=720]/18", "--no-playlist", "-g", "--no-warnings", &url])
        .output()
        .map_err(|e| e.to_string())?;
    if !o.status.success() { return Err(String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("yt-dlp").to_string()); }
    String::from_utf8_lossy(&o.stdout).lines().next().map(|x| x.trim().to_string()).filter(|x| x.starts_with("http")).ok_or_else(|| "yt-dlp не вернул ссылку на видео".into())
}

/// Поиск на YouTube силами yt-dlp (ytsearch): без ключа API и без квоты 100 поисков в сутки.
pub fn search(exe: &str, q: &str, n: u32) -> Result<Vec<silo_core::model::Track>, String> {
    let o = cmd(exe)
        .args(["--flat-playlist", "-J", "--no-warnings", &format!("ytsearch{n}:{q}")])
        .output()
        .map_err(|e| format!("yt-dlp не запустился: {e}"))?;
    if !o.status.success() { return Err(String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("yt-dlp").to_string()); }
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).map_err(|e| e.to_string())?;
    Ok(v["entries"].as_array().cloned().unwrap_or_default().iter().filter_map(|e| {
        let id = e["id"].as_str()?.to_string();
        let ch = e["channel"].as_str().or(e["uploader"].as_str()).unwrap_or("").to_string();
        Some(yt_track(&id, e["title"].as_str().unwrap_or(""), &ch, e["duration"].as_f64().unwrap_or(0.0) as u32))
    }).collect())
}

/// Трек YouTube по id видео (обложка — миниатюра видео).
pub fn yt_track(id: &str, title: &str, channel: &str, dur: u32) -> silo_core::model::Track {
    let mut t = silo_core::model::Track::stub(&format!("youtube:{id}"), vec![]);
    let page = format!("https://www.youtube.com/watch?v={id}");
    t.source = "youtube".into();
    t.url = page.clone();
    t.license_url = page;
    t.name = title.to_string();
    t.artist = channel.trim_end_matches(" - Topic").to_string();
    t.image = format!("https://i.ytimg.com/vi/{id}/mqdefault.jpg");
    t.duration = dur;
    t
}

/// Запустить yt-dlp с аргументами и разобрать JSON-вывод (-J).
pub fn json(exe: &str, args: &[String]) -> Result<serde_json::Value, String> {
    let o = cmd(exe).args(["--no-warnings"]).args(args).output().map_err(|e| format!("yt-dlp не запустился: {e}"))?;
    if !o.status.success() { return Err(String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("yt-dlp").to_string()); }
    serde_json::from_slice(&o.stdout).map_err(|e| e.to_string())
}
