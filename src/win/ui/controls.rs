//! Fonts and the few helpers every window shares. The look itself lives in
//! `skin`. Positions and sizes are logical pixels; nwg scales them.

use native_windows_gui as nwg;
use nwg::NwgError;

pub struct Theme {
    pub body: nwg::Font,
    pub small: nwg::Font,
    pub medium: nwg::Font,
    pub bold: nwg::Font,
    pub heading: nwg::Font,
    pub title: nwg::Font,
    pub section: nwg::Font,
    pub wordmark: nwg::Font,
    /// Segoe Fluent Icons (Windows 11) or Segoe MDL2 Assets (Windows 10).
    pub icon: nwg::Font,
    pub icon_small: nwg::Font,
    pub app_icon: Option<nwg::Icon>,
}

fn font(family: &str, size: u32, weight: u32) -> Result<nwg::Font, NwgError> {
    let mut f = nwg::Font::default();
    nwg::Font::builder()
        .family(family)
        .size_absolute(size)
        .weight(weight)
        .build(&mut f)?;
    Ok(f)
}

impl Theme {
    pub fn new() -> Result<Theme, NwgError> {
        super::skin::enable_dark_mode();
        // Satoshi, as on macOS; Segoe UI if the fonts cannot be registered.
        let (regular, medium) = if super::skin::load_fonts() {
            ("Satoshi", "Satoshi Medium")
        } else {
            log::warn!("bundled fonts not registered, using Segoe UI");
            ("Segoe UI", "Segoe UI Semibold")
        };
        let _ = nwg::Font::set_global_family(regular);
        let windows = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
        let icons = if std::path::Path::new(&windows)
            .join(r"Fonts\SegoeIcons.ttf")
            .exists()
        {
            "Segoe Fluent Icons"
        } else {
            "Segoe MDL2 Assets"
        };
        let app_icon = nwg::EmbedResource::load(None)
            .ok()
            .and_then(|e| e.icon(1, None));
        Ok(Theme {
            body: font(regular, 15, 400)?,
            small: font(regular, 13, 400)?,
            medium: font(medium, 15, 500)?,
            bold: font(regular, 15, 700)?,
            heading: font(regular, 20, 700)?,
            title: font(regular, 28, 700)?,
            section: font(medium, 11, 500)?,
            wordmark: font(regular, 17, 700)?,
            icon: font(icons, 18, 400)?,
            icon_small: font(icons, 15, 400)?,
            app_icon,
        })
    }
}

pub fn window(
    theme: &Theme,
    title: &str,
    size: (i32, i32),
    resizable: bool,
) -> Result<nwg::Window, NwgError> {
    let mut w = nwg::Window::default();
    let mut flags = nwg::WindowFlags::WINDOW | nwg::WindowFlags::MINIMIZE_BOX;
    if resizable {
        flags |= nwg::WindowFlags::RESIZABLE;
    }
    nwg::Window::builder()
        .title(title)
        .size(size)
        .center(true)
        .flags(flags)
        .icon(theme.app_icon.as_ref())
        .build(&mut w)?;
    Ok(w)
}

pub fn timer(parent: &nwg::Window, ms: u32) -> Result<nwg::AnimationTimer, NwgError> {
    let mut t = nwg::AnimationTimer::default();
    nwg::AnimationTimer::builder()
        .parent(parent)
        .interval(std::time::Duration::from_millis(ms as u64))
        .build(&mut t)?;
    Ok(t)
}

/// Edit controls want CRLF; the rest of Hlas uses LF.
pub fn box_text(t: &nwg::TextBox) -> String {
    t.text().replace("\r\n", "\n")
}

pub fn set_box_text(t: &nwg::TextBox, s: &str) {
    t.set_text(&s.replace("\r\n", "\n").replace('\n', "\r\n"));
}

/// Brings a window to the front and focuses it.
pub fn present(w: &nwg::Window) {
    w.set_visible(true);
    if let Some(hwnd) = w.handle.hwnd() {
        unsafe {
            let h = windows::Win32::Foundation::HWND(hwnd as _);
            let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                h,
                windows::Win32::UI::WindowsAndMessaging::SW_RESTORE,
            );
            let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(h);
        }
    }
}
