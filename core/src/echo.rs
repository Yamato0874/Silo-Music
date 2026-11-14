use crate::model::Track;
use std::collections::{HashMap, HashSet};

/// Склонность к новому (пользовательская подсказка модели): сдвигает стартовые ожидания по незнакомым жанрам.
/// Это не фиксированная доля: реальное исследование определяется неопределённостью модели и поведением человека.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode { Familiar, Balanced, Discovery }
impl Mode {
    pub fn parse(s: &str) -> Mode {
        match s { "familiar" => Mode::Familiar, "discovery" => Mode::Discovery, _ => Mode::Balanced }
    }
    /// ожидаемая «понравится» для жанра, о котором ещё ничего не известно
    fn base_prior(self) -> f64 {
        match self { Mode::Familiar => 0.30, Mode::Balanced => 0.45, Mode::Discovery => 0.60 }
    }
}

/// Малый детерминированный ГСЧ (xorshift64*), чтобы поведение можно было проверять тестами.
pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Self { Rng(seed.max(1) ^ 0x9E37_79B9_7F4A_7C15) }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12; self.0 ^= self.0 << 25; self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    pub fn f(&mut self) -> f64 { ((self.next() >> 11) as f64 / (1u64 << 53) as f64).clamp(1e-12, 1.0 - 1e-12) }
    pub fn below(&mut self, n: usize) -> usize { if n == 0 { 0 } else { (self.next() % n as u64) as usize } }
    fn gauss(&mut self) -> f64 { (-2.0 * self.f().ln()).sqrt() * (2.0 * std::f64::consts::PI * self.f()).cos() }
    /// Gamma(shape, 1), метод Марсальи–Цанга
    fn gamma(&mut self, shape: f64) -> f64 {
        if shape < 1.0 { return self.gamma(shape + 1.0) * self.f().powf(1.0 / shape); }
        let d = shape - 1.0 / 3.0;
        let c = 1.0 / (9.0 * d).sqrt();
        loop {
            let x = self.gauss();
            let v = (1.0 + c * x).powi(3);
            if v <= 0.0 { continue; }
            if self.f().ln() < 0.5 * x * x + d - d * v + d * v.ln() { return d * v; }
        }
    }
    /// Beta(a, b)
    pub fn beta(&mut self, a: f64, b: f64) -> f64 {
        let (x, y) = (self.gamma(a), self.gamma(b));
        x / (x + y)
    }
}

use crate::taxonomy;

/// Любимый исполнитель, выбранный при знакомстве: имя и теги (жанры), если известны.
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub struct SeedArtist { pub name: String, #[serde(default)] pub tags: Vec<String> }

/// Событие прослушивания или оценки. reward 0..1: насколько трек «зашёл».
#[derive(Debug, Clone)]
pub struct Event { pub tags: Vec<String>, pub artist: String, pub reward: f64, pub weight: f64, pub age_secs: i64, pub quality: Option<f64> }
impl Event {
    /// Жанр-относительная качественность трека (0..1), если известна.
    pub fn with_quality(mut self, q: Option<f64>) -> Self { self.quality = q; self }
    /// Исполнитель трека: по нему ведётся отдельная оценка «насколько нравится».
    pub fn with_artist(mut self, a: &str) -> Self { self.artist = a.to_string(); self }
}

/// Прослушивание: доля прослушанного → награда. Меньше 10% (мгновенный пропуск) = 0, от 80% = 1, между — линейно.
pub fn play_event(tags: Vec<String>, fraction: f64, age_secs: i64) -> Event {
    // мгновенный пропуск (меньше 10%) весит больше обычного прослушивания: модель быстрее учится тому, что не нравится
    let reward = ((fraction - 0.1) / 0.7).clamp(0.0, 1.0);
    Event { tags, artist: String::new(), reward, weight: if fraction < 0.1 { 1.6 } else { 1.0 }, age_secs, quality: None }
}
/// Оценка: «нравится» — сильный плюс, «не нравится» — лёгкий минус. Он слегка сдвигает оценку исполнителя, его жанров
/// и похожей музыки, но никого не банит: полное исключение касается только самого трека (решает хранилище, не модель).
pub fn rating_event(tags: Vec<String>, value: i32, age_secs: i64) -> Event {
    if value > 0 { Event { tags, artist: String::new(), reward: 1.0, weight: 3.0, age_secs, quality: None } } else { Event { tags, artist: String::new(), reward: 0.0, weight: 1.0, age_secs, quality: None } }
}

/// Модель вкуса: бета-распределение «понравится» для каждого жанра. Строится заново из журнала событий,
/// поэтому всегда отражает то, что и как человек слушает. Старые события затухают, а скорость затухания зависит от активности.
#[derive(Debug, Clone)]
pub struct Model {
    ev: HashMap<String, (f64, f64)>,     // (успехи, неудачи) с затуханием
    liked_n: HashMap<String, f64>,
    pairs: HashMap<(String, String), f64>,
    seed: HashSet<String>,
    /// склонность человека к новому: сглаженная доля «зашедших» исследовательских треков, 0..1
    pub appetite: f64,
    pub half_life_days: f64,
    m0: f64,
    /// какую по популярности в жанре музыку человек обычно дослушивает/лайкает (0..1; 0,5 — середина жанра)
    q_pref: f64,
    q_n: f64,
    /// оценка исполнителей: (успехи, неудачи) с затуханием
    artists: HashMap<String, (f64, f64)>,
    art_n: f64,
}
const S0: f64 = 3.0; // «вес» априорного ожидания

impl Model {
    pub fn build(events: &[Event], mode: Mode, seed: &[String]) -> Model {
        let mut evs: Vec<&Event> = events.iter().collect();
        evs.sort_by(|a, b| b.age_secs.cmp(&a.age_secs)); // от старых к новым
        // адаптивное затухание: активным слушателям данных много, они дрейфуют быстрее — забываем быстрее
        let n14 = events.iter().filter(|e| e.age_secs < 14 * 86400).count() as f64;
        let half_life_days = 7.0 + 23.0 / (1.0 + n14 / 20.0);
        // склонность к новому: как человек реагирует на жанры, которые не были для него «любимыми»
        let tax = taxonomy::get();
        let exps: Vec<Vec<(String, f64)>> = evs.iter().map(|e| tax.expand(&e.tags)).collect();
        let mut run: HashMap<String, (f64, f64)> = HashMap::new();
        let mut appetite = 0.5;
        for (e, ex) in evs.iter().zip(&exps) {
            // жанр «знаком», если по нему самому (а не по родителю) уже есть уверенная положительная история
            let known = ex.iter().filter(|x| x.1 >= 1.0).any(|(t, _)| run.get(t).map_or(false, |(a, b)| a + b >= 3.0 && a / (a + b) > 0.6));
            if !e.tags.is_empty() && !known && e.weight < 2.0 { appetite = 0.85 * appetite + 0.15 * e.reward; }
            for (t, c) in ex { let r = run.entry(t.clone()).or_default(); r.0 += c * e.weight * e.reward; r.1 += c * e.weight * (1.0 - e.reward); }
        }
        let m0 = (mode.base_prior() + 0.3 * (appetite - 0.5)).clamp(0.15, 0.8);
        let mut ev: HashMap<String, (f64, f64)> = HashMap::new();
        let mut liked_n: HashMap<String, f64> = HashMap::new();
        let mut pairs: HashMap<(String, String), f64> = HashMap::new();
        let (mut sum_q, mut sum_w) = (0.0, 0.0);
        let mut artists: HashMap<String, (f64, f64)> = HashMap::new();
        let mut art_n = 0.0;
        for (e, ex) in evs.iter().zip(&exps) {
            let w = e.weight * 0.5f64.powf(e.age_secs as f64 / 86400.0 / half_life_days);
            if let Some(q) = e.quality { sum_q += w * e.reward * q; sum_w += w * e.reward; }
            if !e.artist.is_empty() {
                let x = artists.entry(e.artist.to_lowercase()).or_default();
                x.0 += w * e.reward; x.1 += w * (1.0 - e.reward); art_n += w;
            }
            // сигнал идёт по уровням: поддерживается сам жанр (1.0), его родитель (0.5), семейство (0.25), чтобы редкие поджанры не страдали от нехватки данных
            for (t, c) in ex { let x = ev.entry(t.clone()).or_default(); x.0 += c * w * e.reward; x.1 += c * w * (1.0 - e.reward); }
            if e.reward > 0.6 {
                let leaf: Vec<&String> = ex.iter().filter(|x| x.1 >= 1.0).map(|x| &x.0).collect();
                for t in &leaf { *liked_n.entry((*t).clone()).or_default() += w; }
                for (i, a) in leaf.iter().enumerate() { for b in &leaf[i + 1..] {
                    let (a, b) = (*a, *b);
                    let k = if a < b { (a.clone(), b.clone()) } else { (b.clone(), a.clone()) };
                    *pairs.entry(k).or_default() += w;
                } }
            }
        }
        // предпочтение качества: взвешенное среднее с мягким приором 0,5 (5 «виртуальных» прослушиваний), чтобы мало данных не давало крайностей
        const PRIOR_N: f64 = 5.0;
        let q_pref = (0.5 * PRIOR_N + sum_q) / (PRIOR_N + sum_w);
        Model { ev, liked_n, pairs, seed: seed.iter().flat_map(|x| tax.resolve(std::slice::from_ref(x))).collect(), appetite, half_life_days, m0, q_pref, q_n: sum_w, artists, art_n }
    }

    /// (целевая качественность, вес влияния). Вес растёт с количеством данных и ограничен 0,35, чтобы качество лишь подталкивало
    /// порядок внутри жанра, а не отсекало и не доминировало.
    pub fn quality_target(&self) -> (f64, f64) { (self.q_pref, 0.35 * self.q_n / (self.q_n + 5.0)) }

    /// Насколько человеку нравится исполнитель, 0..1. Без данных — среднее ожидание по его жанрам; каждое прослушивание,
    /// лайк или дизлайк сдвигает значение (дизлайк — в минус, лайк — в плюс), но оно никогда не доходит до нуля: это не бан.
    /// Похожая музыка получает тот же сдвиг через общие жанры и связи между жанрами.
    pub fn artist_affinity(&self, artist: &str, tags: &[String]) -> f64 {
        let ids = taxonomy::get().resolve(tags);
        let prior = if ids.is_empty() { self.m0 } else { ids.iter().map(|t| self.mean(t)).sum::<f64>() / ids.len() as f64 };
        let (a, b) = self.artists.get(&artist.to_lowercase()).copied().unwrap_or((0.0, 0.0));
        (prior * S0 + a) / (S0 + a + b)
    }
    /// Вес оценки исполнителя при подборе: растёт с данными, не больше 0,35.
    pub fn artist_weight(&self) -> f64 { 0.35 * self.art_n / (self.art_n + 5.0) }

    pub fn evidence(&self, t: &str) -> f64 { self.ev.get(t).map_or(0.0, |x| x.0 + x.1) + self.seed_credit(t).map_or(0.0, |c| c.0 / 2.0) }

    /// Стартовый плюс от выбранных при знакомстве жанров и исполнителей: полный для самого узла, слабее для его потомков.
    fn seed_credit(&self, t: &str) -> Option<(f64, f64)> {
        if self.seed.contains(t) { return Some((2.0, 0.5)); }
        if taxonomy::get().ancestors(t).iter().any(|a| self.seed.contains(*a)) { return Some((1.0, 0.25)); }
        None
    }

    /// Любимые исполнители из знакомства: мягкий стартовый плюс исполнителю и его жанрам. Вытесняется реальной историей.
    pub fn with_seed_artists(mut self, list: &[SeedArtist]) -> Model {
        for a in list.iter().filter(|a| !a.name.trim().is_empty()) {
            let x = self.artists.entry(a.name.trim().to_lowercase()).or_default();
            x.0 += 2.0; x.1 += 0.4; self.art_n += 1.0;
            for id in taxonomy::get().resolve(&a.tags) { self.seed.insert(id); }
        }
        self
    }

    /// Связь жанров: априорная соседность или выученная совместная встречаемость в понравившемся.
    fn sim(&self, a: &str, b: &str) -> f64 {
        let learned = {
            let k = if a < b { (a.to_string(), b.to_string()) } else { (b.to_string(), a.to_string()) };
            let p = self.pairs.get(&k).copied().unwrap_or(0.0);
            let (na, nb) = (self.liked_n.get(a).copied().unwrap_or(0.0), self.liked_n.get(b).copied().unwrap_or(0.0));
            if na > 0.0 && nb > 0.0 { p / (na * nb).sqrt() } else { 0.0 }
        };
        learned.max(taxonomy::get().prior_sim(a, b))
    }

    /// Апостериор Beta(a, b) жанра: ожидание по умолчанию сдвигается от понравившихся «соседей» (перенос вкуса).
    pub fn posterior(&self, t: &str) -> (f64, f64) {
        let mut m = self.m0;
        let mut best = 0.0f64;
        for (k, (a, b)) in &self.ev {
            if k == t || a + b < 3.0 { continue; }
            let mean = a / (a + b);
            if mean > 0.5 { best = best.max(self.sim(k, t) * (mean - 0.5)); }
        }
        m = (m + 0.6 * best).clamp(0.1, 0.85);
        let (a, b) = self.ev.get(t).copied().unwrap_or((0.0, 0.0));
        // иерархия: у жанра без своих данных ожидание тянется к ближайшему родителю с историей (в плюс и в минус)
        for p in taxonomy::get().ancestors(t) {
            if let Some((pa, pb)) = self.ev.get(p) {
                let n = pa + pb;
                if n >= 3.0 { let conf = 0.6 * n / (n + 6.0); m = (m * (1.0 - conf) + pa / n * conf).clamp(0.1, 0.85); break; }
            }
        }
        let seed = self.seed_credit(t).unwrap_or((0.0, 0.0));
        (m * S0 + a + seed.0, (1.0 - m) * S0 + b + seed.1)
    }
    pub fn mean(&self, t: &str) -> f64 { let (a, b) = self.posterior(t); a / (a + b) }

    /// Жанры, по которым уже есть данные, лучшие первыми.
    pub fn known(&self) -> Vec<String> {
        let mut v: Vec<String> = self.ev.keys().cloned().chain(self.seed.iter().cloned()).collect::<HashSet<_>>().into_iter().collect();
        v.retain(|t| taxonomy::get().node(t).map_or(true, |n| n.kind != "group" && n.kind != "lang"));
        v.sort_by(|a, b| self.mean(b).partial_cmp(&self.mean(a)).unwrap().then(a.cmp(b)));
        v
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Slot { pub tag: String, pub explore: bool }

/// План подборки методом Томпсона: для каждого слота из апостериора каждого жанра берётся случайная оценка, выигрывает наибольшая.
/// Уверенно любимые жанры выигрывают чаще, но неуверенные и незнакомые иногда выигрывают, так что исследование
/// возникает само из неопределённости и ослабевает по мере уверенности. На слот рассматривается любимое + два незнакомых жанра
/// (чаще соседних). Повтор жанра слегка штрафуется, четыре подряд запрещены.
pub fn plan(model: &Model, n: usize, rng: &mut Rng) -> Vec<Slot> {
    let tax = taxonomy::get();
    let all = tax.plannable();
    let known = model.known();
    let mut out: Vec<Slot> = vec![];
    for _ in 0..n {
        let top: Vec<&String> = known.iter().take(3).collect();
        let mut cands: Vec<String> = known.iter().filter(|t| model.evidence(t) >= 3.0 || model.seed.contains(*t)).cloned().collect();
        for _ in 0..3 {
            let near_tags: Vec<&str> = top.iter().flat_map(|t| tax.neighbors(t)).collect();
            let t = if !near_tags.is_empty() && rng.f() < 0.7 { near_tags[rng.below(near_tags.len())] } else { all[rng.below(all.len())] };
            if !cands.iter().any(|c| c == t) { cands.push(t.to_string()); }
        }
        let mut best: Option<(f64, String)> = None;
        for t in &cands {
            if out.len() >= 3 && out[out.len() - 3..].iter().all(|x| x.tag == *t) { continue; }
            let (a, b) = model.posterior(t);
            let recent = out.iter().rev().take(2).filter(|s| s.tag == *t).count() as f64;
            let score = rng.beta(a, b) / (1.0 + 0.1 * recent);
            if best.as_ref().map_or(true, |x| score > x.0) { best = Some((score, t.clone())); }
        }
        let mut tag = best.map(|x| x.1).unwrap_or_else(|| all[rng.below(all.len())].to_string());
        // спуск по иерархии: выбрано широкое семейство или жанр — иногда уточняем до поджанра, который сейчас «выигрывает» по своему апостериору
        let kids: Vec<&str> = tax.children(&tag).into_iter().filter(|k| tax.node(k).map_or(false, |n| !n.tags.is_empty())).collect();
        if !kids.is_empty() && rng.f() < 0.6 {
            let mut bk: Option<(f64, &str)> = None;
            for k in kids { let (a, b) = model.posterior(k); let sc = rng.beta(a, b); if bk.map_or(true, |x| sc > x.0) { bk = Some((sc, k)); } }
            if let Some((_, k)) = bk { tag = k.to_string(); }
        }
        // «исследование» — когда нет данных ни по самому жанру, ни по его родителям (поджанр любимого семейства — не открытие)
        let explore = model.evidence(&tag) < 3.0 && tax.ancestors(&tag).iter().all(|a| model.evidence(a) < 3.0);
        out.push(Slot { explore, tag });
    }
    out
}

/// Мягкая сортировка треков жанра: случайный порядок, слегка смещённый к привычной для человека качественности
/// (`wq`) и к исполнителям, которые ему нравятся (`wa`). Ничего не отбрасывает: далёкие от вкуса треки остаются
/// в списке и могут оказаться вверху.
pub fn rerank(
    list: &mut Vec<Track>,
    quality: &dyn Fn(&Track) -> Option<f64>,
    target: f64,
    wq: f64,
    affinity: &dyn Fn(&Track) -> f64,
    wa: f64,
    rng: &mut Rng,
) {
    let wr = (1.0 - wq - wa).max(0.3); // доля случайности не падает ниже 30%
    let mut keyed: Vec<(f64, Track)> = list.drain(..).map(|t| {
        let dist = quality(&t).map_or(0.25, |q| (q - target).abs()); // неизвестная качественность — нейтрально
        (wq * dist + wa * (1.0 - affinity(&t)) + wr * rng.f(), t)
    }).collect();
    keyed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    list.extend(keyed.into_iter().map(|x| x.1));
}

/// Ключ «одного и того же трека» в разных версиях: исполнитель плюс название без скобок и пометок
/// (official video, lyrics, audio, HD, клип и т. п.), только буквы и цифры.
pub fn track_key(t: &Track) -> String {
    fn clean(s: &str) -> String {
        let mut out = String::new();
        let mut depth = 0;
        for ch in s.chars() {
            match ch { '(' | '[' | '{' | '（' => depth += 1, ')' | ']' | '}' | '）' => { if depth > 0 { depth -= 1; } } _ if depth == 0 => out.push(ch), _ => {} }
        }
        out.to_lowercase()
    }
    let artist: String = clean(&t.artist).replace(" - topic", "").replace("vevo", "").chars().filter(|c| c.is_alphanumeric()).collect();
    let mut title = clean(&t.name);
    // «Исполнитель - Название» в самом названии: убираем префикс с исполнителем
    if let Some((a, rest)) = title.split_once(" - ") {
        let a: String = a.chars().filter(|c| c.is_alphanumeric()).collect();
        if !a.is_empty() && (a == artist || artist.contains(&a) || a.contains(&artist)) { title = rest.to_string(); }
    }
    const NOISE: [&str; 14] = ["official", "video", "lyrics", "lyric", "audio", "hd", "hq", "4k", "remastered", "клип", "официальный", "музыкальное", "версия", "visualizer"];
    let words: Vec<&str> = title.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty() && !NOISE.contains(w)).collect();
    format!("{}|{}", artist, words.concat())
}

/// Собрать плейлист по плану из найденных треков. Не берёт слышанное и треки, отправленные в бан (исполнителей и жанры не отсекает),
/// не ставит одного исполнителя ближе чем через 4 трека и больше двух раз за подборку.
pub fn compose(
    slots: &[Slot],
    pool: &HashMap<String, Vec<Track>>,
    played: &HashSet<String>,
    played_keys: &HashSet<String>,
    rated: &HashMap<String, i32>,
) -> Vec<Track> {
    let mut cursor: HashMap<&str, usize> = HashMap::new();
    let mut out: Vec<Track> = vec![];
    for slot in slots {
        let Some(list) = pool.get(&slot.tag) else { continue };
        let cur = cursor.entry(slot.tag.as_str()).or_insert(0);
        while *cur < list.len() {
            let t = &list[*cur];
            *cur += 1;
            let key = track_key(t);
            let bad = played.contains(&t.id)
                || played_keys.contains(&key)
                || rated.get(&t.id) == Some(&-2)
                || out.iter().any(|x| x.id == t.id || track_key(x) == key)
                || out.iter().filter(|x| x.artist == t.artist).count() >= 2
                || out.iter().rev().take(4).any(|x| x.artist == t.artist);
            if !bad { out.push(t.clone()); break; }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    const DAY: i64 = 86400;
    fn tg(x: &str) -> Vec<String> { vec![x.to_string()] }
    fn tr(id: &str, artist: &str) -> Track {
        Track { id: id.into(), source: "jamendo".into(), kind: "music".into(), url: "".into(), name: id.into(), artist: artist.into(),
            album: "".into(), image: "".into(), audio: "".into(), download_url: "".into(), download_allowed: true, duration: 1, tags: vec![], license_url: "".into(), pop: 0.0 }
    }
    fn rock_fan() -> Vec<Event> { (0..30).map(|i| play_event(tg("rock"), 1.0, i * DAY / 3)).collect() }
    fn in_rock(s: &Slot) -> bool { taxonomy::get().root(&s.tag) == "rock" }
    fn share(events: &[Event], mode: Mode, f: impl Fn(&Slot) -> bool) -> f64 {
        let m = Model::build(events, mode, &[]);
        let slots = plan(&m, 3000, &mut Rng::new(5));
        slots.iter().filter(|s| f(s)).count() as f64 / 3000.0
    }

    #[test]
    fn partial_listening_gives_graded_reward() {
        assert_eq!(play_event(tg("a"), 0.05, 0).reward, 0.0);
        assert_eq!(play_event(tg("a"), 0.9, 0).reward, 1.0);
        let mid = play_event(tg("a"), 0.45, 0).reward;
        assert!(mid > 0.3 && mid < 0.7);
    }
    #[test]
    fn rock_fan_gets_mostly_rock_but_not_only_rock() {
        let s = share(&rock_fan(), Mode::Balanced, in_rock);
        assert!(s > 0.55 && s < 0.95, "{s}");
    }
    #[test]
    fn exploration_is_emergent_and_responds_to_the_user() {
        // человек, которому заходят незнакомые жанры, получает больше нового, чем тот, кто их пропускает
        let mk = |reward: f64| {
            let mut e = rock_fan();
            for (i, t) in ["jazz", "folk", "ambient", "reggae", "funk", "lounge", "classical", "soul"].iter().enumerate() {
                e.push(Event { tags: tg(t), artist: String::new(), reward, weight: 1.0, age_secs: (i as i64) * DAY / 2, quality: None });
            }
            e
        };
        let open = share(&mk(1.0), Mode::Balanced, |s| !in_rock(s));
        let closed = share(&mk(0.0), Mode::Balanced, |s| !in_rock(s));
        assert!(open > closed + 0.05, "{open} {closed}");
        let m_open = Model::build(&mk(1.0), Mode::Balanced, &[]);
        let m_closed = Model::build(&mk(0.0), Mode::Balanced, &[]);
        assert!(m_open.appetite > 0.6 && m_closed.appetite < 0.4, "{} {}", m_open.appetite, m_closed.appetite);
    }
    #[test]
    fn mode_is_a_bias_not_a_fixed_rate() {
        let ev = rock_fan();
        let f = share(&ev, Mode::Familiar, |s| s.explore);
        let b = share(&ev, Mode::Balanced, |s| s.explore);
        let d = share(&ev, Mode::Discovery, |s| s.explore);
        assert!(f < b && b < d, "{f} {b} {d}");
        assert!(f > 0.0 && d < 0.8);
    }
    #[test]
    fn stale_evidence_increases_exploration() {
        let fresh = rock_fan();
        let stale: Vec<Event> = fresh.iter().map(|e| Event { age_secs: e.age_secs + 150 * DAY, ..e.clone() }).collect();
        let (f, s) = (share(&fresh, Mode::Balanced, |x| !in_rock(x)), share(&stale, Mode::Balanced, |x| !in_rock(x)));
        assert!(s > f, "{f} {s}");
    }
    #[test]
    fn heavy_listeners_forget_faster() {
        let few: Vec<Event> = (0..5).map(|i| play_event(tg("a"), 1.0, i * DAY)).collect();
        let many: Vec<Event> = (0..400).map(|i| play_event(tg("a"), 1.0, (i % 14) * DAY)).collect();
        assert!(Model::build(&many, Mode::Balanced, &[]).half_life_days < Model::build(&few, Mode::Balanced, &[]).half_life_days - 10.0);
    }
    #[test]
    fn dislike_is_moderate_not_a_block() {
        let mut ev = rock_fan();
        ev.push(rating_event(tg("rock"), -1, 0));
        let m = Model::build(&ev, Mode::Balanced, &[]);
        assert!(m.mean("rock") > 0.8, "{}", m.mean("rock"));
        // у малоизвестного жанра повторные дизлайки заметно снижают ожидание, но не обнуляют его
        let ev2 = vec![rating_event(tg("pop"), -1, 0), rating_event(tg("pop"), -1, DAY)];
        let p = Model::build(&ev2, Mode::Balanced, &[]).mean("pop");
        assert!(p < 0.3 && p > 0.05, "{p}");
    }
    #[test]
    fn taste_transfers_to_related_genres() {
        let ev: Vec<Event> = (0..12).map(|i| play_event(tg("jazz"), 1.0, i * DAY)).collect();
        let m = Model::build(&ev, Mode::Balanced, &[]);
        assert!(m.mean("blues") > m.mean("metal") + 0.05, "{} {}", m.mean("blues"), m.mean("metal"));
    }
    #[test]
    fn learns_user_specific_links() {
        // человек слушает «rap» и «classical» вместе — между ними возникает выученная связь, а у «reggae» нет
        let mut ev: Vec<Event> = (0..15).map(|i| Event { tags: vec!["hiphop".into(), "classical".into()], artist: String::new(), reward: 1.0, weight: 1.0, age_secs: i * DAY, quality: None }).collect();
        ev.extend((0..15).map(|i| play_event(tg("hiphop"), 1.0, i * DAY)));
        let m = Model::build(&ev, Mode::Balanced, &[]);
        assert!(m.sim("hiphop", "classical") > 0.4);
        assert_eq!(m.sim("hiphop", "reggae"), taxonomy::get().prior_sim("hiphop", "reggae"));
    }
    #[test]
    fn seed_tastes_start_the_model_and_fade_into_history() {
        let seed = vec!["hiphop".to_string()];
        let m = Model::build(&[], Mode::Balanced, &seed);
        assert!(m.mean("hiphop") > m.mean("metal"));
        assert_eq!(plan(&m, 50, &mut Rng::new(2)).len(), 50);
        // реальная история против сида: после многих пропусков rap его ожидание падает
        let ev: Vec<Event> = (0..20).map(|i| play_event(tg("hiphop"), 0.02, i * DAY)).collect();
        assert!(Model::build(&ev, Mode::Balanced, &seed).mean("hiphop") < m.mean("hiphop") - 0.2);
    }
    #[test]
    fn no_long_genre_runs_and_empty_history_works() {
        for ev in [rock_fan(), vec![]] {
            let m = Model::build(&ev, Mode::Familiar, &[]);
            let slots = plan(&m, 400, &mut Rng::new(9));
            assert!(slots.windows(4).all(|w| !(w[0].tag == w[1].tag && w[1].tag == w[2].tag && w[2].tag == w[3].tag)));
            assert_eq!(slots.len(), 400);
        }
    }
    #[test]
    fn quality_preference_is_learned_and_bounded() {
        let mk = |q: f64| -> Vec<Event> { (0..40).map(|i| play_event(tg("rock"), 1.0, i * DAY / 4).with_quality(Some(q))).collect() };
        let top = Model::build(&mk(0.9), Mode::Balanced, &[]);
        let low = Model::build(&mk(0.15), Mode::Balanced, &[]);
        let none = Model::build(&rock_fan(), Mode::Balanced, &[]);
        assert!(top.quality_target().0 > 0.75 && low.quality_target().0 < 0.3);
        assert_eq!(none.quality_target().0, 0.5);
        assert!(top.quality_target().1 <= 0.35 && none.quality_target().1 == 0.0);
        // мало данных — смещение слабое
        let few = Model::build(&[play_event(tg("rock"), 1.0, 0).with_quality(Some(1.0))], Mode::Balanced, &[]);
        assert!(few.quality_target().0 < 0.65 && few.quality_target().1 < 0.1);
        // пропущенное не считается «любимым качеством»
        let skipped: Vec<Event> = (0..40).map(|i| play_event(tg("rock"), 0.0, i * DAY / 4).with_quality(Some(1.0))).collect();
        assert_eq!(Model::build(&skipped, Mode::Balanced, &[]).quality_target().0, 0.5);
    }
    #[test]
    fn rerank_leans_toward_taste_without_dropping_anything() {
        let tracks: Vec<Track> = (0..100).map(|i| tr(&i.to_string(), "A")).collect();
        let q = |t: &Track| Some(t.id.parse::<f64>().unwrap() / 99.0);
        let avg_top = |target: f64| {
            let mut l = tracks.clone();
            rerank(&mut l, &q, target, 0.35, &|_| 0.5, 0.0, &mut Rng::new(4));
            assert_eq!(l.len(), 100);
            l.iter().take(20).map(|t| q(t).unwrap()).sum::<f64>() / 20.0
        };
        let (hi, lo) = (avg_top(0.95), avg_top(0.05));
        assert!(hi > lo + 0.15, "{hi} {lo}");
        // мягко: даже при высоком вкусе в верхушке бывают и непопулярные
        let mut l = tracks.clone();
        rerank(&mut l, &q, 0.95, 0.35, &|_| 0.5, 0.0, &mut Rng::new(4));
        assert!(l.iter().take(30).any(|t| q(t).unwrap() < 0.4));
    }
    #[test]
    fn dislike_nudges_artist_and_similar_music_but_never_bans() {
        let rock: Vec<Event> = (0..20).map(|i| play_event(tg("rock"), 1.0, i * DAY / 2).with_artist("Other")).collect();
        let base = Model::build(&rock, Mode::Balanced, &[]);
        let neutral = base.artist_affinity("X", &tg("rock"));
        let mut ev = rock.clone();
        ev.push(rating_event(tg("rock"), -1, 0).with_artist("X"));
        let m1 = Model::build(&ev, Mode::Balanced, &[]);
        let one = m1.artist_affinity("X", &tg("rock"));
        assert!(one < neutral - 0.05 && one > neutral - 0.35, "{neutral} {one}");
        // другие исполнители и жанр практически не пострадали
        assert!(m1.artist_affinity("Other", &tg("rock")) > neutral - 0.02);
        assert!(m1.mean("rock") > base.mean("rock") - 0.06 && m1.mean("rock") < base.mean("rock"));
        // много дизлайков: оценка падает, но не обнуляется
        for i in 0..8 { ev.push(rating_event(tg("rock"), -1, i * DAY / 4).with_artist("X")); }
        let many = Model::build(&ev, Mode::Balanced, &[]).artist_affinity("X", &tg("rock"));
        assert!(many < one && many > 0.05, "{many}");
        // лайк сдвигает вверх (проверяем там, где есть куда расти — без истории оценка нейтральная)
        let empty = Model::build(&[], Mode::Balanced, &[]).artist_affinity("X", &tg("rock"));
        let liked = Model::build(&[rating_event(tg("rock"), 1, 0).with_artist("X")], Mode::Balanced, &[]);
        assert!(liked.artist_affinity("X", &tg("rock")) > empty + 0.15);
    }
    #[test]
    fn disliked_artist_stays_in_the_pool_just_lower() {
        let mut ev: Vec<Event> = (0..20).map(|i| play_event(tg("rock"), 1.0, i * DAY / 2).with_artist("Fav")).collect();
        for i in 0..3 { ev.push(rating_event(tg("rock"), -1, i * DAY).with_artist("Meh")); }
        let m = Model::build(&ev, Mode::Balanced, &[]);
        let tracks: Vec<Track> = (0..60).map(|i| tr(&i.to_string(), if i % 2 == 0 { "Fav" } else { "Meh" })).collect();
        let mut l = tracks.clone();
        let aff = |t: &Track| m.artist_affinity(&t.artist, &tg("rock"));
        rerank(&mut l, &|_| None, 0.5, 0.0, &aff, m.artist_weight(), &mut Rng::new(8));
        assert_eq!(l.len(), 60);
        let fav_top = l.iter().take(20).filter(|t| t.artist == "Fav").count();
        assert!(fav_top > 10 && fav_top < 20, "{fav_top}"); // перевес есть, но Meh остаётся и в верхушке
        assert!(m.artist_weight() > 0.0 && m.artist_weight() <= 0.35);
    }
    #[test]
    fn beta_sampler_has_correct_mean() {
        let mut r = Rng::new(1);
        let mean = (0..20000).map(|_| r.beta(2.0, 6.0)).sum::<f64>() / 20000.0;
        assert!((mean - 0.25).abs() < 0.01, "{mean}");
    }
    #[test]
    fn compose_respects_artist_and_filters() {
        let mut pool = HashMap::new();
        pool.insert("rock".to_string(), vec![tr("1", "A"), tr("2", "A"), tr("3", "B"), tr("4", "A"), tr("5", "A"), tr("6", "C"), tr("7", "D")]);
        let slots: Vec<Slot> = (0..7).map(|_| Slot { tag: "rock".into(), explore: false }).collect();
        let played: HashSet<String> = ["3".to_string()].into_iter().collect();
        let mut rated = HashMap::new();
        rated.insert("6".to_string(), -2);
        let out = compose(&slots, &pool, &played, &HashSet::new(), &rated);
        let ids: Vec<_> = out.iter().map(|t| t.id.as_str()).collect();
        assert!(!ids.contains(&"3") && !ids.contains(&"6"), "{ids:?}");
        assert!(out.iter().filter(|t| t.artist == "A").count() <= 2);
        assert!(out.windows(2).all(|w| w[0].artist != w[1].artist));
    }

    #[test]
    fn versions_of_one_track_are_deduplicated() {
        let mut a = tr("1", "Кино"); a.name = "Группа крови (Official Video)".into();
        let mut b = tr("2", "Кино - Topic"); b.name = "Кино - Группа крови [Lyrics]".into();
        let mut c = tr("3", "Кино"); c.name = "Звезда по имени Солнце".into();
        assert_eq!(track_key(&a), track_key(&b));
        assert_ne!(track_key(&a), track_key(&c));
        let mut pool = HashMap::new();
        pool.insert("rock".to_string(), vec![a.clone(), b.clone(), c.clone()]);
        let slots: Vec<Slot> = (0..3).map(|_| Slot { tag: "rock".into(), explore: false }).collect();
        let played_keys: HashSet<String> = [track_key(&a)].into_iter().collect();
        let out = compose(&slots, &pool, &HashSet::new(), &played_keys, &HashMap::new());
        assert_eq!(out.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), vec!["3"]);
    }
    #[test]
    fn instant_skips_weigh_more_than_plays() {
        assert!(play_event(tg("a"), 0.02, 0).weight > play_event(tg("a"), 0.9, 0).weight);
    }

    #[test]
    fn subgenre_listening_lifts_parents_and_siblings_without_data() {
        let ev: Vec<Event> = (0..20).map(|i| play_event(tg("deephouse"), 1.0, i * DAY / 2)).collect();
        let m = Model::build(&ev, Mode::Balanced, &[]);
        assert!(m.mean("deep_house") > m.mean("house") - 0.02, "лист не слабее родителя");
        assert!(m.mean("house") > m.mean("death_metal") + 0.15);
        // у соседнего поджанра данных нет, но он получает перенос через родителя
        assert!(m.mean("tech_house") > m.mean("death_metal") + 0.1, "{} {}", m.mean("tech_house"), m.mean("death_metal"));
        assert!(m.evidence("tech_house") == 0.0);
    }
    #[test]
    fn rare_subgenre_inherits_parent_dislike_softly() {
        let ev: Vec<Event> = (0..20).map(|i| play_event(tg("trap"), 0.0, i * DAY / 2)).collect();
        let m = Model::build(&ev, Mode::Balanced, &[]);
        let base = Model::build(&[], Mode::Balanced, &[]).mean("drill");
        assert!(m.mean("drill") < base, "{} {base}", m.mean("drill"));
        assert!(m.mean("drill") > 0.1, "не бан");
    }
    #[test]
    fn plan_dives_into_subgenres_of_a_chosen_family() {
        let m = Model::build(&[], Mode::Balanced, &["rock".to_string()]);
        let slots = plan(&m, 600, &mut Rng::new(3));
        let tax = taxonomy::get();
        let sub = slots.iter().filter(|s| tax.ancestors(&s.tag).contains(&"rock")).count();
        let fam = slots.iter().filter(|s| s.tag == "rock").count();
        assert!(sub > 30 && fam > 30, "{sub} {fam}");
        let distinct: HashSet<&str> = slots.iter().map(|s| s.tag.as_str()).collect();
        assert!(distinct.len() > 25, "много разных жанров: {}", distinct.len());
    }
    #[test]
    fn favorite_artists_give_a_soft_start_that_history_overrides() {
        let fav = vec![SeedArtist { name: "Massive Attack".into(), tags: vec!["trip hop".into()] }, SeedArtist { name: "Fugees".into(), tags: vec![] }];
        let none = Model::build(&[], Mode::Balanced, &[]);
        let m = Model::build(&[], Mode::Balanced, &[]).with_seed_artists(&fav);
        let tags = tg("triphop");
        assert!(m.artist_affinity("massive attack", &tags) > none.artist_affinity("massive attack", &tags) + 0.2);
        assert!(m.mean("trip_hop") > none.mean("trip_hop") + 0.1);
        assert!(m.artist_affinity("Fugees", &[]) > 0.5);
        // мягко: чужой исполнитель не страдает, а реальные пропуски быстро перебивают стартовый плюс
        assert!((m.artist_affinity("someone", &tags) - none.artist_affinity("someone", &tags)).abs() < 0.2);
        let skips: Vec<Event> = (0..12).map(|i| play_event(tags.clone(), 0.0, i * DAY / 2).with_artist("Massive Attack")).collect();
        let m2 = Model::build(&skips, Mode::Balanced, &[]).with_seed_artists(&fav);
        assert!(m2.artist_affinity("Massive Attack", &tags) < m.artist_affinity("Massive Attack", &tags) - 0.2);
    }
}
