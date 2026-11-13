//! Клип YouTube в режиме «без плеера»: ссылку даёт yt-dlp, кадры декодирует Media Foundation (встроен в Windows),
//! показ синхронизирован с позицией звука нашего движка. Видео без звука: звук идёт отдельно через аудиодвижок.
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::*};
use std::sync::{Arc, Mutex};

use eframe::egui;
use windows::core::{GUID, PCWSTR};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;

pub struct Frame { pub img: egui::ColorImage, pub ts: f64 }

struct Shared {
    target: AtomicU64,
    playing: AtomicBool,
    seek: Mutex<Option<f64>>,
    frame: Mutex<Option<Frame>>,
    status: Mutex<String>,
    stop: AtomicBool,
}

pub struct Video {
    sh: Arc<Shared>,
    pub tex: Option<egui::TextureHandle>,
    pub aspect: f32,
}

impl Drop for Video { fn drop(&mut self) { self.sh.stop.store(true, Relaxed); } }

impl Video {
    pub fn open(exe: String, vid: String, ctx: egui::Context, rt: tokio::runtime::Handle, pos: f64, playing: bool) -> Video {
        let sh = Arc::new(Shared { target: AtomicU64::new(pos.to_bits()), playing: AtomicBool::new(playing), seek: Mutex::new(None), frame: Mutex::new(None), status: Mutex::new("…".into()), stop: AtomicBool::new(false) });
        let s2 = sh.clone();
        rt.spawn_blocking(move || {
            let url = match crate::ytdlp::video_url(&exe, &vid) { Ok(u) => u, Err(e) => { *s2.status.lock().unwrap() = e; ctx.request_repaint(); return; } };
            if let Err(e) = unsafe { run(&s2, &url, &ctx) } { *s2.status.lock().unwrap() = format!("видео: {e}"); ctx.request_repaint(); }
        });
        Video { sh, tex: None, aspect: 16.0 / 9.0 }
    }
    pub fn set_playing(&self, p: bool) { self.sh.playing.store(p, Relaxed); }
    pub fn seek(&self, sec: f64) { *self.sh.seek.lock().unwrap() = Some(sec); self.sh.target.store(sec.to_bits(), Relaxed); }
    /// Каждый кадр UI: сообщить позицию звука и забрать готовый кадр.
    pub fn update(&mut self, ctx: &egui::Context, pos: f64, playing: bool) -> Option<egui::TextureId> {
        self.sh.target.store(pos.to_bits(), Relaxed);
        self.sh.playing.store(playing, Relaxed);
        if let Some(f) = self.sh.frame.lock().unwrap().take() {
            self.aspect = f.img.size[0] as f32 / f.img.size[1].max(1) as f32;
            match &mut self.tex { Some(t) => t.set(f.img, egui::TextureOptions::LINEAR), None => self.tex = Some(ctx.load_texture("yt-video", f.img, egui::TextureOptions::LINEAR)) }
        }
        self.tex.as_ref().map(|t| t.id())
    }
    pub fn status(&self) -> String { self.sh.status.lock().unwrap().clone() }
}

unsafe fn run(sh: &Shared, url: &str, ctx: &egui::Context) -> windows::core::Result<()> {
    MFStartup(MF_VERSION, MFSTARTUP_FULL)?;
    let mut attrs: Option<IMFAttributes> = None;
    MFCreateAttributes(&mut attrs, 2)?;
    let attrs = attrs.unwrap();
    attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)?;
    let w: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
    let reader = MFCreateSourceReaderFromURL(PCWSTR(w.as_ptr()), &attrs)?;
    let vs = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
    let mt = MFCreateMediaType()?;
    mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
    mt.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
    reader.SetCurrentMediaType(vs, None, &mt)?;
    let cur = reader.GetCurrentMediaType(vs)?;
    let fs = cur.GetUINT64(&MF_MT_FRAME_SIZE)?;
    let (fw, fh) = ((fs >> 32) as usize, (fs & 0xffff_ffff) as usize);
    *sh.status.lock().unwrap() = String::new();
    let mut last_push = std::time::Instant::now() - std::time::Duration::from_secs(1);
    let mut pending: Option<(i64, IMFSample)> = None;
    while !sh.stop.load(Relaxed) {
        let target = f64::from_bits(sh.target.load(Relaxed));
        let seek = sh.seek.lock().unwrap().take();
        // перемотка: явная или большой разрыв со звуком
        let lag = pending.as_ref().map(|(ts, _)| target - *ts as f64 / 1e7).unwrap_or(0.0);
        if let Some(s) = seek.or(if lag > 3.0 || lag < -3.0 { Some(target) } else { None }) {
            let pv = PROPVARIANT::from((s.max(0.0) * 1e7) as i64);
            let _ = reader.SetCurrentPosition(&GUID::zeroed(), &pv);
            pending = None;
        }
        if pending.is_none() {
            let (mut flags, mut ts) = (0u32, 0i64);
            let mut sample: Option<IMFSample> = None;
            reader.ReadSample(vs, 0, None, Some(&mut flags), Some(&mut ts), Some(&mut sample))?;
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 { std::thread::sleep(std::time::Duration::from_millis(100)); continue; }
            if let Some(s) = sample { pending = Some((ts, s)); } else { continue; }
        }
        let (ts, _) = pending.as_ref().unwrap();
        let t = *ts as f64 / 1e7;
        let target = f64::from_bits(sh.target.load(Relaxed));
        if t > target + 0.02 {
            // кадр из будущего: ждём звук (на паузе тоже стоим)
            std::thread::sleep(std::time::Duration::from_millis(if sh.playing.load(Relaxed) { 8 } else { 40 }));
            if t - target > 0.6 && !sh.playing.load(Relaxed) && sh.frame.lock().unwrap().is_none() && last_push.elapsed().as_secs_f64() > 2.0 { /* после перемотки на паузе покажем ближайший кадр */ } else { continue; }
        }
        let (_, s) = pending.take().unwrap();
        // кадры, которые уже опоздали, пропускаем без копирования (кроме редких, чтобы картинка не замирала)
        if target - t > 0.15 && last_push.elapsed().as_millis() < 300 { continue; }
        let buf = s.ConvertToContiguousBuffer()?;
        let mut p: *mut u8 = std::ptr::null_mut();
        let mut len = 0u32;
        buf.Lock(&mut p, None, Some(&mut len))?;
        let data = std::slice::from_raw_parts(p, len as usize);
        let stride = if fh > 0 { len as usize / fh } else { fw * 4 };
        let mut rgba = vec![0u8; fw * fh * 4];
        for y in 0..fh {
            let row = &data[y * stride..(y * stride + fw * 4).min(data.len())];
            for (x, px) in row.chunks_exact(4).enumerate() { let o = (y * fw + x) * 4; rgba[o] = px[2]; rgba[o + 1] = px[1]; rgba[o + 2] = px[0]; rgba[o + 3] = 255; }
        }
        buf.Unlock()?;
        *sh.frame.lock().unwrap() = Some(Frame { img: egui::ColorImage::from_rgba_unmultiplied([fw, fh], &rgba), ts: t });
        last_push = std::time::Instant::now();
        ctx.request_repaint();
    }
    Ok(())
}
