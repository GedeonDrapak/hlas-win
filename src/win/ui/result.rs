//! "Your text" window: shown when a transcript could not be pasted (app
//! switched, elevated target, possible background audio, Smart text failed,
//! audio file import). The text is editable and one click copies it.

use super::controls::{self as c, Theme};
use crate::win::clipboard;
use native_windows_gui as nwg;
use nwg::NwgError;
use std::cell::RefCell;
use std::rc::Rc;

pub struct ResultWindow {
    window: nwg::Window,
    message: nwg::Label,
    text: nwg::TextBox,
    show_original: nwg::CheckBox,
    copy: nwg::Button,
    close: nwg::Button,
    content: RefCell<(String, String)>,
    handler: RefCell<Option<nwg::EventHandler>>,
}

impl ResultWindow {
    pub fn build(theme: Rc<Theme>) -> Result<Rc<ResultWindow>, NwgError> {
        let t = &*theme;
        let w = c::window(t, "Your text", (580, 420), false)?;
        let message = c::label(&w, "", (24, 18), (532, 44), &t.body)?;
        let text = c::text_box(&w, (24, 68), (532, 262), &t.body, false)?;
        let show_original = c::check(
            &w,
            t,
            "Show original transcript",
            (24, 344),
            (260, 26),
            false,
        )?;
        let close = c::button(&w, "Close", (328, 366), (110, 34), &t.body)?;
        let copy = c::button(&w, "Copy text", (446, 366), (110, 34), &t.bold)?;
        let ui = Rc::new(ResultWindow {
            window: w,
            message,
            text,
            show_original,
            copy,
            close,
            content: RefCell::new((String::new(), String::new())),
            handler: RefCell::new(None),
        });
        let weak = Rc::downgrade(&ui);
        let handler = nwg::full_bind_event_handler(&ui.window.handle, move |evt, data, handle| {
            let Some(ui) = weak.upgrade() else { return };
            use nwg::Event as E;
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
                    let shown = if c::is_checked(&ui.show_original) {
                        original
                    } else {
                        text
                    };
                    c::set_box_text(&ui.text, &shown);
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
        c::set_checked(&self.show_original, false);
        self.show_original.set_visible(original != text);
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
