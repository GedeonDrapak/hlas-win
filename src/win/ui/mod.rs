//! Native windows (native-windows-gui): Settings, History, the result window
//! and the welcome tour. All of them live on one UI thread; other threads ask
//! for them through `send`, which queues a command and wakes the thread with
//! an nwg Notice. Windows are built once and hidden on close, so reopening is
//! instant and nwg is initialised exactly once.

mod controls;
mod history;
mod onboarding;
mod result;
mod settings;
mod skin;

use native_windows_gui as nwg;
use once_cell::sync::OnceCell;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Mutex;

pub use controls::Theme;

pub enum Command {
    OpenSettings,
    OpenHistory,
    /// Welcome tour at a step (0 to 3).
    OpenOnboarding(usize),
    ShowResult {
        text: String,
        original: String,
        message: String,
    },
    /// File picker for "Transcribe audio file".
    PickAudioFile,
}

static QUEUE: Mutex<VecDeque<Command>> = Mutex::new(VecDeque::new());
static WAKE: OnceCell<nwg::NoticeSender> = OnceCell::new();

pub fn send(cmd: Command) {
    QUEUE.lock().unwrap().push_back(cmd);
    if let Some(w) = WAKE.get() {
        w.notice();
    }
}

struct Ui {
    theme: Rc<Theme>,
    settings: RefCell<Option<Rc<settings::SettingsWindow>>>,
    history: RefCell<Option<Rc<history::HistoryWindow>>>,
    result: RefCell<Option<Rc<result::ResultWindow>>>,
    onboarding: RefCell<Option<Rc<onboarding::Onboarding>>>,
    host: nwg::MessageWindow,
}

impl Ui {
    fn dispatch(&self, cmd: Command) {
        let outcome = match cmd {
            Command::OpenSettings => self.open_settings(),
            Command::OpenHistory => self.open_history(),
            Command::OpenOnboarding(step) => self.open_onboarding(step),
            Command::ShowResult {
                text,
                original,
                message,
            } => self.show_result(&text, &original, &message),
            Command::PickAudioFile => {
                pick_audio_file(&self.host);
                Ok(())
            }
        };
        if let Err(e) = outcome {
            log::error!("window failed: {e}");
        }
    }

    fn open_settings(&self) -> Result<(), nwg::NwgError> {
        if self.settings.borrow().is_none() {
            *self.settings.borrow_mut() =
                Some(settings::SettingsWindow::build(self.theme.clone())?);
        }
        self.settings.borrow().as_ref().unwrap().show();
        Ok(())
    }

    fn open_history(&self) -> Result<(), nwg::NwgError> {
        if self.history.borrow().is_none() {
            *self.history.borrow_mut() = Some(history::HistoryWindow::build(self.theme.clone())?);
        }
        self.history.borrow().as_ref().unwrap().show();
        Ok(())
    }

    fn show_result(&self, text: &str, original: &str, message: &str) -> Result<(), nwg::NwgError> {
        if self.result.borrow().is_none() {
            *self.result.borrow_mut() = Some(result::ResultWindow::build(self.theme.clone())?);
        }
        self.result
            .borrow()
            .as_ref()
            .unwrap()
            .show(text, original, message);
        Ok(())
    }

    fn open_onboarding(&self, step: usize) -> Result<(), nwg::NwgError> {
        if self.onboarding.borrow().is_none() {
            *self.onboarding.borrow_mut() =
                Some(onboarding::Onboarding::build(self.theme.clone())?);
        }
        self.onboarding.borrow().as_ref().unwrap().show(step);
        Ok(())
    }
}

fn pick_audio_file(host: &nwg::MessageWindow) {
    let mut dialog = nwg::FileDialog::default();
    let built = nwg::FileDialog::builder()
        .title("Transcribe an audio file (up to 10 minutes)")
        .action(nwg::FileDialogAction::Open)
        .filters("Audio(*.wav;*.mp3;*.m4a;*.aac;*.mp4;*.flac;*.ogg)|All files(*.*)")
        .build(&mut dialog);
    if built.is_err() {
        return;
    }
    if dialog.run(Some(&host.handle)) {
        if let Ok(path) = dialog.get_selected_item() {
            super::coordinator::send(super::coordinator::Event::ImportFile(path.into()));
        }
    }
}

/// Starts the UI thread. Commands sent before it is ready are queued.
pub fn start() {
    std::thread::Builder::new()
        .name("hlas-ui".into())
        .spawn(|| {
            if let Err(e) = nwg::init() {
                log::error!("nwg init failed: {e}");
                return;
            }
            let theme = match Theme::new() {
                Ok(t) => Rc::new(t),
                Err(e) => {
                    log::error!("theme failed: {e}");
                    return;
                }
            };
            let mut host = nwg::MessageWindow::default();
            let mut notice = nwg::Notice::default();
            if nwg::MessageWindow::builder().build(&mut host).is_err()
                || nwg::Notice::builder()
                    .parent(&host)
                    .build(&mut notice)
                    .is_err()
            {
                log::error!("UI host window failed");
                return;
            }
            let ui = Rc::new(Ui {
                theme,
                settings: RefCell::new(None),
                history: RefCell::new(None),
                result: RefCell::new(None),
                onboarding: RefCell::new(None),
                host,
            });
            let handler_ui = ui.clone();
            let notice_handle = notice.handle;
            let handler =
                nwg::full_bind_event_handler(&ui.host.handle, move |evt, _data, handle| {
                    if evt == nwg::Event::OnNotice && handle == notice_handle {
                        loop {
                            let next = QUEUE.lock().unwrap().pop_front();
                            let Some(cmd) = next else { break };
                            handler_ui.dispatch(cmd);
                        }
                    }
                });
            let _ = WAKE.set(notice.sender());
            // Anything queued before the notice existed.
            notice.sender().notice();
            nwg::dispatch_thread_events();
            nwg::unbind_event_handler(&handler);
            drop(notice);
        })
        .expect("ui thread");
}
