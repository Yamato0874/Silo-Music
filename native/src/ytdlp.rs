//! Режим «Библиотека yt-dlp»: поиск, звук и видео YouTube через yt-dlp (без ключа API и без квоты).
//! yt-dlp.exe кладётся рядом с программой (установщик может его включать) или указывается в настройках.
use std::collections::HashMap;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};

/// Папка, куда распаковывается встроенный yt-dlp (рядом с кэшем приложения).
fn bin_dir() -> PathBuf {
    std::env::var_os("SILO_LOCAL_DIR").map(PathBuf::from).unwrap_or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("app.silo.player.native")).join("bin")
}

/// Состояние скачивания yt-dlp: (идёт ли скачивание, сообщение, была ли попытка).
static DL: Mutex<(bool, String, bool)> = Mutex::new((false, String::new(), false));
pub fn download_state() -> (bool, String) { let g = DL.lock().unwrap(); (g.0, g.1.clone()) }
pub fn download_attempted() -> bool { DL.lock().unwrap().2 }

/// Скачать официальный yt-dlp.exe (github.com/yt-dlp/yt-dlp/releases) в папку данных приложения и сверить SHA-256 с официальным списком.
/// Запускается один раз при первом обращении к режиму yt-dlp, если yt-dlp не найден. Ничего не запускает и не распаковывает скрытно.
pub fn start_download(rt: &tokio::runtime::Handle) {
    {
        let mut g = DL.lock().unwrap();
        if g.0 || g.2 { return; }
        *g = (true, "Скачиваю yt-dlp…".into(), true);
    }
    rt.spawn(async {
        let r = async {
            use sha2::{Digest, Sha256};
            let base = "https://github.com/yt-dlp/yt-dlp/releases/latest/download";
            let c = silo_core::http();
            let sums = c.get(format!("{base}/SHA2-256SUMS")).send().await.map_err(|e| e.to_string())?.text().await.map_err(|e| e.to_string())?;
            let want = sums.lines().find(|l| l.trim_end().ends_with(" yt-dlp.exe")).and_then(|l| l.split_whitespace().next()).map(|s| s.to_lowercase()).ok_or("нет суммы в списке")?;
            let bytes = c.get(format!("{base}/yt-dlp.exe")).timeout(std::time::Duration::from_secs(600)).send().await.map_err(|e| e.to_string())?.bytes().await.map_err(|e| e.to_string())?;
            let got = format!("{:x}", Sha256::digest(&bytes));
            if got != want { return Err("контрольная сумма yt-dlp не совпала".to_string()); }
            let dir = bin_dir();
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let tmp = dir.join("yt-dlp.exe.part");
            std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
            std::fs::rename(&tmp, dir.join("yt-dlp.exe")).map_err(|e| e.to_string())?;
            Ok::<(), String>(())
        }.await;
        let mut g = DL.lock().unwrap();
        *g = match r { Ok(()) => (false, "готово".into(), true), Err(e) => (false, format!("не удалось скачать yt-dlp: {e}"), true) };
    });
}

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
static LAST_ERR: Mutex<String> = Mutex::new(String::new());
/// Браузер, из которого yt-dlp берёт cookies ("" = не брать). Нужен, когда YouTube просит подтвердить, что вы не бот.
static COOKIES: Mutex<String> = Mutex::new(String::new());
pub fn set_cookies_browser(b: &str) { *COOKIES.lock().unwrap() = b.trim().to_string(); }

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
    // скачанный или распакованный из exe yt-dlp в папке данных приложения
    let own = bin_dir().join("yt-dlp.exe");
    if own.is_file() { return Some(own); }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join("yt-dlp.exe")).find(|p| p.is_file())
}

fn cmd(exe: &str) -> Command {
    let mut c = Command::new(exe);
    c.creation_flags(CREATE_NO_WINDOW).stdin(Stdio::null());
    c
}

/// Знает ли эта версия yt-dlp флаг (проверяется один раз по --help, чтобы старая версия не падала от незнакомой опции).
fn knows(exe: &str, flag: &str) -> bool {
    static C: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();
    let m = C.get_or_init(Default::default);
    let key = format!("{exe}|{flag}");
    if let Some(v) = m.lock().unwrap().get(&key) { return *v; }
    let v = cmd(exe).arg("--help").output().map(|o| String::from_utf8_lossy(&o.stdout).contains(flag)).unwrap_or(false);
    m.lock().unwrap().insert(key, v);
    v
}

/// Node.js нужен новым версиям yt-dlp для разбора защиты YouTube (по умолчанию он включён только для Deno).
fn node_path() -> Option<String> {
    let p = std::env::var_os("PATH").and_then(|path| std::env::split_paths(&path).map(|d| d.join("node.exe")).find(|p| p.is_file()));
    p.or_else(|| ["C:\\Program Files\\nodejs\\node.exe"].iter().map(PathBuf::from).find(|p| p.is_file())).map(|p| p.to_string_lossy().to_string())
}

/// Команда yt-dlp с общими параметрами: тайм-ауты, повторы, среда выполнения JS, если она есть.
fn base(exe: &str) -> Command {
    let mut c = cmd(exe);
    c.args(["--no-warnings", "--socket-timeout", "20", "--retries", "3"]);
    { let b = COOKIES.lock().unwrap().clone(); if !b.is_empty() { c.args(["--cookies-from-browser", &b]); } }
    if knows(exe, "--js-runtimes") {
        if let Some(n) = node_path() { c.args(["--js-runtimes", &format!("node:{n}")]); }
    }
    c
}

fn last_line(b: &[u8]) -> String {
    let s = String::from_utf8_lossy(b);
    s.lines().filter(|l| l.contains("ERROR")).last().or_else(|| s.lines().last()).unwrap_or("yt-dlp").trim().to_string()
}

/// Версия yt-dlp (для проверки в настройках).
pub fn version(exe: &str) -> Result<String, String> {
    let o = cmd(exe).arg("--version").output().map_err(|e| e.to_string())?;
    if !o.status.success() { return Err(last_line(&o.stderr)); }
    Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// Обновить yt-dlp (YouTube часто меняется, и старая версия перестаёт работать). Запускается по кнопке в настройках.
pub fn update(exe: &str) -> Result<String, String> {
    let o = cmd(exe).arg("-U").output().map_err(|e| e.to_string())?;
    let text = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    if !o.status.success() { return Err(last_line(text.as_bytes())); }
    Ok(text.lines().filter(|l| !l.trim().is_empty()).last().unwrap_or("готово").trim().to_string())
}

/// Процесс, который пишет звуковую дорожку (m4a/AAC, её умеет наш декодер) в stdout.
pub fn audio_pipe(exe: &str, vid: &str) -> Result<Child, String> {
    let url = format!("https://www.youtube.com/watch?v={vid}");
    let mut c = base(exe);
    c.args(["-f", "bestaudio[ext=m4a]/140/bestaudio[acodec^=mp4a]", "--no-playlist", "--no-part", "--quiet", "-o", "-", &url])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = c.spawn().map_err(|e| format!("yt-dlp не запустился: {e}"))?;
    if let Some(mut err) = child.stderr.take() {
        std::thread::spawn(move || {
            use std::io::Read;
            let mut s = Vec::new();
            let _ = err.read_to_end(&mut s);
            let line = last_line(&s);
            if !s.is_empty() { *LAST_ERR.lock().unwrap() = line; }
        });
    }
    Ok(child)
}

/// Скачать звуковую дорожку в файл (для заранее подготовленных треков).
pub fn audio_file(exe: &str, vid: &str, out: &std::path::Path) -> Result<(), String> {
    let url = format!("https://www.youtube.com/watch?v={vid}");
    let o = base(exe)
        .args(["-f", "bestaudio[ext=m4a]/140/bestaudio[acodec^=mp4a]", "--no-playlist", "--no-part", "--force-overwrites", "--quiet", "-o"]).arg(out).arg(&url)
        .output().map_err(|e| format!("yt-dlp не запустился: {e}"))?;
    if !o.status.success() { let _ = std::fs::remove_file(out); return Err(last_line(&o.stderr)); }
    if std::fs::metadata(out).map(|m| m.len()).unwrap_or(0) < 10_000 { let _ = std::fs::remove_file(out); return Err("yt-dlp: файл пустой".into()); }
    Ok(())
}

/// Остановить yt-dlp вместе с дочерними процессами (Node/Deno).
/// 
pub fn kill_tree(pid: u32) {
    let _ = cmd("taskkill").args(["/F", "/T", "/PID", &pid.to_string()]).output();
}

/// Прямая ссылка на видеодорожку H.264 (без звука, до 720p): её открывает Media Foundation.
pub fn video_url(exe: &str, vid: &str) -> Result<String, String> {
    let url = format!("https://www.youtube.com/watch?v={vid}");
    let o = base(exe)
        .args(["-f", "bestvideo[vcodec^=avc1][height<=720][ext=mp4]/best[vcodec^=avc1][height<=720]/18", "--no-playlist", "-g", &url])
        .output()
        .map_err(|e| e.to_string())?;
    if !o.status.success() { return Err(last_line(&o.stderr)); }
    String::from_utf8_lossy(&o.stdout).lines().next().map(|x| x.trim().to_string()).filter(|x| x.starts_with("http")).ok_or_else(|| "yt-dlp не вернул ссылку на видео".into())
}

/// Запустить yt-dlp с аргументами и разобрать JSON-вывод (-J).
pub fn json(exe: &str, args: &[String]) -> Result<serde_json::Value, String> {
    let o = base(exe).args(args).output().map_err(|e| format!("yt-dlp не запустился: {e}"))?;
    if !o.status.success() { return Err(last_line(&o.stderr)); }
    serde_json::from_slice(&o.stdout).map_err(|e| format!("yt-dlp: непонятный ответ ({e})"))
}

/// Поиск на YouTube силами yt-dlp (ytsearch): без ключа API и без квоты 100 поисков в сутки.
pub fn search(exe: &str, q: &str, n: u32) -> Result<Vec<silo_core::model::Track>, String> {
    let v = json(exe, &["--flat-playlist".into(), "-J".into(), format!("ytsearch{n}:{q}")])?;
    Ok(tracks_of(&v))
}

/// Треки из ответа yt-dlp (поиск или плейлист в режиме --flat-playlist).
pub fn tracks_of(v: &serde_json::Value) -> Vec<silo_core::model::Track> {
    v["entries"].as_array().cloned().unwrap_or_default().iter().filter_map(|e| {
        let id = e["id"].as_str()?;
        if id.len() != 11 { return None; }
        let ch = e["channel"].as_str().or(e["uploader"].as_str()).unwrap_or("");
        Some(yt_track(id, e["title"].as_str().unwrap_or(""), ch, e["duration"].as_f64().unwrap_or(0.0) as u32))
    }).collect()
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

/// Процентное кодирование для адресов (кириллица, пробелы, &, #).
pub fn urlenc(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_flat_search() {
        let j = serde_json::json!({"_type":"playlist","entries":[
            {"_type":"url","id":"tLVVhfJqohU","title":"Группа крови","duration":284.0,"channel":"Kino - Topic","uploader":"Kino"},
            {"_type":"url","id":"UCxxxxxxxxxxxxxxxxxxxxxx","title":"канал"},
            {"_type":"url","id":"J6x7N_SDkyU","title":"Без длины","uploader":"X"}]});
        let t = tracks_of(&j);
        assert_eq!(t.len(), 2);
        assert_eq!((t[0].id.as_str(), t[0].artist.as_str(), t[0].duration), ("youtube:tLVVhfJqohU", "Kino", 284));
        assert_eq!(t[1].duration, 0);
    }
    #[test]
    fn encodes_query() {
        assert_eq!(urlenc("a b&c"), "a%20b%26c");
        assert_eq!(urlenc("кино"), "%D0%BA%D0%B8%D0%BD%D0%BE");
    }
}
