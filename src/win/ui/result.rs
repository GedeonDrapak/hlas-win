//! "Your text" window: shown when a transcript could not be pasted (app
//! switched, elevated target, possible background audio, Smart text failed,
//! audio file import). The text is editable and one click copies it.

use super::controls::{self as c, Theme};
use super::skin::{self, ds, Kind, Skin, Text};
use crate::win::clipboard;
use native_windows_gui as nwg;
use nwg::NwgError;
use std::cell::RefCell;
use std::rc::Rc;

pub struct ResultWindow {
    window: nwg::Window,
    skin: Rc<Skin>,
    message: Text,
    text: nwg::TextBox,
    show_original: nwg::Button,
    original_label: Text,
    copy: nwg::Button,
    close: nwg::Button,
    content: RefCell<(String, String)>,
    handler: RefCell<Option<nwg::EventHandler>>,
}

/// Group of the "show original" switch, shown only when there is one.
const ORIGINAL: u8 = 1;

impl ResultWindow {
    pub fn build(theme: Rc<Theme>) -> Result<Rc<ResultWindow>, NwgError> {
        let t = theme.clone();
        let w = c::window(&t, "Your text", (580, 420), false)?;
        let skin = Skin::new(theme, &w, ds::BG);
        let s = &*skin;
        s.label(
            0,
            "Your text",
            (24, 18, 532, 30),
            &t.heading,
            ds::FG,
            skin::LINE,
        );
        let message = s.label(0, "", (24, 50, 532, 40), &t.body, ds::FG2, skin::WRAP);
        let text = s.text_box(0, (24, 94, 532, 236), false)?;
        let show_original = s.toggle(ORIGINAL, 24, 350, ds::BG, false)?;
        let original_label = s.label(
            ORIGINAL,
            "Show original transcript",
            (74, 344, 220, 34),
            &t.body,
            ds::FG2,
            skin::LINE,
        );
        let close = s.button(0, "Close", (330, 362, 110, 38), Kind::Ghost, ds::BG)?;
        let copy = s.button(0, "Copy text", (446, 362, 110, 38), Kind::Primary, ds::BG)?;
        let ui = Rc::new(ResultWindow {
            window: w,
            skin,
            message,
            text,
            show_original,
            original_label,
            copy,
            close,
            content: RefCell::new((String::new(), String::new())),
            handler: RefCell::new(None),
        });
        let weak = Rc::downgrade(&ui);
        let handler = nwg::full_bind_event_handler(&ui.window.handle, move |evt, data, handle| {
            let Some(ui) = weak.upgrade() else { return };
            use nwg::Event as E;
            if evt == E::OnButtonClick {
                ui.skin.click(&handle);
            }
            match evt {
                E::OnWindowClose if handle == ui.window.handle => {
                    if let nwg::EventData::OnWindowClose(d) = data {
                        d.close(false);
                    }
                    ui.window.set_visible(false);
                }
                E::OnButtonClick if handle == ui.close.handle => ui.window.set_visible(false),
                E::OnButtonClick if handle == ui.copy.handle => {
                    let ok = clipboard::set_text(&c::box_text(&ui.text), false).is_ok();
                    ui.copy.set_text(if ok { "Copied" } else { "Try again" });
                }
                E::OnButtonClick if handle == ui.show_original.handle => {
                    let (text, original) = ui.content.borrow().clone();
                    let original_on = ui.skin.is_on(&ui.show_original.handle);
                    ui.original_label
                        .set_color(if original_on { ds::FG } else { ds::FG2 });
                    c::set_box_text(&ui.text, if original_on { &original } else { &text });
                    ui.copy.set_text("Copy text");
                }
                _ => {}
            }
        });
        *ui.handler.borrow_mut() = Some(handler);
        Ok(ui)
    }

    pub fn show(&self, text: &str, original: &str, message: &str) {
        *self.content.borrow_mut() = (text.to_string(), original.to_string());
        self.message.set_text(message);
        c::set_box_text(&self.text, text);
        self.skin.set_on(&self.show_original.handle, false);
        self.original_label.set_color(ds::FG2);
        self.skin
            .set_visible(if original != text { 1 << ORIGINAL } else { 0 });
        self.copy.set_text("Copy text");
        c::present(&self.window);
        // Keep it above the app the user was dictating into.
        if let Some(hwnd) = self.window.handle.hwnd() {
            unsafe {
                use windows::Win32::UI::WindowsAndMessaging::*;
                let _ = SetWindowPos(
                    windows::Win32::Foundation::HWND(hwnd as _),
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE,
                );
            }
        }
        self.copy.set_focus();
    }
}
