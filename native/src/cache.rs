//! Кэш звука на диске. Следующие два трека заранее скачиваются в файлы: старт мгновенный, перемотка и gapless работают,
//! а «Средний» апскейл берёт готовые файлы. Улучшенные версии лежат в подпапке enh. Кэш чистится по размеру.
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Ключ файла по id трека (FNV-1a, 64 бита).
pub fn key(id: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in id.bytes() { h ^= b as u64; h = h.wrapping_mul(0x100000001b3); }
    format!("{h:016x}")
}

#[derive(Default)]
struct St {
    files: HashMap<String, PathBuf>,
    inflight: HashSet<String>,
    /// ключ → момент (секунды с запуска часов), раньше которого не повторяем неудавшуюся загрузку
    retry_at: HashMap<String, f64>,
}

#[derive(Clone)]
pub struct Cache {
    pub dir: PathBuf,
    pub enh_dir: PathBuf,
    st: Arc<Mutex<St>>,
}

const EXTS: [&str; 8] = ["m4a", "mp3", "flac", "ogg", "opus", "wav", "aac", "webm"];

impl Cache {
    pub fn new(root: &Path) -> Cache {
        let dir = root.join("audio");
        let enh_dir = root.join("enh");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::create_dir_all(&enh_dir);
        let mut files = HashMap::new();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let p = e.path();
                let name = p.file_name().and_then(|x| x.to_str()).unwrap_or("").to_string();
                // недокачанное с прошлого запуска удаляем
                if name.contains(".dl") { let _ = std::fs::remove_file(&p); continue; }
                if let Some((k, ext)) = name.rsplit_once('.') { if EXTS.contains(&ext) { files.insert(k.to_string(), p); } }
            }
        }
        if let Ok(rd) = std::fs::read_dir(&enh_dir) { for e in rd.flatten() { if e.file_name().to_string_lossy().contains(".tmp") { let _ = std::fs::remove_file(e.path()); } } }
        Cache { dir, enh_dir, st: Arc::new(Mutex::new(St { files, ..Default::default() })) }
    }

    /// Скачанный оригинал трека, если есть.
    pub fn orig(&self, key: &str) -> Option<PathBuf> {
        let p = self.st.lock().unwrap().files.get(key).cloned()?;
        p.is_file().then_some(p)
    }
    pub fn enhanced_path(&self, key: &str) -> PathBuf { self.enh_dir.join(format!("{key}.wav")) }
    /// Улучшенная версия, если готова.
    pub fn enhanced(&self, key: &str) -> Option<PathBuf> {
        let p = self.enhanced_path(key);
        std::fs::metadata(&p).ok().filter(|m| m.len() > 1024).map(|_| p)
    }
    pub fn is_inflight(&self, key: &str) -> bool { self.st.lock().unwrap().inflight.contains(key) }
    /// Занять загрузку: false — уже идёт, или недавно не удалась.
    pub fn begin(&self, key: &str, now: f64) -> bool {
        let mut s = self.st.lock().unwrap();
        if s.inflight.contains(key) || s.retry_at.get(key).is_some_and(|t| *t > now) { return false; }
        s.inflight.insert(key.to_string());
        true
    }
    pub fn finish(&self, key: &str, path: Option<PathBuf>, now: f64) {
        let mut s = self.st.lock().unwrap();
        s.inflight.remove(key);
        match path { Some(p) => { s.files.insert(key.to_string(), p); s.retry_at.remove(key); } None => { s.retry_at.insert(key.to_string(), now + 60.0); } }
    }
    pub fn path_for(&self, key: &str, ext: &str) -> PathBuf { self.dir.join(format!("{key}.{ext}")) }
    pub fn tmp_for(&self, key: &str, ext: &str) -> PathBuf { self.dir.join(format!("{key}.dl.{ext}")) }

    /// Размеры: (оригиналы, улучшенные, число улучшенных).
    pub fn sizes(&self) -> (u64, u64, usize) {
        let sum = |d: &Path| -> (u64, usize) { std::fs::read_dir(d).map(|rd| rd.flatten().filter_map(|e| e.metadata().ok()).filter(|m| m.is_file()).fold((0, 0), |a, m| (a.0 + m.len(), a.1 + 1))).unwrap_or((0, 0)) };
        let (o, _) = sum(&self.dir);
        let (e, n) = sum(&self.enh_dir);
        (o, e, n)
    }

    /// Удалить самые старые файлы, пока папка не уложится в лимит; ключи из keep не трогаем.
    pub fn trim(&self, enhanced: bool, max_bytes: u64, keep: &HashSet<String>) {
        let dir = if enhanced { &self.enh_dir } else { &self.dir };
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut v: Vec<(std::time::SystemTime, u64, PathBuf)> = rd.flatten().filter_map(|e| { let m = e.metadata().ok()?; m.is_file().then(|| (m.modified().unwrap_or(std::time::UNIX_EPOCH), m.len(), e.path())) }).collect();
        let mut total: u64 = v.iter().map(|x| x.1).sum();
        v.sort_by_key(|x| x.0);
        for (_, len, p) in v {
            if total <= max_bytes { break; }
            let k = p.file_stem().and_then(|s| s.to_str()).unwrap_or("").split('.').next().unwrap_or("").to_string();
            if keep.contains(&k) { continue; }
            if std::fs::remove_file(&p).is_ok() { total = total.saturating_sub(len); self.st.lock().unwrap().files.remove(&k); }
        }
    }
    pub fn clear_enhanced(&self) { if let Ok(rd) = std::fs::read_dir(&self.enh_dir) { for e in rd.flatten() { let _ = std::fs::remove_file(e.path()); } } }
}

/// Расширение файла по адресу: известные аудио-форматы, иначе mp3 (Jamendo отдаёт mp3 без расширения в адресе).
pub fn ext_of_url(url: &str) -> &'static str {
    let low = url.to_lowercase();
    let path = low.split(['?', '#']).next().unwrap_or("");
    for e in EXTS { if path.ends_with(&format!(".{e}")) { return e; } }
    if low.contains("format=flac") { "flac" } else { "mp3" }
}

/// Скачать звук по прямому адресу в файл (через временный файл).
pub async fn download_http(url: &str, tmp: &Path, dst: &Path) -> Result<PathBuf, String> {
    use std::io::Write;
    let mut r = silo_core::http().get(url).timeout(std::time::Duration::from_secs(900)).send().await.map_err(|e| e.to_string())?;
    if !r.status().is_success() { return Err(format!("HTTP {}", r.status())); }
    let res: Result<(), String> = async {
        let mut f = std::io::BufWriter::new(std::fs::File::create(tmp).map_err(|e| e.to_string())?);
        while let Some(c) = r.chunk().await.map_err(|e| e.to_string())? { f.write_all(&c).map_err(|e| e.to_string())?; }
        f.flush().map_err(|e| e.to_string())
    }.await;
    if let Err(e) = res { let _ = std::fs::remove_file(tmp); return Err(e); }
    std::fs::rename(tmp, dst).map_err(|e| e.to_string())?;
    Ok(dst.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keys_are_stable_and_distinct() {
        assert_eq!(key("youtube:abc"), key("youtube:abc"));
        assert_ne!(key("youtube:abc"), key("youtube:abd"));
        assert_eq!(key("x").len(), 16);
    }
    #[test]
    fn url_extensions() {
        assert_eq!(ext_of_url("https://x/a.FLAC?x=1"), "flac");
        assert_eq!(ext_of_url("https://prod.storage.jamendo.com/?trackid=1&format=mp32"), "mp3");
        assert_eq!(ext_of_url("https://x/?format=flac"), "flac");
    }
    #[test]
    fn inflight_and_retry() {
        let d = std::env::temp_dir().join(format!("silo-cache-test-{}", std::process::id()));
        let c = Cache::new(&d);
        assert!(c.begin("k", 0.0));
        assert!(!c.begin("k", 0.0));
        c.finish("k", None, 10.0);
        assert!(!c.begin("k", 20.0)); // кулдаун 60 с
        assert!(c.begin("k", 80.0));
        let f = c.path_for("k", "mp3");
        std::fs::write(&f, b"data").unwrap();
        c.finish("k", Some(f.clone()), 90.0);
        assert_eq!(c.orig("k"), Some(f));
        let _ = std::fs::remove_dir_all(&d);
    }
    #[test]
    fn trim_removes_oldest_but_keeps_listed() {
        let d = std::env::temp_dir().join(format!("silo-cache-trim-{}", std::process::id()));
        let c = Cache::new(&d);
        for (i, k) in ["a", "b", "c"].iter().enumerate() {
            std::fs::write(c.path_for(k, "mp3"), vec![0u8; 100]).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(30 * (i as u64 + 1)));
        }
        let keep: HashSet<String> = ["a".to_string()].into_iter().collect();
        c.trim(false, 150, &keep);
        assert!(c.path_for("a", "mp3").exists()); // самый старый, но в списке «оставить»
        assert!(!c.path_for("b", "mp3").exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
