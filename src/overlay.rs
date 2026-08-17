//! The floating status pill, bottom-center of the primary display.
//!
//! It is a borderless, topmost, no-activate, click-through window so it never
//! steals focus from the app being dictated into - the Windows counterpart to
//! the macOS non-activating NSPanel. It runs on its own thread with a message
//! loop; the control thread drives it by sending `State` over a channel that a
//! WM_TIMER polls.

use crossbeam_channel::{Receiver, Sender};
use once_cell::sync::OnceCell;
use std::sync::Mutex;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateRoundRectRgn, CreateSolidBrush, DrawTextW, EndPaint, FillRect,
    InvalidateRect, SetBkMode, SetTextColor, SetWindowRgn, DT_CENTER, DT_SINGLELINE, DT_VCENTER,
    PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Hidden,
    Recording,
    DownloadingModel,
    Transcribing,
    Error,
}

const WIDTH: i32 = 280;
const HEIGHT: i32 = 48;
const BOTTOM_GAP: i32 = 60;
const TIMER_ID: usize = 1;

static COMMANDS: OnceCell<Receiver<State>> = OnceCell::new();
static CURRENT: Mutex<State> = Mutex::new(State::Hidden);

/// Starts the overlay thread and returns a sender the control loop uses to set
/// the pill's state.
pub fn start() -> Sender<State> {
    let (tx, rx) = crossbeam_channel::unbounded();
    let _ = COMMANDS.set(rx);

    std::thread::spawn(|| unsafe {
        let hinstance = GetModuleHandleW(None).expect("module handle");
        let class = w!("HlasOverlay");

        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance.into(),
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassW(&wc);

        let (x, y) = bottom_center();
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT | WS_EX_LAYERED,
            class,
            w!("Hlas"),
            WS_POPUP,
            x,
            y,
            WIDTH,
            HEIGHT,
            None,
            None,
            hinstance,
            None,
        )
        .expect("overlay window");

        // Rounded-rect region for the pill shape.
        let region = CreateRoundRectRgn(0, 0, WIDTH + 1, HEIGHT + 1, HEIGHT, HEIGHT);
        SetWindowRgn(hwnd, region, true);
        // Uniform opacity for the whole window (0..255).
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 240, LWA_ALPHA);

        // Poll the command channel ~30x/sec.
        SetTimer(hwnd, TIMER_ID, 33, None);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });

    tx
}

fn bottom_center() -> (i32, i32) {
    unsafe {
        let sw = GetSystemMetrics(SM_CXSCREEN);
        let sh = GetSystemMetrics(SM_CYSCREEN);
        ((sw - WIDTH) / 2, sh - HEIGHT - BOTTOM_GAP)
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER => {
            if let Some(rx) = COMMANDS.get() {
                let mut changed = false;
                // Drain to the latest state.
                while let Ok(state) = rx.try_recv() {
                    *CURRENT.lock().unwrap() = state;
                    changed = true;
                }
                if changed {
                    let state = *CURRENT.lock().unwrap();
                    match state {
                        State::Hidden => {
                            let _ = ShowWindow(hwnd, SW_HIDE);
                        }
                        _ => {
                            let (x, y) = bottom_center();
                            let _ = SetWindowPos(
                                hwnd,
                                HWND_TOPMOST,
                                x,
                                y,
                                WIDTH,
                                HEIGHT,
                                SWP_NOACTIVATE,
                            );
                            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                            let _ = InvalidateRect(hwnd, None, true);
                        }
                    }
                }
            }
            LRESULT(0)
        }
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);

    let mut rect = RECT {
        left: 0,
        top: 0,
        right: WIDTH,
        bottom: HEIGHT,
    };
    // Dark near-black fill (0x161719), matching the app palette.
    let bg = CreateSolidBrush(COLORREF(0x00191716));
    FillRect(hdc, &rect, bg);

    let state = *CURRENT.lock().unwrap();
    let label: Vec<u16> = match state {
        State::Recording => "● Recording",
        State::DownloadingModel => "Downloading local model…",
        State::Transcribing => "Transcribing…",
        State::Error => "Couldn't transcribe - see log",
        State::Hidden => "",
    }
    .encode_utf16()
    .chain(std::iter::once(0))
    .collect();

    SetBkMode(hdc, TRANSPARENT);
    // Bio-digital green text (0x00E67A stored as COLORREF 0x007AE600 BGR).
    SetTextColor(hdc, COLORREF(0x007AE600));
    let mut text = label.clone();
    DrawTextW(
        hdc,
        &mut text,
        &mut rect,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );

    let _ = EndPaint(hwnd, &ps);
    let _ = bg;
    let _ = PCWSTR::from_raw(label.as_ptr());
}
