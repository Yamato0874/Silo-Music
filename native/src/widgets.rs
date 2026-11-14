//! Общие элементы интерфейса: кнопки, чипы, ползунки, переключатели, обложки, заглушки.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

use crate::theme::{mix, paint_icon, Pal};

#[derive(Clone, Copy, PartialEq)]
pub enum V { Ghost, Solid, Primary, Chip, Danger, Nav }

/// Окно в фоне (не в фокусе или свёрнуто): перерисовка не чаще раза в секунду, чтобы не мешать играм.
pub static BG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn repaint(ctx: &egui::Context, ms: u64) {
    let ms = if BG.load(std::sync::atomic::Ordering::Relaxed) { ms.max(1000) } else { ms.max(40) };
    ctx.request_repaint_after(std::time::Duration::from_millis(ms));
}

/// Кнопка-иконка (квадратная, прозрачная, подсветка при наведении).
pub fn icon_btn(ui: &mut Ui, pal: &Pal, icon: &str, size: f32, active: bool, tip: &str) -> Response {
    let (rect, r) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let h = ui.ctx().animate_bool(r.id, r.hovered());
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        if h > 0.0 || r.is_pointer_button_down_on() {
            let c = if r.is_pointer_button_down_on() { pal.press() } else { pal.hover() };
            p.rect_filled(rect, CornerRadius::same((size / 4.0) as u8), mix(Color32::TRANSPARENT, c, h.max(0.5)));
        }
        let col = if active { pal.acc } else if r.hovered() { pal.fg } else { mix(pal.mut_, pal.fg, 0.35) };
        let s = (size * 0.56).round();
        paint_icon(p, Rect::from_center_size(rect.center(), Vec2::splat(s)), icon, col, active && matches!(icon, "like" | "dislike" | "heart"));
    }
    if !tip.is_empty() { r.on_hover_text(tip) } else { r }
}

/// Кнопка с текстом (и иконкой).
pub fn btn(ui: &mut Ui, pal: &Pal, v: V, icon: Option<&str>, text: &str, active: bool) -> Response {
    btn_sized(ui, pal, v, icon, text, active, 34.0, true)
}
pub fn btn_sm(ui: &mut Ui, pal: &Pal, v: V, icon: Option<&str>, text: &str, active: bool) -> Response {
    btn_sized(ui, pal, v, icon, text, active, 28.0, true)
}
pub fn btn_off(ui: &mut Ui, pal: &Pal, v: V, icon: Option<&str>, text: &str, enabled: bool) -> Response {
    btn_sized(ui, pal, v, icon, text, false, 34.0, enabled)
}

#[allow(clippy::too_many_arguments)]
pub fn btn_sized(ui: &mut Ui, pal: &Pal, v: V, icon: Option<&str>, text: &str, active: bool, h: f32, enabled: bool) -> Response {
    let font = FontId::proportional(if h < 30.0 { 13.0 } else { 14.0 });
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, pal.fg);
    let isz = if h < 30.0 { 15.0 } else { 17.0 };
    let pad = if v == V::Chip { 12.0 } else { 14.0 };
    let gap = if icon.is_some() && !text.is_empty() { 8.0 } else { 0.0 };
    let w = if v == V::Nav { ui.available_width() } else { pad * 2.0 + galley.size().x + icon.map_or(0.0, |_| isz) + gap };
    let w = if text.is_empty() { h } else { w };
    let (rect, mut r) = ui.allocate_exact_size(Vec2::new(w, h), if enabled { Sense::click() } else { Sense::hover() });
    let hov = enabled && r.hovered();
    let down = enabled && r.is_pointer_button_down_on();
    let t = ui.ctx().animate_bool(r.id.with("h"), hov);
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let rad = CornerRadius::same(if v == V::Chip { (h / 2.0) as u8 } else { 8 });
        let (mut bg, mut fg, mut stroke) = match v {
            V::Primary => (pal.acc, pal.on_acc, Stroke::NONE),
            V::Danger => (pal.danger(), Color32::WHITE, Stroke::NONE),
            V::Solid => (pal.raise(), pal.fg, Stroke::new(1.0, pal.line)),
            V::Chip => if active { (pal.acc_soft(), pal.acc, Stroke::new(1.0, mix(pal.line, pal.acc, 0.6))) } else { (pal.raise(), pal.fg, Stroke::new(1.0, pal.line)) },
            V::Nav => if active { (pal.acc_soft(), pal.acc, Stroke::NONE) } else { (Color32::TRANSPARENT, mix(pal.mut_, pal.fg, 0.4), Stroke::NONE) },
            V::Ghost => (Color32::TRANSPARENT, if active { pal.acc } else { pal.fg }, Stroke::NONE),
        };
        if active && v == V::Solid { bg = pal.acc_soft(); fg = pal.acc; stroke = Stroke::new(1.0, mix(pal.line, pal.acc, 0.6)); }
        if t > 0.0 {
            bg = match v {
                V::Primary | V::Danger => mix(bg, Color32::WHITE, 0.10 * t),
                _ => mix(bg, if down { pal.press() } else { pal.hover() }, if bg == Color32::TRANSPARENT { 1.0 } else { 0.7 * t }),
            };
            if v == V::Nav && !active { fg = pal.fg; }
        }
        if !enabled { fg = mix(fg, pal.bg, 0.55); bg = mix(bg, pal.bg, 0.4); }
        if bg != Color32::TRANSPARENT { p.rect(rect, rad, bg, stroke, egui::StrokeKind::Inside); }
        let mut x = rect.left() + if text.is_empty() { (h - isz) / 2.0 } else { pad };
        if v == V::Nav { x = rect.left() + 12.0; }
        if let Some(ic) = icon {
            paint_icon(p, Rect::from_min_size(Pos2::new(x, rect.center().y - isz / 2.0), Vec2::splat(isz)), ic, if v == V::Nav && active { pal.acc } else { fg }, false);
            x += isz + gap;
        }
        if !text.is_empty() {
            let g = p.layout_no_wrap(text.to_string(), FontId::proportional(if h < 30.0 { 13.0 } else { 14.0 }), fg);
            p.galley(Pos2::new(x, rect.center().y - g.size().y / 2.0), g, fg);
        }
    }
    if hov { r = r.on_hover_cursor(egui::CursorIcon::PointingHand); }
    r
}

/// Горизонтальный ползунок: тонкая дорожка, закрашенная часть, кружок растёт при наведении.
/// Возвращает (ответ, изменилось ли значение). Колесо мыши тоже меняет значение (шаг wheel).
pub fn slider(ui: &mut Ui, pal: &Pal, v: &mut f32, min: f32, max: f32, w: f32, wheel: f32) -> (Response, bool) {
    let (rect, r) = ui.allocate_exact_size(Vec2::new(w, 22.0), Sense::click_and_drag());
    let mut changed = false;
    if let Some(p) = r.interact_pointer_pos() {
        if r.dragged() || r.clicked() || r.is_pointer_button_down_on() {
            let f = ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            let nv = min + f * (max - min);
            if (nv - *v).abs() > f32::EPSILON { *v = nv; changed = true; }
        }
    }
    if r.hovered() && wheel != 0.0 {
        let dy = ui.input(|i| i.smooth_scroll_delta.y);
        if dy.abs() > 0.5 { *v = (*v + wheel * dy.signum()).clamp(min, max); changed = true; }
    }
    paint_track(ui, pal, rect, (*v - min) / (max - min).max(1e-6), r.hovered() || r.dragged(), r.id);
    (r, changed)
}

pub fn paint_track(ui: &Ui, pal: &Pal, rect: Rect, f: f32, hot: bool, id: egui::Id) {
    let t = ui.ctx().animate_bool(id.with("hot"), hot);
    let p = ui.painter();
    let h = 4.0 + 2.0 * t;
    let tr = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), h));
    p.rect_filled(tr, CornerRadius::same(3), mix(pal.line, pal.fg, 0.08));
    let x = tr.left() + tr.width() * f.clamp(0.0, 1.0);
    p.rect_filled(Rect::from_min_max(tr.min, Pos2::new(x, tr.max.y)), CornerRadius::same(3), if hot { pal.acc } else { mix(pal.fg, pal.acc, 0.55) });
    let kr = 4.0 + 3.0 * t;
    if t > 0.01 { p.circle_filled(Pos2::new(x, tr.center().y), kr, pal.fg); }
}

/// Вертикальный ползунок (эквалайзер).
pub fn vslider(ui: &mut Ui, pal: &Pal, v: &mut f32, min: f32, max: f32, h: f32) -> (Response, bool) {
    let (rect, r) = ui.allocate_exact_size(Vec2::new(26.0, h), Sense::click_and_drag());
    let mut changed = false;
    if let Some(p) = r.interact_pointer_pos() {
        if r.dragged() || r.is_pointer_button_down_on() {
            let f = 1.0 - ((p.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
            let nv = (min + f * (max - min)).round();
            if nv != *v { *v = nv; changed = true; }
        }
    }
    if r.double_clicked() { *v = 0.0; changed = true; }
    let pnt = ui.painter();
    let tr = Rect::from_center_size(rect.center(), Vec2::new(4.0, rect.height() - 10.0));
    pnt.rect_filled(tr, CornerRadius::same(2), mix(pal.line, pal.fg, 0.08));
    let zero = tr.bottom() - tr.height() * ((0.0 - min) / (max - min));
    let y = tr.bottom() - tr.height() * ((*v - min) / (max - min));
    pnt.rect_filled(Rect::from_x_y_ranges(tr.x_range(), zero.min(y)..=zero.max(y)), CornerRadius::same(2), pal.acc);
    pnt.circle_filled(Pos2::new(tr.center().x, y), if r.hovered() || r.dragged() { 7.0 } else { 5.5 }, pal.fg);
    (r, changed)
}

/// Переключатель «вкл/выкл».
pub fn switch(ui: &mut Ui, pal: &Pal, on: &mut bool, enabled: bool) -> Response {
    let (rect, mut r) = ui.allocate_exact_size(Vec2::new(40.0, 22.0), if enabled { Sense::click() } else { Sense::hover() });
    if r.clicked() { *on = !*on; r.mark_changed(); }
    let t = ui.ctx().animate_bool_with_time(r.id, *on, 0.15);
    let p = ui.painter();
    let bg = mix(mix(pal.line, pal.fg, 0.1), pal.acc, t);
    p.rect_filled(rect, CornerRadius::same(11), if enabled { bg } else { mix(bg, pal.bg, 0.5) });
    let x = rect.left() + 11.0 + t * (rect.width() - 22.0);
    p.circle_filled(Pos2::new(x, rect.center().y), 8.0, if *on { pal.on_acc } else { pal.fg });
    r
}

/// Вращающийся индикатор.
pub fn spinner(ui: &mut Ui, pal: &Pal, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint_spinner(ui, pal, rect);
}
pub fn paint_spinner(ui: &Ui, pal: &Pal, rect: Rect) {
    let t = ui.input(|i| i.time) as f32;
    let c = rect.center();
    let r = rect.width() / 2.0 - 2.0;
    let pts: Vec<Pos2> = (0..=24).map(|k| { let a = t * 5.0 + k as f32 / 24.0 * 4.7; c + Vec2::new(a.cos(), a.sin()) * r }).collect();
    ui.painter().add(egui::Shape::line(pts, Stroke::new(2.2, pal.acc)));
    repaint(ui.ctx(), 40);
}

/// Заголовок страницы с пояснением; действия справа добавляет `actions`.
pub fn page_head(ui: &mut Ui, pal: &Pal, title: &str, sub: &str, actions: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(title).font(crate::theme::bold(26.0)).color(pal.fg));
            if !sub.is_empty() { ui.label(egui::RichText::new(sub).color(pal.mut_).size(13.5)); }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { actions(ui); });
    });
    ui.add_space(14.0);
}
pub fn section_head(ui: &mut Ui, pal: &Pal, title: &str, actions: impl FnOnce(&mut Ui)) {
    ui.add_space(18.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).font(crate::theme::bold(17.0)).color(pal.fg));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { actions(ui); });
    });
    ui.add_space(6.0);
}

/// Пустое состояние: иконка в круге, заголовок, пояснение, действия.
pub fn empty(ui: &mut Ui, pal: &Pal, icon: &str, title: &str, text: &str, actions: impl FnOnce(&mut Ui)) {
    ui.add_space(30.0);
    ui.vertical_centered(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(56.0), Sense::hover());
        ui.painter().circle_filled(rect.center(), 28.0, pal.acc_soft());
        paint_icon(ui.painter(), Rect::from_center_size(rect.center(), Vec2::splat(26.0)), icon, pal.acc, false);
        ui.add_space(10.0);
        ui.label(egui::RichText::new(title).font(crate::theme::bold(16.0)).color(pal.fg));
        if !text.is_empty() {
            ui.add_space(2.0);
            ui.allocate_ui(Vec2::new(420.0f32.min(ui.available_width()), 60.0), |ui| { ui.vertical_centered(|ui| ui.label(egui::RichText::new(text).color(pal.mut_))); });
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            // по центру: примерная ширина кнопок неизвестна, поэтому просто отступ
            ui.add_space((ui.available_width() / 2.0 - 120.0).max(0.0));
            actions(ui);
        });
    });
    ui.add_space(20.0);
}

pub fn skeleton_rows(ui: &mut Ui, pal: &Pal, n: usize) {
    let t = ui.input(|i| i.time) as f32;
    for i in 0..n {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 56.0), Sense::hover());
        let a = 0.5 + 0.5 * (t * 3.0 - i as f32 * 0.4).sin();
        let c = mix(pal.raise(), pal.hover(), a);
        let p = ui.painter();
        p.rect_filled(Rect::from_min_size(rect.min + Vec2::new(8.0, 8.0), Vec2::splat(40.0)), CornerRadius::same(6), c);
        p.rect_filled(Rect::from_min_size(rect.min + Vec2::new(60.0, 14.0), Vec2::new(rect.width() * (0.3 + (i * 17 % 40) as f32 / 100.0), 10.0)), CornerRadius::same(4), c);
        p.rect_filled(Rect::from_min_size(rect.min + Vec2::new(60.0, 32.0), Vec2::new(rect.width() * (0.18 + (i * 11 % 20) as f32 / 100.0), 9.0)), CornerRadius::same(4), c);
    }
    repaint(ui.ctx(), 60);
}

/// Поле ввода в рамке с иконкой.
pub fn field(ui: &mut Ui, pal: &Pal, text: &mut String, hint: &str, icon: Option<&str>, w: f32, id: &str) -> Response {
    let h = 38.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
    let te_id = egui::Id::new(id);
    let focused = ui.memory(|m| m.has_focus(te_id));
    ui.painter().rect(rect, CornerRadius::same(10), pal.sunk(), Stroke::new(1.0, if focused { pal.acc } else { pal.line }), egui::StrokeKind::Inside);
    let mut x0 = rect.left() + 12.0;
    if let Some(ic) = icon {
        paint_icon(ui.painter(), Rect::from_min_size(Pos2::new(x0, rect.center().y - 9.0), Vec2::splat(18.0)), ic, pal.mut_, false);
        x0 += 26.0;
    }
    let inner = Rect::from_min_max(Pos2::new(x0, rect.top() + 4.0), Pos2::new(rect.right() - 10.0, rect.bottom() - 4.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::left_to_right(egui::Align::Center)));
    child.add(egui::TextEdit::singleline(text).id(te_id).hint_text(egui::RichText::new(hint).color(pal.mut_)).frame(egui::Frame::NONE).desired_width(inner.width()).vertical_align(egui::Align::Center))
}

pub fn fmt_dur(s: f64) -> String {
    if s <= 0.0 || !s.is_finite() { return String::new(); }
    format!("{}:{:02}", (s / 60.0).floor() as i64, (s % 60.0).floor() as i64)
}
pub fn fmt_time(s: f64) -> String { let s = if s.is_finite() { s.max(0.0) } else { 0.0 }; format!("{}:{:02}", (s / 60.0).floor() as i64, (s % 60.0).floor() as i64) }

/// Анимированные полоски «сейчас играет».
pub fn bars(ui: &Ui, pal: &Pal, rect: Rect, playing: bool) {
    let t = ui.input(|i| i.time) as f32;
    let p = ui.painter();
    for k in 0..3 {
        let ph = if playing { (t * (5.0 + k as f32 * 1.7) + k as f32).sin() * 0.5 + 0.5 } else { 0.3 };
        let h = rect.height() * (0.3 + 0.7 * ph);
        let x = rect.left() + rect.width() * (0.2 + 0.3 * k as f32);
        p.rect_filled(Rect::from_min_max(Pos2::new(x - 1.5, rect.bottom() - h), Pos2::new(x + 1.5, rect.bottom())), CornerRadius::same(1), pal.acc);
    }
    if playing { repaint(ui.ctx(), 60); }
}

// ---------- обложки: загрузка по сети в фоне, уменьшение, кэш текстур ----------

#[derive(Clone)]
enum Img { Loading, Ready(egui::TextureHandle), Bad }

#[derive(Clone)]
pub struct Images {
    map: Arc<Mutex<HashMap<String, (Img, u64)>>>,
    rt: tokio::runtime::Handle,
    tick: Arc<std::sync::atomic::AtomicU64>,
}

impl Images {
    pub fn new(rt: tokio::runtime::Handle) -> Images { Images { map: Default::default(), rt, tick: Default::default() } }
    pub fn get(&self, ctx: &egui::Context, url: &str) -> Option<egui::TextureHandle> {
        if url.is_empty() { return None; }
        let now = self.tick.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut m = self.map.lock().unwrap();
        if let Some((img, used)) = m.get_mut(url) {
            *used = now;
            return match img { Img::Ready(t) => Some(t.clone()), _ => None };
        }
        // не больше ~600 обложек в памяти: выбрасываем давно не показанные
        if m.len() > 600 {
            let mut v: Vec<(String, u64)> = m.iter().map(|(k, v)| (k.clone(), v.1)).collect();
            v.sort_by_key(|x| x.1);
            for (k, _) in v.into_iter().take(150) { m.remove(&k); }
        }
        m.insert(url.to_string(), (Img::Loading, now));
        drop(m);
        let (map, url2, ctx2) = (self.map.clone(), url.to_string(), ctx.clone());
        self.rt.spawn(async move {
            let bytes = if url2.starts_with("http") {
                match silo_core::http().get(&url2).send().await { Ok(r) if r.status().is_success() => r.bytes().await.ok().map(|b| b.to_vec()), _ => None }
            } else { std::fs::read(&url2).ok() };
            let res = tokio::task::spawn_blocking(move || {
                let b = bytes?;
                let im = image::load_from_memory(&b).ok()?;
                let im = if im.width() > 360 || im.height() > 360 { im.thumbnail(360, 360) } else { im };
                let rgba = im.to_rgba8();
                Some(egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()))
            }).await.ok().flatten();
            let v = match res { Some(ci) => Img::Ready(ctx2.load_texture(&url2, ci, egui::TextureOptions::LINEAR)), None => Img::Bad };
            if let Some(e) = map.lock().unwrap().get_mut(&url2) { e.0 = v; }
            ctx2.request_repaint();
        });
        None
    }
}

/// Обложка с запасным видом (нота), если картинки нет. cover = обрезать по квадрату.
pub fn cover(ui: &mut Ui, pal: &Pal, imgs: &Images, url: &str, size: Vec2, radius: u8) -> Response {
    let (rect, r) = ui.allocate_exact_size(size, Sense::click());
    paint_cover(ui, pal, imgs, url, rect, radius);
    r
}
pub fn paint_cover(ui: &Ui, pal: &Pal, imgs: &Images, url: &str, rect: Rect, radius: u8) {
    if !ui.is_rect_visible(rect) { return; }
    match imgs.get(ui.ctx(), url) {
        Some(t) => {
            // обрезка до пропорций прямоугольника (миниатюры YouTube 16:9)
            let [w, h] = t.size();
            let (iw, ih) = (w as f32, h as f32);
            let ar = rect.width() / rect.height();
            let uv = if iw / ih > ar { let k = (ar * ih) / iw; Rect::from_min_max(Pos2::new((1.0 - k) / 2.0, 0.0), Pos2::new((1.0 + k) / 2.0, 1.0)) } else { let k = (iw / ar) / ih; Rect::from_min_max(Pos2::new(0.0, (1.0 - k) / 2.0), Pos2::new(1.0, (1.0 + k) / 2.0)) };
            egui::Image::new((t.id(), rect.size())).uv(uv).corner_radius(CornerRadius::same(radius)).paint_at(ui, rect);
        }
        None => {
            let p = ui.painter();
            p.rect_filled(rect, CornerRadius::same(radius), mix(pal.raise(), pal.acc, 0.10));
            let s = (rect.width().min(rect.height()) * 0.42).max(10.0);
            paint_icon(p, Rect::from_center_size(rect.center(), Vec2::splat(s)), "music", mix(pal.mut_, pal.acc, 0.3), false);
        }
    }
}

pub fn text_galley_w(ui: &Ui, s: &str, size: f32) -> f32 {
    ui.painter().layout_no_wrap(s.to_string(), FontId::proportional(size), Color32::WHITE).size().x
}

/// Текст, обрезанный многоточием по ширине.
pub fn label_trunc(ui: &mut Ui, text: &str, font: FontId, color: Color32, w: f32) -> Response {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_string(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(w);
    let g = ui.painter().layout_job(job);
    let (rect, r) = ui.allocate_exact_size(Vec2::new(w.min(g.size().x.max(1.0)), g.size().y), Sense::click());
    ui.painter().galley(rect.min, g, color);
    r
}
pub fn paint_trunc(ui: &Ui, pos: Pos2, text: &str, font: FontId, color: Color32, w: f32, anchor: Align2) -> Rect {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_string(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(w.max(10.0));
    let g = ui.painter().layout_job(job);
    let rect = anchor.anchor_size(pos, g.size());
    ui.painter().galley(rect.min, g, color);
    rect
}
