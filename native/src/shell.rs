//! Каркас окна: навигация слева, страница, нижняя панель плеера, очередь и текст справа,
//! сцена (клип и караоке), тосты, диалоги, перетаскивание файлов, мини-плеер, трей.
use eframe::egui::{self, Align, Align2, Color32, CornerRadius, FontId, Layout, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};
use silo_core::model::Track;

use crate::app::{Act, Ask, Route, SiloApp, Tab, ToastKind, NAV};
use crate::theme::{bold, mix, paint_icon};
use crate::widgets::*;

const FOOT_H: f32 = 84.0;

impl eframe::App for SiloApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // команды трея, глобальных клавиш и плашки Windows
        while let Ok(c) = self.plat.rx.try_recv() {
            use crate::platform::Cmd;
            match c {
                Cmd::Toggle => self.toggle(),
                Cmd::Play => self.set_playing(true),
                Cmd::Pause => self.set_playing(false),
                Cmd::Next => self.skip(1),
                Cmd::Prev => self.skip(-1),
                Cmd::SeekTo(s) => self.seek(s),
                Cmd::Show => { ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true)); ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false)); ctx.send_viewport_cmd(egui::ViewportCommand::Focus); }
                Cmd::Quit => { self.quit = true; ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
            }
        }
        // крестик прячет окно в трей, музыка продолжает играть; выход — из меню трея
        if ctx.input(|i| i.viewport().close_requested()) && self.prefs.tray && !self.quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        // самопроверка: рисовать кадры, даже если окно перекрыто другими окнами
        if self.demo.is_some() { ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaintWhileHidden); }
        // в фоне (игра, другое окно) почти не рисуем
        let bg = ctx.input(|i| !i.viewport().focused.unwrap_or(true) || i.viewport().minimized.unwrap_or(false));
        crate::widgets::BG.store(bg && self.demo.is_none(), std::sync::atomic::Ordering::Relaxed);
        self.tick();
        self.plat.set_state(self.ps.playing, self.ps.pos);
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        crate::demo::frame(self, &ctx);
        self.hotkeys(&ctx);
        let pal = self.pal;
        egui::CentralPanel::no_frame().frame(egui::Frame::new().fill(pal.bg)).show(ui, |ui| {
            egui::Panel::top("titlebar").exact_size(if self.mini { 22.0 } else { 34.0 }).frame(egui::Frame::new().fill(pal.bg)).show_inside(ui, |ui| self.titlebar(ui));
            if !self.prefs.onboarded { self.onboarding(ui); return; }
            if self.mini { self.footer(ui, true); return; }
            egui::Panel::bottom("foot").exact_size(FOOT_H).frame(egui::Frame::new().fill(pal.panel).stroke(Stroke::new(1.0, pal.line))).show_inside(ui, |ui| self.footer(ui, false));
            let narrow = ui.available_width() < 900.0;
            egui::Panel::left("nav").exact_size(if narrow { 64.0 } else { 224.0 }).resizable(false).frame(egui::Frame::new().fill(pal.panel).inner_margin(egui::Margin::symmetric(10, 14))).show_inside(ui, |ui| self.nav(ui, narrow));
            if (self.show_q || self.show_l) && !self.stage {
                egui::Panel::right("side").exact_size(344.0).resizable(false).frame(egui::Frame::new().fill(pal.panel).inner_margin(egui::Margin::same(14))).show_inside(ui, |ui| {
                    if self.show_q { self.queue_panel(ui) } else { self.lyrics_panel(ui) }
                });
            }
            let main = ui.available_rect_before_wrap();
            if self.stage { self.stage(ui, main); } else {
                egui::CentralPanel::no_frame().show_inside(ui, |ui| {
                    if self.prefs.yt_mode == "ytdlp" && self.ytdlp_exe().is_none() {
                        let mut go = false;
                        egui::Frame::new().fill(pal.acc_soft()).stroke(Stroke::new(1.0, mix(pal.line, pal.acc, 0.5))).corner_radius(8).inner_margin(egui::Margin::symmetric(12, 8)).outer_margin(egui::Margin { left: 12, right: 12, top: 8, bottom: 0 }).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (r, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                                paint_icon(ui.painter(), r, "alert", pal.acc, false);
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if btn_sm(ui, &pal, V::Solid, None, self.t("settings"), false).clicked() { go = true; }
                                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| { let (busy, msg) = crate::ytdlp::download_state(); let text = if busy { "Скачиваю yt-dlp (один раз, около 24 МБ)…".to_string() } else if msg.starts_with("не удалось") { format!("{msg}. {}", self.t("ytDlpBanner")) } else { self.t("ytDlpBanner").to_string() }; ui.add(egui::Label::new(RichText::new(text).color(pal.fg).size(12.5)).wrap()); });
                                });
                            });
                        });
                        if go { self.open_settings_tab("yt"); }
                    }
                    if !self.online { ui.label(RichText::new(self.t("offline")).color(pal.mut_)); }
                    let id = egui::Id::new(("page", self.tab as u8, self.artist.clone()));
                    egui::ScrollArea::vertical().id_salt(id).auto_shrink([false, false]).show(ui, |ui| {
                        // страница не шире 1180 px и по центру на широком окне
                        let full = ui.available_width();
                        let w = (full - 56.0).min(1180.0).max(200.0);
                        let pad = ((full - w) / 2.0).max(28.0);
                        ui.add_space(22.0);
                        ui.horizontal_top(|ui| {
                            ui.add_space(pad);
                            ui.vertical(|ui| { ui.set_width(w); self.page(ui); });
                        });
                        ui.add_space(40.0);
                    });
                });
                self.place_embed(main);
            }
        });
        self.resize_edges(&ctx);
        let acts: Vec<Act> = std::mem::take(&mut self.pages.acts);
        for a in acts { self.apply(a); }
        self.drop_files(&ctx);
        self.toasts_ui(&ctx);
        self.dialogs(&ctx);
        self.sync_embed_lyrics();
    }
}

impl SiloApp {
    /// Тонкая шапка окна: перетаскивание, двойной щелчок — развернуть, свернуть/развернуть/закрыть.
    fn titlebar(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let r = ui.max_rect();
        let drag = ui.interact(r, egui::Id::new("tb-drag"), Sense::click_and_drag());
        let maxed = ui.input(|i| i.viewport().maximized.unwrap_or(false));
        if drag.drag_started_by(egui::PointerButton::Primary) { ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag); }
        if drag.double_clicked() { ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!maxed)); }
        if !self.mini {
            let now = self.cur().map(|c| format!("{} · {}", c.name, c.artist)).unwrap_or_default();
            paint_icon(ui.painter(), Rect::from_min_size(r.min + Vec2::new(12.0, 10.0), Vec2::splat(15.0)), "echo", pal.acc, false);
            ui.painter().text(r.min + Vec2::new(34.0, r.height() / 2.0), Align2::LEFT_CENTER, "Silo", bold(13.0), pal.fg);
            if !now.is_empty() { paint_trunc(ui, r.min + Vec2::new(72.0, r.height() / 2.0 - 8.0), &now, FontId::proportional(12.5), pal.mut_, (r.width() - 260.0).max(50.0), Align2::LEFT_TOP); }
        }
        let bw = if self.mini { 34.0 } else { 46.0 };
        // статус нейросети «Средний»: что сейчас делается
        if !self.mini {
            use crate::enhance::Install;
            let (q, run, _, paused) = self.enh.queue_info();
            let msg = match self.enh.state() {
                Install::Installing { step, .. } => Some(format!("{}: {step}", if self.en() { "Installing the enhancer" } else { "Установка улучшителя" })),
                Install::Ready if self.prefs.upscale == 2 && paused => Some(if self.en() { "Enhancing paused (full-screen program)".into() } else { "Улучшение на паузе (полноэкранная программа)".into() }),
                Install::Ready if self.prefs.upscale == 2 && (run.is_some() || q > 0) => Some(format!("{}: {}", if self.en() { "Enhancing next tracks" } else { "Улучшаю следующие треки" }, q + usize::from(run.is_some()))),
                _ => None,
            };
            if let Some(m) = msg { paint_trunc(ui, Pos2::new(r.right() - bw * 3.0 - 12.0, r.center().y), &m, FontId::proportional(12.0), pal.acc, (r.width() - 420.0).max(60.0), Align2::RIGHT_CENTER); }
        }
        let mut x = r.right() - bw;
        for (k, ic) in [(2, "close"), (1, if maxed { "restore" } else { "max" }), (0, "min")] {
            if self.mini && k == 1 { continue; }
            let br = Rect::from_min_size(Pos2::new(x, r.top()), Vec2::new(bw, r.height()));
            let resp = ui.interact(br, egui::Id::new(("tb", k)), Sense::click());
            if resp.hovered() { ui.painter().rect_filled(br, CornerRadius::ZERO, if k == 2 { Color32::from_rgb(0xc4, 0x2b, 0x1c) } else { pal.hover() }); }
            let c = br.center();
            let col = if resp.hovered() && k == 2 { Color32::WHITE } else { pal.fg };
            let st = Stroke::new(1.1, col);
            let p = ui.painter();
            match ic {
                "close" => { p.line_segment([c + Vec2::new(-5.0, -5.0), c + Vec2::new(5.0, 5.0)], st); p.line_segment([c + Vec2::new(5.0, -5.0), c + Vec2::new(-5.0, 5.0)], st); }
                "max" => { p.rect_stroke(Rect::from_center_size(c, Vec2::splat(10.0)), CornerRadius::ZERO, st, egui::StrokeKind::Inside); }
                "restore" => { p.rect_stroke(Rect::from_center_size(c + Vec2::new(-1.5, 1.5), Vec2::splat(8.0)), CornerRadius::ZERO, st, egui::StrokeKind::Inside); p.line_segment([c + Vec2::new(-2.5, -4.5), c + Vec2::new(4.5, -4.5)], st); p.line_segment([c + Vec2::new(4.5, -4.5), c + Vec2::new(4.5, 2.5)], st); }
                _ => { p.line_segment([c + Vec2::new(-5.0, 0.0), c + Vec2::new(5.0, 0.0)], st); }
            }
            if resp.clicked() {
                let cmd = match k { 2 => egui::ViewportCommand::Close, 1 => egui::ViewportCommand::Maximized(!maxed), _ => egui::ViewportCommand::Minimized(true) };
                ui.ctx().send_viewport_cmd(cmd);
            }
            x -= bw;
        }
    }

    /// Без системной рамки края окна тянем сами.
    fn resize_edges(&self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) { return; }
        let r = ctx.content_rect();
        let Some(p) = ctx.input(|i| i.pointer.hover_pos()) else { return };
        let m = 5.0;
        let (l, rt, t, b) = (p.x < r.left() + m, p.x > r.right() - m, p.y < r.top() + m, p.y > r.bottom() - m);
        use egui::viewport::ResizeDirection as D;
        let dir = match (l, rt, t, b) {
            (true, _, true, _) => Some((D::NorthWest, egui::CursorIcon::ResizeNorthWest)),
            (_, true, true, _) => Some((D::NorthEast, egui::CursorIcon::ResizeNorthEast)),
            (true, _, _, true) => Some((D::SouthWest, egui::CursorIcon::ResizeSouthWest)),
            (_, true, _, true) => Some((D::SouthEast, egui::CursorIcon::ResizeSouthEast)),
            (true, ..) => Some((D::West, egui::CursorIcon::ResizeWest)),
            (_, true, ..) => Some((D::East, egui::CursorIcon::ResizeEast)),
            (_, _, true, _) => Some((D::North, egui::CursorIcon::ResizeNorth)),
            (_, _, _, true) => Some((D::South, egui::CursorIcon::ResizeSouth)),
            _ => None,
        };
        if let Some((d, cur)) = dir {
            ctx.set_cursor_icon(cur);
            if ctx.input(|i| i.pointer.primary_pressed()) { ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(d)); }
        }
    }

    fn nav(&mut self, ui: &mut Ui, narrow: bool) {
        let pal = self.pal;
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
            paint_icon(ui.painter(), r, "echo", pal.acc, false);
            if !narrow { ui.label(RichText::new("Silo").font(bold(19.0)).color(pal.fg)); ui.label(RichText::new(concat!("native ", env!("CARGO_PKG_VERSION"))).size(11.0).color(pal.mut_)); }
        });
        ui.add_space(14.0);
        for (group, items) in NAV.iter() {
            if !narrow { ui.label(RichText::new(self.t(group).to_uppercase()).size(11.0).color(pal.mut_)); }
            ui.add_space(2.0);
            for (tab, key, ic) in items.iter() {
                let active = self.tab == *tab;
                let label = if narrow { "" } else { self.t(key) };
                let r = btn_sized(ui, &pal, V::Nav, Some(ic), label, active, 36.0, true);
                if *tab == Tab::Echo && self.wave { ui.painter().circle_filled(Pos2::new(r.rect.right() - 14.0, r.rect.center().y), 4.0, pal.acc); }
                if r.clicked() { self.tab = *tab; }
                if narrow { r.on_hover_text(self.t(key)); }
            }
            ui.add_space(12.0);
        }
    }

    fn page(&mut self, ui: &mut Ui) {
        match self.tab {
            Tab::Home => self.page_home(ui),
            Tab::Search => self.page_search(ui),
            Tab::Browse => self.page_browse(ui),
            Tab::Echo => self.page_echo(ui),
            Tab::Liked => self.page_liked(ui),
            Tab::Playlists => self.page_playlists(ui),
            Tab::Downloads => self.page_downloads(ui),
            Tab::History => self.page_history(ui),
            Tab::Import => self.page_import(ui),
            Tab::Banned => self.page_banned(ui),
            Tab::Settings => self.page_settings(ui),
            Tab::Artist => self.page_artist(ui),
        }
    }

    /// Окно официального плеера: маленькая панель в углу (если «показывать видео») или почти невидимая.
    fn place_embed(&mut self, main: Rect) {
        if self.route != Route::Embed { return; }
        let r = if self.prefs.show_video { [main.right() - 396.0, main.bottom() - 230.0, 380.0, 214.0] } else { [main.right() - 2.0, main.bottom() - 2.0, 2.0, 2.0] };
        if self.embed_rect != Some(r) { if let Some(e) = &self.embed { e.set_rect(r); } self.embed_rect = Some(r); }
    }

    // ---------- нижняя панель ----------
    fn footer(&mut self, ui: &mut Ui, mini: bool) {
        let pal = self.pal;
        let full = ui.available_rect_before_wrap();
        let cur = self.cur().cloned();
        let ps = self.ps;
        // слева: обложка, название, исполнитель, оценки
        let left = Rect::from_min_size(full.min, Vec2::new((full.width() * 0.3).clamp(220.0, 360.0), full.height()));
        let mut lu = ui.new_child(egui::UiBuilder::new().max_rect(left.shrink2(Vec2::new(14.0, 0.0))).layout(Layout::left_to_right(Align::Center)));
        if let Some(c) = &cur {
            let cs = if mini { 56.0 } else { 56.0 };
            let (cr, resp) = lu.allocate_exact_size(Vec2::splat(cs), Sense::click());
            if self.viz && !BG.load(std::sync::atomic::Ordering::Relaxed) { self.paint_viz(&lu, cr); } else { paint_cover(&lu, &pal, &self.imgs, &c.image, cr, 8); }
            if resp.on_hover_text(self.t("vizToggle")).clicked() { self.viz = !self.viz; }
            lu.add_space(10.0);
            let w = (left.width() - cs - 130.0).max(60.0);
            lu.vertical(|ui| {
                ui.add_space(14.0);
                label_trunc(ui, &c.name, bold(14.5), pal.fg, w).on_hover_text(&c.name);
                let a = if c.artist.is_empty() { src_name(&c.source, self.en()) } else { c.artist.clone() };
                let r = label_trunc(ui, &a, FontId::proportional(13.0), pal.mut_, w);
                if !c.artist.is_empty() && !mini && r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() { self.pages.acts.push(Act::Artist(c.artist.clone())); }
            });
            if !mini { self.rate_buttons(&mut lu, c); }
        } else {
            let (cr, _) = lu.allocate_exact_size(Vec2::splat(56.0), Sense::hover());
            paint_cover(&lu, &pal, &self.imgs, "", cr, 8);
            lu.add_space(10.0);
            lu.label(RichText::new(self.t("nothingPlaying")).color(pal.mut_));
        }
        // справа: громкость, скорость, текст, очередь, сцена, мини
        let rw = if mini { 150.0 } else { 330.0 };
        let right = Rect::from_min_max(Pos2::new(full.right() - rw, full.top()), full.max);
        let mut ru = ui.new_child(egui::UiBuilder::new().max_rect(right.shrink2(Vec2::new(12.0, 0.0))).layout(Layout::right_to_left(Align::Center)));
        if icon_btn(&mut ru, &pal, if self.mini { "expand" } else { "mini" }, 30.0, self.mini, self.t(if self.mini { "miniOff" } else { "mini" })).clicked() { self.toggle_mini(); }
        if !mini {
            if icon_btn(&mut ru, &pal, "queue", 30.0, self.show_q, &format!("{} (Q)", self.t("queue"))).clicked() { self.show_q = !self.show_q; self.show_l = false; }
            if icon_btn(&mut ru, &pal, "lyrics", 30.0, self.show_l, &format!("{} (T)", self.t("lyrics"))).clicked() { self.show_l = !self.show_l; self.show_q = false; if self.show_l { self.load_lyrics(); } }
            if icon_btn(&mut ru, &pal, "karaoke", 30.0, self.karaoke, self.t("karaokeHint")).clicked() { self.karaoke = !self.karaoke; self.stage = self.karaoke; if self.karaoke { self.load_lyrics(); } }
            if icon_btn(&mut ru, &pal, "video", 30.0, self.stage && !self.karaoke, self.t("videoHint")).clicked() { self.stage = !(self.stage && !self.karaoke); self.karaoke = false; }
            let rate = self.prefs.rate;
            let sp = icon_btn(&mut ru, &pal, "speed", 30.0, (rate - 1.0).abs() > 0.01, &format!("{}: {rate}×", self.t("speed")));
            egui::Popup::menu(&sp).show(|ui| {
                for r in [0.75f32, 1.0, 1.25, 1.5, 2.0] { if ui.selectable_label((self.prefs.rate - r).abs() < 0.01, format!("{r}×")).clicked() { self.prefs.rate = r; } }
            });
        }
        let mut v = self.prefs.volume;
        let (vr, ch) = slider(&mut ru, &pal, &mut v, 0.0, 1.0, if mini { 70.0 } else { 100.0 }, 0.05);
        if ch { self.prefs.volume = v; self.set_volume_live(v); }
        vr.on_hover_text(format!("{}: {}%", self.t("volume"), (v * 100.0).round()));
        let vic = if self.prefs.volume == 0.0 { "mute" } else if self.prefs.volume < 0.4 { "volumeLow" } else { "volume" };
        if icon_btn(&mut ru, &pal, vic, 30.0, false, &format!("{} (M)", self.t("volume"))).clicked() { self.prefs.volume = if self.prefs.volume == 0.0 { 0.8 } else { 0.0 }; self.set_volume_live(self.prefs.volume); }
        // центр: кнопки и полоса перемотки
        let cl = left.right() + 10.0;
        let cr_ = right.left() - 10.0;
        if cr_ - cl < 120.0 { return; }
        let center = Rect::from_min_max(Pos2::new(cl, full.top()), Pos2::new(cr_, full.bottom()));
        let bw = 36.0;
        let cy = center.top() + if mini { center.height() / 2.0 - 14.0 } else { 26.0 };
        let cx = center.center().x;
        let mut cbtn = |ui: &mut Ui, x: f32, icon: &str, size: f32, active: bool, tip: &str| -> egui::Response {
            let r = Rect::from_center_size(Pos2::new(x, cy), Vec2::splat(size));
            let mut c = ui.new_child(egui::UiBuilder::new().max_rect(r));
            icon_btn(&mut c, &pal, icon, size, active, tip)
        };
        if cbtn(ui, cx - bw * 2.3, "shuffle", 30.0, self.prefs.shuffle, &format!("{} (S)", self.t("shuffle"))).clicked() { self.prefs.shuffle = !self.prefs.shuffle; self.shuffle_changed(); }
        if cbtn(ui, cx - bw * 1.15, "prev", 34.0, false, self.t("prev")).clicked() { self.skip(-1); }
        // большая круглая кнопка
        let pr = Rect::from_center_size(Pos2::new(cx, cy), Vec2::splat(40.0));
        let presp = ui.interact(pr, ui.id().with("playbtn"), Sense::click());
        let hov = ui.ctx().animate_bool(presp.id, presp.hovered());
        ui.painter().circle_filled(pr.center(), 20.0 + hov * 1.5, pal.acc);
        let busy = self.route == Route::Waiting || (self.route == Route::Embed && self.embed.as_ref().is_some_and(|e| !e.st.lock().unwrap().ready) && cur.is_some());
        if busy { paint_spinner(ui, &pal, pr.shrink(8.0)); } else { paint_icon(ui.painter(), pr.shrink(10.0), if ps.playing { "pause" } else { "play" }, pal.on_acc, true); }
        if presp.on_hover_text(self.t(if ps.playing { "pause" } else { "play" })).clicked() { self.toggle(); }
        if cbtn(ui, cx + bw * 1.15, "next", 34.0, false, self.t("next")).clicked() { self.skip(1); }
        let rep = self.prefs.repeat.clone();
        if cbtn(ui, cx + bw * 2.3, if rep == "one" { "repeatOne" } else { "repeat" }, 30.0, rep != "off", &format!("{} (R)", self.t("repeat"))).clicked() {
            self.prefs.repeat = match rep.as_str() { "off" => "all", "all" => "one", _ => "off" }.into();
        }
        if mini { return; }
        // полоса перемотки: между тиками бегунок движется сам, перетаскивание не дёргает плеер до отпускания
        let by = center.top() + 60.0;
        let tw = 44.0;
        let bar = Rect::from_min_max(Pos2::new(center.left() + tw, by - 10.0), Pos2::new(center.right() - tw, by + 10.0));
        let resp = ui.interact(bar, ui.id().with("seek"), Sense::click_and_drag());
        let dur = ps.dur;
        let mut shown = ps.pos;
        if let Some(p) = resp.interact_pointer_pos() {
            if dur > 0.0 && (resp.dragged() || resp.is_pointer_button_down_on()) { self.seek_drag = Some(((p.x - bar.left()) / bar.width()).clamp(0.0, 1.0)); }
        }
        if let Some(f) = self.seek_drag {
            shown = f as f64 * dur;
            if resp.drag_stopped() || resp.clicked() || !resp.is_pointer_button_down_on() && !resp.dragged() {
                self.seek_drag = None;
                if dur > 0.0 { self.seek(f as f64 * dur); }
            }
        }
        if resp.hovered() && dur > 0.0 {
            if let Some(h) = resp.hover_pos() { resp.clone().on_hover_text_at_pointer(fmt_time(((h.x - bar.left()) / bar.width()).clamp(0.0, 1.0) as f64 * dur)); }
        }
        let f = if dur > 0.0 { (shown / dur) as f32 } else { 0.0 };
        paint_track(ui, &pal, bar, f, resp.hovered() || resp.dragged(), resp.id);
        ui.painter().text(Pos2::new(center.left() + tw - 8.0, by), Align2::RIGHT_CENTER, fmt_time(shown), FontId::proportional(12.0), pal.mut_);
        ui.painter().text(Pos2::new(center.right() - tw + 8.0, by), Align2::LEFT_CENTER, fmt_time(dur), FontId::proportional(12.0), pal.mut_);
        if ps.playing { repaint(ui.ctx(), 40); }
    }

    pub fn rate_buttons(&mut self, ui: &mut Ui, t: &Track) {
        let pal = self.pal;
        let v = self.ratings.get(&t.id).copied().unwrap_or(0);
        // в нижней панели и в строках порядок одинаковый: лайк, дизлайк, бан
        let items = [("like", 1, self.t("like")), ("dislike", -1, self.t("dislike")), ("ban", -2, self.t("ban"))];
        let mut out = None;
        let rtl = ui.layout().prefer_right_to_left();
        let iter: Vec<_> = if rtl { items.iter().rev().collect() } else { items.iter().collect() };
        for (ic, val, tip) in iter {
            let on = v == *val;
            let r = icon_btn(ui, &pal, ic, 28.0, on, tip);
            if on && *val < 0 { ui.painter().rect_stroke(r.rect.shrink(3.0), CornerRadius::same(6), Stroke::new(1.0, mix(pal.bg, pal.danger(), 0.5)), egui::StrokeKind::Inside); }
            if r.clicked() { out = Some(if on { 0 } else { *val }); }
        }
        if let Some(x) = out { self.pages.acts.push(Act::Rate(t.clone(), x)); }
    }

    fn paint_viz(&self, ui: &Ui, rect: Rect) {
        let pal = self.pal;
        let mut buf = vec![];
        self.eng.analyser(&mut buf);
        ui.painter().rect_filled(rect, CornerRadius::same(8), pal.sunk());
        if buf.len() < 512 { return; }
        let n = 24;
        let s = &buf[buf.len() - 512..];
        let sr = self.eng.sample_rate() as f32;
        let w = rect.width() / n as f32;
        for k in 0..n {
            // логарифмическая шкала 60 Гц…12 кГц, мощность через корреляцию с синусом/косинусом
            let f = 60.0 * (200.0f32).powf(k as f32 / (n - 1) as f32);
            let (mut re, mut im) = (0.0f32, 0.0f32);
            for (i, x) in s.iter().enumerate() { let a = std::f32::consts::TAU * f * i as f32 / sr; let hw = 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / 511.0).cos(); re += x * hw * a.cos(); im += x * hw * a.sin(); }
            let m = ((re * re + im * im).sqrt() / 40.0).clamp(0.0, 1.0).sqrt();
            let h = (rect.height() - 6.0) * m;
            ui.painter().rect_filled(Rect::from_min_max(Pos2::new(rect.left() + k as f32 * w + 1.0, rect.bottom() - 3.0 - h), Pos2::new(rect.left() + (k + 1) as f32 * w - 1.0, rect.bottom() - 3.0)), CornerRadius::same(1), pal.acc);
        }
        repaint(ui.ctx(), 40);
    }

    fn toggle_mini(&mut self) {
        self.mini = !self.mini;
        let c = &self.ctx;
        if self.mini {
            c.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(Vec2::new(360.0, 106.0)));
            c.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(560.0, 106.0)));
            c.send_viewport_cmd(egui::ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
        } else {
            c.send_viewport_cmd(egui::ViewportCommand::WindowLevel(egui::WindowLevel::Normal));
            c.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(1180.0, 760.0)));
            c.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(Vec2::new(800.0, 560.0)));
        }
    }

    // ---------- очередь ----------
    fn queue_panel(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let up = self.upcoming();
        ui.horizontal(|ui| {
            ui.label(RichText::new(self.t("queue")).font(bold(17.0)));
            if !up.is_empty() { ui.label(RichText::new(up.len().to_string()).color(pal.mut_)); }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_btn(ui, &pal, "close", 28.0, false, self.t("close")).clicked() { self.show_q = false; }
                if icon_btn(ui, &pal, "trash", 28.0, false, self.t("clearQueue")).clicked() && !up.is_empty() {
                    let old = self.order.clone();
                    self.order.truncate(self.pos as usize + 1);
                    self.mark_session();
                    self.toast_act(self.t("queueCleared"), ToastKind::Info, Some((self.t("undo").into(), Box::new(move |a: &mut SiloApp| { a.order = old; a.mark_session(); }))));
                }
                if icon_btn(ui, &pal, "save", 28.0, false, self.t("saveQueue")).clicked() {
                    let mut list: Vec<Track> = self.cur().cloned().into_iter().collect();
                    list.extend(up.iter().cloned());
                    if !list.is_empty() {
                        let name = format!("{} {}", self.t("queue"), now_str());
                        match crate::backend::save_playlist(&self.be, name, list) { Ok(_) => { self.refresh_playlists(); self.toast(self.t("queueSaved"), ToastKind::Ok); } Err(e) => self.err(e) }
                    }
                }
            });
        });
        ui.add_space(8.0);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if let Some(c) = self.cur().cloned() {
                ui.label(RichText::new(self.t("nowPlaying")).size(12.0).color(pal.mut_));
                let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 52.0), Sense::hover());
                ui.painter().rect_filled(r, CornerRadius::same(8), pal.acc_soft());
                paint_cover(ui, &pal, &self.imgs, &c.image, Rect::from_min_size(r.min + Vec2::new(6.0, 6.0), Vec2::splat(40.0)), 6);
                paint_trunc(ui, r.min + Vec2::new(54.0, 9.0), &c.name, bold(13.5), pal.fg, r.width() - 90.0, Align2::LEFT_TOP);
                paint_trunc(ui, r.min + Vec2::new(54.0, 28.0), &c.artist, FontId::proportional(12.5), pal.mut_, r.width() - 90.0, Align2::LEFT_TOP);
                bars(ui, &pal, Rect::from_min_size(Pos2::new(r.right() - 26.0, r.center().y - 8.0), Vec2::new(16.0, 16.0)), self.ps.playing);
                ui.add_space(8.0);
            }
            if up.is_empty() {
                empty(ui, &pal, "queue", self.t("queueEmptyT"), self.t("queueEmpty"), |_| {});
                return;
            }
            ui.label(RichText::new(self.t(if self.wave { "upNext" } else { "nextUp" })).size(12.0).color(pal.mut_));
            let mut act: Option<(u8, usize, usize)> = None;
            for (i, t) in up.iter().enumerate().take(300) {
                let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 50.0), Sense::click_and_drag());
                let hov = resp.hovered();
                if hov { ui.painter().rect_filled(r, CornerRadius::same(8), pal.hover()); }
                resp.dnd_set_drag_payload(i);
                if let Some(from) = resp.dnd_release_payload::<usize>() { act = Some((1, *from, i)); }
                if resp.dnd_hover_payload::<usize>().is_some() { ui.painter().hline(r.x_range(), r.top(), Stroke::new(2.0, pal.acc)); }
                paint_icon(ui.painter(), Rect::from_min_size(r.min + Vec2::new(2.0, 17.0), Vec2::splat(14.0)), "grip", pal.mut_, false);
                paint_cover(ui, &pal, &self.imgs, &t.image, Rect::from_min_size(r.min + Vec2::new(20.0, 6.0), Vec2::splat(38.0)), 6);
                paint_trunc(ui, r.min + Vec2::new(66.0, 8.0), &t.name, bold(13.0), pal.fg, r.width() - 104.0, Align2::LEFT_TOP);
                paint_trunc(ui, r.min + Vec2::new(66.0, 27.0), &t.artist, FontId::proportional(12.0), pal.mut_, r.width() - 104.0, Align2::LEFT_TOP);
                if hov {
                    let xr = Rect::from_center_size(Pos2::new(r.right() - 18.0, r.center().y), Vec2::splat(26.0));
                    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(xr));
                    if icon_btn(&mut c, &pal, "close", 26.0, false, self.t("removeFromQueue")).clicked() { act = Some((2, i, 0)); }
                }
                if resp.clicked() && act.is_none() { act = Some((0, i, 0)); }
            }
            match act {
                Some((0, i, _)) => { self.pos += 1 + i as isize; }
                Some((1, a, b)) => self.move_queue(a, b),
                Some((2, i, _)) => self.remove_from_queue(i),
                _ => {}
            }
        });
    }

    // ---------- текст песни ----------
    fn lyrics_panel(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        ui.horizontal(|ui| {
            ui.label(RichText::new(self.t("lyrics")).font(bold(17.0)));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| { if icon_btn(ui, &pal, "close", 28.0, false, self.t("close")).clicked() { self.show_l = false; } });
        });
        self.load_lyrics();
        let pos = self.ps.pos;
        let Some(l) = &self.lyrics else { return };
        if l.loading { ui.add_space(30.0); ui.vertical_centered(|ui| spinner(ui, &pal, 24.0)); return; }
        let lines = l.lines.clone();
        let plain = l.plain.clone();
        let mut seek_to = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if !lines.is_empty() {
                let cur = lines.iter().rposition(|x| x.0 <= pos);
                for (i, (at, text)) in lines.iter().enumerate() {
                    let on = Some(i) == cur;
                    let r = ui.add(egui::Label::new(RichText::new(if text.is_empty() { "♪" } else { text }).size(if on { 17.0 } else { 15.0 }).color(if on { pal.fg } else { pal.mut_ })).sense(Sense::click()).wrap());
                    if on && self.ps.playing { r.scroll_to_me(Some(Align::Center)); }
                    if r.clicked() { seek_to = Some(*at); }
                }
            } else if !plain.is_empty() {
                ui.label(RichText::new(plain).color(pal.fg));
            } else {
                empty(ui, &pal, "lyrics", self.t("noLyrics"), "", |_| {});
            }
        });
        if let Some(s) = seek_to { self.seek(s); }
    }

    // ---------- сцена: клип и караоке ----------
    fn stage(&mut self, ui: &mut Ui, rect: Rect) {
        let pal = self.pal;
        ui.painter().rect_filled(rect, CornerRadius::ZERO, Color32::from_rgb(8, 8, 10));
        let cur = self.cur().cloned();
        // фон: клип (yt-dlp), официальный плеер или крупная обложка
        let mut has_video = false;
        if self.route == Route::Embed && self.embed.is_some() {
            let r = [rect.left(), rect.top(), rect.width(), rect.height()];
            if self.embed_rect != Some(r) { if let Some(e) = &self.embed { e.set_rect(r); } self.embed_rect = Some(r); }
            has_video = true;
        } else if let Some(v) = self.video.as_mut() {
            let (pos, pl) = (self.ps.pos, self.ps.playing);
            if let Some(tid) = v.update(&self.ctx, pos, pl) {
                let ar = v.aspect;
                let (w, h) = if rect.width() / rect.height() > ar { (rect.height() * ar, rect.height()) } else { (rect.width(), rect.width() / ar) };
                egui::Image::new((tid, Vec2::new(w, h))).paint_at(ui, Rect::from_center_size(rect.center(), Vec2::new(w, h)));
                has_video = true;
            } else {
                let st = v.status();
                ui.painter().text(rect.center() + Vec2::new(0.0, 140.0), Align2::CENTER_CENTER, if st == "…" { "Загружаю клип…".to_string() } else { st }, FontId::proportional(13.0), Color32::from_gray(150));
            }
        }
        if !has_video {
            if let Some(c) = &cur {
                let s = (rect.height() * 0.5).min(rect.width() * 0.5).min(420.0);
                paint_cover(ui, &pal, &self.imgs, &c.image, Rect::from_center_size(rect.center() - Vec2::new(0.0, if self.karaoke { rect.height() * 0.18 } else { 0.0 }), Vec2::splat(s)), 14);
                if cur.as_ref().is_some_and(|c| c.source == "youtube") && self.prefs.yt_mode == "ytdlp" && !self.prefs.ytdlp_video {
                    ui.painter().text(rect.center_bottom() - Vec2::new(0.0, 30.0), Align2::CENTER_CENTER, self.t("ytDlpVideo"), FontId::proportional(12.0), Color32::from_gray(140));
                }
            }
        }
        // караоке: бегущий снизу вверх текст поверх клипа; текущая строка крупная и яркая
        if self.karaoke && !(self.route == Route::Embed && self.embed.is_some()) {
            self.load_lyrics();
            let pos = self.ps.pos + 0.15;
            let kp = ui.painter_at(rect);
            if let Some(l) = &self.lyrics {
                if !l.lines.is_empty() {
                    // подложка для читаемости
                    let shade = Rect::from_min_max(Pos2::new(rect.left(), rect.top() + rect.height() * 0.35), rect.max);
                    let mesh = {
                        let mut m = egui::Mesh::default();
                        let (t, b) = (Color32::from_black_alpha(0), Color32::from_black_alpha(190));
                        m.colored_vertex(shade.left_top(), t); m.colored_vertex(shade.right_top(), t); m.colored_vertex(shade.right_bottom(), b); m.colored_vertex(shade.left_bottom(), b);
                        m.add_triangle(0, 1, 2); m.add_triangle(0, 2, 3); m
                    };
                    kp.add(mesh);
                    let lh = (rect.height() / 11.0).clamp(36.0, 64.0);
                    let cur_i = l.lines.iter().rposition(|x| x.0 <= pos);
                    // строки плавно едут вверх: текущая строка подъезжает к опорной линии
                    let target = cur_i.map(|i| i as f32).unwrap_or(-1.0);
                    let anim = ui.ctx().animate_value_with_time(ui.id().with("kara"), target, 0.45);
                    let anchor = rect.top() + rect.height() * 0.72;
                    for (i, (_, text)) in l.lines.iter().enumerate() {
                        let y = anchor + (i as f32 - anim) * lh;
                        if y < rect.top() + rect.height() * 0.3 || y > rect.bottom() + lh { continue; }
                        let on = Some(i) == cur_i;
                        let fade = (1.0 - ((y - anchor).abs() / (rect.height() * 0.45))).clamp(0.15, 1.0);
                        let size = if on { lh * 0.62 } else { lh * 0.44 };
                        let col = if on { pal.acc } else { Color32::from_white_alpha((235.0 * fade) as u8) };
                        let t = if text.is_empty() { "♪" } else { text.as_str() };
                        let font = if on { bold(size) } else { FontId::proportional(size) };
                        let g = kp.layout(t.to_string(), font, col, rect.width() * 0.9);
                        let p = Pos2::new(rect.center().x - g.size().x / 2.0, y - g.size().y / 2.0);
                        kp.galley(p + Vec2::new(0.0, 2.0), kp.layout(t.to_string(), if on { bold(size) } else { FontId::proportional(size) }, Color32::from_black_alpha(200), rect.width() * 0.9), Color32::BLACK);
                        kp.galley(p, g, col);
                    }
                    repaint(ui.ctx(), 33);
                } else if !l.loading {
                    kp.text(rect.center_bottom() - Vec2::new(0.0, 60.0), Align2::CENTER_CENTER, self.t("noLyrics"), FontId::proportional(16.0), Color32::from_gray(200));
                }
            }
        }
        // кнопка закрытия сцены
        let cr = Rect::from_min_size(rect.right_top() + Vec2::new(-46.0, 12.0), Vec2::splat(34.0));
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(cr));
        if !(self.route == Route::Embed && self.embed.is_some()) && icon_btn(&mut c, &pal, "close", 34.0, false, "Esc").clicked() { self.stage = false; self.karaoke = false; }
        if self.ps.playing { repaint(ui.ctx(), 40); }
    }

    // ---------- перетаскивание файлов в окно ----------
    fn drop_files(&mut self, ctx: &egui::Context) {
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        let dropped: Vec<std::path::PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if hovering {
            let r = ctx.content_rect();
            let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop")));
            p.rect_filled(r, CornerRadius::ZERO, Color32::from_black_alpha(150));
            p.text(r.center(), Align2::CENTER_CENTER, self.t("dropHere"), bold(22.0), Color32::WHITE);
        }
        if dropped.is_empty() { return; }
        let exts = ["mp3", "flac", "ogg", "opus", "m4a", "wav", "aac"];
        let list: Vec<Track> = dropped.iter().filter(|p| p.extension().and_then(|e| e.to_str()).is_some_and(|e| exts.contains(&e.to_lowercase().as_str()))).map(|p| {
            let f = p.to_string_lossy().to_string();
            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
            let (a, b) = stem.split_once(" - ").map(|(a, b)| (a.trim().to_string(), b.trim().to_string())).unwrap_or((String::new(), stem.clone()));
            let mut t = Track::stub(&format!("local:file:{f}"), vec![]);
            t.source = "local".into(); t.name = b; t.artist = a; t.audio = f;
            t
        }).collect();
        if list.is_empty() { self.toast(self.t("dropBad"), ToastKind::Info); } else { self.play(list, 0, false); }
    }

    // ---------- тосты ----------
    fn toasts_ui(&mut self, ctx: &egui::Context) {
        if self.toasts.is_empty() { return; }
        let pal = self.pal;
        let r = ctx.content_rect();
        let mut y = r.bottom() - FOOT_H - 14.0;
        let mut run: Option<usize> = None;
        let mut close: Option<u64> = None;
        for (k, t) in self.toasts.iter().enumerate().rev() {
            let id = egui::Id::new(("toast", t.id));
            let w = 380.0f32.min(r.width() - 40.0);
            let area = egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(Pos2::new(r.right() - w - 20.0, y - 56.0)).show(ctx, |ui| {
                egui::Frame::new().fill(pal.panel).stroke(Stroke::new(1.0, pal.line)).corner_radius(10).inner_margin(egui::Margin::symmetric(12, 10)).shadow(ui.visuals().popup_shadow).show(ui, |ui| {
                    ui.set_width(w - 24.0);
                    ui.horizontal(|ui| {
                        let (ic, col) = match t.kind { ToastKind::Ok => ("check", pal.ok()), ToastKind::Error => ("alert", pal.danger()), ToastKind::Info => ("info", pal.acc) };
                        let (rr, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
                        paint_icon(ui.painter(), rr, ic, col, false);
                        ui.add(egui::Label::new(RichText::new(&t.text).color(pal.fg)).wrap());
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if icon_btn(ui, &pal, "close", 22.0, false, "").clicked() { close = Some(t.id); }
                            if let Some((label, _)) = &t.action { if btn_sm(ui, &pal, V::Ghost, None, label, true).clicked() { run = Some(k); } }
                        });
                    });
                });
            });
            y -= area.response.rect.height() + 8.0;
        }
        if let Some(k) = run { let t = self.toasts.remove(k); if let Some((_, j)) = t.action { j(self); } }
        if let Some(id) = close { self.toasts.retain(|t| t.id != id); }
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
    }

    // ---------- диалоги ----------
    fn dialogs(&mut self, ctx: &egui::Context) {
        if self.asks.is_empty() { return; }
        let pal = self.pal;
        let mut done: Option<Option<String>> = None; // Some(None) отмена, Some(Some(v)) ок
        let en = self.en();
        let t = |k| crate::i18n::tr(k, en);
        let mut info_act: Option<Act> = None;
        let resp = egui::Modal::new(egui::Id::new("ask")).frame(egui::Frame::new().fill(pal.panel).corner_radius(14).inner_margin(egui::Margin::same(20)).stroke(Stroke::new(1.0, pal.line))).show(ctx, |ui| {
            ui.set_width(420.0);
            match &mut self.asks[0] {
                Ask::Confirm { title, body, ok, danger, check, .. } => {
                    ui.label(RichText::new(title.as_str()).font(bold(18.0)));
                    ui.add_space(6.0);
                    ui.label(RichText::new(body.as_str()).color(pal.mut_));
                    if let Some((label, v)) = check { ui.add_space(6.0); ui.checkbox(v, label.as_str()); }
                    ui.add_space(14.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if btn(ui, &pal, if *danger { V::Danger } else { V::Primary }, None, ok, false).clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) { done = Some(Some(String::new())); }
                        if btn(ui, &pal, V::Solid, None, t("cancel"), false).clicked() { done = Some(None); }
                    });
                }
                Ask::Prompt { title, value, placeholder, ok, .. } => {
                    ui.label(RichText::new(title.as_str()).font(bold(18.0)));
                    ui.add_space(10.0);
                    let r = ui.add(egui::TextEdit::singleline(value).hint_text(placeholder.as_str()).desired_width(f32::INFINITY));
                    if !r.has_focus() && !r.lost_focus() { r.request_focus(); }
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.add_space(14.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if (btn_off(ui, &pal, V::Primary, None, ok, !value.trim().is_empty()).clicked() || enter) && !value.trim().is_empty() { done = Some(Some(value.trim().to_string())); }
                        if btn(ui, &pal, V::Solid, None, t("cancel"), false).clicked() { done = Some(None); }
                    });
                }
                Ask::Info(tr) => {
                    ui.label(RichText::new(t("about")).font(bold(18.0)));
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        cover(ui, &pal, &self.imgs, &tr.image, Vec2::splat(64.0), 8);
                        ui.vertical(|ui| { ui.label(RichText::new(&tr.name).font(bold(15.0))); ui.label(RichText::new(&tr.artist).color(pal.mut_)); });
                    });
                    ui.add_space(10.0);
                    egui::Grid::new("info").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                        if !tr.artist.is_empty() { ui.label(RichText::new(t("artist")).color(pal.mut_)); if ui.link(&tr.artist).clicked() { info_act = Some(Act::Artist(tr.artist.clone())); } ui.end_row(); }
                        if !tr.album.is_empty() { ui.label(RichText::new(t("album")).color(pal.mut_)); ui.label(&tr.album); ui.end_row(); }
                        ui.label(RichText::new(t("source")).color(pal.mut_)); ui.label(src_name(&tr.source, en)); ui.end_row();
                        if tr.duration > 0 { ui.label(RichText::new(t("duration")).color(pal.mut_)); ui.label(fmt_dur(tr.duration as f64)); ui.end_row(); }
                        if !tr.license_url.is_empty() { ui.label(RichText::new(t("license")).color(pal.mut_)); if ui.link(tr.license_url.trim_start_matches("https://").trim_start_matches("http://")).clicked() { info_act = Some(Act::OpenUrl(tr.license_url.clone())); } ui.end_row(); }
                        if !tr.tags.is_empty() { ui.label(RichText::new(t("tags")).color(pal.mut_)); ui.label(tr.tags.iter().take(12).cloned().collect::<Vec<_>>().join(", ")); ui.end_row(); }
                    });
                    ui.add_space(14.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if btn(ui, &pal, V::Primary, None, t("close"), false).clicked() { done = Some(None); }
                        if !tr.url.is_empty() && btn(ui, &pal, V::Solid, Some("external"), t("openSource"), false).clicked() { info_act = Some(Act::OpenUrl(tr.url.clone())); }
                    });
                }
            }
        });
        if resp.should_close() && done.is_none() { done = Some(None); }
        if let Some(a) = info_act { if matches!(a, Act::Artist(_)) { done = Some(None); } self.pages.acts.push(a); }
        if let Some(r) = done {
            let a = self.asks.remove(0);
            match (a, r) {
                (Ask::Confirm { on_ok, check, .. }, Some(_)) => on_ok(self, check.map(|c| c.1).unwrap_or(false)),
                (Ask::Prompt { on_ok, .. }, Some(v)) => on_ok(self, v),
                _ => {}
            }
            self.resize_edges(&ctx);
        let acts: Vec<Act> = std::mem::take(&mut self.pages.acts);
            for a in acts { self.apply(a); }
        }
    }
}

pub fn src_name(s: &str, en: bool) -> String {
    match s { "jamendo" => "Jamendo", "deezer" => "Deezer", "youtube" => "YouTube", "soundcloud" => "SoundCloud", "ccmixter" => "ccMixter", "archive" => "Internet Archive", "local" => if en { "Downloaded" } else { "Скачано" }, x => x }.to_string()
}
pub fn now_str() -> String {
    // дата и время для имени плейлиста очереди
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0) + 3 * 3600;
    let (d, r) = (s.div_euclid(86400), s.rem_euclid(86400));
    // дни с 1970-01-01 → дата (алгоритм Хиннанта)
    let z = d + 719468; let era = z.div_euclid(146097); let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; let y = yoe + era * 400; let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); let mp = (5 * doy + 2) / 153;
    let dd = doy - (153 * mp + 2) / 5 + 1; let mm = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:02}.{:02}.{} {:02}:{:02}", dd, mm, if mm <= 2 { y + 1 } else { y }, r / 3600, r % 3600 / 60)
}
