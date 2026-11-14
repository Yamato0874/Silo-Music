//! Собственный аудиодвижок: symphonia (декодирование) + cpal (вывод через WASAPI).
//! Две «деки» для gapless и кроссфейда, 10-полосный эквалайзер, выравнивание громкости (компрессор),
//! скорость без изменения высоты тона (WSOLA) и отвод сигнала для визуализатора.
//! Играет локальные файлы и прямые HTTP-потоки (Jamendo, превью Deezer, Internet Archive, ccMixter, SoundCloud API).
use std::collections::VecDeque;
use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering::*};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, SeekMode, SeekTo, TrackType};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::Time;

pub const EQ_FREQS: [f32; 10] = [32.0, 64.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0];

#[derive(Clone, Debug)]
pub enum Src {
    File(String),
    Url(String),
    /// поток YouTube через yt-dlp (id видео, путь к yt-dlp)
    Ytdlp(String, String),
}

#[derive(Debug, Clone)]
pub enum Event {
    /// трек доиграл до конца (token)
    Ended(u64),
    /// без паузы или через кроссфейд перешли на заранее загруженный следующий трек (token)
    Advanced(u64),
    /// не удалось открыть или декодировать (token, текст)
    Error(u64, String),
}

fn f32_load(a: &AtomicU32) -> f32 { f32::from_bits(a.load(Relaxed)) }
fn f32_store(a: &AtomicU32, v: f32) { a.store(v.to_bits(), Relaxed) }
fn f64_load(a: &AtomicU64) -> f64 { f64::from_bits(a.load(Relaxed)) }
fn f64_store(a: &AtomicU64, v: f64) { a.store(v.to_bits(), Relaxed) }

/// Одна дорожка: декодер в своём потоке наполняет буфер (стерео, частота устройства), колбэк вывода забирает.
pub struct Deck {
    pub token: u64,
    buf: Mutex<VecDeque<f32>>,
    eof: AtomicBool,
    cancel: AtomicBool,
    seek_req: Mutex<Option<f64>>,
    consumed: AtomicU64,
    base: AtomicU64,
    /// позиция до последней перемотки: к ней возвращаемся, если источник перемотку не позволил
    seek_from: AtomicU64,
    dur: AtomicU64,
    speed: AtomicU32,
    started: AtomicBool,
    ended_sent: AtomicBool,
}

impl Deck {
    fn new(token: u64, dur_hint: f64, speed: f32) -> Arc<Deck> {
        Arc::new(Deck {
            token,
            buf: Mutex::new(VecDeque::new()),
            eof: AtomicBool::new(false),
            cancel: AtomicBool::new(false),
            seek_req: Mutex::new(None),
            consumed: AtomicU64::new(0),
            base: AtomicU64::new(0f64.to_bits()),
            seek_from: AtomicU64::new(0f64.to_bits()),
            dur: AtomicU64::new(dur_hint.to_bits()),
            speed: AtomicU32::new(speed.to_bits()),
            started: AtomicBool::new(false),
            ended_sent: AtomicBool::new(false),
        })
    }
    fn pos(&self, rate: u32) -> f64 {
        f64_load(&self.base) + self.consumed.load(Relaxed) as f64 * f32_load(&self.speed) as f64 / rate.max(1) as f64
    }
    fn drained(&self) -> bool { self.eof.load(Relaxed) && self.buf.lock().map(|b| b.is_empty()).unwrap_or(false) }
}

#[derive(Clone, Default, PartialEq)]
pub struct Fx {
    pub eq_on: bool,
    pub eq: [f32; 10],
    pub normalize: bool,
    /// апскейл: 0 нет, 1 слабый (sinc-пересэмплер + эксайтер), 2 средний, 3 сильный (пока как слабый)
    pub upscale: u8,
}

struct Fade { len: u64, done: u64 }

struct Shared {
    decks: Mutex<[Option<Arc<Deck>>; 2]>,
    active: AtomicUsize,
    playing: AtomicBool,
    volume: AtomicU32,
    out_rate: AtomicU32,
    crossfade: AtomicU32,
    gapless: AtomicBool,
    fade: Mutex<Option<Fade>>,
    fx: Mutex<Fx>,
    fx_ver: AtomicU64,
    analyser: Mutex<VecDeque<f32>>,
    events: Sender<Event>,
    repaint: Mutex<Option<Box<dyn Fn() + Send>>>,
    device_lost: AtomicBool,
    /// качественный пересэмплер (апскейл)
    hq: AtomicBool,
}

/// Публичный «пульт» движка.
#[derive(Clone)]
pub struct Engine {
    sh: Arc<Shared>,
    rt: tokio::runtime::Handle,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct PState {
    pub playing: bool,
    pub pos: f64,
    pub dur: f64,
    pub token: u64,
}

impl Engine {
    pub fn start(rt: tokio::runtime::Handle) -> (Engine, Receiver<Event>) {
        let (tx, rx) = std::sync::mpsc::channel();
        let sh = Arc::new(Shared {
            decks: Mutex::new([None, None]),
            active: AtomicUsize::new(0),
            playing: AtomicBool::new(false),
            volume: AtomicU32::new(0.8f32.to_bits()),
            out_rate: AtomicU32::new(48000),
            crossfade: AtomicU32::new(0f32.to_bits()),
            gapless: AtomicBool::new(true),
            fade: Mutex::new(None),
            fx: Mutex::new(Fx::default()),
            fx_ver: AtomicU64::new(1),
            analyser: Mutex::new(VecDeque::with_capacity(2048)),
            events: tx,
            repaint: Mutex::new(None),
            device_lost: AtomicBool::new(false),
            hq: AtomicBool::new(false),
        });
        let s2 = sh.clone();
        // поток-владелец устройства вывода: при отключении наушников пересоздаёт поток на новом устройстве
        std::thread::Builder::new().name("audio-out".into()).spawn(move || loop {
            s2.device_lost.store(false, Relaxed);
            let stream = open_output(s2.clone());
            if stream.is_none() { std::thread::sleep(Duration::from_secs(2)); continue; }
            while !s2.device_lost.load(Relaxed) { std::thread::sleep(Duration::from_millis(300)); }
            drop(stream);
        }).ok();
        (Engine { sh, rt }, rx)
    }
    pub fn set_repaint(&self, f: Box<dyn Fn() + Send>) { *self.sh.repaint.lock().unwrap() = Some(f); }

    fn rate(&self) -> u32 { self.sh.out_rate.load(Relaxed) }

    /// Запустить трек сразу (или поставить на паузу на позиции `start_at`, если autoplay = false).
    pub fn play(&self, token: u64, src: Src, dur_hint: f64, start_at: f64, autoplay: bool, speed: f32) {
        let mut d = self.sh.decks.lock().unwrap();
        *self.sh.fade.lock().unwrap() = None;
        let a = self.sh.active.load(Relaxed);
        // уже загружен заранее (gapless/кроссфейд) — просто переключаемся
        if let Some(n) = d[1 - a].clone().filter(|n| n.token == token) {
            if let Some(o) = d[a].take() { o.cancel.store(true, Relaxed); }
            self.sh.active.store(1 - a, Relaxed);
            if start_at > 0.0 { *n.seek_req.lock().unwrap() = Some(start_at); }
            drop(d);
            self.sh.playing.store(autoplay, Relaxed);
            return;
        }
        for x in d.iter_mut() { if let Some(o) = x.take() { o.cancel.store(true, Relaxed); } }
        let deck = Deck::new(token, dur_hint, speed);
        if start_at > 0.0 { *deck.seek_req.lock().unwrap() = Some(start_at); f64_store(&deck.base, start_at); }
        d[a] = Some(deck.clone());
        drop(d);
        self.sh.playing.store(autoplay, Relaxed);
        spawn_decoder(self.sh.clone(), deck, src, self.rt.clone());
    }
    /// Заранее открыть следующий трек (для gapless и кроссфейда).
    pub fn preload(&self, token: u64, src: Src, dur_hint: f64, speed: f32) {
        let mut d = self.sh.decks.lock().unwrap();
        let a = self.sh.active.load(Relaxed);
        if d[1 - a].as_ref().is_some_and(|n| n.token == token) { return; }
        if self.sh.fade.lock().unwrap().is_some() { return; }
        if let Some(o) = d[1 - a].take() { o.cancel.store(true, Relaxed); }
        let deck = Deck::new(token, dur_hint, speed);
        d[1 - a] = Some(deck.clone());
        drop(d);
        spawn_decoder(self.sh.clone(), deck, src, self.rt.clone());
    }
    pub fn cancel_preload(&self) {
        let mut d = self.sh.decks.lock().unwrap();
        let a = self.sh.active.load(Relaxed);
        if self.sh.fade.lock().unwrap().is_some() { return; }
        if let Some(o) = d[1 - a].take() { o.cancel.store(true, Relaxed); }
    }
    pub fn stop(&self) {
        let mut d = self.sh.decks.lock().unwrap();
        for x in d.iter_mut() { if let Some(o) = x.take() { o.cancel.store(true, Relaxed); } }
        *self.sh.fade.lock().unwrap() = None;
        self.sh.playing.store(false, Relaxed);
    }
    pub fn set_playing(&self, p: bool) { self.sh.playing.store(p, Relaxed); }
    pub fn toggle(&self) { self.sh.playing.fetch_xor(true, Relaxed); }
    pub fn playing(&self) -> bool { self.sh.playing.load(Relaxed) }
    pub fn seek(&self, sec: f64) {
        let d = self.sh.decks.lock().unwrap();
        if let Some(x) = &d[self.sh.active.load(Relaxed)] {
            let s = sec.max(0.0);
            f64_store(&x.seek_from, x.pos(self.rate()));
            *x.seek_req.lock().unwrap() = Some(s);
            // позиция сразу на новом месте, пока декодер перематывает
            f64_store(&x.base, s);
            x.consumed.store(0, Relaxed);
            x.ended_sent.store(false, Relaxed);
        }
    }
    pub fn set_volume(&self, v: f32) { f32_store(&self.sh.volume, v.clamp(0.0, 1.0)); }
    pub fn set_crossfade(&self, sec: f32, gapless: bool) { f32_store(&self.sh.crossfade, sec); self.sh.gapless.store(gapless, Relaxed); }
    pub fn set_fx(&self, fx: Fx) {
        let mut f = self.sh.fx.lock().unwrap();
        if *f != fx { *f = fx; self.sh.fx_ver.fetch_add(1, Relaxed); }
    }
    /// Скорость 0.5–2×: тон сохраняется. Буфер перестраивается с текущей позиции.
    pub fn set_speed(&self, speed: f32) {
        let r = self.rate();
        let d = self.sh.decks.lock().unwrap();
        for x in d.iter().flatten() {
            if (f32_load(&x.speed) - speed).abs() < 1e-3 { continue; }
            let p = x.pos(r);
            f32_store(&x.speed, speed);
            *x.seek_req.lock().unwrap() = Some(p);
            f64_store(&x.base, p);
            x.consumed.store(0, Relaxed);
        }
    }
    pub fn state(&self) -> PState {
        let d = self.sh.decks.lock().unwrap();
        let r = self.rate();
        match &d[self.sh.active.load(Relaxed)] {
            Some(x) => {
                let dur = f64_load(&x.dur);
                let pos = x.pos(r);
                PState { playing: self.sh.playing.load(Relaxed), pos: if dur > 0.0 { pos.min(dur) } else { pos }, dur, token: x.token }
            }
            None => PState::default(),
        }
    }
    /// Последние ~1024 моно-сэмпла после эффектов (для визуализатора).
    pub fn analyser(&self, out: &mut Vec<f32>) {
        out.clear();
        if let Ok(a) = self.sh.analyser.lock() { out.extend(a.iter()); }
    }
    pub fn sample_rate(&self) -> u32 { self.rate() }
}

// ---------- вывод ----------

fn open_output(sh: Arc<Shared>) -> Option<cpal::Stream> {
    let host = cpal::default_host();
    let dev = host.default_output_device()?;
    let cfg = dev.default_output_config().ok()?;
    let fmt = cfg.sample_format();
    let config: cpal::StreamConfig = cfg.into();
    sh.out_rate.store(config.sample_rate, Relaxed);
    let ch = config.channels as usize;
    let s = match fmt {
        cpal::SampleFormat::F32 => build::<f32>(&dev, config, ch, sh),
        cpal::SampleFormat::I16 => build::<i16>(&dev, config, ch, sh),
        cpal::SampleFormat::I32 => build::<i32>(&dev, config, ch, sh),
        cpal::SampleFormat::U16 => build::<u16>(&dev, config, ch, sh),
        _ => None,
    }?;
    s.play().ok()?;
    Some(s)
}

fn build<T: cpal::SizedSample + cpal::FromSample<f32>>(dev: &cpal::Device, config: cpal::StreamConfig, ch: usize, sh: Arc<Shared>) -> Option<cpal::Stream> {
    let mut mix = Mixer::new(sh.clone());
    let sh2 = sh.clone();
    dev.build_output_stream(
        config,
        move |data: &mut [T], _| {
            let frames = data.len() / ch.max(1);
            mix.render(frames);
            for (i, fr) in data.chunks_mut(ch).enumerate() {
                let (l, r) = (mix.out[i * 2], mix.out[i * 2 + 1]);
                for (c, s) in fr.iter_mut().enumerate() {
                    *s = T::from_sample(match c { 0 => l, 1 => r, _ => 0.0 });
                }
            }
        },
        move |_e| { sh2.device_lost.store(true, Relaxed); },
        None,
    ).ok()
}

/// Микшер внутри колбэка вывода: две деки, кроссфейд, эквалайзер, компрессор, громкость с плавным нарастанием.
struct Mixer {
    sh: Arc<Shared>,
    out: Vec<f32>,
    tmp: Vec<f32>,
    eq: [[Biquad; 2]; 10],
    fx: Fx,
    fx_ver: u64,
    comp_env: f32,
    gain: f32,
    /// эксайтер: полоса выше ~4,5 кГц → мягкое насыщение → снова фильтр (только новые гармоники)
    exc: [[Biquad; 2]; 2],
}

impl Mixer {
    fn new(sh: Arc<Shared>) -> Mixer {
        Mixer { sh, out: vec![], tmp: vec![], eq: [[Biquad::default(); 2]; 10], fx: Fx::default(), fx_ver: 0, comp_env: 0.0, gain: 0.0, exc: [[Biquad::default(); 2]; 2] }
    }
    fn pull(deck: &Deck, dst: &mut [f32]) -> usize {
        let Ok(mut b) = deck.buf.try_lock() else { return 0 };
        let n = dst.len().min(b.len()) & !1;
        for (i, x) in b.drain(..n).enumerate() { dst[i] = x; }
        n
    }
    fn render(&mut self, frames: usize) {
        let n = frames * 2;
        self.out.clear();
        self.out.resize(n, 0.0);
        self.tmp.resize(n, 0.0);
        let rate = self.sh.out_rate.load(Relaxed);
        let playing = self.sh.playing.load(Relaxed);
        let target = if playing { 1.0 } else { 0.0 };
        // на паузе после затухания ничего не забираем из буфера: позиция стоит
        if !playing && self.gain <= 0.0001 { self.gain = 0.0; self.tap(); return; }
        let Ok(decks) = self.sh.decks.try_lock() else { return };
        let a = self.sh.active.load(Relaxed);
        let (cur, nxt) = (decks[a].clone(), decks[1 - a].clone());
        drop(decks);
        let mut switched: Option<u64> = None;
        if let Some(c) = &cur {
            let got = Self::pull(c, &mut self.out[..n]);
            c.consumed.fetch_add((got / 2) as u64, Relaxed);
            if got > 0 { c.started.store(true, Relaxed); }
            // кроссфейд
            let cf = f32_load(&self.sh.crossfade) as f64;
            let dur = f64_load(&c.dur);
            if let Some(nx) = &nxt {
                let mut fade = self.sh.fade.lock().unwrap();
                if fade.is_none() && cf > 0.0 && dur > 0.0 && playing {
                    let left = dur - c.pos(rate);
                    if left <= cf && left > 0.3 { *fade = Some(Fade { len: (left * rate as f64) as u64, done: 0 }); }
                }
                if let Some(f) = fade.as_mut() {
                    let got2 = Self::pull(nx, &mut self.tmp[..n]);
                    nx.consumed.fetch_add((got2 / 2) as u64, Relaxed);
                    for i in 0..n / 2 {
                        let p = ((f.done + i as u64) as f32 / f.len.max(1) as f32).min(1.0);
                        let (go, gi) = ((1.0 - p).sqrt(), p.sqrt());
                        for k in 0..2 { let j = i * 2 + k; self.out[j] = self.out[j] * go + if j < got2 { self.tmp[j] * gi } else { 0.0 }; }
                    }
                    f.done += (n / 2) as u64;
                    if f.done >= f.len || c.drained() {
                        *fade = None;
                        c.cancel.store(true, Relaxed);
                        self.sh.active.store(1 - a, Relaxed);
                        if let Ok(mut d) = self.sh.decks.try_lock() { d[a] = None; }
                        switched = Some(nx.token);
                    }
                }
            }
            if switched.is_none() && got < n && c.drained() {
                // gapless: следующий уже загружен — продолжаем без паузы
                let gap = self.sh.gapless.load(Relaxed) || cf > 0.0;
                match &nxt {
                    Some(nx) if gap && (nx.buf.lock().map(|b| !b.is_empty()).unwrap_or(false) || nx.eof.load(Relaxed)) => {
                        let got2 = Self::pull(nx, &mut self.out[got..n]);
                        nx.consumed.fetch_add((got2 / 2) as u64, Relaxed);
                        self.sh.active.store(1 - a, Relaxed);
                        if let Ok(mut d) = self.sh.decks.try_lock() { d[a] = None; }
                        switched = Some(nx.token);
                    }
                    _ => {
                        if !c.ended_sent.swap(true, Relaxed) { let _ = self.sh.events.send(Event::Ended(c.token)); self.ping(); }
                    }
                }
            }
        }
        if let Some(t) = switched { let _ = self.sh.events.send(Event::Advanced(t)); self.ping(); }
        // эффекты
        let ver = self.sh.fx_ver.load(Relaxed);
        if ver != self.fx_ver {
            if let Ok(f) = self.sh.fx.try_lock() {
                self.fx = f.clone();
                self.fx_ver = ver;
                for (i, fq) in EQ_FREQS.iter().enumerate() {
                    let kind = if i == 0 { 0 } else if i == 9 { 2 } else { 1 };
                    let g = if self.fx.eq_on { self.fx.eq[i] } else { 0.0 };
                    for c in 0..2 { self.eq[i][c].set(kind, *fq, g, 1.1, rate as f32); }
                }
                for c in 0..2 { self.exc[0][c].set(3, 4500.0, 0.0, 0.707, rate as f32); self.exc[1][c].set(3, 6000.0, 0.0, 0.707, rate as f32); }
                self.sh.hq.store(self.fx.upscale > 0, Relaxed);
            }
        }
        let eq_on = self.fx.eq_on && self.fx.eq.iter().any(|g| g.abs() > 0.01);
        let vol = f32_load(&self.sh.volume);
        let vol = vol * vol; // ощущаемая громкость ближе к линейной шкале ползунка
        // компрессор «выравнивания»: порог -26 дБ, 4:1, мягкое колено; атака 3 мс, спад 250 мс
        let (att, rel) = ((-1.0 / (0.003 * rate as f32)).exp(), (-1.0 / (0.25 * rate as f32)).exp());
        for i in 0..n / 2 {
            let (mut l, mut r) = (self.out[i * 2], self.out[i * 2 + 1]);
            if eq_on {
                for b in self.eq.iter_mut() { l = b[0].run(l); r = b[1].run(r); }
            }
            if self.fx.upscale > 0 {
                // мягкий эксайтер: добавляет верхние гармоники, которые срезает сжатие mp3/aac
                let amt = match self.fx.upscale { 1 => 0.10, 2 => 0.14, _ => 0.18 };
                let (hl, hr) = (self.exc[0][0].run(l), self.exc[0][1].run(r));
                let (sl, sr) = ((hl * 4.0).tanh() * 0.25, (hr * 4.0).tanh() * 0.25);
                l += self.exc[1][0].run(sl) * amt * 4.0;
                r += self.exc[1][1].run(sr) * amt * 4.0;
            }
            if self.fx.normalize {
                let lv = l.abs().max(r.abs());
                let k = if lv > self.comp_env { att } else { rel };
                self.comp_env = k * self.comp_env + (1.0 - k) * lv;
                let db = 20.0 * self.comp_env.max(1e-6).log10();
                let over = db + 26.0;
                let red = if over <= -12.0 { 0.0 } else if over < 12.0 { (1.0 - 1.0 / 4.0) * (over + 12.0).powi(2) / 48.0 } else { (1.0 - 1.0 / 4.0) * over };
                let g = 10f32.powf((-red + 8.0) / 20.0); // + компенсация громкости
                l *= g; r *= g;
            }
            self.gain += (target - self.gain) * 0.004; // ~5 мс на нарастание/затухание
            let g = self.gain * vol;
            self.out[i * 2] = (l * g).clamp(-1.0, 1.0);
            self.out[i * 2 + 1] = (r * g).clamp(-1.0, 1.0);
        }
        self.tap();
    }
    fn tap(&mut self) {
        if let Ok(mut a) = self.sh.analyser.try_lock() {
            for fr in self.out.chunks(2) { a.push_back((fr[0] + fr[1]) * 0.5); }
            while a.len() > 1024 { a.pop_front(); }
        }
    }
    fn ping(&self) { if let Ok(r) = self.sh.repaint.try_lock() { if let Some(f) = r.as_ref() { f(); } } }
}

/// Биквадратный фильтр (RBJ): полка снизу, пики, полка сверху — как BiquadFilterNode в Web Audio.
#[derive(Clone, Copy, Default)]
struct Biquad { b0: f32, b1: f32, b2: f32, a1: f32, a2: f32, z1: f32, z2: f32 }
impl Biquad {
    fn set(&mut self, kind: u8, f: f32, gain_db: f32, q: f32, fs: f32) {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * (f / fs).min(0.49);
        let (cw, sw) = (w0.cos(), w0.sin());
        let (b0, b1, b2, a0, a1, a2);
        match kind {
            0 | 2 => {
                // полки с S = 1
                let alpha = sw / 2.0 * ((a + 1.0 / a) * (1.0 / 1.0 - 1.0) + 2.0).sqrt();
                let sa = 2.0 * a.sqrt() * alpha;
                if kind == 0 {
                    b0 = a * ((a + 1.0) - (a - 1.0) * cw + sa); b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cw); b2 = a * ((a + 1.0) - (a - 1.0) * cw - sa);
                    a0 = (a + 1.0) + (a - 1.0) * cw + sa; a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cw); a2 = (a + 1.0) + (a - 1.0) * cw - sa;
                } else {
                    b0 = a * ((a + 1.0) + (a - 1.0) * cw + sa); b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cw); b2 = a * ((a + 1.0) + (a - 1.0) * cw - sa);
                    a0 = (a + 1.0) - (a - 1.0) * cw + sa; a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cw); a2 = (a + 1.0) - (a - 1.0) * cw - sa;
                }
            }
            3 => {
                // фильтр верхних частот
                let alpha = sw / (2.0 * q);
                b0 = (1.0 + cw) / 2.0; b1 = -(1.0 + cw); b2 = (1.0 + cw) / 2.0;
                a0 = 1.0 + alpha; a1 = -2.0 * cw; a2 = 1.0 - alpha;
            }
            _ => {
                let alpha = sw / (2.0 * q);
                b0 = 1.0 + alpha * a; b1 = -2.0 * cw; b2 = 1.0 - alpha * a;
                a0 = 1.0 + alpha / a; a1 = -2.0 * cw; a2 = 1.0 - alpha / a;
            }
        }
        self.b0 = b0 / a0; self.b1 = b1 / a0; self.b2 = b2 / a0; self.a1 = a1 / a0; self.a2 = a2 / a0;
    }
    #[inline]
    fn run(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

// ---------- источник по HTTP: качаем последовательно в память, чтение ждёт докачки ----------

struct HttpBuf { data: Vec<u8>, done: bool, err: Option<String>, len: Option<u64> }
struct HttpSource { st: Arc<(Mutex<HttpBuf>, Condvar)>, pos: u64, cancel: Arc<AtomicBool> }

impl HttpSource {
    /// Звук YouTube: yt-dlp пишет файл m4a (AAC) в stdout, мы читаем его как обычный поток.
    fn open_ytdlp(vid: &str, exe: &str, cancel: Arc<AtomicBool>) -> Result<HttpSource, String> {
        let mut child = crate::ytdlp::audio_pipe(exe, vid)?;
        let mut out = child.stdout.take().ok_or("yt-dlp: нет вывода")?;
        let st = Arc::new((Mutex::new(HttpBuf { data: vec![], done: false, err: None, len: None }), Condvar::new()));
        let (st2, c2) = (st.clone(), cancel.clone());
        // трек сменили или закрыли: останавливаем yt-dlp вместе с Node, даже если он ждёт сеть и ничего не пишет
        let finished = Arc::new(AtomicBool::new(false));
        { let (pid, c3, f3) = (child.id(), cancel.clone(), finished.clone());
          std::thread::spawn(move || { while !f3.load(Relaxed) { if c3.load(Relaxed) { crate::ytdlp::kill_tree(pid); break; } std::thread::sleep(Duration::from_millis(100)); } }); }
        std::thread::spawn(move || {
            let mut chunk = vec![0u8; 64 * 1024];
            loop {
                if c2.load(Relaxed) { break; }
                match out.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => { let mut b = st2.0.lock().unwrap(); b.data.extend_from_slice(&chunk[..n]); st2.1.notify_all(); }
                    Err(e) => { st2.0.lock().unwrap().err = Some(e.to_string()); break; }
                }
            }
            let ok = child.wait().map(|s| s.success()).unwrap_or(false);
            finished.store(true, Relaxed);
            let mut b = st2.0.lock().unwrap();
            if !ok && b.data.is_empty() { b.err = Some(crate::ytdlp::last_error()); }
            b.done = true;
            b.len = Some(b.data.len() as u64);
            st2.1.notify_all();
        });
        // m4a нужно дочитать целиком перед разбором? нет: у звука YouTube заголовок moov в начале
        let (m, cv) = &*st;
        let mut b = m.lock().unwrap();
        while b.data.len() < 64 * 1024 && !b.done {
            let (nb, to) = cv.wait_timeout(b, Duration::from_secs(40)).unwrap();
            b = nb;
            if to.timed_out() { return Err("yt-dlp не отвечает".into()); }
        }
        if b.data.is_empty() { return Err(b.err.clone().unwrap_or_else(|| "yt-dlp: пустой ответ".into())); }
        drop(b);
        Ok(HttpSource { st, pos: 0, cancel })
    }
    fn open(url: &str, rt: &tokio::runtime::Handle, cancel: Arc<AtomicBool>) -> Result<HttpSource, String> {
        let st = Arc::new((Mutex::new(HttpBuf { data: vec![], done: false, err: None, len: None }), Condvar::new()));
        let (st2, url2, c2) = (st.clone(), url.to_string(), cancel.clone());
        rt.spawn(async move {
            let res = async {
                let mut r = silo_core::http().get(&url2).timeout(Duration::from_secs(600)).send().await.map_err(|e| e.to_string())?;
                if !r.status().is_success() { return Err(format!("HTTP {}", r.status())); }
                { let mut b = st2.0.lock().unwrap(); b.len = r.content_length(); if let Some(l) = b.len { b.data.reserve(l as usize); } }
                while let Some(ch) = r.chunk().await.map_err(|e| e.to_string())? {
                    if c2.load(Relaxed) { return Ok(()); }
                    let mut b = st2.0.lock().unwrap();
                    b.data.extend_from_slice(&ch);
                    st2.1.notify_all();
                }
                Ok(())
            }.await;
            let mut b = st2.0.lock().unwrap();
            b.done = true;
            if let Err(e) = res { b.err = Some(e); }
            st2.1.notify_all();
        });
        // ждём заголовки/первые байты, чтобы сразу отдать ошибку сети
        let (m, cv) = &*st;
        let mut b = m.lock().unwrap();
        while b.data.len() < 16 * 1024 && !b.done {
            let (nb, to) = cv.wait_timeout(b, Duration::from_secs(20)).unwrap();
            b = nb;
            if to.timed_out() { return Err("сеть не отвечает".into()); }
        }
        if b.data.is_empty() { if let Some(e) = &b.err { return Err(e.clone()); } }
        drop(b);
        Ok(HttpSource { st, pos: 0, cancel })
    }
}
impl Read for HttpSource {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let (m, cv) = &*self.st;
        let mut b = m.lock().unwrap();
        loop {
            let have = b.data.len() as u64;
            if self.pos < have {
                let n = out.len().min((have - self.pos) as usize);
                out[..n].copy_from_slice(&b.data[self.pos as usize..self.pos as usize + n]);
                self.pos += n as u64;
                return Ok(n);
            }
            if b.done || self.cancel.load(Relaxed) { return Ok(0); }
            b = cv.wait_timeout(b, Duration::from_millis(200)).unwrap().0;
        }
    }
}
impl Seek for HttpSource {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        let (len, have) = { let b = self.st.0.lock().unwrap(); (b.len.unwrap_or(b.data.len() as u64), b.data.len() as u64) };
        let p = match to {
            SeekFrom::Start(x) => x as i64,
            SeekFrom::Current(x) => self.pos as i64 + x,
            SeekFrom::End(x) => len.max(have) as i64 + x,
        };
        self.pos = p.max(0) as u64;
        Ok(self.pos)
    }
}
impl MediaSource for HttpSource {
    fn is_seekable(&self) -> bool { self.st.0.lock().unwrap().len.is_some() }
    fn byte_len(&self) -> Option<u64> { self.st.0.lock().unwrap().len }
}

// ---------- декодер ----------

fn ext_of(s: &str) -> Option<String> {
    let path = s.split(['?', '#']).next().unwrap_or("");
    let e = path.rsplit('.').next()?.to_lowercase();
    (e.len() <= 4 && e != path.to_lowercase()).then_some(e)
}

fn spawn_decoder(sh: Arc<Shared>, deck: Arc<Deck>, src: Src, rt: tokio::runtime::Handle) {
    std::thread::Builder::new().name("decoder".into()).spawn(move || {
        if let Err(e) = decode(&sh, &deck, src, &rt) {
            if !deck.cancel.load(Relaxed) {
                deck.eof.store(true, Relaxed);
                let _ = sh.events.send(Event::Error(deck.token, e));
                if let Ok(r) = sh.repaint.lock() { if let Some(f) = r.as_ref() { f(); } }
            }
        }
    }).ok();
}

fn decode(sh: &Shared, deck: &Deck, src: Src, rt: &tokio::runtime::Handle) -> Result<(), String> {
    let cancel = Arc::new(AtomicBool::new(false));
    let (ms, ext): (Box<dyn MediaSource>, Option<String>) = match &src {
        Src::File(p) => (Box::new(std::fs::File::open(p).map_err(|e| format!("файл не открылся: {e}"))?), ext_of(p)),
        Src::Url(u) => (Box::new(HttpSource::open(u, rt, cancel.clone())?), ext_of(u)),
        Src::Ytdlp(v, exe) => (Box::new(HttpSource::open_ytdlp(v, exe, cancel.clone())?), Some("m4a".into())),
    };
    struct CancelOnDrop(Arc<AtomicBool>);
    impl Drop for CancelOnDrop { fn drop(&mut self) { self.0.store(true, Relaxed); } }
    let _guard = CancelOnDrop(cancel);
    let mss = MediaSourceStream::new(ms, Default::default());
    let mut hint = Hint::new();
    if let Some(e) = &ext { hint.with_extension(e); }
    let mut format = symphonia::default::get_probe()
        .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
        .map_err(|e| format!("формат не поддерживается: {e}"))?;
    let track = format.default_track(TrackType::Audio).ok_or("в файле нет звуковой дорожки")?;
    let tid = track.id;
    let params = track.codec_params.as_ref().and_then(|c| c.audio()).ok_or("нет параметров кодека")?.clone();
    let src_rate = params.sample_rate.unwrap_or(44100);
    if let (Some(n), Some(tb)) = (track.num_frames, track.time_base) {
        if let Some(t) = tb.calc_time(symphonia::core::units::Timestamp::new(n as i64)) { let d = t.as_secs_f64(); if d > 1.0 { f64_store(&deck.dur, d); } }
    } else if let Some(n) = track.num_frames {
        let d = n as f64 / src_rate as f64;
        if d > 1.0 { f64_store(&deck.dur, d); }
    }
    let mut decoder = symphonia::default::get_codecs().make_audio_decoder(&params, &AudioDecoderOptions::default()).map_err(|e| format!("кодек не поддерживается: {e}"))?;
    let mut rs = Resampler::default();
    let mut ws = Wsola::default();
    let mut pcm: Vec<f32> = vec![];
    let mut st: Vec<f32> = vec![];
    let mut res: Vec<f32> = vec![];
    let mut fin: Vec<f32> = vec![];
    loop {
        if deck.cancel.load(Relaxed) { return Ok(()); }
        let out_rate = sh.out_rate.load(Relaxed);
        let req = deck.seek_req.lock().unwrap().take();
        if let Some(t) = req {
            let time = Time::try_from_secs_f64(t).unwrap_or(Time::ZERO);
            match format.seek(SeekMode::Accurate, SeekTo::Time { time, track_id: Some(tid) }) {
                Ok(s) => {
                    decoder.reset();
                    rs = Resampler::default();
                    ws = Wsola::default();
                    deck.buf.lock().unwrap().clear();
                    let actual = format.tracks().iter().find(|x| x.id == tid).and_then(|x| x.time_base).and_then(|tb| tb.calc_time(s.actual_ts)).map(|t| t.as_secs_f64()).unwrap_or(t);
                    // позиция могла сдвинуться ещё раз, пока перематывали
                    if deck.seek_req.lock().unwrap().is_none() { f64_store(&deck.base, actual); deck.consumed.store(0, Relaxed); }
                    deck.eof.store(false, Relaxed);
                    deck.ended_sent.store(false, Relaxed);
                }
                Err(_) => {
                    // источник пока не умеет перематывать (поток YouTube ещё докачивается): играем дальше с того же места
                    if deck.seek_req.lock().unwrap().is_none() { f64_store(&deck.base, f64_load(&deck.seek_from)); deck.consumed.store(0, Relaxed); }
                }
            }
        }
        let buffered = deck.buf.lock().unwrap().len();
        if buffered > (out_rate as usize) * 2 * 3 || deck.eof.load(Relaxed) {
            std::thread::sleep(Duration::from_millis(15));
            continue;
        }
        let pkt = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => { deck.eof.store(true, Relaxed); continue; }
            Err(symphonia::core::errors::Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => { deck.eof.store(true, Relaxed); continue; }
            Err(e) => {
                if deck.started.load(Relaxed) { deck.eof.store(true, Relaxed); continue; }
                return Err(format!("ошибка чтения: {e}"));
            }
        };
        if pkt.track_id != tid { continue; }
        let buf = match decoder.decode(&pkt) {
            Ok(b) => b,
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(format!("ошибка декодирования: {e}")),
        };
        let ch = buf.spec().channels().count().max(1);
        let rate_in = buf.spec().rate();
        pcm.resize(buf.samples_interleaved(), 0.0);
        buf.copy_to_slice_interleaved(&mut pcm[..]);
        // в стерео
        st.clear();
        for fr in pcm.chunks(ch) {
            let l = fr[0];
            let r = if ch > 1 { fr[1] } else { l };
            st.push(l); st.push(r);
        }
        res.clear();
        if sh.hq.load(Relaxed) { rs.process_sinc(&st, rate_in.max(1), out_rate, &mut res); } else { rs.process(&st, rate_in.max(1), out_rate, &mut res); }
        let speed = f32_load(&deck.speed);
        let out: &Vec<f32> = if (speed - 1.0).abs() > 0.01 { fin.clear(); ws.process(&res, speed, out_rate, &mut fin); &fin } else { &res };
        deck.buf.lock().unwrap().extend(out.iter().copied());
    }
}

/// Пересэмплирование кубической интерполяцией (стерео, чередование каналов).
#[derive(Default)]
pub struct Resampler { pos: f64, hist: [[f32; 2]; 3], sinc_hist: Vec<f32>, spos: f64 }
impl Resampler {
    /// Апскейл «Слабый»: оконный sinc (Ланцош, 16 точек) — чище верх, без «звона» кубической интерполяции.
    pub fn process_sinc(&mut self, inp: &[f32], from: u32, to: u32, out: &mut Vec<f32>) {
        // та же частота: звук проходит без изменений
        if from == to { out.extend_from_slice(inp); return; }
        const A: isize = 8;
        self.sinc_hist.extend_from_slice(inp);
        let n = (self.sinc_hist.len() / 2) as isize;
        let step = from as f64 / to as f64;
        let cutoff = if to < from { to as f64 / from as f64 } else { 1.0 };
        let h = &self.sinc_hist;
        if self.spos < A as f64 { self.spos = A as f64; }
        while self.spos < (n - A - 1) as f64 {
            let c = self.spos.floor() as isize;
            let frac = self.spos - c as f64;
            let (mut l, mut r, mut wsum) = (0.0f64, 0.0f64, 0.0f64);
            for k in (-A + 1)..=A {
                let x = (k as f64 - frac) * cutoff;
                let w = if x.abs() < 1e-9 { 1.0 } else { let px = std::f64::consts::PI * x; (px.sin() / px) * ((px / A as f64).sin() / (px / A as f64)) };
                let j = ((c + k) * 2) as usize;
                l += h[j] as f64 * w; r += h[j + 1] as f64 * w; wsum += w;
            }
            let g = if wsum.abs() > 1e-9 { 1.0 / wsum } else { 1.0 };
            out.push((l * g) as f32); out.push((r * g) as f32);
            self.spos += step;
        }
        let drop = (self.spos.floor() as isize - A).max(0) as usize;
        self.sinc_hist.drain(..drop * 2);
        self.spos -= drop as f64;
    }
    fn process(&mut self, inp: &[f32], from: u32, to: u32, out: &mut Vec<f32>) {
        if from == to { out.extend_from_slice(inp); return; }
        let step = from as f64 / to as f64;
        let n = inp.len() / 2;
        // окно: hist (3 прошлых кадра) + новые кадры
        let get = |h: &[[f32; 2]; 3], i: isize, c: usize| -> f32 { if i < 0 { h[(3 + i) as usize][c] } else { inp[i as usize * 2 + c] } };
        while self.pos < n as f64 - 1.0 {
            let i = self.pos.floor() as isize;
            let t = (self.pos - i as f64) as f32;
            for c in 0..2 {
                let (y0, y1, y2, y3) = (get(&self.hist, i - 1, c), get(&self.hist, i, c), get(&self.hist, i + 1, c), get(&self.hist, (i + 2).min(n as isize - 1), c));
                let a = -0.5 * y0 + 1.5 * y1 - 1.5 * y2 + 0.5 * y3;
                let b = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
                let cc = -0.5 * y0 + 0.5 * y2;
                out.push(((a * t + b) * t + cc) * t + y1);
            }
            self.pos += step;
        }
        self.pos -= n as f64;
        for k in 0..3 {
            let i = n as isize - 3 + k as isize;
            self.hist[k] = if i >= 0 { [inp[i as usize * 2], inp[i as usize * 2 + 1]] } else { self.hist[(3 + i) as usize] };
        }
    }
}

/// Скорость без изменения тона: WSOLA (перекрытие окон с поиском наилучшего совпадения).
#[derive(Default)]
struct Wsola { inp: Vec<f32>, out_tail: Vec<f32>, read: f64 }
impl Wsola {
    fn process(&mut self, x: &[f32], speed: f32, rate: u32, out: &mut Vec<f32>) {
        let n = ((rate as usize) / 40).max(256) & !1; // окно ~25 мс
        let hs = n / 2; // шаг синтеза
        let tol = n / 4; // поиск совпадения
        self.inp.extend_from_slice(x);
        let frames = |v: &Vec<f32>| v.len() / 2;
        if self.out_tail.is_empty() { self.out_tail = vec![0.0; hs * 2]; }
        loop {
            let start = self.read as usize;
            if start + n + tol * 2 + hs >= frames(&self.inp) { break; }
            // ищем сдвиг, при котором начало окна лучше всего продолжает предыдущий хвост
            let mut best = 0usize; let mut bc = f32::MIN;
            let mut k = 0;
            while k <= tol * 2 {
                let mut c = 0.0;
                let mut j = 0;
                while j < hs { let a = self.inp[(start + k + j) * 2] + self.inp[(start + k + j) * 2 + 1]; c += a * (self.out_tail[j * 2] + self.out_tail[j * 2 + 1]); j += 4; }
                if c > bc { bc = c; best = k; }
                k += 2;
            }
            let s = start + best;
            for j in 0..hs {
                let w = 0.5 - 0.5 * (std::f32::consts::PI * j as f32 / hs as f32).cos(); // нарастание
                for c in 0..2 { out.push(self.out_tail[j * 2 + c] * (1.0 - w) + self.inp[(s + j) * 2 + c] * w); }
            }
            // новый хвост — вторая половина окна
            for j in 0..hs { for c in 0..2 { self.out_tail[j * 2 + c] = self.inp[(s + hs + j) * 2 + c]; } }
            self.read += hs as f64 * speed as f64;
        }
        // освобождаем прочитанное
        let drop_f = (self.read as usize).saturating_sub(tol * 2);
        if drop_f > 0 { self.inp.drain(..drop_f * 2); self.read -= drop_f as f64; }
    }
}
