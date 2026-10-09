//! The floating status pill, bottom-center of the monitor with the active
//! window - the Windows counterpart of the macOS non-activating panel.
//!
//! Borderless, topmost, never takes focus and lets clicks through. It runs on
//! its own thread; other threads send `Pill` states over a channel that a
//! 30 fps timer drains. Painting is double-buffered GDI, scaled to the
//! monitor's DPI, and every GDI object is released after use.

use super::single_instance::SHOW_MESSAGE;
use super::wide;
use crossbeam_channel::{Receiver, Sender};
use once_cell::sync::OnceCell;
use std::cell::RefCell;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::*;

#[derive(Debug, Clone, PartialEq)]
pub enum Pill {
    Hidden,
    Recording { smart: bool },
    Transcribing,
    Formatting,
    Downloading(u8),
    Cancelled,
    Message { text: String, error: bool },
}

const TIMER_ID: usize = 1;
/// Recording only appears after this long, so a Right Ctrl shortcut that
/// starts and immediately cancels a dictation never flashes the pill.
const SHOW_DELAY: Duration = Duration::from_millis(140);
const BARS: usize = 18;

const BG: u32 = 0x161719;
const BORDER: u32 = 0x2C2F33;
const FG: u32 = 0xECEEED;
const FG_DIM: u32 = 0x8E9693;
const BRAND: u32 = 0x00E67A;
const RED: u32 = 0xFF6B6B;

static TX: OnceCell<Sender<(Pill, Option<Duration>)>> = OnceCell::new();
static RX: OnceCell<Receiver<(Pill, Option<Duration>)>> = OnceCell::new();
static LEVEL: AtomicU32 = AtomicU32::new(0);

struct View {
    pill: Pill,
    since: Instant,
    hide_at: Option<Instant>,
    visible: bool,
    levels: [f32; BARS],
    scale: f32,
    width: i32,
    height: i32,
}

thread_local! {
    static VIEW: RefCell<View> = RefCell::new(View {
        pill: Pill::Hidden,
        since: Instant::now(),
        hide_at: None,
        visible: false,
        levels: [0.0; BARS],
        scale: 1.0,
        width: 300,
        height: 44,
    });
}

/// Colors are written as 0xRRGGBB; GDI wants 0x00BBGGRR.
fn rgb(c: u32) -> COLORREF {
    COLORREF(((c & 0xFF) << 16) | (c & 0xFF00) | ((c >> 16) & 0xFF))
}

pub fn show(pill: Pill) {
    if let Some(tx) = TX.get() {
        let _ = tx.send((pill, None));
    }
}

/// Shows a state and hides it automatically after `dur`.
pub fn show_for(pill: Pill, dur: Duration) {
    if let Some(tx) = TX.get() {
        let _ = tx.send((pill, Some(dur)));
    }
}

pub fn hide() {
    show(Pill::Hidden);
}

pub fn set_level(level: f32) {
    LEVEL.store(level.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
}

pub fn start() {
    let (tx, rx) = crossbeam_channel::unbounded();
    let _ = TX.set(tx);
    let _ = RX.set(rx);
    std::thread::Builder::new()
        .name("hlas-overlay".into())
        .spawn(|| unsafe { run() })
        .expect("overlay thread");
}

unsafe fn run() {
    let hinstance = GetModuleHandleW(None).expect("module handle");
    let class = wide("HlasOverlay");
    let wc = WNDCLASSW {
        lpfnWndProc: Some(wndproc),
        hInstance: hinstance.into(),
        lpszClassName: PCWSTR(class.as_ptr()),
        hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
        ..Default::default()
    };
    RegisterClassW(&wc);
    let title = wide("Hlas");
    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT | WS_EX_LAYERED,
        PCWSTR(class.as_ptr()),
        PCWSTR(title.as_ptr()),
        WS_POPUP,
        0,
        0,
        300,
        44,
        None,
        None,
        hinstance,
        None,
    )
    .expect("overlay window");
    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 246, LWA_ALPHA);
    SetTimer(hwnd, TIMER_ID, 33, None);
    // Make sure the broadcast id is registered before a second launch sends it.
    let _ = *SHOW_MESSAGE;

    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == *SHOW_MESSAGE {
        super::ui::send(super::ui::Command::OpenSettings);
        return LRESULT(0);
    }
    match msg {
        WM_TIMER => {
            tick(hwnd);
            LRESULT(0)
        }
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn tick(hwnd: HWND) {
    let rx = RX.get().expect("overlay channel");
    VIEW.with(|v| {
        let mut v = v.borrow_mut();
        let mut changed = false;
        while let Ok((pill, dur)) = rx.try_recv() {
            if pill != v.pill || dur.is_some() {
                v.since = Instant::now();
            }
            if !matches!(pill, Pill::Recording { .. }) {
                v.levels = [0.0; BARS];
            }
            v.pill = pill;
            v.hide_at = dur.map(|d| Instant::now() + d);
            changed = true;
        }
        if let Some(at) = v.hide_at {
            if Instant::now() >= at {
                v.pill = Pill::Hidden;
                v.hide_at = None;
                changed = true;
            }
        }

        let should_show = match v.pill {
            Pill::Hidden => false,
            Pill::Recording { .. } => v.since.elapsed() >= SHOW_DELAY,
            _ => true,
        };
        if !should_show {
            if v.visible {
                let _ = ShowWindow(hwnd, SW_HIDE);
                v.visible = false;
            }
            return;
        }
        if matches!(v.pill, Pill::Recording { .. }) {
            v.levels.rotate_left(1);
            v.levels[BARS - 1] = f32::from_bits(LEVEL.load(Ordering::Relaxed));
            changed = true;
        }
        if matches!(v.pill, Pill::Transcribing | Pill::Formatting) {
            changed = true; // animated dots
        }
        if !v.visible || changed {
            if !v.visible {
                place(hwnd, &mut v);
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                v.visible = true;
            } else if changed && matches!(v.pill, Pill::Message { .. }) {
                place(hwnd, &mut v);
            }
            let _ = InvalidateRect(hwnd, None, false);
        }
    });
}

/// Bottom-center of the work area of the monitor showing the active window.
unsafe fn place(hwnd: HWND, v: &mut View) {
    let fg = GetForegroundWindow();
    let monitor = if fg.0.is_null() {
        MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY)
    } else {
        MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST)
    };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let _ = GetMonitorInfoW(monitor, &mut info);
    let (mut dx, mut dy) = (96u32, 96u32);
    let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
    v.scale = dx as f32 / 96.0;
    let logical_w = if matches!(v.pill, Pill::Message { .. }) {
        460.0
    } else {
        300.0
    };
    v.width = (logical_w * v.scale) as i32;
    v.height = (44.0 * v.scale) as i32;
    let work = info.rcWork;
    let x = work.left + ((work.right - work.left) - v.width) / 2;
    let y = work.bottom - v.height - (28.0 * v.scale) as i32;
    let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, v.width, v.height, SWP_NOACTIVATE);
    let region = CreateRoundRectRgn(0, 0, v.width + 1, v.height + 1, v.height, v.height);
    // The system owns the region after a successful call.
    SetWindowRgn(hwnd, region, true);
}

fn label(pill: &Pill) -> (String, String, u32) {
    match pill {
        Pill::Recording { smart } => (
            if *smart {
                "Listening - Smart text"
            } else {
                "Listening"
            }
            .into(),
            String::new(),
            FG,
        ),
        Pill::Transcribing => ("Transcribing".into(), "Esc to cancel".into(), FG),
        Pill::Formatting => ("Making smart text".into(), "Esc to cancel".into(), FG),
        Pill::Downloading(p) => (
            format!("Downloading model {p}%"),
            "Esc to cancel".into(),
            FG,
        ),
        Pill::Cancelled => ("Dictation cancelled".into(), String::new(), FG_DIM),
        Pill::Message { text, .. } => (text.clone(), String::new(), FG),
        Pill::Hidden => (String::new(), String::new(), FG),
    }
}

unsafe fn font(px: i32, weight: i32) -> HFONT {
    let face = wide("Segoe UI");
    CreateFontW(
        -px,
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        DEFAULT_CHARSET.0 as u32,
        OUT_DEFAULT_PRECIS.0 as u32,
        CLIP_DEFAULT_PRECIS.0 as u32,
        CLEARTYPE_QUALITY.0 as u32,
        0,
        PCWSTR(face.as_ptr()),
    )
}

unsafe fn fill(hdc: HDC, r: RECT, color: u32) {
    let brush = CreateSolidBrush(rgb(color));
    FillRect(hdc, &r, brush);
    let _ = DeleteObject(brush);
}

unsafe fn text(hdc: HDC, s: &str, mut r: RECT, color: u32, f: HFONT, flags: DRAW_TEXT_FORMAT) {
    let old = SelectObject(hdc, f);
    SetTextColor(hdc, rgb(color));
    let mut w: Vec<u16> = s.encode_utf16().collect();
    if !w.is_empty() {
        DrawTextW(
            hdc,
            &mut w,
            &mut r,
            flags | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
        );
    }
    SelectObject(hdc, old);
}

unsafe fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let screen = BeginPaint(hwnd, &mut ps);
    VIEW.with(|v| {
        let v = v.borrow();
        let (w, h, s) = (v.width, v.height, v.scale);
        let px = |n: f32| (n * s).round() as i32;

        let mem = CreateCompatibleDC(screen);
        let bmp = CreateCompatibleBitmap(screen, w, h);
        let old_bmp = SelectObject(mem, bmp);

        fill(
            mem,
            RECT {
                left: 0,
                top: 0,
                right: w,
                bottom: h,
            },
            BG,
        );
        let pen = CreatePen(PS_SOLID, 1, rgb(BORDER));
        let old_pen = SelectObject(mem, pen);
        let old_brush = SelectObject(mem, GetStockObject(NULL_BRUSH));
        let _ = RoundRect(mem, 0, 0, w, h, h, h);
        SelectObject(mem, old_brush);
        SelectObject(mem, old_pen);
        let _ = DeleteObject(pen);

        SetBkMode(mem, TRANSPARENT);
        let (main, aside, color) = label(&v.pill);
        let left = px(20.0);
        let mut text_left = left;
        let mid = h / 2;

        match &v.pill {
            Pill::Recording { .. } => {
                // Live waveform from the last ~0.6 s of input levels.
                let (bar, gap) = (px(3.0).max(2), px(2.0).max(1));
                for (i, level) in v.levels.iter().enumerate() {
                    let half = ((level * h as f32 * 0.32) as i32).max(px(1.5).max(1));
                    let x = left + i as i32 * (bar + gap);
                    fill(
                        mem,
                        RECT {
                            left: x,
                            top: mid - half,
                            right: x + bar,
                            bottom: mid + half,
                        },
                        BRAND,
                    );
                }
                text_left = left + BARS as i32 * (bar + gap) + px(12.0);
                let secs = v.since.elapsed().as_secs();
                let elapsed = format!("{}:{:02}", secs / 60, secs % 60);
                let f = font(px(12.0), 400);
                text(
                    mem,
                    &elapsed,
                    RECT {
                        left: 0,
                        top: 0,
                        right: w - px(20.0),
                        bottom: h,
                    },
                    FG_DIM,
                    f,
                    DT_RIGHT,
                );
                let _ = DeleteObject(f);
            }
            Pill::Transcribing | Pill::Formatting | Pill::Downloading(_) => {
                let phase = (v.since.elapsed().as_millis() / 220) as usize % 3;
                let d = px(6.0);
                for i in 0..3 {
                    let x = left + i as i32 * px(10.0);
                    let c = if i == phase { BRAND } else { 0x2E5F48 };
                    let brush = CreateSolidBrush(rgb(c));
                    let old = SelectObject(mem, brush);
                    let pen = SelectObject(mem, GetStockObject(NULL_PEN));
                    let _ = Ellipse(mem, x, mid - d / 2, x + d, mid + d / 2);
                    SelectObject(mem, pen);
                    SelectObject(mem, old);
                    let _ = DeleteObject(brush);
                }
                text_left = left + px(38.0);
                if let Pill::Downloading(p) = v.pill {
                    let track = RECT {
                        left: px(20.0),
                        top: h - px(7.0),
                        right: w - px(20.0),
                        bottom: h - px(5.0),
                    };
                    fill(mem, track, 0x2C2F33);
                    let done = track.left + (track.right - track.left) * p as i32 / 100;
                    fill(
                        mem,
                        RECT {
                            right: done,
                            ..track
                        },
                        BRAND,
                    );
                }
            }
            Pill::Cancelled | Pill::Message { .. } => {
                let dot = match &v.pill {
                    Pill::Message { error: true, .. } => RED,
                    Pill::Message { .. } => BRAND,
                    _ => FG_DIM,
                };
                let d = px(7.0);
                let brush = CreateSolidBrush(rgb(dot));
                let old = SelectObject(mem, brush);
                let pen = SelectObject(mem, GetStockObject(NULL_PEN));
                let _ = Ellipse(mem, left, mid - d / 2, left + d, mid + d / 2);
                SelectObject(mem, pen);
                SelectObject(mem, old);
                let _ = DeleteObject(brush);
                text_left = left + px(17.0);
            }
            Pill::Hidden => {}
        }

        let aside_w = if aside.is_empty() { 0 } else { px(90.0) };
        let elapsed_w = if matches!(v.pill, Pill::Recording { .. }) {
            px(44.0)
        } else {
            0
        };
        let f = font(px(13.0), 600);
        text(
            mem,
            &main,
            RECT {
                left: text_left,
                top: 0,
                right: w - px(18.0) - aside_w - elapsed_w,
                bottom: h,
            },
            color,
            f,
            DT_LEFT | DT_END_ELLIPSIS,
        );
        let _ = DeleteObject(f);
        if !aside.is_empty() {
            let f = font(px(11.0), 400);
            text(
                mem,
                &aside,
                RECT {
                    left: 0,
                    top: 0,
                    right: w - px(20.0),
                    bottom: h,
                },
                FG_DIM,
                f,
                DT_RIGHT,
            );
            let _ = DeleteObject(f);
        }

        let _ = BitBlt(screen, 0, 0, w, h, mem, 0, 0, SRCCOPY);
        SelectObject(mem, old_bmp);
        let _ = DeleteObject(bmp);
        let _ = DeleteDC(mem);
    });
    let _ = EndPaint(hwnd, &ps);
}
