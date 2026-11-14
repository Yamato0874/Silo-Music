//! «Средний» апскейл: нейросеть Apollo («lossy → качество без потерь» для музыки; код и веса CC BY-SA 4.0).
//! Ничего из этого не лежит в установщике: по кнопке в Настройках программа сама ставит менеджер uv, Python 3.10, PyTorch,
//! код Apollo и веса в отдельную папку; удалить её можно одной кнопкой. Это постоянный режим: пока он включён,
//! следующие треки очереди заранее скачиваются, улучшаются и играют из кэша.
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{BufRead, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use eframe::egui;
use sha2::{Digest, Sha256};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const BELOW_NORMAL: u32 = 0x0000_4000;
const UV_URL: &str = "https://github.com/astral-sh/uv/releases/latest/download/uv-x86_64-pc-windows-msvc.zip";
const APOLLO_ZIP: &str = "https://github.com/JusperLee/Apollo/archive/refs/heads/main.zip";
const APOLLO_WEIGHTS: &str = "https://huggingface.co/JusperLee/Apollo/resolve/main/pytorch_model.bin";

#[derive(Clone, Debug, PartialEq)]
pub enum Install {
    Missing,
    Installing { step: String, detail: String, frac: Option<f32> },
    Ready,
    Failed(String),
}

pub struct EJob { pub key: String, pub orig: PathBuf, pub out: PathBuf, pub secs: f64 }

struct S {
    install: Install,
    queue: VecDeque<EJob>,
    wanted: HashSet<String>,
    running: Option<String>,
    done: u32,
    failed: HashMap<String, String>,
    cancel: bool,
    paused: bool,
    worker: bool,
}

#[derive(Clone)]
pub struct Enhancer {
    pub root: PathBuf,
    sh: Arc<(Mutex<S>, Condvar)>,
    rt: tokio::runtime::Handle,
    ctx: egui::Context,
    installing: Arc<AtomicBool>,
}

pub fn files_ready(root: &Path) -> bool {
    root.join("ready.json").is_file() && root.join("venv/Scripts/python.exe").is_file() && root.join("apollo/inference.py").is_file() && root.join("weights/pytorch_model.bin").is_file()
}

impl Enhancer {
    pub fn new(root: PathBuf, rt: tokio::runtime::Handle, ctx: egui::Context) -> Enhancer {
        let install = if files_ready(&root) { Install::Ready } else { Install::Missing };
        let s = S { install, queue: VecDeque::new(), wanted: HashSet::new(), running: None, done: 0, failed: HashMap::new(), cancel: false, paused: false, worker: false };
        Enhancer { root, sh: Arc::new((Mutex::new(s), Condvar::new())), rt, ctx, installing: Arc::new(AtomicBool::new(false)) }
    }
    /// Только для самопроверки интерфейса: подменить состояние.
    pub fn demo_state(&self, i: Install) { self.set(|s| s.install = i); }
    pub fn state(&self) -> Install { self.sh.0.lock().unwrap().install.clone() }
    pub fn ready(&self) -> bool { self.sh.0.lock().unwrap().install == Install::Ready }
    /// (в очереди, ключ в работе, готово за сеанс, пауза из-за полноэкранной программы)
    pub fn queue_info(&self) -> (usize, Option<String>, u32, bool) {
        let s = self.sh.0.lock().unwrap();
        (s.queue.len(), s.running.clone(), s.done, s.paused)
    }
    pub fn failed_count(&self) -> usize { self.sh.0.lock().unwrap().failed.len() }
    pub fn log_path(&self) -> PathBuf { self.root.join("install.log") }
    pub fn enhance_log_path(&self) -> PathBuf { self.root.join("enhance.log") }

    fn set(&self, f: impl FnOnce(&mut S)) { f(&mut self.sh.0.lock().unwrap()); self.ctx.request_repaint(); }
    fn step(&self, step: &str, detail: &str, frac: Option<f32>) {
        let (step, detail) = (step.to_string(), detail.to_string());
        self.set(|s| s.install = Install::Installing { step, detail, frac });
    }
    fn log(&self, line: &str) {
        let _ = std::fs::create_dir_all(&self.root);
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(self.log_path()) { let _ = writeln!(f, "{line}"); }
    }

    /// Поставить в очередь улучшения нужные сейчас треки (остальные из очереди выбрасываются).
    pub fn want(&self, jobs: Vec<EJob>) {
        let start = {
            let mut s = self.sh.0.lock().unwrap();
            if s.install != Install::Ready { return; }
            s.wanted = jobs.iter().map(|j| j.key.clone()).collect();
            let wanted = s.wanted.clone();
            s.queue.retain(|j| wanted.contains(&j.key));
            for j in jobs {
                if s.queue.iter().any(|q| q.key == j.key) || s.running.as_deref() == Some(&j.key) || s.failed.contains_key(&j.key) || j.out.is_file() { continue; }
                s.queue.push_back(j);
            }
            let start = !s.worker;
            s.worker = true;
            self.sh.1.notify_all();
            start
        };
        if start { let me = self.clone(); std::thread::Builder::new().name("enhance".into()).spawn(move || me.worker()).ok(); }
    }

    fn worker(&self) {
        loop {
            let job = {
                let (m, cv) = &*self.sh;
                let mut s = m.lock().unwrap();
                loop {
                    if let Some(j) = s.queue.pop_front() { s.running = Some(j.key.clone()); break j; }
                    s = cv.wait(s).unwrap();
                }
            };
            // во время полноэкранной игры или видео не нагружаем ни процессор, ни видеокарту
            while crate::platform::fullscreen_app_active() { self.set(|s| s.paused = true); std::thread::sleep(Duration::from_secs(3)); }
            self.set(|s| s.paused = false);
            if !self.sh.0.lock().unwrap().wanted.contains(&job.key) { self.set(|s| s.running = None); continue; }
            let res = self.process(&job);
            self.set(|s| {
                s.running = None;
                match &res { Ok(()) => s.done += 1, Err(e) => { s.failed.insert(job.key.clone(), e.clone()); } }
            });
            if let Err(e) = res { self.elog(&format!("{}: {e}", job.key)); }
        }
    }
    fn elog(&self, line: &str) {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(self.enhance_log_path()) { let _ = writeln!(f, "{line}"); }
    }

    fn process(&self, job: &EJob) -> Result<(), String> {
        if job.secs > 15.0 * 60.0 { return Err("трек длиннее 15 минут, пропускаю".into()); }
        let work = self.root.join("work");
        std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
        let (tin, tout) = (work.join(format!("{}.in.wav", job.key)), work.join(format!("{}.out.wav", job.key)));
        let cleanup = || { let _ = std::fs::remove_file(&tin); let _ = std::fs::remove_file(&tout); };
        let r = (|| -> Result<(), String> {
            render_wav(&job.orig, &tin, 44100)?;
            // сначала фрагменты по 6 с; если не хватило памяти видеокарты (4 ГБ), пробуем короче
            let mut res = self.run_apollo(&tin, &tout, 6.0, 1.0, job.secs);
            if let Err(e) = &res { if e.to_lowercase().contains("out of memory") { res = self.run_apollo(&tin, &tout, 3.0, 0.5, job.secs); } }
            res?;
            to_pcm16(&tout, &job.out)
        })();
        cleanup();
        r
    }

    fn python(&self) -> PathBuf { self.root.join("venv/Scripts/python.exe") }

    fn run_apollo(&self, inp: &Path, out: &Path, chunk: f32, overlap: f32, secs: f64) -> Result<(), String> {
        let mut c = Command::new(self.python());
        c.current_dir(self.root.join("apollo"))
            .args(["inference.py", "--in_wav"]).arg(inp).arg("--out_wav").arg(out)
            .arg("--checkpoint").arg(self.root.join("weights/pytorch_model.bin"))
            .args(["--device", "auto", "--chunk-seconds", &chunk.to_string(), "--overlap-seconds", &overlap.to_string(), "--chunk-pad-seconds", &overlap.to_string(), "--chunk-batch-size", "1"])
            .env("HF_HUB_OFFLINE", "1").env("PYTHONUTF8", "1").env("PYTHONIOENCODING", "utf-8").env("PYTHONUNBUFFERED", "1")
            .creation_flags(CREATE_NO_WINDOW | BELOW_NORMAL).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = c.spawn().map_err(|e| format!("Python не запустился: {e}"))?;
        let pid = child.id();
        let finished = Arc::new(AtomicBool::new(false));
        let limit = Duration::from_secs_f64((secs * 6.0).max(600.0));
        { let f = finished.clone(); std::thread::spawn(move || { let t0 = std::time::Instant::now(); while !f.load(Relaxed) { if t0.elapsed() > limit { crate::ytdlp::kill_tree(pid); break; } std::thread::sleep(Duration::from_millis(500)); } }); }
        // выводы читаем в потоках, чтобы процесс не завис на заполненном канале
        let tail = Arc::new(Mutex::new(String::new()));
        let mut handles = vec![];
        for s in [child.stdout.take().map(|x| Box::new(x) as Box<dyn std::io::Read + Send>), child.stderr.take().map(|x| Box::new(x) as Box<dyn std::io::Read + Send>)].into_iter().flatten() {
            let t = tail.clone();
            handles.push(std::thread::spawn(move || { for l in std::io::BufReader::new(s).lines().map_while(Result::ok) { let mut g = t.lock().unwrap(); g.push_str(&l); g.push('\n'); if g.len() > 6000 { let cut = g.len() - 4000; *g = g[cut..].to_string(); } } }));
        }
        let st = child.wait().map_err(|e| e.to_string());
        finished.store(true, Relaxed);
        for h in handles { let _ = h.join(); }
        let st = st?;
        let text = tail.lock().unwrap().clone();
        if !st.success() || !out.is_file() {
            let last = text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("Apollo завершился с ошибкой").to_string();
            self.elog(&text);
            return Err(if text.to_lowercase().contains("out of memory") { format!("out of memory: {last}") } else { last });
        }
        Ok(())
    }

    // ---------- установка по кнопке ----------

    pub fn cancel_install(&self) { self.set(|s| s.cancel = true); }

    pub fn uninstall(&self) -> Result<(), String> {
        if self.installing.load(Relaxed) { return Err("идёт установка".into()); }
        let r = std::fs::remove_dir_all(&self.root).map_err(|e| e.to_string());
        self.set(|s| { s.install = Install::Missing; s.queue.clear(); s.failed.clear(); });
        r
    }

    pub fn start_install(&self) {
        if self.installing.swap(true, Relaxed) { return; }
        self.set(|s| { s.cancel = false; s.install = Install::Installing { step: "Начинаю".into(), detail: String::new(), frac: None }; });
        let me = self.clone();
        std::thread::Builder::new().name("ai-install".into()).spawn(move || {
            me.log(&format!("--- установка {:?}", std::time::SystemTime::now()));
            let r = me.install_run();
            me.installing.store(false, Relaxed);
            match r {
                Ok(()) => { me.log("готово"); me.set(|s| s.install = Install::Ready); }
                Err(e) => {
                    me.log(&format!("ОШИБКА: {e}"));
                    // отмена или сбой: кэш скачанных колёс и недокачанные файлы не оставляем (место на диске)
                    let _ = std::fs::remove_dir_all(me.root.join("uvcache"));
                    let _ = std::fs::remove_file(me.root.join("weights/pytorch_model.bin.part"));
                    me.set(|s| s.install = Install::Failed(e));
                }
            }
        }).ok();
    }

    fn cancelled(&self) -> bool { self.sh.0.lock().unwrap().cancel }

    fn uv(&self) -> Command {
        let mut c = Command::new(self.root.join("tools/uv.exe"));
        c.env("UV_PYTHON_INSTALL_DIR", self.root.join("python")).env("UV_CACHE_DIR", self.root.join("uvcache"))
            .env("UV_INDEX_STRATEGY", "unsafe-best-match").env("UV_LINK_MODE", "hardlink").env("UV_PYTHON_PREFERENCE", "only-managed")
            .env("UV_NO_CONFIG", "1").env("UV_NO_PROGRESS", "1").creation_flags(CREATE_NO_WINDOW).stdin(Stdio::null());
        c
    }

    /// Запустить команду, писать вывод в журнал, раз в секунду показывать, сколько уже скачано/установлено.
    fn run_logged(&self, mut c: Command, step: &str, watch: &[PathBuf]) -> Result<String, String> {
        c.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = c.spawn().map_err(|e| format!("не запустилось: {e}"))?;
        let pid = child.id();
        let finished = Arc::new(AtomicBool::new(false));
        { // слежение за размером папок и отменой
            let (me, f, w, st) = (self.clone(), finished.clone(), watch.to_vec(), step.to_string());
            let expect_mb: u64 = if step.starts_with("3/6") { 5200 } else { 0 };
            std::thread::spawn(move || {
                let t0 = std::time::Instant::now();
                while !f.load(Relaxed) {
                    if me.cancelled() { crate::ytdlp::kill_tree(pid); break; }
                    // считаем только кэш uv: venv ссылается на те же файлы (жёсткие ссылки), их размер дважды считать нельзя
                    let bytes: u64 = w.iter().map(|p| dir_size(p)).sum();
                    let mb = bytes / 1_048_576;
                    let (detail, frac) = if expect_mb > 0 { (format!("распаковано {} из ≈ {} МБ, прошло {} мин", mb, expect_mb, t0.elapsed().as_secs() / 60), Some((mb as f32 / expect_mb as f32).min(0.99))) } else { (format!("{} МБ, прошло {} мин", mb, t0.elapsed().as_secs() / 60), None) };
                    me.step(&st, &detail, frac);
                    std::thread::sleep(Duration::from_millis(1000));
                }
            });
        }
        let text = Arc::new(Mutex::new(String::new()));
        let mut hs = vec![];
        for s in [child.stdout.take().map(|x| Box::new(x) as Box<dyn std::io::Read + Send>), child.stderr.take().map(|x| Box::new(x) as Box<dyn std::io::Read + Send>)].into_iter().flatten() {
            let (me, t) = (self.clone(), text.clone());
            hs.push(std::thread::spawn(move || { for l in std::io::BufReader::new(s).lines().map_while(Result::ok) { me.log(&l); let mut g = t.lock().unwrap(); g.push_str(&l); g.push('\n'); if g.len() > 20000 { let cut = g.len() - 10000; *g = g[cut..].to_string(); } } }));
        }
        let st = child.wait().map_err(|e| e.to_string());
        finished.store(true, Relaxed);
        for h in hs { let _ = h.join(); }
        let st = st?;
        let out = text.lock().unwrap().clone();
        if self.cancelled() { return Err("отменено".into()); }
        if !st.success() { return Err(out.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("команда завершилась с ошибкой").to_string()); }
        Ok(out)
    }

    fn download(&self, url: &str, dst: &Path, step: &str, expect_sha: Option<String>) -> Result<(), String> {
        self.rt.block_on(async {
            let r = silo_core::http().get(url).timeout(Duration::from_secs(3600)).send().await.map_err(|e| format!("{step}: {e}"))?;
            if !r.status().is_success() { return Err(format!("{step}: HTTP {}", r.status())); }
            let total = r.content_length();
            if let Some(p) = dst.parent() { let _ = std::fs::create_dir_all(p); }
            let tmp = dst.with_extension("part");
            let mut f = std::io::BufWriter::new(std::fs::File::create(&tmp).map_err(|e| e.to_string())?);
            let (mut h, mut got, mut last) = (Sha256::new(), 0u64, std::time::Instant::now());
            let mut r = r;
            while let Some(c) = r.chunk().await.map_err(|e| format!("{step}: {e}"))? {
                if self.cancelled() { drop(f); let _ = std::fs::remove_file(&tmp); return Err("отменено".into()); }
                h.update(&c); f.write_all(&c).map_err(|e| e.to_string())?; got += c.len() as u64;
                if last.elapsed() > Duration::from_millis(400) {
                    last = std::time::Instant::now();
                    self.step(step, &format!("{} МБ{}", got / 1_048_576, total.map(|t| format!(" из {}", t / 1_048_576)).unwrap_or_default()), total.map(|t| got as f32 / t as f32));
                }
            }
            f.flush().map_err(|e| e.to_string())?;
            drop(f);
            let sum = format!("{:x}", h.finalize());
            if let Some(want) = expect_sha {
                if !sum.eq_ignore_ascii_case(want.trim()) { let _ = std::fs::remove_file(&tmp); return Err(format!("{step}: контрольная сумма не совпала (получено {sum})")); }
                self.log(&format!("{step}: sha256 совпал"));
            }
            std::fs::rename(&tmp, dst).map_err(|e| e.to_string())
        })
    }

    /// Контрольная сумма файла Hugging Face из заголовка первого ответа (x-linked-etag).
    fn hf_sha(&self, url: &str) -> Option<String> {
        self.rt.block_on(async {
            let c = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).user_agent("SiloPlayer/1.1").timeout(Duration::from_secs(30)).build().ok()?;
            let r = c.get(url).send().await.ok()?;
            let v = r.headers().get("x-linked-etag")?.to_str().ok()?.trim_matches('"').trim_start_matches("W/").trim_matches('"').to_string();
            (v.len() == 64 && v.chars().all(|c| c.is_ascii_hexdigit())).then_some(v)
        })
    }

    fn install_run(&self) -> Result<(), String> {
        let root = &self.root;
        std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
        let ck = |s: &Self| if s.cancelled() { Err("отменено".to_string()) } else { Ok(()) };
        // 1. менеджер uv
        if !root.join("tools/uv.exe").is_file() {
            self.step("1/6 Менеджер Python (uv)", "", None);
            let zip = root.join("tools/uv.zip");
            let sha = self.rt.block_on(async { silo_core::http().get(format!("{UV_URL}.sha256")).send().await.ok()?.text().await.ok() }).and_then(|t| t.split_whitespace().next().map(String::from));
            self.download(UV_URL, &zip, "1/6 Менеджер Python (uv)", sha)?;
            unzip(&zip, &root.join("tools"))?;
            let _ = std::fs::remove_file(&zip);
            // внутри архива uv.exe лежит в корне или в подпапке
            if !root.join("tools/uv.exe").is_file() { if let Some(p) = find_file(&root.join("tools"), "uv.exe") { std::fs::copy(p, root.join("tools/uv.exe")).map_err(|e| e.to_string())?; } }
            if !root.join("tools/uv.exe").is_file() { return Err("в архиве uv нет uv.exe".into()); }
        }
        ck(self)?;
        // 2. Python 3.10
        if !self.python().is_file() {
            self.step("2/6 Python 3.10", "скачиваю и создаю окружение", None);
            let mut c = self.uv();
            c.args(["venv", "--python", "3.10"]).arg(root.join("venv"));
            self.run_logged(c, "2/6 Python 3.10", &[root.join("python")]).map_err(|e| format!("Python: {e}"))?;
        }
        ck(self)?;
        // 3. PyTorch (самая тяжёлая часть)
        let torch_ok = Command::new(self.python()).args(["-c", "import torch;print(torch.__version__)"]).creation_flags(CREATE_NO_WINDOW).output().map(|o| o.status.success()).unwrap_or(false);
        if !torch_ok {
            let mut c = self.uv();
            c.args(["pip", "install", "--python"]).arg(self.python())
                .args(["torch==2.0.0+cu118", "torchaudio==2.0.1+cu118", "--index-url", "https://download.pytorch.org/whl/cu118", "--extra-index-url", "https://pypi.org/simple"]);
            self.run_logged(c, "3/6 PyTorch (скачать ≈ 2,7 ГБ, на диске ≈ 5 ГБ, самый долгий шаг)", &[root.join("uvcache")]).map_err(|e| format!("PyTorch: {e}"))?;
        }
        ck(self)?;
        // 4. остальные пакеты
        self.step("4/6 Библиотеки для Apollo", "", None);
        let mut c = self.uv();
        c.args(["pip", "install", "--python"]).arg(self.python())
            .args(["numpy==1.26.3", "soundfile", "huggingface-hub==0.24.6", "omegaconf", "hydra-core", "pytorch-lightning==2.2.1", "librosa", "scipy", "scikit-learn", "numba", "h5py", "torchmetrics", "ptflops"]);
        self.run_logged(c, "4/6 Библиотеки для Apollo", &[root.join("uvcache")]).map_err(|e| format!("библиотеки: {e}"))?;
        ck(self)?;
        // 5. код и веса Apollo
        if !root.join("apollo/inference.py").is_file() {
            self.step("5/6 Код Apollo", "", None);
            let zip = root.join("apollo.zip");
            self.download(APOLLO_ZIP, &zip, "5/6 Код Apollo", None)?;
            let tmp = root.join("apollo_tmp");
            let _ = std::fs::remove_dir_all(&tmp);
            unzip(&zip, &tmp)?;
            let inner = std::fs::read_dir(&tmp).map_err(|e| e.to_string())?.flatten().map(|e| e.path()).find(|p| p.is_dir()).ok_or("пустой архив Apollo")?;
            let _ = std::fs::remove_dir_all(root.join("apollo"));
            std::fs::rename(inner, root.join("apollo")).map_err(|e| e.to_string())?;
            let _ = std::fs::remove_dir_all(&tmp);
            let _ = std::fs::remove_file(&zip);
            if !root.join("apollo/inference.py").is_file() { return Err("в архиве Apollo нет inference.py".into()); }
        }
        ck(self)?;
        if !root.join("weights/pytorch_model.bin").is_file() {
            let sha = self.hf_sha(APOLLO_WEIGHTS);
            if sha.is_none() { self.log("контрольная сумма весов недоступна, проверка размера"); }
            self.download(APOLLO_WEIGHTS, &root.join("weights/pytorch_model.bin"), "5/6 Веса Apollo (≈ 67 МБ)", sha)?;
            if std::fs::metadata(root.join("weights/pytorch_model.bin")).map(|m| m.len()).unwrap_or(0) < 10_000_000 { let _ = std::fs::remove_file(root.join("weights/pytorch_model.bin")); return Err("файл весов слишком мал".into()); }
        }
        ck(self)?;
        // 6. самопроверка на коротком сигнале; если не хватает какой-то библиотеки, доустанавливаем и повторяем
        self.step("6/6 Проверка", "тестовый запуск Apollo", None);
        let work = root.join("work");
        std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
        let (tin, tout) = (work.join("selfcheck.in.wav"), work.join("selfcheck.out.wav"));
        write_test_wav(&tin)?;
        let mut last_err = String::new();
        for attempt in 0..8 {
            ck(self)?;
            let mut c = Command::new(self.python());
            c.current_dir(root.join("apollo")).args(["-c", "import look2hear.models"]).env("PYTHONUTF8", "1").creation_flags(CREATE_NO_WINDOW).stdin(Stdio::null());
            let out = c.output().map_err(|e| e.to_string())?;
            let err = String::from_utf8_lossy(&out.stderr).to_string();
            if !out.status.success() {
                self.log(&err);
                if let Some(m) = missing_module(&err) {
                    self.step("6/6 Проверка", &format!("доустанавливаю {m} (попытка {})", attempt + 1), None);
                    let mut c = self.uv();
                    c.args(["pip", "install", "--python"]).arg(self.python()).arg(pip_name(&m));
                    if let Err(e) = self.run_logged(c, "6/6 Проверка", &[]) { last_err = e; continue; }
                    continue;
                }
                return Err(format!("look2hear не загружается: {}", err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("")));
            }
            match self.run_apollo(&tin, &tout, 6.0, 1.0, 3.0) {
                Ok(()) => { last_err.clear(); break; }
                Err(e) => { last_err = e.clone(); if let Some(m) = missing_module(&e) { let mut c = self.uv(); c.args(["pip", "install", "--python"]).arg(self.python()).arg(pip_name(&m)); let _ = self.run_logged(c, "6/6 Проверка", &[]); continue; } else { break; } }
            }
        }
        let _ = std::fs::remove_file(&tin);
        if !last_err.is_empty() || !tout.is_file() { return Err(format!("тестовый запуск Apollo не удался: {last_err}")); }
        let _ = std::fs::remove_file(&tout);
        // кэш скачанных колёс больше не нужен: освобождаем место
        let _ = std::fs::remove_dir_all(root.join("uvcache"));
        std::fs::write(root.join("ready.json"), format!("{{\"apollo\":\"main\",\"torch\":\"2.0.0+cu118\",\"installed\":{}}}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0))).map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn dir_size(p: &Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![p.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() { match e.metadata() { Ok(m) if m.is_dir() => stack.push(e.path()), Ok(m) => total += m.len(), _ => {} } }
    }
    total
}
fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if p.is_dir() { if let Some(f) = find_file(&p, name) { return Some(f); } } else if p.file_name().and_then(|n| n.to_str()) == Some(name) { return Some(p); }
    }
    None
}
fn unzip(zip: &Path, to: &Path) -> Result<(), String> {
    let f = std::fs::File::open(zip).map_err(|e| e.to_string())?;
    let mut a = zip::ZipArchive::new(f).map_err(|e| format!("архив повреждён: {e}"))?;
    a.extract(to).map_err(|e| format!("не распаковался: {e}"))
}

/// Имя модуля из «ModuleNotFoundError: No module named 'x.y'».
pub fn missing_module(text: &str) -> Option<String> {
    let i = text.find("No module named")?;
    let rest = &text[i + "No module named".len()..];
    let q = rest.find(['\'', '"'])?;
    let rest = &rest[q + 1..];
    let e = rest.find(['\'', '"'])?;
    Some(rest[..e].split('.').next()?.to_string())
}
/// Имя пакета pip по имени модуля.
pub fn pip_name(module: &str) -> String {
    match module { "sklearn" => "scikit-learn", "yaml" => "pyyaml", "cv2" => "opencv-python-headless", "PIL" => "pillow", "skimage" => "scikit-image", "hydra" => "hydra-core", m => return m.replace('_', "-") }.to_string()
}

/// Тестовый сигнал для проверки: 3 с, 44,1 кГц, стерео.
fn write_test_wav(path: &Path) -> Result<(), String> {
    let spec = hound::WavSpec { channels: 2, sample_rate: 44100, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut w = hound::WavWriter::create(path, spec).map_err(|e| e.to_string())?;
    for i in 0..44100 * 3 { let t = i as f32 / 44100.0; let v = ((t * 440.0 * std::f32::consts::TAU).sin() * 0.3 * 32767.0) as i16; w.write_sample(v).map_err(|e| e.to_string())?; w.write_sample(v).map_err(|e| e.to_string())?; }
    w.finalize().map_err(|e| e.to_string())
}

/// Декодировать любой файл (m4a, mp3, flac, ogg, wav) в стерео WAV (32-бит float) на заданной частоте: вход для Apollo.
pub fn render_wav(src: &Path, dst: &Path, rate: u32) -> Result<f64, String> {
    use symphonia::core::codecs::audio::AudioDecoderOptions;
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::formats::{FormatOptions, TrackType};
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    let file = std::fs::File::open(src).map_err(|e| format!("файл не открылся: {e}"))?;
    let mut hint = Hint::new();
    if let Some(e) = src.extension().and_then(|e| e.to_str()) { hint.with_extension(e); }
    let mut format = symphonia::default::get_probe().probe(&hint, MediaSourceStream::new(Box::new(file), Default::default()), FormatOptions::default(), MetadataOptions::default()).map_err(|e| format!("формат не поддерживается: {e}"))?;
    let track = format.default_track(TrackType::Audio).ok_or("нет звуковой дорожки")?;
    let tid = track.id;
    let params = track.codec_params.as_ref().and_then(|c| c.audio()).ok_or("нет параметров кодека")?.clone();
    let mut decoder = symphonia::default::get_codecs().make_audio_decoder(&params, &AudioDecoderOptions::default()).map_err(|e| format!("кодек не поддерживается: {e}"))?;
    let spec = hound::WavSpec { channels: 2, sample_rate: rate, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
    let mut w = hound::WavWriter::create(dst, spec).map_err(|e| e.to_string())?;
    let mut rs = crate::audio::Resampler::default();
    let (mut pcm, mut st, mut res) = (Vec::<f32>::new(), Vec::<f32>::new(), Vec::<f32>::new());
    let mut frames = 0u64;
    loop {
        let pkt = match format.next_packet() { Ok(Some(p)) => p, Ok(None) => break, Err(symphonia::core::errors::Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break, Err(e) => return Err(format!("ошибка чтения: {e}")) };
        if pkt.track_id != tid { continue; }
        let buf = match decoder.decode(&pkt) { Ok(b) => b, Err(symphonia::core::errors::Error::DecodeError(_)) => continue, Err(e) => return Err(format!("ошибка декодирования: {e}")) };
        let ch = buf.spec().channels().count().max(1);
        let rin = buf.spec().rate().max(1);
        pcm.resize(buf.samples_interleaved(), 0.0);
        buf.copy_to_slice_interleaved(&mut pcm[..]);
        st.clear();
        for fr in pcm.chunks(ch) { let l = fr[0]; st.push(l); st.push(if ch > 1 { fr[1] } else { l }); }
        res.clear();
        rs.process_sinc(&st, rin, rate, &mut res);
        for s in &res { w.write_sample(*s).map_err(|e| e.to_string())?; }
        frames += (res.len() / 2) as u64;
    }
    w.finalize().map_err(|e| e.to_string())?;
    Ok(frames as f64 / rate as f64)
}

/// Результат Apollo (float32) → 16-бит WAV в кэш (вдвое меньше места).
pub fn to_pcm16(src: &Path, dst: &Path) -> Result<(), String> {
    let mut r = hound::WavReader::open(src).map_err(|e| format!("результат не читается: {e}"))?;
    let spec = r.spec();
    let out_spec = hound::WavSpec { channels: spec.channels, sample_rate: spec.sample_rate, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let tmp = dst.with_extension("tmp.wav");
    let mut w = hound::WavWriter::create(&tmp, out_spec).map_err(|e| e.to_string())?;
    let clamp = |x: f32| (x.clamp(-1.0, 1.0) * 32767.0).round() as i16;
    match spec.sample_format {
        hound::SampleFormat::Float => for s in r.samples::<f32>() { w.write_sample(clamp(s.map_err(|e| e.to_string())?)).map_err(|e| e.to_string())?; },
        hound::SampleFormat::Int => { let scale = (1u64 << (spec.bits_per_sample - 1)) as f32; for s in r.samples::<i32>() { w.write_sample(clamp(s.map_err(|e| e.to_string())? as f32 / scale)).map_err(|e| e.to_string())?; } }
    }
    w.finalize().map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, dst).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn module_names() {
        assert_eq!(missing_module("ModuleNotFoundError: No module named 'sklearn'").as_deref(), Some("sklearn"));
        assert_eq!(missing_module("ModuleNotFoundError: No module named 'rotary_embedding_torch.x'").as_deref(), Some("rotary_embedding_torch"));
        assert_eq!(missing_module("ok"), None);
        assert_eq!(pip_name("sklearn"), "scikit-learn");
        assert_eq!(pip_name("pytorch_lightning"), "pytorch-lightning");
    }
    #[test]
    fn render_resamples_and_converts() {
        let d = std::env::temp_dir().join(format!("silo-enh-test-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        // 1 с, 48 кГц, моно-синус 1 кГц → 44,1 кГц стерео float
        let src = d.join("a.wav");
        let spec = hound::WavSpec { channels: 1, sample_rate: 48000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(&src, spec).unwrap();
        for i in 0..48000 { w.write_sample(((i as f32 / 48000.0 * 1000.0 * std::f32::consts::TAU).sin() * 20000.0) as i16).unwrap(); }
        w.finalize().unwrap();
        let mid = d.join("b.wav");
        let secs = render_wav(&src, &mid, 44100).unwrap();
        assert!((secs - 1.0).abs() < 0.05, "длина {secs}");
        let fin = d.join("c.wav");
        to_pcm16(&mid, &fin).unwrap();
        let r = hound::WavReader::open(&fin).unwrap();
        assert_eq!((r.spec().channels, r.spec().sample_rate, r.spec().bits_per_sample), (2, 44100, 16));
        // амплитуда сохранилась (~20000/32768)
        let peak = hound::WavReader::open(&fin).unwrap().samples::<i16>().map(|s| s.unwrap().abs()).max().unwrap();
        assert!(peak > 18000 && peak < 22000, "пик {peak}");
        let _ = std::fs::remove_dir_all(&d);
    }
}
