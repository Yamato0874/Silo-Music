//! «Слабый» апскейл: достройка верха, срезанного при сжатии (mp3, AAC, Opus), в реальном времени.
//! Принцип как у SBR в HE-AAC: короткое преобразование Фурье (окно 2048, шаг 1024), по сглаженному спектру находим частоту среза
//! (выше неё почти пусто), и в пустую полосу копируем (с ослаблением 12–24 дБ) полосу прямо под срезом. Ниже среза звук не меняется;
//! если среза нет (полнополосный трек), ничего не добавляется. Задержка 2047 отсчётов (около 46 мс).
use std::collections::VecDeque;

const N: usize = 2048;
const H: usize = 1024;

/// БПФ (радикс-2) с готовой таблицей поворотов; f32 хватает для звука.
pub struct Fft { n: usize, rev: Vec<u32>, cos: Vec<f32>, sin: Vec<f32> }
impl Fft {
    pub fn new(n: usize) -> Fft {
        let bits = n.trailing_zeros();
        let rev = (0..n as u32).map(|i| i.reverse_bits() >> (32 - bits)).collect();
        let cos = (0..n / 2).map(|k| (std::f64::consts::TAU * k as f64 / n as f64).cos() as f32).collect();
        let sin = (0..n / 2).map(|k| (std::f64::consts::TAU * k as f64 / n as f64).sin() as f32).collect();
        Fft { n, rev, cos, sin }
    }
    pub fn run(&self, re: &mut [f32], im: &mut [f32], inverse: bool) {
        let n = self.n;
        for i in 0..n { let j = self.rev[i] as usize; if i < j { re.swap(i, j); im.swap(i, j); } }
        let mut len = 2;
        while len <= n {
            let (half, step) = (len / 2, n / len);
            for i in (0..n).step_by(len) {
                for k in 0..half {
                    let (c, s) = (self.cos[k * step], if inverse { self.sin[k * step] } else { -self.sin[k * step] });
                    let (a, b) = (i + k, i + k + half);
                    let (tr, ti) = (re[b] * c - im[b] * s, re[b] * s + im[b] * c);
                    re[b] = re[a] - tr; im[b] = im[a] - ti;
                    re[a] += tr; im[a] += ti;
                }
            }
            len <<= 1;
        }
        if inverse { let k = 1.0 / n as f32; for i in 0..n { re[i] *= k; im[i] *= k; } }
    }
}

struct Ch {
    pending: Vec<f32>,
    hist: Vec<f32>,
    ola: Vec<f32>,
    outq: VecDeque<f32>,
    /// сглаженный по времени спектр мощности
    s: Vec<f32>,
    /// сглаженный номер бина среза и доля достройки (0..1)
    kc: f32,
    ext: f32,
    re: Vec<f32>,
    im: Vec<f32>,
}
impl Ch {
    fn new() -> Ch {
        Ch { pending: Vec::with_capacity(H), hist: vec![0.0; N], ola: vec![0.0; N], outq: VecDeque::with_capacity(4 * H), s: vec![0.0; N / 2 + 1], kc: 0.0, ext: 0.0, re: vec![0.0; N], im: vec![0.0; N] }
    }
    fn reset(&mut self) {
        self.pending.clear(); self.hist.iter_mut().for_each(|x| *x = 0.0); self.ola.iter_mut().for_each(|x| *x = 0.0);
        self.outq.clear(); self.s.iter_mut().for_each(|x| *x = 0.0); self.kc = 0.0; self.ext = 0.0;
    }
}

/// Достройка верха для стерео (чередующиеся L, R). Обработка на месте блоками любого размера.
pub struct Bwe {
    fs: f32,
    fft: Fft,
    win: Vec<f32>,
    ch: [Ch; 2],
    /// ослабление достроенной полосы в начале (дБ); к концу полосы добавляется ещё 12 дБ
    atten_db: f32,
}

impl Bwe {
    pub fn new(fs: f32, atten_db: f32) -> Bwe {
        // корень из окна Ханна: при шаге N/2 даёт точное восстановление, когда спектр не менялся
        let win = (0..N).map(|i| (std::f64::consts::PI * (i as f64 + 0.5) / N as f64).sin() as f32).collect();
        Bwe { fs, fft: Fft::new(N), win, ch: [Ch::new(), Ch::new()], atten_db }
    }
    pub fn set_rate(&mut self, fs: f32) { if (fs - self.fs).abs() > 1.0 { self.fs = fs; self.reset(); } }
    pub fn reset(&mut self) { for c in self.ch.iter_mut() { c.reset(); } }

    pub fn process(&mut self, x: &mut [f32]) {
        for fr in x.chunks_exact_mut(2) {
            for c in 0..2 {
                self.ch[c].pending.push(fr[c]);
                if self.ch[c].pending.len() == H { self.frame(c); }
                fr[c] = self.ch[c].outq.pop_front().unwrap_or(0.0);
            }
        }
    }

    fn frame(&mut self, c: usize) {
        let (fft, win, fs, atten) = (&self.fft, &self.win, self.fs, self.atten_db);
        let ch = &mut self.ch[c];
        ch.hist.copy_within(H.., 0);
        let (head, tail) = ch.hist.split_at_mut(N - H);
        let _ = head;
        tail.copy_from_slice(&ch.pending);
        ch.pending.clear();
        for i in 0..N { ch.re[i] = ch.hist[i] * win[i]; ch.im[i] = 0.0; }
        fft.run(&mut ch.re, &mut ch.im, false);
        let nb = N / 2;
        for k in 0..=nb { let p = ch.re[k] * ch.re[k] + ch.im[k] * ch.im[k]; ch.s[k] = 0.9 * ch.s[k] + 0.1 * p; }
        let bin_hz = fs / N as f32;
        let kmax = ((20500.0 / bin_hz) as usize).min(nb - 20);
        let (ra, rb) = ((3000.0 / bin_hz) as usize, (8000.0 / bin_hz) as usize);
        // частота среза: самый верхний бин, где сглаженный спектр ещё не на 50 дБ слабее опорной полосы 3–8 кГц
        let r: f32 = ch.s[ra..rb].iter().sum::<f32>() / (rb - ra) as f32;
        let mut target = 0.0f32;
        let mut kc_meas = 0usize;
        if r > 1e-9 {
            let thr = r * 1e-5;
            const W: usize = 16;
            let mut sum: f32 = ch.s[nb - W + 1..=nb].iter().sum();
            let mut k = nb - W + 1;
            loop {
                if sum / W as f32 > thr { kc_meas = k + W - 1; break; }
                if k <= ra { break; }
                k -= 1;
                sum += ch.s[k] - ch.s[k + W];
            }
            // достраиваем, только если срез заметно ниже 20,5 кГц и выше 6 кГц
            if kc_meas > 0 && kc_meas + 24 < kmax && kc_meas as f32 * bin_hz > 6000.0 { target = 1.0; }
        }
        if target > 0.0 {
            if ch.ext < 0.001 { ch.kc = kc_meas as f32; } else { ch.kc += 0.2 * (kc_meas as f32 - ch.kc); }
        }
        ch.ext += (target - ch.ext).clamp(-0.1, 0.05);
        if ch.ext > 0.001 {
            let kc = ch.kc.round() as usize;
            if kc + 8 < kmax {
                let w = kmax - kc;
                for k in kc + 1..=kmax {
                    let src = k - w;
                    let t = (k - kc) as f32 / w as f32;
                    let g = ch.ext * 10f32.powf(-(atten + 12.0 * t) / 20.0);
                    let (sr, si) = (ch.re[src] * g, ch.im[src] * g);
                    ch.re[k] += sr; ch.im[k] += si;
                    ch.re[N - k] += sr; ch.im[N - k] -= si;
                }
            }
        }
        fft.run(&mut ch.re, &mut ch.im, true);
        for i in 0..N { ch.ola[i] += ch.re[i] * win[i]; }
        for i in 0..H { ch.outq.push_back(ch.ola[i]); }
        ch.ola.copy_within(H.., 0);
        for v in ch.ola[N - H..].iter_mut() { *v = 0.0; }
    }
}

#[cfg(test)]
pub mod testutil {
    /// БПФ в двойной точности для проверок.
    pub fn fft(re: &mut [f64], im: &mut [f64], inverse: bool) {
        let n = re.len();
        let mut j = 0;
        for i in 1..n {
            let mut bit = n >> 1;
            while j & bit != 0 { j ^= bit; bit >>= 1; }
            j ^= bit;
            if i < j { re.swap(i, j); im.swap(i, j); }
        }
        let mut len = 2;
        while len <= n {
            let ang = 2.0 * std::f64::consts::PI / len as f64 * if inverse { 1.0 } else { -1.0 };
            let (wr, wi) = (ang.cos(), ang.sin());
            for i in (0..n).step_by(len) {
                let (mut cr, mut ci) = (1.0, 0.0);
                for k in 0..len / 2 {
                    let (ur, ui) = (re[i + k], im[i + k]);
                    let (vr, vi) = (re[i + k + len / 2] * cr - im[i + k + len / 2] * ci, re[i + k + len / 2] * ci + im[i + k + len / 2] * cr);
                    re[i + k] = ur + vr; im[i + k] = ui + vi;
                    re[i + k + len / 2] = ur - vr; im[i + k + len / 2] = ui - vi;
                    let nr = cr * wr - ci * wi; ci = cr * wi + ci * wr; cr = nr;
                }
            }
            len <<= 1;
        }
        if inverse { for i in 0..n { re[i] /= n as f64; im[i] /= n as f64; } }
    }
    /// Розовый шум 20 Гц … cut Гц с «кирпичным» срезом (как у mp3 с ограничением полосы), среднеквадратичное значение rms.
    pub fn pink_noise_cut(n: usize, fs: f64, cut: f64, rms: f64, seed: u64) -> Vec<f32> {
        let mut re = vec![0.0f64; n];
        let mut im = vec![0.0f64; n];
        let mut x = seed | 1;
        let mut rnd = || { x ^= x << 13; x ^= x >> 7; x ^= x << 17; (x >> 11) as f64 / (1u64 << 53) as f64 };
        for k in 1..n / 2 {
            let f = k as f64 * fs / n as f64;
            if f < 20.0 || f > cut { continue; }
            let a = 1.0 / f.sqrt();
            let ph = rnd() * std::f64::consts::TAU;
            re[k] = a * ph.cos(); im[k] = a * ph.sin();
            re[n - k] = re[k]; im[n - k] = -im[k];
        }
        fft(&mut re, &mut im, true);
        let cur = (re.iter().map(|v| v * v).sum::<f64>() / n as f64).sqrt();
        re.iter().map(|v| (v * rms / cur) as f32).collect()
    }
    /// Средняя плотность мощности сигнала в полосе [lo, hi] Гц, в дБ (по окну Ханна, средняя часть сигнала).
    pub fn band_db(x: &[f32], fs: f64, lo: f64, hi: f64) -> f64 {
        let n = (1usize << 16).min(x.len().next_power_of_two() / 2);
        let off = (x.len() - n) / 2;
        let mut re: Vec<f64> = (0..n).map(|i| x[i + off] as f64 * (0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n as f64).cos())).collect();
        let mut im = vec![0.0; n];
        fft(&mut re, &mut im, false);
        let (mut e, mut c) = (0.0, 0usize);
        for k in 1..n / 2 { let f = k as f64 * fs / n as f64; if f >= lo && f <= hi { e += re[k] * re[k] + im[k] * im[k]; c += 1; } }
        10.0 * (e / c.max(1) as f64 / (n as f64 * n as f64) + 1e-30).log10()
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::*;
    use super::*;

    fn run_stereo(bwe: &mut Bwe, mono: &[f32]) -> Vec<f32> {
        let mut inter: Vec<f32> = mono.iter().flat_map(|&s| [s, s]).collect();
        // блоками разного размера, как в колбэке вывода
        let mut pos = 0;
        let sizes = [480usize, 1024, 333, 2048, 7];
        let mut k = 0;
        while pos < inter.len() {
            let len = (sizes[k % sizes.len()] * 2).min(inter.len() - pos);
            bwe.process(&mut inter[pos..pos + len]);
            pos += len; k += 1;
        }
        inter.chunks(2).map(|f| f[0]).collect()
    }

    #[test]
    fn fft_matches_reference() {
        let n = 2048;
        let x: Vec<f32> = (0..n).map(|i| ((i * 7919) % 1000) as f32 / 500.0 - 1.0).collect();
        let (mut re, mut im) = (x.clone(), vec![0.0f32; n]);
        Fft::new(n).run(&mut re, &mut im, false);
        let (mut rr, mut ri): (Vec<f64>, Vec<f64>) = (x.iter().map(|&v| v as f64).collect(), vec![0.0; n]);
        fft(&mut rr, &mut ri, false);
        let err = (0..n).map(|k| ((re[k] as f64 - rr[k]).powi(2) + (im[k] as f64 - ri[k]).powi(2)).sqrt()).fold(0.0, f64::max);
        assert!(err < 0.01, "ошибка БПФ {err}");
        Fft::new(n).run(&mut re, &mut im, true);
        let back = (0..n).map(|k| (re[k] - x[k]).abs()).fold(0.0, f32::max);
        assert!(back < 1e-4, "обратное преобразование {back}");
    }

    #[test]
    fn full_band_signal_passes_unchanged() {
        let fs = 44100.0;
        let x = pink_noise_cut(1 << 17, fs as f64, 21900.0, 0.1, 7);
        let mut b = Bwe::new(fs, 12.0);
        let y = run_stereo(&mut b, &x);
        // задержка 2047 отсчётов; сравниваем после разгона
        let lag = 2 * H - 1;
        let err = (20000..100000).map(|i| (y[i + lag] - x[i]).abs()).fold(0.0, f32::max);
        assert!(err < 2e-3, "полнополосный сигнал изменился: {err}");
    }

    fn check_cut(fs: f64, cut: f64) {
        let x = pink_noise_cut(1 << 17, fs, cut, 0.1, 99);
        let mut b = Bwe::new(fs as f32, 12.0);
        let y = run_stereo(&mut b, &x);
        let (below_in, below_out) = (band_db(&x, fs, 8000.0, cut - 1000.0), band_db(&y, fs, 8000.0, cut - 1000.0));
        let (src_out, hi_out) = (band_db(&y, fs, cut - 4000.0, cut - 500.0), band_db(&y, fs, cut + 500.0, 20000.0));
        let hi_in = band_db(&x, fs, cut + 500.0, 20000.0);
        eprintln!("срез {cut:.0} Гц, {fs:.0} Гц: ниже среза {below_in:.1} → {below_out:.1} дБ; выше среза было {hi_in:.1} стало {hi_out:.1} дБ; полоса под срезом {src_out:.1} дБ (разница {:.1})", hi_out - src_out);
        assert!((below_out - below_in).abs() < 0.3, "ниже среза звук изменился на {:.2} дБ", below_out - below_in);
        assert!(hi_in < -120.0, "тестовый сигнал не обрезан: {hi_in}");
        let d = hi_out - src_out;
        assert!(d > -30.0 && d < -10.0, "достроенный верх на {d:.1} дБ от полосы под срезом, ожидалось от -30 до -10");
        // общая громкость почти не меняется
        let rms = |v: &[f32]| (v.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / v.len() as f64).sqrt();
        let (a, c) = (rms(&x[20000..100000]), rms(&y[22047..102047]));
        assert!((20.0 * (c / a).log10()).abs() < 0.2, "громкость изменилась: {:.2} дБ", 20.0 * (c / a).log10());
    }

    #[test]
    fn rebuilds_top_above_16k_cut_44100() { check_cut(44100.0, 16000.0); }
    #[test]
    fn rebuilds_top_above_16k_cut_48000() { check_cut(48000.0, 16000.0); }
    #[test]
    fn rebuilds_top_above_14k_cut() { check_cut(44100.0, 14000.0); }

    #[test]
    fn silence_and_impulses_are_stable() {
        let mut b = Bwe::new(44100.0, 12.0);
        let mut x = vec![0.0f32; 20000];
        let y = run_stereo(&mut b, &x);
        assert!(y.iter().all(|v| *v == 0.0));
        for i in (0..x.len()).step_by(3000) { x[i] = 1.0; }
        let y = run_stereo(&mut b, &x);
        assert!(y.iter().all(|v| v.is_finite() && v.abs() < 2.0));
    }
}
