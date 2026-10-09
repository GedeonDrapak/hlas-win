//! Shared fonts and small builders so each window reads as layout, not
//! boilerplate. Positions and sizes are logical pixels; nwg scales them.

use native_windows_gui as nwg;
use nwg::NwgError;

pub struct Theme {
    pub body: nwg::Font,
    pub small: nwg::Font,
    pub bold: nwg::Font,
    pub title: nwg::Font,
    pub section: nwg::Font,
    pub icon: Option<nwg::Icon>,
    /// The window background, so check boxes and radios blend in.
    pub bg: [u8; 3],
}

fn font(size: u32, weight: u32) -> Result<nwg::Font, NwgError> {
    let mut f = nwg::Font::default();
    nwg::Font::builder()
        .family("Segoe UI")
        .size_absolute(size)
        .weight(weight)
        .build(&mut f)?;
    Ok(f)
}

impl Theme {
    pub fn new() -> Result<Theme, NwgError> {
        let body = font(15, 400)?;
        let _ = nwg::Font::set_global_family("Segoe UI");
        let icon = nwg::EmbedResource::load(None)
            .ok()
            .and_then(|e| e.icon(1, None));
        let bg = unsafe {
            let c = windows::Win32::Graphics::Gdi::GetSysColor(
                windows::Win32::Graphics::Gdi::COLOR_WINDOW,
            );
            [
                (c & 0xFF) as u8,
                ((c >> 8) & 0xFF) as u8,
                ((c >> 16) & 0xFF) as u8,
            ]
        };
        Ok(Theme {
            body,
            small: font(13, 400)?,
            bold: font(15, 600)?,
            title: font(26, 700)?,
            section: font(12, 700)?,
            icon,
            bg,
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
        .icon(theme.icon.as_ref())
        .build(&mut w)?;
    Ok(w)
}

pub fn label(
    parent: &nwg::Window,
    text: &str,
    pos: (i32, i32),
    size: (i32, i32),
    font: &nwg::Font,
) -> Result<nwg::Label, NwgError> {
    let mut l = nwg::Label::default();
    nwg::Label::builder()
        .text(text)
        .position(pos)
        .size(size)
        .font(Some(font))
        .parent(parent)
        .build(&mut l)?;
    Ok(l)
}

pub fn button(
    parent: &nwg::Window,
    text: &str,
    pos: (i32, i32),
    size: (i32, i32),
    font: &nwg::Font,
) -> Result<nwg::Button, NwgError> {
    let mut b = nwg::Button::default();
    nwg::Button::builder()
        .text(text)
        .position(pos)
        .size(size)
        .font(Some(font))
        .parent(parent)
        .build(&mut b)?;
    Ok(b)
}

pub fn input(
    parent: &nwg::Window,
    text: &str,
    pos: (i32, i32),
    size: (i32, i32),
    font: &nwg::Font,
    secret: bool,
) -> Result<nwg::TextInput, NwgError> {
    let mut t = nwg::TextInput::default();
    nwg::TextInput::builder()
        .text(text)
        .position(pos)
        .size(size)
        .font(Some(font))
        .password(if secret { Some('\u{2022}') } else { None })
        .parent(parent)
        .build(&mut t)?;
    Ok(t)
}

pub fn text_box(
    parent: &nwg::Window,
    pos: (i32, i32),
    size: (i32, i32),
    font: &nwg::Font,
    readonly: bool,
) -> Result<nwg::TextBox, NwgError> {
    let mut t = nwg::TextBox::default();
    nwg::TextBox::builder()
        .position(pos)
        .size(size)
        .font(Some(font))
        .readonly(readonly)
        .flags(
            nwg::TextBoxFlags::VISIBLE
                | nwg::TextBoxFlags::VSCROLL
                | nwg::TextBoxFlags::AUTOVSCROLL
                | nwg::TextBoxFlags::TAB_STOP,
        )
        .parent(parent)
        .build(&mut t)?;
    Ok(t)
}

pub fn check(
    parent: &nwg::Window,
    theme: &Theme,
    text: &str,
    pos: (i32, i32),
    size: (i32, i32),
    checked: bool,
) -> Result<nwg::CheckBox, NwgError> {
    let mut c = nwg::CheckBox::default();
    nwg::CheckBox::builder()
        .text(text)
        .position(pos)
        .size(size)
        .font(Some(&theme.body))
        .background_color(Some(theme.bg))
        .check_state(if checked {
            nwg::CheckBoxState::Checked
        } else {
            nwg::CheckBoxState::Unchecked
        })
        .parent(parent)
        .build(&mut c)?;
    Ok(c)
}

pub fn radio(
    parent: &nwg::Window,
    theme: &Theme,
    text: &str,
    pos: (i32, i32),
    size: (i32, i32),
    first: bool,
) -> Result<nwg::RadioButton, NwgError> {
    let mut r = nwg::RadioButton::default();
    let mut flags = nwg::RadioButtonFlags::VISIBLE | nwg::RadioButtonFlags::TAB_STOP;
    if first {
        flags |= nwg::RadioButtonFlags::GROUP;
    }
    nwg::RadioButton::builder()
        .text(text)
        .position(pos)
        .size(size)
        .flags(flags)
        .font(Some(&theme.body))
        .background_color(Some(theme.bg))
        .parent(parent)
        .build(&mut r)?;
    Ok(r)
}

pub fn combo(
    parent: &nwg::Window,
    items: Vec<String>,
    selected: Option<usize>,
    pos: (i32, i32),
    width: i32,
    font: &nwg::Font,
) -> Result<nwg::ComboBox<String>, NwgError> {
    let mut c = nwg::ComboBox::default();
    nwg::ComboBox::builder()
        .collection(items)
        .selected_index(selected)
        .position(pos)
        .size((width, 26))
        .font(Some(font))
        .parent(parent)
        .build(&mut c)?;
    Ok(c)
}

pub fn progress(
    parent: &nwg::Window,
    pos: (i32, i32),
    size: (i32, i32),
) -> Result<nwg::ProgressBar, NwgError> {
    let mut p = nwg::ProgressBar::default();
    nwg::ProgressBar::builder()
        .position(pos)
        .size(size)
        .range(0..100)
        .parent(parent)
        .build(&mut p)?;
    Ok(p)
}

pub fn timer(parent: &nwg::Window, ms: u32) -> Result<nwg::AnimationTimer, NwgError> {
    let mut t = nwg::AnimationTimer::default();
    nwg::AnimationTimer::builder()
        .parent(parent)
        .interval(std::time::Duration::from_millis(ms as u64))
        .build(&mut t)?;
    Ok(t)
}

pub fn is_checked(c: &nwg::CheckBox) -> bool {
    c.check_state() == nwg::CheckBoxState::Checked
}

pub fn set_checked(c: &nwg::CheckBox, on: bool) {
    c.set_check_state(if on {
        nwg::CheckBoxState::Checked
    } else {
        nwg::CheckBoxState::Unchecked
    });
}

pub fn radio_on(r: &nwg::RadioButton) -> bool {
    r.check_state() == nwg::RadioButtonState::Checked
}

pub fn set_radio(r: &nwg::RadioButton, on: bool) {
    r.set_check_state(if on {
        nwg::RadioButtonState::Checked
    } else {
        nwg::RadioButtonState::Unchecked
    });
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
