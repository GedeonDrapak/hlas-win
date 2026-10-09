//! History: search, read, copy and reformat the last 50 dictations.
//! Stored only on this PC.

use super::controls::{self as c, Theme};
use crate::core::config::OutputMode;
use crate::win::coordinator::{self, Event};
use crate::win::{clipboard, state};
use chrono::TimeZone;
use native_windows_gui as nwg;
use nwg::NwgError;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub struct HistoryWindow {
    window: nwg::Window,
    search: nwg::TextInput,
    list: nwg::ListBox<String>,
    detail: nwg::TextBox,
    copy: nwg::Button,
    copy_original: nwg::Button,
    smart: nwg::Button,
    clear: nwg::Button,
    note: nwg::Label,
    timer: nwg::AnimationTimer,
    ids: RefCell<Vec<u64>>,
    seen_generation: Cell<u64>,
    clear_armed: Cell<bool>,
    handler: RefCell<Option<nwg::EventHandler>>,
}

fn when(unix: i64) -> String {
    chrono::Local
        .timestamp_opt(unix, 0)
        .single()
        .map(|t| t.format("%d.%m. %H:%M").to_string())
        .unwrap_or_default()
}

fn one_line(text: &str, max: usize) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        flat
    } else {
        format!("{}...", flat.chars().take(max).collect::<String>())
    }
}

impl HistoryWindow {
    pub fn build(theme: Rc<Theme>) -> Result<Rc<HistoryWindow>, NwgError> {
        let t = &*theme;
        let w = c::window(t, "Hlas History", (680, 540), false)?;
        let search = c::input(&w, "", (20, 18), (640, 28), &t.body, false)?;
        let mut list = nwg::ListBox::default();
        nwg::ListBox::builder()
            .position((20, 56))
            .size((640, 220))
            .font(Some(&t.body))
            .collection(Vec::new())
            .parent(&w)
            .build(&mut list)?;
        let detail = c::text_box(&w, (20, 286), (640, 170), &t.body, true)?;
        let copy = c::button(&w, "Copy text", (20, 468), (130, 32), &t.bold)?;
        let copy_original = c::button(&w, "Copy original", (158, 468), (130, 32), &t.body)?;
        let smart = c::button(&w, "Make smart text", (296, 468), (150, 32), &t.body)?;
        let clear = c::button(&w, "Clear all", (530, 468), (130, 32), &t.body)?;
        let note = c::label(
            &w,
            "Stored only on this PC. Search covers the text and the original transcript.",
            (20, 508),
            (640, 20),
            &t.small,
        )?;
        let timer = c::timer(&w, 1000)?;

        let ui = Rc::new(HistoryWindow {
            window: w,
            search,
            list,
            detail,
            copy,
            copy_original,
            smart,
            clear,
            note,
            timer,
            ids: RefCell::new(Vec::new()),
            seen_generation: Cell::new(0),
            clear_armed: Cell::new(false),
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
                E::OnTextInput if handle == ui.search.handle => ui.reload(),
                E::OnListBoxSelect if handle == ui.list.handle => ui.show_selected(),
                E::OnButtonClick if handle == ui.copy.handle => ui.copy_selected(false),
                E::OnButtonClick if handle == ui.copy_original.handle => ui.copy_selected(true),
                E::OnButtonClick if handle == ui.smart.handle => {
                    if let Some((id, raw)) = ui.selected().map(|e| (e.id, e.original().to_string()))
                    {
                        coordinator::send(Event::MakeSmart {
                            raw,
                            history_id: Some(id),
                        });
                        ui.note
                            .set_text("Making smart text... the result opens in its own window.");
                    }
                }
                E::OnButtonClick if handle == ui.clear.handle => {
                    if ui.clear_armed.get() {
                        state::clear_history();
                        ui.clear_armed.set(false);
                        ui.clear.set_text("Clear all");
                        ui.reload();
                    } else {
                        ui.clear_armed.set(true);
                        ui.clear.set_text("Click to confirm");
                    }
                }
                E::OnTimerTick
                    if handle == ui.timer.handle
                        && ui.window.visible()
                        && state::generation() != ui.seen_generation.get() =>
                {
                    ui.reload()
                }
                _ => {}
            }
        });
        *ui.handler.borrow_mut() = Some(handler);
        ui.timer.start();
        Ok(ui)
    }

    pub fn show(&self) {
        self.clear_armed.set(false);
        self.clear.set_text("Clear all");
        self.reload();
        c::present(&self.window);
        self.search.set_focus();
    }

    fn selected(&self) -> Option<crate::core::history::Entry> {
        let index = self.list.selection()?;
        let id = *self.ids.borrow().get(index)?;
        state::with_history(|h| h.get(id).cloned())
    }

    fn reload(&self) {
        self.seen_generation.set(state::generation());
        let previous = self.selected().map(|e| e.id);
        let query = self.search.text();
        let rows: Vec<(u64, String)> = state::with_history(|h| {
            h.search(&query)
                .into_iter()
                .map(|e| {
                    let mode = if e.mode == OutputMode::Smart {
                        "Smart"
                    } else {
                        "Text"
                    };
                    (
                        e.id,
                        format!("{}   {}   {}", when(e.date), mode, one_line(&e.text, 80)),
                    )
                })
                .collect()
        });
        let empty = rows.is_empty();
        *self.ids.borrow_mut() = rows.iter().map(|(id, _)| *id).collect();
        self.list
            .set_collection(rows.into_iter().map(|(_, s)| s).collect());
        let index = previous
            .and_then(|p| self.ids.borrow().iter().position(|id| *id == p))
            .or(if empty { None } else { Some(0) });
        self.list.set_selection(index);
        if empty {
            let hint = if state::config().history_enabled {
                "Nothing here yet. Hold your push-to-talk key and speak."
            } else {
                "History is off. Turn it on in Settings."
            };
            c::set_box_text(&self.detail, hint);
        }
        self.show_selected();
    }

    fn show_selected(&self) {
        let Some(e) = self.selected() else {
            self.copy.set_enabled(false);
            self.copy_original.set_enabled(false);
            self.smart.set_enabled(false);
            return;
        };
        let mut text = e.text.clone();
        if e.original() != e.text {
            text.push_str("\n\n--- Original transcript ---\n");
            text.push_str(e.original());
        }
        c::set_box_text(&self.detail, &text);
        self.copy.set_enabled(true);
        self.copy_original.set_enabled(e.original() != e.text);
        self.smart.set_enabled(true);
    }

    fn copy_selected(&self, original: bool) {
        if let Some(e) = self.selected() {
            let text = if original {
                e.original().to_string()
            } else {
                e.text.clone()
            };
            match clipboard::set_text(&text, false) {
                Ok(_) => self.note.set_text("Copied."),
                Err(_) => self.note.set_text("The clipboard is busy. Try again."),
            }
        }
    }
}
