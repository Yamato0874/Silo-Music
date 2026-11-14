//! Связь с Windows: трей, глобальные клавиши, системная плашка (SMTC) с медиаклавишами,
//! уведомления, автозапуск, один экземпляр, открытие ссылок, дочернее окно WebView2.
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Mutex;

use eframe::egui;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use silo_core::model::Track;

pub enum Cmd { Toggle, Next, Prev, Show, Quit, Play, Pause, SeekTo(f64) }

struct Hwnd(isize);
impl HasWindowHandle for Hwnd {
    fn window_handle(&self) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        let h = raw_window_handle::Win32WindowHandle::new(std::num::NonZeroIsize::new(self.0).ok_or(raw_window_handle::HandleError::Unavailable)?);
        Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(RawWindowHandle::Win32(h)) })
    }
}

pub struct Platform {
    hwnd: isize,
    pub rx: Receiver<Cmd>,
    _tray: Option<tray_icon::TrayIcon>,
    _hk: Option<global_hotkey::GlobalHotKeyManager>,
    smtc: Mutex<Option<souvlaki::MediaControls>>,
    last_state: Mutex<(bool, i64)>,
}

fn app_icon() -> Option<(Vec<u8>, u32, u32)> {
    let im = image::load_from_memory(include_bytes!("../../app-icon.png")).ok()?.thumbnail(64, 64).to_rgba8();
    let (w, h) = im.dimensions();
    Some((im.into_raw(), w, h))
}
pub fn window_icon() -> Option<egui::IconData> { app_icon().map(|(rgba, width, height)| egui::IconData { rgba, width, height }) }

impl Platform {
    pub fn new(cc: &eframe::CreationContext<'_>, ctx: &egui::Context) -> Platform {
        let hwnd = match cc.window_handle().map(|h| h.as_raw()) { Ok(RawWindowHandle::Win32(h)) => h.hwnd.get(), _ => 0 };
        let (tx, rx) = std::sync::mpsc::channel::<Cmd>();
        let tray = make_tray(&tx, ctx);
        let hk = make_hotkeys(&tx, ctx);
        // системная плашка Windows и медиаклавиши
        // окно без системной рамки: просим Windows 11 скруглить углы сглаженно (иначе углы «в ступеньку»)
        if hwnd != 0 {
            let pref: u32 = 2; // DWMWCP_ROUND
            unsafe { windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute(hwnd as *mut std::ffi::c_void, 33, &pref as *const u32 as *const std::ffi::c_void, 4); }
        }
        let smtc = (hwnd != 0).then(|| {
            let cfg = souvlaki::PlatformConfig { display_name: "Silo Player", dbus_name: "silo_native", hwnd: Some(hwnd as *mut std::ffi::c_void) };
            let mut c = souvlaki::MediaControls::new(cfg).ok()?;
            let (t2, c2) = (tx.clone(), ctx.clone());
            c.attach(move |e| {
                use souvlaki::MediaControlEvent as E;
                let cmd = match e { E::Toggle => Cmd::Toggle, E::Play => Cmd::Play, E::Pause => Cmd::Pause, E::Next => Cmd::Next, E::Previous => Cmd::Prev, E::SetPosition(p) => Cmd::SeekTo(p.0.as_secs_f64()), _ => return };
                let _ = t2.send(cmd);
                c2.request_repaint();
            }).ok()?;
            Some(c)
        }).flatten();
        // второй запуск программы просит показать окно
        let (t3, c3) = (tx.clone(), ctx.clone());
        std::thread::spawn(move || single_instance_listener(move || { let _ = t3.send(Cmd::Show); c3.request_repaint(); }));
        Platform { hwnd, rx, _tray: tray, _hk: hk, smtc: Mutex::new(smtc), last_state: Mutex::new((false, -1)) }
    }
    pub fn make_embed(&self, dir: std::path::PathBuf) -> Result<crate::embed::Embed, String> {
        crate::embed::Embed::new(&Hwnd(self.hwnd), dir, [0.0, 0.0, 2.0, 2.0])
    }
    pub fn set_metadata(&self, t: &Track) {
        if let Some(c) = self.smtc.lock().unwrap().as_mut() {
            let _ = c.set_metadata(souvlaki::MediaMetadata { title: Some(&t.name), artist: Some(&t.artist), album: Some(&t.album), cover_url: (!t.image.is_empty()).then_some(t.image.as_str()), duration: (t.duration > 0).then(|| std::time::Duration::from_secs(t.duration as u64)) });
        }
    }
    /// Состояние для плашки: не чаще раза в секунду или при смене паузы.
    pub fn set_state(&self, playing: bool, pos: f64) {
        let mut l = self.last_state.lock().unwrap();
        let sec = pos as i64;
        if l.0 == playing && (sec - l.1).abs() < 2 && sec >= l.1 { return; }
        *l = (playing, sec);
        if let Some(c) = self.smtc.lock().unwrap().as_mut() {
            let progress = Some(souvlaki::MediaPosition(std::time::Duration::from_secs_f64(pos.max(0.0))));
            let _ = c.set_playback(if playing { souvlaki::MediaPlayback::Playing { progress } } else { souvlaki::MediaPlayback::Paused { progress } });
        }
    }
}

fn make_tray(tx: &Sender<Cmd>, ctx: &egui::Context) -> Option<tray_icon::TrayIcon> {
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    let en = false;
    let t = |k| crate::i18n::tr(k, en);
    let toggle = MenuItem::with_id("toggle", t("toggleT"), true, None);
    let next = MenuItem::with_id("next", t("nextT"), true, None);
    let prev = MenuItem::with_id("prev", t("prevT"), true, None);
    let show = MenuItem::with_id("show", t("show"), true, None);
    let quit = MenuItem::with_id("quit", t("quit"), true, None);
    let menu = Menu::with_items(&[&toggle, &next, &prev, &PredefinedMenuItem::separator(), &show, &quit]).ok()?;
    let (rgba, w, h) = app_icon()?;
    let icon = tray_icon::Icon::from_rgba(rgba, w, h).ok()?;
    let (t2, c2) = (tx.clone(), ctx.clone());
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        let cmd = match e.id.0.as_str() { "toggle" => Cmd::Toggle, "next" => Cmd::Next, "prev" => Cmd::Prev, "show" => Cmd::Show, "quit" => Cmd::Quit, _ => return };
        let _ = t2.send(cmd);
        c2.request_repaint();
    }));
    let (t3, c3) = (tx.clone(), ctx.clone());
    tray_icon::TrayIconEvent::set_event_handler(Some(move |e: tray_icon::TrayIconEvent| {
        if let tray_icon::TrayIconEvent::Click { button: tray_icon::MouseButton::Left, button_state: tray_icon::MouseButtonState::Up, .. } = e {
            let _ = t3.send(Cmd::Show);
            c3.request_repaint();
        }
    }));
    tray_icon::TrayIconBuilder::new().with_menu(Box::new(menu)).with_menu_on_left_click(false).with_tooltip("Silo Player (native)").with_icon(icon).build().ok()
}

fn make_hotkeys(tx: &Sender<Cmd>, ctx: &egui::Context) -> Option<global_hotkey::GlobalHotKeyManager> {
    use global_hotkey::hotkey::{Code, HotKey, Modifiers};
    let m = global_hotkey::GlobalHotKeyManager::new().ok()?;
    let keys = [(Code::Space, 0u8), (Code::ArrowRight, 1), (Code::ArrowLeft, 2)];
    let mut ids = vec![];
    for (k, n) in keys {
        let hk = HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), k);
        if m.register(hk).is_ok() { ids.push((hk.id(), n)); }
    }
    let (t2, c2) = (tx.clone(), ctx.clone());
    global_hotkey::GlobalHotKeyEvent::set_event_handler(Some(move |e: global_hotkey::GlobalHotKeyEvent| {
        if e.state != global_hotkey::HotKeyState::Pressed { return; }
        let Some((_, n)) = ids.iter().find(|x| x.0 == e.id) else { return };
        let _ = t2.send(match n { 0 => Cmd::Toggle, 1 => Cmd::Next, _ => Cmd::Prev });
        c2.request_repaint();
    }));
    Some(m)
}

pub fn notify(title: &str, body: &str) {
    let (t, b) = (title.to_string(), body.to_string());
    std::thread::spawn(move || { let _ = notify_rust::Notification::new().app_id("app.silo.player.native").summary(&t).body(&b).show(); });
}

/// Автозапуск с Windows (в трее): ключ Run в реестре текущего пользователя.
pub fn set_autostart(on: bool) {
    use windows_sys::Win32::System::Registry::*;
    let w = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let key = w("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    let name = w("SiloPlayerNative");
    unsafe {
        let mut hk: HKEY = std::ptr::null_mut();
        if RegOpenKeyExW(HKEY_CURRENT_USER, key.as_ptr(), 0, KEY_SET_VALUE, &mut hk) != 0 { return; }
        if on {
            let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
            let val = w(&format!("\"{exe}\" --tray"));
            RegSetValueExW(hk, name.as_ptr(), 0, REG_SZ, val.as_ptr() as *const u8, (val.len() * 2) as u32);
        } else {
            RegDeleteValueW(hk, name.as_ptr());
        }
        RegCloseKey(hk);
    }
}

/// Открыть https-ссылку в системном браузере (страница трека, лицензия, получение ключей).
pub fn open_url(url: &str) -> Result<(), String> {
    if !url.starts_with("https://") && !url.starts_with("http://") { return Err("ссылка не разрешена".into()); }
    std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Полноэкранное окно другой программы (игра, видео): тяжёлые фоновые задачи на это время ждут.
pub fn fullscreen_app_active() -> bool {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId};
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() { return false; }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == std::process::id() { return false; }
        let mut cls = [0u16; 64];
        let n = GetClassNameW(hwnd, cls.as_mut_ptr(), 64).max(0) as usize;
        let name = String::from_utf16_lossy(&cls[..n]);
        if ["Progman", "WorkerW", "Shell_TrayWnd", "Shell_SecondaryTrayWnd"].contains(&name.as_str()) { return false; }
        let mut r: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut r) == 0 { return false; }
        let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(mon, &mut mi) == 0 { return false; }
        r.left <= mi.rcMonitor.left && r.top <= mi.rcMonitor.top && r.right >= mi.rcMonitor.right && r.bottom >= mi.rcMonitor.bottom
    }
}

const EVT: &str = "Local\\SiloPlayerNativeShow";
fn wide(s: &str) -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() }

/// Один экземпляр: true — мы первые. Иначе будим первый экземпляр и выходим.
pub fn single_instance() -> bool {
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows_sys::Win32::System::Threading::{CreateMutexW, OpenEventW, SetEvent, EVENT_MODIFY_STATE};
    unsafe {
        let name = wide("Local\\SiloPlayerNativeMutex");
        let h = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        if !h.is_null() && GetLastError() == ERROR_ALREADY_EXISTS {
            let e = OpenEventW(EVENT_MODIFY_STATE, 0, wide(EVT).as_ptr());
            if !e.is_null() { SetEvent(e); }
            return false;
        }
        true
    }
}
fn single_instance_listener(on: impl Fn()) {
    use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject, INFINITE};
    unsafe {
        let e = CreateEventW(std::ptr::null(), 0, 0, wide(EVT).as_ptr());
        if e.is_null() { return; }
        loop { if WaitForSingleObject(e, INFINITE) == 0 { on(); } else { break; } }
    }
}

/// Уведомления от имени Silo (а не Windows PowerShell): регистрируем AppUserModelID с именем и значком.
pub fn register_app_id(dir: &std::path::Path) {
    use windows_sys::Win32::System::Registry::*;
    let icon = dir.join("icon.png");
    if !icon.exists() { let _ = std::fs::write(&icon, include_bytes!("../../app-icon.png")); }
    unsafe {
        let mut hk: HKEY = std::ptr::null_mut();
        let key = wide(r"Software\Classes\AppUserModelId\app.silo.player.native");
        if RegCreateKeyExW(HKEY_CURRENT_USER, key.as_ptr(), 0, std::ptr::null(), 0, KEY_SET_VALUE, std::ptr::null(), &mut hk, std::ptr::null_mut()) != 0 { return; }
        for (n, v) in [("DisplayName", "Silo Player".to_string()), ("IconUri", icon.display().to_string())] {
            let (n, v) = (wide(n), wide(&v));
            RegSetValueExW(hk, n.as_ptr(), 0, REG_SZ, v.as_ptr() as *const u8, (v.len() * 2) as u32);
        }
        RegCloseKey(hk);
    }
}

/// Имя в плашке Windows вместо «Неизвестное приложение».
pub fn set_app_id() {
    // фоновые задачи уступают играм; звук играет в своём потоке с высоким приоритетом
    unsafe { windows_sys::Win32::System::Threading::SetPriorityClass(windows_sys::Win32::System::Threading::GetCurrentProcess(), windows_sys::Win32::System::Threading::BELOW_NORMAL_PRIORITY_CLASS); }
    let id = wide("app.silo.player.native");
    unsafe { windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(id.as_ptr()); }
}
