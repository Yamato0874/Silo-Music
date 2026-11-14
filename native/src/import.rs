//! Импорт плейлистов из файлов и текста (то же, что src/Import.tsx): M3U/M3U8, PLS, CSV, JSON, список строк, ссылки.
use silo_core::model::Track;

#[derive(Clone, Debug)]
pub struct Wanted { pub artist: String, pub title: String, pub raw: String }

fn clean(s: &str) -> String { s.split_whitespace().collect::<Vec<_>>().join(" ") }

fn split_sep(s: &str) -> Vec<String> {
    // « - », « – », « — » с пробелами вокруг
    let mut parts = vec![];
    let mut cur = String::new();
    let ch: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < ch.len() {
        if ch[i].is_whitespace() && i + 2 < ch.len() && ['-', '–', '—'].contains(&ch[i + 1]) && ch[i + 2].is_whitespace() {
            parts.push(cur.trim().to_string()); cur.clear(); i += 3; continue;
        }
        cur.push(ch[i]); i += 1;
    }
    parts.push(cur.trim().to_string());
    parts
}
fn from_line(line: &str) -> Option<Wanted> {
    let s = clean(line);
    if s.is_empty() { return None; }
    let p = split_sep(&s);
    Some(if p.len() >= 2 { Wanted { artist: p[0].clone(), title: p[1..].join(" - "), raw: s } } else { Wanted { artist: String::new(), title: s.clone(), raw: s } })
}

fn csv(text: &str) -> Vec<Vec<String>> {
    let first = text.lines().next().unwrap_or("");
    let d = if first.matches(';').count() > first.matches(',').count() { ';' } else { ',' };
    let (mut rows, mut row, mut cell, mut q) = (vec![], vec![], String::new(), false);
    let ch: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < ch.len() {
        let c = ch[i];
        if q { if c == '"' { if ch.get(i + 1) == Some(&'"') { cell.push('"'); i += 1; } else { q = false; } } else { cell.push(c); } }
        else if c == '"' { q = true; }
        else if c == d { row.push(std::mem::take(&mut cell)); }
        else if c == '\n' || c == '\r' { if c == '\r' && ch.get(i + 1) == Some(&'\n') { i += 1; } row.push(std::mem::take(&mut cell)); rows.push(std::mem::take(&mut row)); }
        else { cell.push(c); }
        i += 1;
    }
    if !cell.is_empty() || !row.is_empty() { row.push(cell); rows.push(row); }
    rows.into_iter().filter(|r: &Vec<String>| r.iter().any(|x| !x.trim().is_empty())).collect()
}

fn walk_json(x: &serde_json::Value, out: &mut Vec<Wanted>) {
    use serde_json::Value as V;
    match x {
        V::Array(a) => a.iter().for_each(|y| walk_json(y, out)),
        V::String(s) => { if let Some(w) = from_line(s) { out.push(w); } }
        V::Object(o) => {
            let title = ["title", "trackName", "track_name", "name"].iter().find_map(|k| o.get(*k).and_then(|v| v.as_str()))
                .or_else(|| o.get("track").and_then(|v| v.as_str()));
            let artist = ["artist", "artistName", "artist_name", "artists", "author"].iter().find_map(|k| o.get(*k)).map(|a| match a {
                V::String(s) => Some(s.clone()),
                V::Array(v) => Some(v.iter().filter_map(|x| x.as_str().map(String::from).or_else(|| x["name"].as_str().map(String::from))).collect::<Vec<_>>().join(", ")),
                V::Object(_) => a["name"].as_str().map(String::from),
                _ => None,
            }).flatten();
            if let Some(t) = title {
                if artist.is_some() || o.contains_key("duration") || o.contains_key("album") {
                    let a = clean(&artist.unwrap_or_default());
                    out.push(Wanted { raw: clean(&format!("{a} - {t}")), artist: a, title: clean(t) });
                    return;
                }
            }
            o.values().for_each(|v| walk_json(v, out));
        }
        _ => {}
    }
}

/// Разбор файла/текста: желаемые треки и ссылки на плейлисты.
pub fn parse(name: &str, text: &str) -> (Vec<Wanted>, Vec<String>) {
    let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
    let body = text.trim_start_matches('\u{feff}');
    let (mut wanted, mut urls) = (vec![], vec![]);
    if ext == "json" || body.trim_start().starts_with(['[', '{']) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) { walk_json(&v, &mut wanted); return (wanted, urls); }
    }
    let first = body.lines().next().unwrap_or("").to_lowercase();
    if ext == "csv" || (ext == "txt" && first.contains(',') && ["artist", "исполнитель", "track", "title"].iter().any(|k| first.contains(k))) {
        let rows = csv(body);
        if rows.is_empty() { return (wanted, urls); }
        let head: Vec<String> = rows[0].iter().map(|h| h.trim().to_lowercase()).collect();
        let find = |names: &[&str]| head.iter().position(|h| names.iter().any(|n| h == n || h.starts_with(n)));
        let mut ti = find(&["track name", "track", "title", "song", "name", "название", "трек", "песня"]);
        let mut ai = find(&["artist name", "artist", "artists", "исполнитель", "автор"]);
        let has_head = ti.is_some() || ai.is_some();
        if !has_head { ai = Some(0); ti = Some(if rows[0].len() > 1 { 1 } else { 0 }); }
        let ti = ti.unwrap_or(0);
        for r in rows.iter().skip(if has_head { 1 } else { 0 }) {
            let title = clean(r.get(ti).map(|x| x.as_str()).unwrap_or(""));
            let artist = match ai { Some(a) if a != ti => clean(r.get(a).map(|x| x.as_str()).unwrap_or("").split([';', ',']).next().unwrap_or("")), _ => String::new() };
            if !title.is_empty() { wanted.push(Wanted { raw: clean(&format!("{artist} - {title}")), artist, title }); }
        }
        return (wanted, urls);
    }
    if ext == "pls" {
        for l in body.lines() {
            let l = l.trim();
            if l.len() > 5 && l[..5].eq_ignore_ascii_case("title") { if let Some((_, v)) = l.split_once('=') { if let Some(w) = from_line(v) { wanted.push(w); } } }
        }
        return (wanted, urls);
    }
    let mut pending: Option<String> = None;
    for l in body.lines() {
        let s = l.trim();
        if s.is_empty() { continue; }
        if s.starts_with("#EXTINF") { pending = s.split_once(',').map(|x| x.1.to_string()); continue; }
        if s.starts_with('#') { continue; }
        let is_url = s.starts_with("http://") || s.starts_with("https://");
        if is_url { urls.push(s.to_string()); if pending.is_none() { continue; } }
        let line = match pending.take() {
            Some(p) => p,
            None => {
                let file = s.rsplit(['\\', '/']).next().unwrap_or(s);
                let file = match file.rfind('.') { Some(i) if file.len() - i <= 5 => &file[..i], _ => file };
                file.trim_start_matches(|c: char| c.is_ascii_digit()).trim_start_matches([' ', '.', '_', '-']).to_string()
            }
        };
        if is_url { urls.pop(); }
        if let Some(w) = from_line(&line) { wanted.push(w); }
    }
    (wanted, urls)
}

fn norm(s: &str) -> String {
    let mut t = s.to_lowercase();
    // убрать скобки
    let mut out = String::new();
    let mut depth = 0;
    for c in t.chars() { match c { '(' | '[' => depth += 1, ')' | ']' => depth = (depth - 1).max(0), _ if depth == 0 => out.push(c), _ => {} } }
    t = out;
    for w in [" feat", " ft.", " ft ", " featuring", " official", " video", " audio", " lyrics", " remastered"] { if let Some(i) = t.find(w) { t.truncate(i); } }
    t.chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}
fn toks(s: &str) -> std::collections::HashSet<String> { norm(s).split(' ').filter(|x| !x.is_empty()).map(String::from).collect() }
fn overlap(a: &std::collections::HashSet<String>, b: &std::collections::HashSet<String>) -> f64 {
    if a.is_empty() { 0.0 } else { a.iter().filter(|x| b.contains(*x)).count() as f64 / a.len() as f64 }
}
/// Насколько найденный трек похож на желаемый (0..1).
pub fn score(w: &Wanted, tr: &Track) -> f64 {
    let (tt, at) = (toks(&w.title), toks(&w.artist));
    let title = overlap(&tt, &toks(&tr.name));
    let all = toks(&format!("{} {}", tr.name, tr.artist));
    let artist = if at.is_empty() { 0.6 } else { overlap(&at, &toks(&tr.artist)).max(overlap(&at, &all)) };
    0.65 * title + 0.35 * artist
}

/// Плейлист в M3U8.
pub fn m3u(list: &[Track]) -> String {
    let mut s = String::from("#EXTM3U\n");
    for x in list {
        let d = if x.duration > 0 { x.duration as i64 } else { -1 };
        s += &format!("#EXTINF:{d},{} - {}\n{}\n", x.artist, x.name, if x.source == "local" { &x.audio } else { &x.url });
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_text_lines_and_urls() {
        let (w, u) = parse("paste.txt", "Кино - Группа крови\nLinkin Park – Numb\nhttps://www.youtube.com/playlist?list=PL123\nПросто название");
        assert_eq!(w.len(), 3);
        assert_eq!((w[0].artist.as_str(), w[0].title.as_str()), ("Кино", "Группа крови"));
        assert_eq!((w[1].artist.as_str(), w[1].title.as_str()), ("Linkin Park", "Numb"));
        assert_eq!((w[2].artist.as_str(), w[2].title.as_str()), ("", "Просто название"));
        assert_eq!(u, vec!["https://www.youtube.com/playlist?list=PL123".to_string()]);
    }
    #[test]
    fn parses_m3u_pls_csv_json() {
        let (w, _) = parse("a.m3u8", "#EXTM3U\n#EXTINF:180,Земфира - Искала\nC:\\Music\\01 - x.mp3\n#EXTINF:200,Ария - Беспечный ангел\nhttps://youtu.be/abc\n");
        assert_eq!(w.len(), 2);
        assert_eq!((w[0].artist.as_str(), w[0].title.as_str()), ("Земфира", "Искала"));
        let (w, _) = parse("a.pls", "[playlist]\nFile1=x\nTitle1=Metallica - One\nTitle2=Pink Floyd - Time\n");
        assert_eq!(w.len(), 2);
        let (w, _) = parse("a.csv", "Track Name,Artist Name,Album\n\"Numb\",\"Linkin Park\",Meteora\nHero;x,Skillet,Awake\n");
        assert_eq!((w[0].artist.as_str(), w[0].title.as_str()), ("Linkin Park", "Numb"));
        assert_eq!(w.len(), 2);
        let (w, _) = parse("a.json", r#"{"items":[{"track":{"name":"Believer","artists":[{"name":"Imagine Dragons"}]}},{"title":"Numb","artist":"Linkin Park"}]}"#);
        assert_eq!(w.len(), 2);
        assert_eq!((w[0].artist.as_str(), w[0].title.as_str()), ("Imagine Dragons", "Believer"));
    }
    #[test]
    fn score_prefers_the_right_track() {
        let w = Wanted { artist: "Linkin Park".into(), title: "Numb".into(), raw: "Linkin Park - Numb".into() };
        let mut good = silo_core::model::Track::stub("youtube:a", vec![]);
        good.name = "Numb (Official Video)".into(); good.artist = "Linkin Park".into();
        let mut bad = silo_core::model::Track::stub("youtube:b", vec![]);
        bad.name = "Top 10 songs".into(); bad.artist = "Charts".into();
        assert!(score(&w, &good) > 0.6 && score(&w, &bad) < 0.3, "{} {}", score(&w, &good), score(&w, &bad));
    }
    #[test]
    fn m3u_export_roundtrip() {
        let mut t = silo_core::model::Track::stub("youtube:a", vec![]);
        t.name = "Numb".into(); t.artist = "Linkin Park".into(); t.duration = 185; t.url = "https://www.youtube.com/watch?v=a".into();
        let m = m3u(&[t]);
        let (w, urls) = parse("x.m3u8", &m);
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].title, "Numb");
        assert!(urls.is_empty() || urls[0].contains("watch?v=a"));
    }
}
