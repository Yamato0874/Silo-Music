//! Темы (те же 7 палитр, что в React-версии) и линейные иконки из тех же SVG-путей.
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};

#[derive(Clone, Copy)]
pub struct Pal {
    pub dark: bool,
    pub bg: Color32,
    pub panel: Color32,
    pub line: Color32,
    pub fg: Color32,
    pub mut_: Color32,
    pub acc: Color32,
    pub on_acc: Color32,
}

fn hex(s: &str) -> Color32 {
    let v = u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap_or(0);
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

/// id, ru, en, тёмная, bg, panel, line, fg, mut, acc, on-acc
pub const THEMES: [(&str, &str, &str, bool, &str, &str, &str, &str, &str, &str, &str); 7] = [
    ("sage", "Шалфей", "Sage", false, "#f3efe3", "#fbf8ef", "#ddd6c0", "#26301f", "#566049", "#3f6b3a", "#ffffff"),
    ("mint", "Графит и мята", "Graphite & mint", true, "#14171a", "#1c2024", "#2c3238", "#e6ecea", "#93a09b", "#3ddc97", "#08130e"),
    ("amber", "Тёплая ночь", "Warm night", true, "#1a1612", "#231d17", "#382e24", "#f1e6d6", "#a8987f", "#f0a830", "#1a1100"),
    ("paper", "Бумага", "Paper", false, "#fafaf7", "#ffffff", "#e3e1d8", "#1e1e1c", "#5f5f55", "#b04a22", "#ffffff"),
    ("ocean", "Глубокий синий", "Deep blue", true, "#0c1424", "#131d33", "#22304d", "#e4ecfa", "#90a1c0", "#5aa2ff", "#04101f"),
    ("forest", "Лес", "Forest", true, "#0f1a14", "#16241b", "#24382b", "#e3efe6", "#8fa898", "#8bd17c", "#08150b"),
    ("mono", "Серый минимализм", "Mono gray", true, "#1e1e20", "#26262a", "#38383d", "#ececec", "#9a9aa2", "#d0d0d8", "#111111"),
];

impl Pal {
    pub fn get(id: &str) -> Pal {
        let t = THEMES.iter().find(|t| t.0 == id).unwrap_or(&THEMES[2]);
        Pal { dark: t.3, bg: hex(t.4), panel: hex(t.5), line: hex(t.6), fg: hex(t.7), mut_: hex(t.8), acc: hex(t.9), on_acc: hex(t.10) }
    }
    pub fn hover(&self) -> Color32 { mix(self.panel, self.fg, 0.07) }
    pub fn press(&self) -> Color32 { mix(self.panel, self.fg, 0.13) }
    pub fn acc_soft(&self) -> Color32 { mix(self.bg, self.acc, 0.16) }
    pub fn raise(&self) -> Color32 { mix(self.panel, self.fg, 0.04) }
    pub fn sunk(&self) -> Color32 { mix(self.panel, self.bg, 0.6) }
    pub fn danger(&self) -> Color32 { Color32::from_rgb(0xe5, 0x48, 0x4d) }
    pub fn ok(&self) -> Color32 { Color32::from_rgb(0x3f, 0xb2, 0x7f) }

    pub fn apply(&self, ctx: &egui::Context) {
        let p = *self;
        ctx.all_styles_mut(|s| {
            let mut v = if p.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
            v.panel_fill = p.bg;
            v.window_fill = p.panel;
            v.extreme_bg_color = p.sunk();
            v.faint_bg_color = p.raise();
            v.window_stroke = Stroke::new(1.0, p.line);
            v.override_text_color = Some(p.fg);
            v.hyperlink_color = p.acc;
            v.selection.bg_fill = mix(p.bg, p.acc, 0.35);
            v.selection.stroke = Stroke::new(1.0, p.fg);
            v.window_corner_radius = 14.into();
            v.menu_corner_radius = 10.into();
            v.popup_shadow = egui::epaint::Shadow { offset: [0, 6], blur: 24, spread: 0, color: Color32::from_black_alpha(if p.dark { 110 } else { 40 }) };
            v.window_shadow = egui::epaint::Shadow { offset: [0, 10], blur: 40, spread: 0, color: Color32::from_black_alpha(if p.dark { 140 } else { 50 }) };
            for (w, fill) in [(&mut v.widgets.noninteractive, p.panel), (&mut v.widgets.inactive, p.raise()), (&mut v.widgets.hovered, p.hover()), (&mut v.widgets.active, p.press()), (&mut v.widgets.open, p.hover())] {
                w.bg_fill = fill;
                w.weak_bg_fill = fill;
                w.bg_stroke = Stroke::new(1.0, p.line);
                w.fg_stroke = Stroke::new(1.5, p.fg);
                w.corner_radius = 8.into();
            }
            v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.line);
            v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.fg);
            v.widgets.hovered.bg_stroke = Stroke::new(1.0, mix(p.line, p.acc, 0.5));
            v.text_edit_bg_color = Some(p.sunk());
            s.visuals = v;
            s.spacing.item_spacing = Vec2::new(8.0, 6.0);
            s.spacing.button_padding = Vec2::new(12.0, 6.0);
            s.spacing.interact_size.y = 30.0;
            s.spacing.scroll.bar_width = 8.0;
            s.spacing.scroll.floating = true;
            s.interaction.selectable_labels = false;
            use egui::{FontFamily, FontId, TextStyle::*};
            s.text_styles = [
                (Small, FontId::new(12.0, FontFamily::Proportional)),
                (Body, FontId::new(14.0, FontFamily::Proportional)),
                (Button, FontId::new(14.0, FontFamily::Proportional)),
                (Heading, FontId::new(26.0, egui::FontFamily::Name("bold".into()))),
                (egui::TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
            ].into();
        });
    }
}

/// Шрифты: Segoe UI из Windows (кириллица, как в React-версии), запасной — встроенный.
pub fn fonts(ctx: &egui::Context) {
    let mut f = egui::FontDefinitions::default();
    let dir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into()) + "\\Fonts\\";
    let mut add = |key: &str, files: &[&str], fam: Option<&str>| {
        for file in files {
            if let Ok(bytes) = std::fs::read(format!("{dir}{file}")) {
                f.font_data.insert(key.into(), egui::FontData::from_owned(bytes).into());
                match fam {
                    None => { f.families.entry(egui::FontFamily::Proportional).or_default().insert(0, key.into()); }
                    Some(n) => { f.families.entry(egui::FontFamily::Name(n.into())).or_default().push(key.into()); }
                }
                return true;
            }
        }
        false
    };
    add("ui", &["segoeui.ttf"], None);
    let bold = add("ui-bold", &["seguisb.ttf", "segoeuib.ttf"], Some("bold"));
    add("sym", &["seguisym.ttf"], None).then(|| ());
    // запасные глифы для жирного семейства
    let prop = f.families.get(&egui::FontFamily::Proportional).cloned().unwrap_or_default();
    let b = f.families.entry(egui::FontFamily::Name("bold".into())).or_default();
    if !bold { b.extend(prop.iter().cloned()); } else { b.extend(prop.iter().cloned()); }
    // символы (♪, стрелки) после основного шрифта
    if let Some(v) = f.families.get_mut(&egui::FontFamily::Proportional) { if let Some(i) = v.iter().position(|x| x == "sym") { let s = v.remove(i); v.insert(1.min(v.len()), s); } }
    ctx.set_fonts(f);
}
pub fn bold(size: f32) -> egui::FontId { egui::FontId::new(size, egui::FontFamily::Name("bold".into())) }

// ---------- иконки: мини-разбор SVG path (M L H V C S Q A Z, абсолютные и относительные) ----------

pub fn icon_path(name: &str) -> &'static str {
    match name {
        "play" => "M7 4.5v15l12-7.5z",
        "pause" => "M7 4h3.5v16H7zM13.5 4H17v16h-3.5z",
        "prev" => "M18 5.5v13L9 12zM6 5v14",
        "next" => "M6 5.5v13l9-6.5zM18 5v14",
        "shuffle" => "M16 3h5v5M4 20 21 3M21 16v5h-5M15 15l6 6M4 4l5 5",
        "repeat" => "M17 2l4 4-4 4M3 11V10a4 4 0 0 1 4-4h14M7 22l-4-4 4-4M21 13v1a4 4 0 0 1-4 4H3",
        "repeatOne" => "M17 2l4 4-4 4M3 11V10a4 4 0 0 1 4-4h14M7 22l-4-4 4-4M21 13v1a4 4 0 0 1-4 4H3M11 10h1.5v5",
        "queue" => "M3 6h18M3 12h18M3 18h11",
        "more" => "M5 12h.01M12 12h.01M19 12h.01",
        "like" => "M7 10v11H3V10zM7 10l4-8a3 3 0 0 1 3 3v4h5.6a2 2 0 0 1 2 2.3l-1.4 8a2 2 0 0 1-2 1.7H7",
        "ban" => "M21 12a9 9 0 1 1-18 0a9 9 0 0 1 18 0zM5.6 5.6l12.8 12.8",
        "dislike" => "M17 14V3h4v11zM17 14l-4 8a3 3 0 0 1-3-3v-4H4.4a2 2 0 0 1-2-2.3l1.4-8a2 2 0 0 1 2-1.7H17",
        "download" => "M12 3v12M7 10l5 5 5-5M5 21h14",
        "close" => "M6 6l12 12M18 6 6 18",
        "volume" => "M4 9v6h4l5 4V5L8 9zM16.5 8.5a5 5 0 0 1 0 7",
        "volumeLow" => "M4 9v6h4l5 4V5L8 9z",
        "mute" => "M4 9v6h4l5 4V5L8 9zM17 9l5 6M22 9l-5 6",
        "back" => "M15 18l-6-6 6-6",
        "chevR" => "M9 6l6 6-6 6",
        "chevD" => "M6 9l6 6 6-6",
        "playNext" => "M5 4v7a4 4 0 0 0 4 4h11M15 10l5 5-5 5",
        "addQueue" => "M3 6h13M3 12h13M3 18h8M18 14v8M14 18h8",
        "save" => "M6 3h12v18l-6-4-6 4z",
        "trash" => "M4 7h16M9 7V4h6v3M6 7l1 14h10l1-14",
        "home" => "M3 11l9-8 9 8M5 10v10h5v-6h4v6h5V10",
        "search" => "M11 19a8 8 0 1 1 0-16a8 8 0 0 1 0 16zM21 21l-4.3-4.3",
        "browse" => "M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z",
        "echo" => "M2 12h.01M6 8v8M10 4v16M14 7v10M18 9v6M22 12h.01",
        "history" => "M3 12a9 9 0 1 0 3-6.7M3 4v5h5M12 7v5l3 2",
        "playlist" => "M3 6h14M3 12h10M3 18h8M18 10v8.5a2.3 2.3 0 1 1-1.5-2.2M18 10l3-1",
        "import" => "M4 14v6h16v-6M12 15V3M8 7l4-4 4 4",
        "settings" => "M4 6h10M18 6h2M4 12h4M12 12h8M4 18h12M20 18h.01M16 4v4M10 10v4M18 16v4",
        "heart" => "M12 20.5S3.5 15.5 3.5 9.2A4.7 4.7 0 0 1 12 6.6a4.7 4.7 0 0 1 8.5 2.6c0 6.3-8.5 11.3-8.5 11.3z",
        "lyrics" => "M5 5h14v10H9l-4 4zM9 9h6M9 12h4",
        "mini" => "M3 5h18v14H3zM12 12h7v5h-7z",
        "expand" => "M4 14v6h6M20 10V4h-6M4 20l7-7M20 4l-7 7",
        "speed" => "M4.5 18a9 9 0 1 1 15 0M12 13l4-5",
        "check" => "M5 12.5l4.5 4.5L19 7.5",
        "info" => "M12 21a9 9 0 1 0 0-18a9 9 0 0 0 0 18zM12 11v6M12 7.5h.01",
        "alert" => "M12 3l10 18H2zM12 10v5M12 18h.01",
        "plus" => "M12 5v14M5 12h14",
        "edit" => "M4 20h4L19 9l-4-4L4 16zM13.5 6.5l4 4",
        "copy" => "M9 9h11v11H9zM5 15H4V4h11v1",
        "refresh" => "M20 11a8 8 0 0 0-14.3-4.3L4 8M4 4v4h4M4 13a8 8 0 0 0 14.3 4.3L20 16M20 20v-4h-4",
        "folder" => "M3 6h6l2 2h10v11H3z",
        "grip" => "M9 6h.01M15 6h.01M9 12h.01M15 12h.01M9 18h.01M15 18h.01",
        "radio" => "M12 13a1 1 0 1 0 0-2a1 1 0 0 0 0 2zM7.8 16.2a6 6 0 0 1 0-8.4M16.2 7.8a6 6 0 0 1 0 8.4M4.9 19.1a10 10 0 0 1 0-14.2M19.1 4.9a10 10 0 0 1 0 14.2",
        "user" => "M12 12a4 4 0 1 0 0-8a4 4 0 0 0 0 8zM4 21a8 8 0 0 1 16 0",
        "video" => "M3 6h13v12H3zM16 10l5-3v10l-5-3",
        "wifiOff" => "M2 2l20 20M12 20h.01M8.5 16a5 5 0 0 1 7 0M5 12.5a10 10 0 0 1 3-2M2 8.5a15 15 0 0 1 4-2.7M10 5.1A15 15 0 0 1 22 8.5",
        "music" => "M9 18V5l11-2v13M9 18a3 3 0 1 1-6 0a3 3 0 0 1 6 0zM20 16a3 3 0 1 1-6 0a3 3 0 0 1 6 0z",
        "eq" => "M6 4v16M12 4v16M18 4v16M4 9h4M10 15h4M16 8h4",
        "sun" => "M12 17a5 5 0 1 0 0-10a5 5 0 0 0 0 10zM12 1v3M12 20v3M4.2 4.2l2.1 2.1M17.7 17.7l2.1 2.1M1 12h3M20 12h3M4.2 19.8l2.1-2.1M17.7 6.3l2.1-2.1",
        "help" => "M12 21a9 9 0 1 0 0-18a9 9 0 0 0 0 18zM9.5 9.5a2.5 2.5 0 1 1 3.5 2.3c-.7.4-1 1-1 1.7M12 17h.01",
        "database" => "M4 6c0-1.7 3.6-3 8-3s8 1.3 8 3-3.6 3-8 3-8-1.3-8-3zM4 6v12c0 1.7 3.6 3 8 3s8-1.3 8-3V6M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3",
        "key" => "M15 15a5 5 0 1 0-4.6-3L3 19.5V22h3v-2h2v-2h2l1.9-1.9A5 5 0 0 0 15 15zM16.5 8.5h.01",
        "moon" => "M20 14.5A8.5 8.5 0 0 1 9.5 4a8.5 8.5 0 1 0 10.5 10.5z",
        "clock" => "M12 21a9 9 0 1 0 0-18a9 9 0 0 0 0 18zM12 7v5l3 2",
        "external" => "M14 4h6v6M20 4l-9 9M18 14v6H4V6h6",
        "karaoke" => "M12 3a3 3 0 0 1 3 3v5a3 3 0 0 1-6 0V6a3 3 0 0 1 3-3zM6 11a6 6 0 0 0 12 0M12 17v4M8 21h8",
        _ => "M12 12h.01",
    }
}

/// Подпути иконки в координатах 24×24 (уже аппроксимированные ломаными), с признаком «замкнут».
fn parse_path(d: &str) -> Vec<(Vec<Pos2>, bool)> {
    let mut toks: Vec<String> = vec![];
    let mut cur = String::new();
    let flush = |cur: &mut String, toks: &mut Vec<String>| { if !cur.is_empty() { toks.push(std::mem::take(cur)); } };
    for c in d.chars() {
        if c.is_ascii_alphabetic() { flush(&mut cur, &mut toks); toks.push(c.to_string()); }
        else if c == ',' || c == ' ' { flush(&mut cur, &mut toks); }
        else if c == '-' { if !cur.is_empty() && !cur.ends_with('e') { flush(&mut cur, &mut toks); } cur.push(c); }
        else if c == '.' { if cur.contains('.') { flush(&mut cur, &mut toks); } cur.push(c); }
        else { cur.push(c); }
    }
    flush(&mut cur, &mut toks);
    let mut out: Vec<(Vec<Pos2>, bool)> = vec![];
    let mut pts: Vec<Pos2> = vec![];
    let (mut x, mut y, mut sx, mut sy) = (0f32, 0f32, 0f32, 0f32);
    let mut last_c: Option<(f32, f32)> = None;
    let mut cmd = 'M';
    let mut i = 0;
    let num = |i: &mut usize| -> f32 { let v = toks.get(*i).and_then(|t| t.parse::<f32>().ok()).unwrap_or(0.0); *i += 1; v };
    let is_num = |i: usize| toks.get(i).is_some_and(|t| !t.chars().next().unwrap().is_ascii_alphabetic());
    while i < toks.len() {
        if !is_num(i) { cmd = toks[i].chars().next().unwrap(); i += 1; if cmd == 'z' || cmd == 'Z' { if !pts.is_empty() { out.push((std::mem::take(&mut pts), true)); } x = sx; y = sy; continue; } }
        let rel = cmd.is_ascii_lowercase();
        let (ox, oy) = if rel { (x, y) } else { (0.0, 0.0) };
        match cmd.to_ascii_uppercase() {
            'M' => { if pts.len() > 1 { out.push((std::mem::take(&mut pts), false)); } pts.clear(); x = ox + num(&mut i); y = oy + num(&mut i); sx = x; sy = y; pts.push(Pos2::new(x, y)); cmd = if rel { 'l' } else { 'L' }; last_c = None; }
            'L' => { x = ox + num(&mut i); y = oy + num(&mut i); pts.push(Pos2::new(x, y)); last_c = None; }
            'H' => { x = (if rel { x } else { 0.0 }) + num(&mut i); pts.push(Pos2::new(x, y)); last_c = None; }
            'V' => { y = (if rel { y } else { 0.0 }) + num(&mut i); pts.push(Pos2::new(x, y)); last_c = None; }
            'C' | 'S' => {
                let (x1, y1) = if cmd.to_ascii_uppercase() == 'C' { (ox + num(&mut i), oy + num(&mut i)) } else { last_c.map(|(cx, cy)| (2.0 * x - cx, 2.0 * y - cy)).unwrap_or((x, y)) };
                let (x2, y2, ex, ey) = (ox + num(&mut i), oy + num(&mut i), ox + num(&mut i), oy + num(&mut i));
                for k in 1..=10 { let t = k as f32 / 10.0; let u = 1.0 - t;
                    pts.push(Pos2::new(u * u * u * x + 3.0 * u * u * t * x1 + 3.0 * u * t * t * x2 + t * t * t * ex, u * u * u * y + 3.0 * u * u * t * y1 + 3.0 * u * t * t * y2 + t * t * t * ey)); }
                last_c = Some((x2, y2)); x = ex; y = ey;
            }
            'Q' => {
                let (x1, y1, ex, ey) = (ox + num(&mut i), oy + num(&mut i), ox + num(&mut i), oy + num(&mut i));
                for k in 1..=8 { let t = k as f32 / 8.0; let u = 1.0 - t; pts.push(Pos2::new(u * u * x + 2.0 * u * t * x1 + t * t * ex, u * u * y + 2.0 * u * t * y1 + t * t * ey)); }
                x = ex; y = ey; last_c = None;
            }
            'A' => {
                let (rx, ry, rot, large, sweep) = (num(&mut i), num(&mut i), num(&mut i), num(&mut i) != 0.0, num(&mut i) != 0.0);
                let (ex, ey) = (ox + num(&mut i), oy + num(&mut i));
                arc(&mut pts, x, y, rx, ry, rot, large, sweep, ex, ey);
                x = ex; y = ey; last_c = None;
            }
            _ => { i += 1; }
        }
    }
    if pts.len() > 1 { out.push((pts, false)); } else if pts.len() == 1 { out.push((pts, false)); }
    out
}

#[allow(clippy::too_many_arguments)]
fn arc(pts: &mut Vec<Pos2>, x1: f32, y1: f32, mut rx: f32, mut ry: f32, rot: f32, large: bool, sweep: bool, x2: f32, y2: f32) {
    let phi = rot.to_radians();
    let (cp, sp) = (phi.cos(), phi.sin());
    let (dx, dy) = ((x1 - x2) / 2.0, (y1 - y2) / 2.0);
    let (x1p, y1p) = (cp * dx + sp * dy, -sp * dx + cp * dy);
    rx = rx.abs(); ry = ry.abs();
    let lam = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lam > 1.0 { rx *= lam.sqrt(); ry *= lam.sqrt(); }
    let num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
    let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let mut co = if den > 0.0 { (num / den).max(0.0).sqrt() } else { 0.0 };
    if large == sweep { co = -co; }
    let (cxp, cyp) = (co * rx * y1p / ry, -co * ry * x1p / rx);
    let (cx, cy) = (cp * cxp - sp * cyp + (x1 + x2) / 2.0, sp * cxp + cp * cyp + (y1 + y2) / 2.0);
    let ang = |ux: f32, uy: f32, vx: f32, vy: f32| { let a = (ux * vy - uy * vx).atan2(ux * vx + uy * vy); a };
    let t1 = ang(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
    let mut dt = ang((x1p - cxp) / rx, (y1p - cyp) / ry, (-x1p - cxp) / rx, (-y1p - cyp) / ry);
    if !sweep && dt > 0.0 { dt -= std::f32::consts::TAU; } else if sweep && dt < 0.0 { dt += std::f32::consts::TAU; }
    let n = ((dt.abs() / 0.25).ceil() as usize).max(2);
    for k in 1..=n {
        let t = t1 + dt * k as f32 / n as f32;
        let (ct, st) = (t.cos(), t.sin());
        pts.push(Pos2::new(cp * rx * ct - sp * ry * st + cx, sp * rx * ct + cp * ry * st + cy));
    }
}

thread_local! {
    static CACHE: std::cell::RefCell<std::collections::HashMap<&'static str, std::rc::Rc<Vec<(Vec<Pos2>, bool)>>>> = Default::default();
}

/// Нарисовать иконку в квадрате rect цветом color. fill — залитая (play/pause и активный лайк).
pub fn paint_icon(p: &egui::Painter, rect: Rect, name: &str, color: Color32, fill: bool) {
    let d = icon_path(name);
    let subs = CACHE.with(|c| c.borrow_mut().entry(d).or_insert_with(|| std::rc::Rc::new(parse_path(d))).clone());
    let s = rect.width().min(rect.height()) / 24.0;
    let o = rect.center() - Vec2::splat(12.0 * s);
    let dots = name == "more" || name == "grip";
    let w = if dots { 2.8 } else if fill { 2.4 } else { 1.7 } * s;
    let solid = matches!(name, "play" | "pause" | "next" | "prev");
    for (pts, closed) in subs.iter() {
        let v: Vec<Pos2> = pts.iter().map(|q| o + q.to_vec2() * s).collect();
        if v.len() == 2 && (v[0] - v[1]).length() < 0.5 * s {
            p.circle_filled(v[0], w * 0.55, color);
            continue;
        }
        if solid && *closed && v.len() > 2 {
            p.add(egui::Shape::convex_polygon(v, color, Stroke::new(w * 0.6, color)));
        } else if *closed {
            p.add(egui::Shape::closed_line(v, Stroke::new(w, color)));
        } else {
            p.add(egui::Shape::line(v, Stroke::new(w, color)));
        }
    }
}
