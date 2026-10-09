//! Dark skin for the native windows, after the macOS design tokens
//! (`Sources/Hlas/DesignTokens.swift`): near-black surfaces, hairline
//! borders, the Eden green accent and Satoshi.
//!
//! The Win32 controls keep their behaviour (focus, keyboard, IME, screen
//! readers). The skin paints everything around them: the window background
//! with its decorations (cards, input fields, text, meters) in one
//! anti-aliased pass, label and edit colours through WM_CTLCOLOR*, and every
//! button owner-drawn as a primary, secondary, ghost or danger button, a
//! switch, a segmented control or a sidebar item. No new dependency: shapes
//! are rasterised into a 32-bit DIB, text is plain GDI.

use super::controls::Theme;
use native_windows_gui as nwg;
use nwg::NwgError;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use windows::core::{w, PCSTR, PCWSTR};
use windows::Win32::Foundation::{BOOL, COLORREF, HANDLE, HINSTANCE, HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWINDOWATTRIBUTE};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
use windows::Win32::UI::Controls::SetWindowTheme;
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging::*;

/// Colours as 0xRRGGBB, from the macOS DS tokens. Never pure black or white.
pub mod ds {
    pub const BG: u32 = 0x0C0D0F;
    pub const SIDEBAR: u32 = 0x111214;
    pub const SURFACE: u32 = 0x161719;
    pub const RAISED: u32 = 0x1E2023;
    pub const FIELD: u32 = 0x0F1012;
    pub const BORDER: u32 = 0x26282B;
    pub const BORDER_STRONG: u32 = 0x34373B;
    pub const FG: u32 = 0xEDEDED;
    pub const FG2: u32 = 0x9E9E9E;
    pub const FG3: u32 = 0x858585;
    pub const BRAND: u32 = 0x00E67E;
    pub const BRAND_HOVER: u32 = 0x2BF295;
    pub const BRAND_PRESSED: u32 = 0x00C46B;
    pub const BRAND_DIM: u32 = 0x1B3A2C;
    pub const INK: u32 = 0x0F0F0F;
    pub const DANGER: u32 = 0xE05252;
    pub const KNOB: u32 = 0xF2F2F2;
}

/// Window messages and styles used here, as plain numbers.
mod m {
    pub const ERASEBKGND: u32 = 0x0014;
    pub const DRAWITEM: u32 = 0x002B;
    pub const CTLCOLOREDIT: u32 = 0x0133;
    pub const CTLCOLORLISTBOX: u32 = 0x0134;
    pub const CTLCOLORBTN: u32 = 0x0135;
    pub const CTLCOLORSTATIC: u32 = 0x0138;
    pub const THEMECHANGED: u32 = 0x031A;
    pub const SETFONT: u32 = 0x0030;
    pub const MOUSEMOVE: u32 = 0x0200;
    pub const LBUTTONDOWN: u32 = 0x0201;
    pub const LBUTTONDBLCLK: u32 = 0x0203;
    pub const MOUSELEAVE: u32 = 0x02A3;
    pub const LB_GETTEXT: u32 = 0x0189;
    pub const LB_GETTEXTLEN: u32 = 0x018A;
    pub const LB_SETITEMHEIGHT: u32 = 0x01A0;
    pub const BS_TYPEMASK: isize = 0x000F;
    pub const BS_OWNERDRAW: isize = 0x000B;
    pub const WS_BORDER: isize = 0x0080_0000;
    pub const WS_CLIPCHILDREN: isize = 0x0200_0000;
    pub const ODT_LISTBOX: u32 = 2;
    pub const ODT_BUTTON: u32 = 4;
    pub const ODS_SELECTED: u32 = 0x0001;
    pub const ODS_DISABLED: u32 = 0x0004;
    pub const ODS_FOCUS: u32 = 0x0010;
    pub const ODS_NOFOCUSRECT: u32 = 0x0200;
}

/// DRAWITEMSTRUCT, declared here so the layout is obvious.
#[repr(C)]
#[allow(dead_code)]
struct DrawItem {
    ctl_type: u32,
    ctl_id: u32,
    item_id: u32,
    item_action: u32,
    item_state: u32,
    hwnd_item: HWND,
    hdc: HDC,
    rc: RECT,
    item_data: usize,
}

static FONT_FILES: [&[u8]; 3] = [
    include_bytes!("../../../assets/fonts/Satoshi-Regular.otf"),
    include_bytes!("../../../assets/fonts/Satoshi-Medium.otf"),
    include_bytes!("../../../assets/fonts/Satoshi-Bold.otf"),
];

/// Registers the bundled Satoshi cuts for this process only. GDI then knows
/// them as "Satoshi" (400, 700) and "Satoshi Medium" (500).
pub fn load_fonts() -> bool {
    FONT_FILES.iter().all(|data| {
        let mut count = 0u32;
        let handle = unsafe {
            AddFontMemResourceEx(
                data.as_ptr() as _,
                data.len() as u32,
                None,
                std::ptr::addr_of_mut!(count) as *const u32,
            )
        };
        !handle.is_invalid() && count > 0
    })
}

/// Dark context menus, dropdown lists and scrollbars for the whole process
/// (uxtheme SetPreferredAppMode(ForceDark) and FlushMenuThemes, by ordinal
/// since Windows 10 1903). Does nothing where they do not exist.
pub fn enable_dark_mode() {
    unsafe {
        let Ok(ux) = LoadLibraryW(w!("uxtheme.dll")) else {
            return;
        };
        if let Some(f) = GetProcAddress(ux, PCSTR(135 as *const u8)) {
            let set = std::mem::transmute::<
                unsafe extern "system" fn() -> isize,
                unsafe extern "system" fn(i32) -> i32,
            >(f);
            set(2);
        }
        // RefreshImmersiveColorPolicyState, ordinal 104.
        if let Some(f) = GetProcAddress(ux, PCSTR(104 as *const u8)) {
            let refresh = std::mem::transmute::<
                unsafe extern "system" fn() -> isize,
                unsafe extern "system" fn(),
            >(f);
            refresh();
        }
        if let Some(f) = GetProcAddress(ux, PCSTR(136 as *const u8)) {
            let flush = std::mem::transmute::<
                unsafe extern "system" fn() -> isize,
                unsafe extern "system" fn(),
            >(f);
            flush();
        }
    }
}

/// Lets one window use the dark variants of its theme
/// (uxtheme AllowDarkModeForWindow, ordinal 133), then reapplies the theme.
fn allow_dark(hwnd: HWND, theme: PCWSTR) {
    unsafe {
        if let Ok(ux) = LoadLibraryW(w!("uxtheme.dll")) {
            if let Some(f) = GetProcAddress(ux, PCSTR(133 as *const u8)) {
                let allow = std::mem::transmute::<
                    unsafe extern "system" fn() -> isize,
                    unsafe extern "system" fn(HWND, BOOL) -> BOOL,
                >(f);
                let _ = allow(hwnd, BOOL(1));
            }
        }
        let _ = SetWindowTheme(hwnd, theme, PCWSTR::null());
        SendMessageW(hwnd, m::THEMECHANGED, WPARAM(0), LPARAM(0));
    }
}

/// Colors are written as 0xRRGGBB; GDI wants 0x00BBGGRR.
fn rgb(c: u32) -> COLORREF {
    COLORREF(((c & 0xFF) << 16) | (c & 0xFF00) | ((c >> 16) & 0xFF))
}

fn mix(dst: u32, src: u32, a: f32) -> u32 {
    let ch = |shift: u32| {
        let d = ((dst >> shift) & 0xFF) as f32;
        let s = ((src >> shift) & 0xFF) as f32;
        ((d + (s - d) * a).round() as u32).min(255) << shift
    };
    ch(16) | ch(8) | ch(0)
}

fn hwnd_of(handle: &nwg::ControlHandle) -> Option<HWND> {
    handle.hwnd().map(|h| HWND(h as _))
}

unsafe fn invalidate(hwnd: isize) {
    if hwnd != 0 {
        let _ = InvalidateRect(HWND(hwnd as _), None, BOOL(0));
    }
}

/// An offscreen 32-bit DIB: anti-aliased shapes are written straight into its
/// pixels (0x00RRGGBB, top-down), text goes through its DC.
struct Canvas {
    dc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u32,
    w: i32,
    h: i32,
}

impl Canvas {
    unsafe fn new(reference: HDC, w: i32, h: i32) -> Option<Canvas> {
        if w <= 0 || h <= 0 {
            return None;
        }
        let dc = CreateCompatibleDC(reference);
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: 0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let Ok(bmp) = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, HANDLE::default(), 0)
        else {
            let _ = DeleteDC(dc);
            return None;
        };
        if bits.is_null() {
            let _ = DeleteObject(bmp);
            let _ = DeleteDC(dc);
            return None;
        }
        let old = SelectObject(dc, bmp);
        SetBkMode(dc, TRANSPARENT);
        Some(Canvas {
            dc,
            bmp,
            old,
            bits: bits as *mut u32,
            w,
            h,
        })
    }

    fn pixels(&mut self) -> &mut [u32] {
        unsafe { std::slice::from_raw_parts_mut(self.bits, (self.w * self.h) as usize) }
    }

    fn fill(&mut self, color: u32) {
        self.pixels().fill(color);
    }

    /// Rounded rectangle, filled or (with `stroke`) outlined, anti-aliased
    /// from its signed distance.
    #[allow(clippy::too_many_arguments)]
    fn shape(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, stroke: Option<f32>, color: u32) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let (cw, ch) = (self.w, self.h);
        let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        let (hx, hy) = (w / 2.0 - r, h / 2.0 - r);
        let x0 = (x.floor() as i32).max(0);
        let x1 = ((x + w).ceil() as i32).min(cw);
        let y0 = (y.floor() as i32).max(0);
        let y1 = ((y + h).ceil() as i32).min(ch);
        let px = self.pixels();
        for py in y0..y1 {
            let qy = ((py as f32 + 0.5) - cy).abs() - hy;
            for pxl in x0..x1 {
                let qx = ((pxl as f32 + 0.5) - cx).abs() - hx;
                let (ox, oy) = (qx.max(0.0), qy.max(0.0));
                let d = (ox * ox + oy * oy).sqrt() + qx.max(qy).min(0.0) - r;
                let mut cover = (0.5 - d).clamp(0.0, 1.0);
                if let Some(sw) = stroke {
                    cover -= (0.5 - d - sw).clamp(0.0, 1.0);
                }
                if cover > 0.0 {
                    let i = (py * cw + pxl) as usize;
                    px[i] = if cover >= 1.0 {
                        color
                    } else {
                        mix(px[i], color, cover)
                    };
                }
            }
        }
    }

    fn rrect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, color: u32) {
        self.shape(x, y, w, h, r, None, color);
    }

    #[allow(clippy::too_many_arguments)]
    fn ring(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, width: f32, color: u32) {
        self.shape(x, y, w, h, r, Some(width), color);
    }

    #[allow(clippy::too_many_arguments)]
    unsafe fn text(
        &self,
        s: &str,
        mut r: RECT,
        color: u32,
        font: HFONT,
        flags: DRAW_TEXT_FORMAT,
        tracking: i32,
    ) {
        let mut wide: Vec<u16> = s.encode_utf16().collect();
        if wide.is_empty() {
            return;
        }
        let old = SelectObject(self.dc, font);
        SetTextColor(self.dc, rgb(color));
        SetTextCharacterExtra(self.dc, tracking);
        DrawTextW(self.dc, &mut wide, &mut r, flags | DT_NOPREFIX);
        SetTextCharacterExtra(self.dc, 0);
        SelectObject(self.dc, old);
    }

    unsafe fn icon(&self, x: i32, y: i32, size: i32) {
        let Ok(module) = GetModuleHandleW(None) else {
            return;
        };
        let Ok(handle) = LoadImageW(
            HINSTANCE::from(module),
            PCWSTR(std::ptr::without_provenance::<u16>(1)),
            IMAGE_ICON,
            size,
            size,
            LR_DEFAULTCOLOR,
        ) else {
            return;
        };
        let icon = HICON(handle.0);
        let _ = DrawIconEx(
            self.dc,
            x,
            y,
            icon,
            size,
            size,
            0,
            HBRUSH::default(),
            DI_NORMAL,
        );
        let _ = DestroyIcon(icon);
    }

    unsafe fn bitmap(&self, bitmap: isize, r: RECT) {
        let src = CreateCompatibleDC(self.dc);
        let old = SelectObject(src, HBITMAP(bitmap as _));
        let _ = BitBlt(
            self.dc,
            r.left,
            r.top,
            r.right - r.left,
            r.bottom - r.top,
            src,
            0,
            0,
            SRCCOPY,
        );
        SelectObject(src, old);
        let _ = DeleteDC(src);
    }

    unsafe fn blit(&self, dst: HDC, x: i32, y: i32) {
        let _ = BitBlt(dst, x, y, self.w, self.h, self.dc, 0, 0, SRCCOPY);
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            let _ = DeleteObject(self.bmp);
            let _ = DeleteDC(self.dc);
        }
    }
}

/// How an owner-drawn button looks.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// Green, for the one main action of a window.
    Primary,
    Secondary,
    /// Text only until hovered.
    Ghost,
    /// Secondary with red text, for clearing data.
    Danger,
    /// Sidebar item with an icon; one per group is selected.
    Nav,
    /// One option of a segmented control; one per group is selected.
    Segment,
    /// On/off switch.
    Toggle,
    /// A field that opens a list.
    Dropdown,
}

struct Button {
    kind: Kind,
    bg: u32,
    on: bool,
    group: u32,
    icon: Option<char>,
    font: HFONT,
    menu: Option<Menu>,
}

type Menu = (Rc<RefCell<Vec<String>>>, Rc<Cell<Option<usize>>>);

enum Item {
    Edit { bg: u32 },
    List { bg: u32 },
    Button(Button),
}

enum Shape {
    Card,
    Field,
    Fill(u32),
    Rule,
    Track,
    Meter(Rc<Cell<f32>>),
    Text {
        text: Rc<RefCell<String>>,
        color: Rc<Cell<u32>>,
        font: HFONT,
        tracking: i32,
        flags: DRAW_TEXT_FORMAT,
    },
    Glyph {
        glyph: char,
        color: u32,
    },
    Logo,
    Bitmap(isize),
}

struct Deco {
    group: u8,
    rect: (i32, i32, i32, i32),
    shape: Shape,
}

/// A thin progress bar or level meter drawn by the window background.
pub struct Meter {
    value: Rc<Cell<f32>>,
    hwnd: isize,
    rect: RECT,
}

impl Meter {
    pub fn set(&self, v: f32) {
        let v = v.clamp(0.0, 1.0);
        if (self.value.get() - v).abs() > 0.004 {
            self.value.set(v);
            unsafe {
                let _ = InvalidateRect(
                    HWND(self.hwnd as _),
                    Some(&self.rect as *const RECT),
                    BOOL(1),
                );
            }
        }
    }
}

/// Text painted by the window background, changeable at run time.
pub struct Text {
    value: Rc<RefCell<String>>,
    color: Rc<Cell<u32>>,
    hwnd: isize,
    rect: RECT,
}

impl Text {
    pub fn set_text(&self, text: &str) {
        if *self.value.borrow() != text {
            *self.value.borrow_mut() = text.to_string();
            self.repaint();
        }
    }

    pub fn text(&self) -> String {
        self.value.borrow().clone()
    }

    pub fn set_color(&self, color: u32) {
        if self.color.replace(color) != color {
            self.repaint();
        }
    }

    fn repaint(&self) {
        unsafe {
            let _ = InvalidateRect(
                HWND(self.hwnd as _),
                Some(&self.rect as *const RECT),
                BOOL(1),
            );
        }
    }
}

/// Text layouts for `Skin::label`.
pub const WRAP: DRAW_TEXT_FORMAT = DRAW_TEXT_FORMAT(DT_LEFT.0 | DT_WORDBREAK.0);
pub const LINE: DRAW_TEXT_FORMAT =
    DRAW_TEXT_FORMAT(DT_LEFT.0 | DT_SINGLELINE.0 | DT_VCENTER.0 | DT_END_ELLIPSIS.0);

/// A dropdown: what it lists and which entry is selected. Same calls as
/// nwg::ComboBox, so windows read like before.
pub struct Dropdown {
    pub button: nwg::Button,
    items: Rc<RefCell<Vec<String>>>,
    selected: Rc<Cell<Option<usize>>>,
}

impl Dropdown {
    pub fn selection(&self) -> Option<usize> {
        self.selected.get()
    }

    pub fn set_selection(&self, index: Option<usize>) {
        let items = self.items.borrow();
        let index = index.filter(|i| *i < items.len());
        self.selected.set(index);
        self.button
            .set_text(index.map(|i| items[i].as_str()).unwrap_or(""));
    }

    pub fn set_collection(&self, items: Vec<String>) {
        *self.items.borrow_mut() = items;
        self.selected.set(None);
        self.button.set_text("");
    }
}

/// A segmented control: one selected option.
pub struct Segmented {
    pub buttons: Vec<nwg::Button>,
}

impl Segmented {
    pub fn index_of(&self, handle: &nwg::ControlHandle) -> Option<usize> {
        self.buttons.iter().position(|b| b.handle == *handle)
    }
}

/// The skin of one window: what each child is, what to paint around them,
/// and which groups (pages, steps) are visible.
pub struct Skin {
    pub theme: Rc<Theme>,
    parent: nwg::ControlHandle,
    hwnd: isize,
    scale: f32,
    bg: u32,
    items: RefCell<HashMap<isize, Item>>,
    pages: RefCell<Vec<(u8, isize)>>,
    decos: RefCell<Vec<Deco>>,
    visible: Cell<u64>,
    hover: Cell<isize>,
    next_group: Cell<u32>,
    me: RefCell<std::rc::Weak<Skin>>,
    brushes: RefCell<HashMap<u32, HBRUSH>>,
    handlers: RefCell<Vec<nwg::RawEventHandler>>,
}

const WINDOW_HANDLER: usize = 0x1_4C41;
const CHILD_HANDLER: usize = 0x1_4C42;

impl Skin {
    pub fn new(theme: Rc<Theme>, window: &nwg::Window, bg: u32) -> Rc<Skin> {
        let hwnd = window.handle.hwnd().map(|h| h as isize).unwrap_or(0);
        unsafe {
            let h = HWND(hwnd as _);
            let dark = BOOL(1);
            let caption = rgb(bg);
            let text = rgb(ds::FG);
            let border = rgb(ds::BORDER);
            for (attr, value) in [
                (20, &dark as *const _ as *const std::ffi::c_void),
                (35, &caption as *const _ as _),
                (36, &text as *const _ as _),
                (34, &border as *const _ as _),
            ] {
                let _ = DwmSetWindowAttribute(h, DWMWINDOWATTRIBUTE(attr), value, 4);
            }
            let style = GetWindowLongPtrW(h, GWL_STYLE);
            SetWindowLongPtrW(h, GWL_STYLE, style | m::WS_CLIPCHILDREN);
            allow_dark(h, w!("DarkMode_Explorer"));
        }
        let skin = Rc::new(Skin {
            theme,
            parent: window.handle,
            hwnd,
            scale: nwg::scale_factor() as f32,
            bg,
            items: RefCell::new(HashMap::new()),
            pages: RefCell::new(Vec::new()),
            decos: RefCell::new(Vec::new()),
            visible: Cell::new(1),
            hover: Cell::new(0),
            next_group: Cell::new(1),
            me: RefCell::new(std::rc::Weak::new()),
            brushes: RefCell::new(HashMap::new()),
            handlers: RefCell::new(Vec::new()),
        });
        *skin.me.borrow_mut() = Rc::downgrade(&skin);
        let weak = Rc::downgrade(&skin);
        if let Ok(h) =
            nwg::bind_raw_event_handler(&window.handle, WINDOW_HANDLER, move |_, msg, w, l| {
                weak.upgrade()?.window_message(msg, w, l)
            })
        {
            skin.handlers.borrow_mut().push(h);
        }
        skin
    }

    fn px(&self, v: i32) -> f32 {
        v as f32 * self.scale
    }

    fn phys(&self, (x, y, w, h): (i32, i32, i32, i32)) -> RECT {
        RECT {
            left: self.px(x).round() as i32,
            top: self.px(y).round() as i32,
            right: self.px(x + w).round() as i32,
            bottom: self.px(y + h).round() as i32,
        }
    }

    fn brush(&self, color: u32) -> HBRUSH {
        *self
            .brushes
            .borrow_mut()
            .entry(color)
            .or_insert_with(|| unsafe { CreateSolidBrush(rgb(color)) })
    }

    fn deco(&self, group: u8, rect: (i32, i32, i32, i32), shape: Shape) {
        self.decos.borrow_mut().push(Deco { group, rect, shape });
    }

    fn page(&self, group: u8, handle: &nwg::ControlHandle) {
        if group != 0 {
            if let Some(h) = handle.hwnd() {
                self.pages.borrow_mut().push((group, h as isize));
            }
        }
    }

    fn shown(&self, group: u8) -> bool {
        group == 0 || self.visible.get() & (1u64 << group) != 0
    }

    /// Shows the decorations and registered controls of the groups in
    /// `mask` (bit n = group n) and hides the other groups.
    pub fn set_visible(&self, mask: u64) {
        self.visible.set(mask | 1);
        for (group, hwnd) in self.pages.borrow().iter() {
            let cmd = if self.shown(*group) { SW_SHOW } else { SW_HIDE };
            unsafe {
                let _ = ShowWindow(HWND(*hwnd as _), cmd);
            }
        }
        unsafe {
            let _ = InvalidateRect(HWND(self.hwnd as _), None, BOOL(1));
        }
    }

    // ---- decorations ------------------------------------------------------

    /// A raised panel with a hairline border (macOS DS.surface).
    pub fn card(&self, group: u8, rect: (i32, i32, i32, i32)) {
        self.deco(group, rect, Shape::Card);
    }

    pub fn fill(&self, group: u8, rect: (i32, i32, i32, i32), color: u32) {
        self.deco(group, rect, Shape::Fill(color));
    }

    /// A one-pixel hairline.
    pub fn rule(&self, group: u8, x: i32, y: i32, w: i32) {
        self.deco(group, (x, y, w, 1), Shape::Rule);
    }

    /// Fixed or changeable text, painted with the background.
    pub fn label(
        &self,
        group: u8,
        text: &str,
        rect: (i32, i32, i32, i32),
        font: &nwg::Font,
        color: u32,
        flags: DRAW_TEXT_FORMAT,
    ) -> Text {
        let value = Rc::new(RefCell::new(text.to_string()));
        let tint = Rc::new(Cell::new(color));
        self.deco(
            group,
            rect,
            Shape::Text {
                text: value.clone(),
                color: tint.clone(),
                font: HFONT(font.handle as _),
                tracking: 0,
                flags,
            },
        );
        Text {
            value,
            color: tint,
            hwnd: self.hwnd,
            rect: self.phys(rect),
        }
    }

    /// Letter-spaced single line (wordmark, brand line).
    pub fn tracked(
        &self,
        group: u8,
        text: &str,
        rect: (i32, i32, i32, i32),
        font: &nwg::Font,
        color: u32,
        tracking: i32,
    ) {
        self.deco(
            group,
            rect,
            Shape::Text {
                text: Rc::new(RefCell::new(text.to_string())),
                color: Rc::new(Cell::new(color)),
                font: HFONT(font.handle as _),
                tracking: self.px(tracking).round() as i32,
                flags: DT_LEFT | DT_SINGLELINE | DT_VCENTER,
            },
        );
    }

    /// An icon from Segoe Fluent Icons (Windows 11) or MDL2 Assets.
    pub fn glyph(&self, group: u8, glyph: char, rect: (i32, i32, i32, i32), color: u32) {
        self.deco(group, rect, Shape::Glyph { glyph, color });
    }

    /// A bitmap already scaled to the physical size of `rect`.
    pub fn bitmap(&self, group: u8, rect: (i32, i32, i32, i32), bitmap: &nwg::Bitmap) {
        self.deco(group, rect, Shape::Bitmap(bitmap.handle as isize));
    }

    /// The app icon (the two-pulse Hlas mark).
    pub fn logo(&self, group: u8, x: i32, y: i32, size: i32) {
        self.deco(group, (x, y, size, size), Shape::Logo);
    }

    pub fn meter(&self, group: u8, rect: (i32, i32, i32, i32)) -> Meter {
        let value = Rc::new(Cell::new(0.0));
        self.deco(group, rect, Shape::Meter(value.clone()));
        let mut r = self.phys(rect);
        r.left -= 2;
        r.right += 2;
        Meter {
            value,
            hwnd: self.hwnd,
            rect: r,
        }
    }

    // ---- controls ---------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub fn button(
        &self,
        group: u8,
        text: &str,
        rect: (i32, i32, i32, i32),
        kind: Kind,
        bg: u32,
    ) -> Result<nwg::Button, NwgError> {
        let font = match kind {
            Kind::Primary => &self.theme.bold,
            Kind::Ghost | Kind::Nav => &self.theme.body,
            _ => &self.theme.medium,
        };
        self.make_button(group, text, rect, kind, bg, 0, None, font)
    }

    #[allow(clippy::too_many_arguments)]
    fn make_button(
        &self,
        group: u8,
        text: &str,
        rect: (i32, i32, i32, i32),
        kind: Kind,
        bg: u32,
        set: u32,
        icon: Option<char>,
        font: &nwg::Font,
    ) -> Result<nwg::Button, NwgError> {
        let mut b = nwg::Button::default();
        nwg::Button::builder()
            .text(text)
            .position((rect.0, rect.1))
            .size((rect.2, rect.3))
            .font(Some(font))
            .parent(self.parent)
            .build(&mut b)?;
        if let Some(h) = hwnd_of(&b.handle) {
            unsafe {
                let style = GetWindowLongPtrW(h, GWL_STYLE);
                SetWindowLongPtrW(h, GWL_STYLE, (style & !m::BS_TYPEMASK) | m::BS_OWNERDRAW);
            }
            self.items.borrow_mut().insert(
                h.0 as isize,
                Item::Button(Button {
                    kind,
                    bg,
                    on: false,
                    group: set,
                    icon,
                    font: HFONT(font.handle as _),
                    menu: None,
                }),
            );
            self.hook_button(&b.handle);
            unsafe { invalidate(h.0 as isize) };
        }
        self.page(group, &b.handle);
        Ok(b)
    }

    /// Hover tracking, and double clicks counted as clicks (owner-drawn
    /// buttons otherwise swallow the second click of a fast double click).
    fn hook_button(&self, handle: &nwg::ControlHandle) {
        let weak = self.me.borrow().clone();
        if let Ok(h) = nwg::bind_raw_event_handler(handle, CHILD_HANDLER, move |hwnd, msg, w, l| {
            let skin = weak.upgrade()?;
            let me = hwnd as isize;
            unsafe {
                match msg {
                    m::ERASEBKGND => Some(1),
                    m::MOUSEMOVE => {
                        if skin.hover.get() != me {
                            let old = skin.hover.replace(me);
                            invalidate(old);
                            invalidate(me);
                            let mut track = TRACKMOUSEEVENT {
                                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                                dwFlags: TME_LEAVE,
                                hwndTrack: HWND(hwnd as _),
                                dwHoverTime: 0,
                            };
                            let _ = TrackMouseEvent(&mut track);
                        }
                        None
                    }
                    m::MOUSELEAVE => {
                        if skin.hover.get() == me {
                            skin.hover.set(0);
                            invalidate(me);
                        }
                        None
                    }
                    m::LBUTTONDBLCLK => {
                        SendMessageW(HWND(hwnd as _), m::LBUTTONDOWN, WPARAM(w), LPARAM(l));
                        Some(0)
                    }
                    _ => None,
                }
            }
        }) {
            self.handlers.borrow_mut().push(h);
        }
    }

    /// An on/off switch, 40x22.
    pub fn toggle(
        &self,
        group: u8,
        x: i32,
        y: i32,
        bg: u32,
        on: bool,
    ) -> Result<nwg::Button, NwgError> {
        let b = self.make_button(
            group,
            "",
            (x, y, 40, 22),
            Kind::Toggle,
            bg,
            0,
            None,
            &self.theme.body,
        )?;
        self.set_on(&b.handle, on);
        Ok(b)
    }

    /// Options side by side in a rounded track; `selected` starts selected.
    pub fn segmented(
        &self,
        group: u8,
        labels: &[&str],
        rect: (i32, i32, i32, i32),
        selected: usize,
    ) -> Result<Segmented, NwgError> {
        let set = self.next_group.get();
        self.next_group.set(set + 1);
        self.deco(group, rect, Shape::Track);
        let (x, y, w, h) = rect;
        let inset = 3;
        let n = labels.len().max(1) as i32;
        let each = (w - 2 * inset) / n;
        let mut buttons = Vec::new();
        for (i, label) in labels.iter().enumerate() {
            let i = i as i32;
            let width = if i == n - 1 {
                w - 2 * inset - each * (n - 1)
            } else {
                each
            };
            buttons.push(self.make_button(
                group,
                label,
                (x + inset + i * each, y + inset, width, h - 2 * inset),
                Kind::Segment,
                ds::RAISED,
                set,
                None,
                &self.theme.medium,
            )?);
        }
        let seg = Segmented { buttons };
        self.select(&seg, selected);
        Ok(seg)
    }

    /// Sidebar items with icons, one selected.
    pub fn nav(
        &self,
        items: &[(char, &str)],
        x: i32,
        y: i32,
        w: i32,
        bg: u32,
    ) -> Result<Segmented, NwgError> {
        let set = self.next_group.get();
        self.next_group.set(set + 1);
        let mut buttons = Vec::new();
        for (i, (icon, label)) in items.iter().enumerate() {
            buttons.push(self.make_button(
                0,
                label,
                (x, y + i as i32 * 38, w, 34),
                Kind::Nav,
                bg,
                set,
                Some(*icon),
                &self.theme.body,
            )?);
        }
        let seg = Segmented { buttons };
        self.select(&seg, 0);
        Ok(seg)
    }

    pub fn selected(&self, seg: &Segmented) -> usize {
        seg.buttons
            .iter()
            .position(|b| self.is_on(&b.handle))
            .unwrap_or(0)
    }

    pub fn select(&self, seg: &Segmented, index: usize) {
        for (i, b) in seg.buttons.iter().enumerate() {
            self.set_on(&b.handle, i == index);
        }
    }

    pub fn is_on(&self, handle: &nwg::ControlHandle) -> bool {
        let Some(h) = handle.hwnd() else {
            return false;
        };
        matches!(self.items.borrow().get(&(h as isize)), Some(Item::Button(b)) if b.on)
    }

    pub fn set_on(&self, handle: &nwg::ControlHandle, on: bool) {
        let Some(h) = handle.hwnd() else {
            return;
        };
        let changed = match self.items.borrow_mut().get_mut(&(h as isize)) {
            Some(Item::Button(b)) if b.on != on => {
                b.on = on;
                true
            }
            _ => false,
        };
        if changed {
            unsafe { invalidate(h as isize) };
        }
    }

    /// Call first for every button click: flips switches and moves the
    /// selection of segmented controls and the sidebar.
    pub fn click(&self, handle: &nwg::ControlHandle) {
        let Some(h) = handle.hwnd().map(|h| h as isize) else {
            return;
        };
        let (kind, set, on) = match self.items.borrow().get(&h) {
            Some(Item::Button(b)) => (b.kind, b.group, b.on),
            _ => return,
        };
        match kind {
            Kind::Toggle => self.set_on(handle, !on),
            Kind::Dropdown => self.open_dropdown(h),
            Kind::Segment | Kind::Nav => {
                let mut changed = Vec::new();
                for (k, item) in self.items.borrow_mut().iter_mut() {
                    if let Item::Button(b) = item {
                        if b.group == set && b.kind == kind {
                            let now = *k == h;
                            if b.on != now {
                                b.on = now;
                                changed.push(*k);
                            }
                        }
                    }
                }
                for k in changed {
                    unsafe { invalidate(k) };
                }
            }
            _ => {}
        }
    }

    /// A borderless EDIT. nwg always adds WS_BORDER, and an edit created
    /// with it draws its own frame inside the control whatever the style
    /// says later, so these are created directly. The rounded field around
    /// them is the frame; text boxes scroll with the wheel and keyboard,
    /// without a scrollbar, like on macOS.
    fn edit(&self, group: u8, rect: (i32, i32, i32, i32), style: u32) -> Result<isize, NwgError> {
        let r = self.phys(rect);
        let id = self.next_group.get() + 0x5000;
        self.next_group.set(self.next_group.get() + 1);
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("EDIT"),
                w!(""),
                WINDOW_STYLE(0x5001_0000 | style),
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                HWND(self.hwnd as _),
                HMENU(id as usize as _),
                HINSTANCE::default(),
                None,
            )
        }
        .map_err(|e| NwgError::control_create(e.to_string()))?;
        unsafe {
            SendMessageW(
                hwnd,
                m::SETFONT,
                WPARAM(self.theme.body.handle as usize),
                LPARAM(1),
            );
        }
        let key = hwnd.0 as isize;
        self.items
            .borrow_mut()
            .insert(key, Item::Edit { bg: ds::FIELD });
        if group != 0 {
            self.pages.borrow_mut().push((group, key));
        }
        Ok(key)
    }

    /// A single-line input inside a rounded field.
    pub fn input(
        &self,
        group: u8,
        rect: (i32, i32, i32, i32),
        secret: bool,
    ) -> Result<nwg::TextInput, NwgError> {
        self.deco(group, rect, Shape::Field);
        let (x, y, w, h) = rect;
        let line = 20;
        // ES_AUTOHSCROLL, plus ES_PASSWORD for keys.
        let style = 0x0080 | if secret { 0x0020 } else { 0 };
        let hwnd = self.edit(group, (x + 12, y + (h - line) / 2, w - 24, line), style)?;
        let mut t = nwg::TextInput::default();
        t.handle = nwg::ControlHandle::Hwnd(hwnd as _);
        Ok(t)
    }

    /// A multi-line text box inside a rounded field.
    pub fn text_box(
        &self,
        group: u8,
        rect: (i32, i32, i32, i32),
        readonly: bool,
    ) -> Result<nwg::TextBox, NwgError> {
        self.deco(group, rect, Shape::Field);
        let (x, y, w, h) = rect;
        // ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN, plus ES_READONLY.
        let style = 0x0004 | 0x0040 | 0x1000 | if readonly { 0x0800 } else { 0 };
        let hwnd = self.edit(group, (x + 12, y + 9, w - 24, h - 18), style)?;
        Ok(nwg::TextBox {
            handle: nwg::ControlHandle::Hwnd(hwnd as _),
        })
    }

    /// A dropdown: a field-like button that opens a dark list.
    pub fn combo(
        &self,
        group: u8,
        items: Vec<String>,
        rect: (i32, i32, i32),
    ) -> Result<Dropdown, NwgError> {
        let items = Rc::new(RefCell::new(items));
        let selected = Rc::new(Cell::new(None));
        let (x, y, w) = rect;
        let on_card = self.decos.borrow().iter().any(|d| {
            let (cx, cy, cw, ch) = d.rect;
            matches!(d.shape, Shape::Card)
                && x >= cx
                && y >= cy
                && x + w <= cx + cw
                && y + 34 <= cy + ch
        });
        let button = self.make_button(
            group,
            "",
            (x, y, w, 34),
            Kind::Dropdown,
            if on_card { ds::SURFACE } else { self.bg },
            0,
            None,
            &self.theme.body,
        )?;
        if let Some(h) = button.handle.hwnd() {
            if let Some(Item::Button(b)) = self.items.borrow_mut().get_mut(&(h as isize)) {
                b.menu = Some((items.clone(), selected.clone()));
            }
        }
        Ok(Dropdown {
            button,
            items,
            selected,
        })
    }

    /// Opens the list of a dropdown under it and applies the choice.
    fn open_dropdown(&self, key: isize) {
        let Some((items, selected)) = (match self.items.borrow().get(&key) {
            Some(Item::Button(b)) => b.menu.clone(),
            _ => None,
        }) else {
            return;
        };
        let list = items.borrow().clone();
        if list.is_empty() {
            return;
        }
        let mut anchor = RECT::default();
        unsafe {
            if GetWindowRect(HWND(key as _), &mut anchor).is_err() {
                return;
            }
        }
        let choice = unsafe {
            popup::run(
                HWND(self.hwnd as _),
                anchor,
                &list,
                selected.get(),
                HFONT(self.theme.body.handle as _),
                HFONT(self.theme.icon_small.handle as _),
                self.scale,
            )
        };
        if let Some(i) = choice {
            selected.set(Some(i));
            unsafe {
                let text: Vec<u16> = list[i].encode_utf16().chain(std::iter::once(0)).collect();
                let _ = SetWindowTextW(HWND(key as _), PCWSTR(text.as_ptr()));
                invalidate(key);
            }
        }
    }

    /// A list drawn as two-line rows ("meta\ttext") inside a field.
    pub fn list(
        &self,
        group: u8,
        rect: (i32, i32, i32, i32),
    ) -> Result<nwg::ListBox<String>, NwgError> {
        self.deco(group, rect, Shape::Field);
        let (x, y, w, h) = rect;
        let mut l = nwg::ListBox::default();
        // LBS_OWNERDRAWFIXED (0x10); nwg adds LBS_HASSTRINGS and LBS_NOTIFY.
        let flags = unsafe {
            nwg::ListBoxFlags::from_bits_unchecked(
                (nwg::ListBoxFlags::VISIBLE | nwg::ListBoxFlags::TAB_STOP).bits() | 0x10,
            )
        };
        nwg::ListBox::builder()
            .position((x + 4, y + 4))
            .size((w - 8, h - 8))
            .flags(flags)
            .font(Some(&self.theme.body))
            .collection(Vec::new())
            .parent(self.parent)
            .build(&mut l)?;
        if let Some(hw) = hwnd_of(&l.handle) {
            unsafe {
                let style = GetWindowLongPtrW(hw, GWL_STYLE);
                SetWindowLongPtrW(hw, GWL_STYLE, style & !m::WS_BORDER);
                let _ = SetWindowPos(
                    hw,
                    HWND::default(),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
                );
                allow_dark(hw, w!("DarkMode_Explorer"));
                SendMessageW(
                    hw,
                    m::LB_SETITEMHEIGHT,
                    WPARAM(0),
                    LPARAM(self.px(54).round() as isize),
                );
            }
            self.items
                .borrow_mut()
                .insert(hw.0 as isize, Item::List { bg: ds::FIELD });
        }
        self.page(group, &l.handle);
        Ok(l)
    }

    // ---- painting ---------------------------------------------------------

    fn window_message(&self, msg: u32, w: usize, l: isize) -> Option<isize> {
        unsafe {
            match msg {
                m::ERASEBKGND => {
                    self.paint_background(HDC(w as _));
                    Some(1)
                }
                m::CTLCOLORSTATIC | m::CTLCOLOREDIT | m::CTLCOLORLISTBOX | m::CTLCOLORBTN => {
                    let hdc = HDC(w as _);
                    let (fg, bg) = match self.items.borrow().get(&l) {
                        Some(Item::Edit { bg }) | Some(Item::List { bg }) => (ds::FG, *bg),
                        Some(Item::Button(b)) => (ds::FG, b.bg),
                        None if msg == m::CTLCOLORLISTBOX => (ds::FG, ds::RAISED),
                        None => (ds::FG, self.bg),
                    };
                    SetTextColor(hdc, rgb(fg));
                    SetBkColor(hdc, rgb(bg));
                    Some(self.brush(bg).0 as isize)
                }
                m::DRAWITEM => {
                    let item = &*(l as *const DrawItem);
                    match item.ctl_type {
                        m::ODT_BUTTON => self.draw_button(item),
                        m::ODT_LISTBOX => self.draw_row(item),
                        _ => return None,
                    }
                    Some(1)
                }
                _ => None,
            }
        }
    }

    unsafe fn paint_background(&self, hdc: HDC) {
        let mut client = RECT::default();
        if GetClientRect(HWND(self.hwnd as _), &mut client).is_err() {
            return;
        }
        let Some(mut c) = Canvas::new(hdc, client.right, client.bottom) else {
            return;
        };
        c.fill(self.bg);
        let s = self.scale;
        let decos = self.decos.borrow();
        for d in decos.iter().filter(|d| self.shown(d.group)) {
            let (x, y, w, h) = (
                self.px(d.rect.0),
                self.px(d.rect.1),
                self.px(d.rect.2),
                self.px(d.rect.3),
            );
            match &d.shape {
                Shape::Card => {
                    c.rrect(x, y, w, h, 12.0 * s, ds::SURFACE);
                    c.ring(x, y, w, h, 12.0 * s, 1.0, ds::BORDER);
                }
                Shape::Field => {
                    c.rrect(x, y, w, h, 8.0 * s, ds::FIELD);
                    c.ring(x, y, w, h, 8.0 * s, 1.0, ds::BORDER_STRONG);
                }
                Shape::Fill(color) => c.rrect(x, y, w, h, 0.0, *color),
                Shape::Rule => c.rrect(x, y.round(), w, 1.0, 0.0, ds::BORDER),
                Shape::Track => {
                    c.rrect(x, y, w, h, 9.0 * s, ds::RAISED);
                    c.ring(x, y, w, h, 9.0 * s, 1.0, ds::BORDER);
                }
                Shape::Meter(v) => {
                    c.rrect(x, y, w, h, h / 2.0, ds::RAISED);
                    let done = w * v.get();
                    if done > 0.5 {
                        c.rrect(x, y, done.max(h), h, h / 2.0, ds::BRAND);
                    }
                }
                _ => {}
            }
        }
        for d in decos.iter().filter(|d| self.shown(d.group)) {
            let r = self.phys(d.rect);
            match &d.shape {
                Shape::Text {
                    text,
                    color,
                    font,
                    tracking,
                    flags,
                } => c.text(&text.borrow(), r, color.get(), *font, *flags, *tracking),
                Shape::Glyph { glyph, color } => c.text(
                    &glyph.to_string(),
                    r,
                    *color,
                    HFONT(self.theme.icon.handle as _),
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                    0,
                ),
                Shape::Logo => c.icon(r.left, r.top, r.right - r.left),
                Shape::Bitmap(bmp) => c.bitmap(*bmp, r),
                _ => {}
            }
        }
        c.blit(hdc, 0, 0);
    }

    unsafe fn draw_button(&self, item: &DrawItem) {
        let key = item.hwnd_item.0 as isize;
        let items = self.items.borrow();
        let Some(Item::Button(b)) = items.get(&key) else {
            return;
        };
        let rc = item.rc;
        let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
        let Some(mut c) = Canvas::new(item.hdc, w, h) else {
            return;
        };
        let state = item.item_state;
        let pressed = state & m::ODS_SELECTED != 0;
        let disabled = state & m::ODS_DISABLED != 0;
        let focus = state & m::ODS_FOCUS != 0 && state & m::ODS_NOFOCUSRECT == 0;
        let hover = !disabled && self.hover.get() == key;
        let s = self.scale;
        let (wf, hf) = (w as f32, h as f32);
        let radius = 8.0 * s;
        c.fill(b.bg);

        let mut text = [0u16; 256];
        let len = GetWindowTextW(item.hwnd_item, &mut text).max(0) as usize;
        let label = String::from_utf16_lossy(&text[..len]);
        let centered = DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS;
        let full = RECT {
            left: 0,
            top: 0,
            right: w,
            bottom: h,
        };

        match b.kind {
            Kind::Primary => {
                let fill = if disabled {
                    ds::BRAND_DIM
                } else if pressed {
                    ds::BRAND_PRESSED
                } else if hover {
                    ds::BRAND_HOVER
                } else {
                    ds::BRAND
                };
                c.rrect(0.0, 0.0, wf, hf, radius, fill);
                if focus {
                    c.ring(
                        1.5 * s,
                        1.5 * s,
                        wf - 3.0 * s,
                        hf - 3.0 * s,
                        radius - 1.5 * s,
                        1.0 * s,
                        ds::INK,
                    );
                }
                let color = if disabled { ds::FG3 } else { ds::INK };
                c.text(&label, full, color, b.font, centered, 0);
            }
            Kind::Secondary | Kind::Danger => {
                let fill = if pressed {
                    ds::SURFACE
                } else if hover {
                    ds::BORDER
                } else {
                    ds::RAISED
                };
                c.rrect(0.0, 0.0, wf, hf, radius, fill);
                let edge = if focus { ds::BRAND } else { ds::BORDER_STRONG };
                c.ring(0.0, 0.0, wf, hf, radius, 1.0 * s.max(1.0), edge);
                let color = if disabled {
                    ds::FG3
                } else if b.kind == Kind::Danger {
                    ds::DANGER
                } else {
                    ds::FG
                };
                c.text(&label, full, color, b.font, centered, 0);
            }
            Kind::Ghost => {
                if hover || pressed {
                    c.rrect(0.0, 0.0, wf, hf, radius, ds::RAISED);
                }
                if focus {
                    c.ring(0.0, 0.0, wf, hf, radius, 1.0, ds::BRAND);
                }
                let color = if disabled {
                    ds::FG3
                } else if hover {
                    ds::FG
                } else {
                    ds::FG2
                };
                c.text(&label, full, color, b.font, centered, 0);
            }
            Kind::Nav => {
                if b.on {
                    c.rrect(0.0, 0.0, wf, hf, radius, ds::RAISED);
                    c.rrect(0.0, hf * 0.28, 3.0 * s, hf * 0.44, 1.5 * s, ds::BRAND);
                } else if hover {
                    c.rrect(0.0, 0.0, wf, hf, radius, 0x18191C);
                }
                if focus {
                    c.ring(0.0, 0.0, wf, hf, radius, 1.0, ds::BRAND);
                }
                let color = if b.on || hover { ds::FG } else { ds::FG2 };
                if let Some(icon) = b.icon {
                    let r = RECT {
                        left: (12.0 * s) as i32,
                        top: 0,
                        right: (34.0 * s) as i32,
                        bottom: h,
                    };
                    let tint = if b.on { ds::BRAND } else { color };
                    c.text(
                        &icon.to_string(),
                        r,
                        tint,
                        HFONT(self.theme.icon_small.handle as _),
                        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                        0,
                    );
                }
                let r = RECT {
                    left: (42.0 * s) as i32,
                    top: 0,
                    right: w - (8.0 * s) as i32,
                    bottom: h,
                };
                c.text(
                    &label,
                    r,
                    color,
                    b.font,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                    0,
                );
            }
            Kind::Segment => {
                if b.on {
                    c.rrect(
                        0.0,
                        0.0,
                        wf,
                        hf,
                        7.0 * s,
                        if disabled { ds::BRAND_DIM } else { ds::BRAND },
                    );
                } else if hover {
                    c.rrect(0.0, 0.0, wf, hf, 7.0 * s, ds::BORDER);
                }
                if focus {
                    c.ring(
                        0.0,
                        0.0,
                        wf,
                        hf,
                        7.0 * s,
                        1.0,
                        if b.on { ds::INK } else { ds::BRAND },
                    );
                }
                let color = if b.on {
                    ds::INK
                } else if hover {
                    ds::FG
                } else {
                    ds::FG2
                };
                c.text(&label, full, color, b.font, centered, 0);
            }
            Kind::Dropdown => {
                c.rrect(0.0, 0.0, wf, hf, radius, ds::FIELD);
                let edge = if focus {
                    ds::BRAND
                } else if hover || pressed {
                    0x4A4D52
                } else {
                    ds::BORDER_STRONG
                };
                c.ring(0.0, 0.0, wf, hf, radius, 1.0, edge);
                let text = RECT {
                    left: (12.0 * s) as i32,
                    top: 0,
                    right: w - (34.0 * s) as i32,
                    bottom: h,
                };
                c.text(
                    &label,
                    text,
                    if disabled { ds::FG3 } else { ds::FG },
                    b.font,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                    0,
                );
                let chevron = RECT {
                    left: w - (32.0 * s) as i32,
                    top: 0,
                    right: w - (10.0 * s) as i32,
                    bottom: h,
                };
                c.text(
                    "\u{E70D}",
                    chevron,
                    ds::FG2,
                    HFONT(self.theme.icon_small.handle as _),
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                    0,
                );
            }
            Kind::Toggle => {
                let track = if disabled {
                    ds::RAISED
                } else if b.on {
                    if hover {
                        ds::BRAND_HOVER
                    } else {
                        ds::BRAND
                    }
                } else if hover {
                    0x4A4D52
                } else {
                    0x3A3D42
                };
                c.rrect(0.0, 0.0, wf, hf, hf / 2.0, track);
                if focus {
                    c.ring(
                        0.0,
                        0.0,
                        wf,
                        hf,
                        hf / 2.0,
                        1.5 * s,
                        if b.on { ds::INK } else { ds::BRAND },
                    );
                }
                let d = hf - 6.0 * s;
                let cx = if b.on {
                    wf - 3.0 * s - d / 2.0
                } else {
                    3.0 * s + d / 2.0
                };
                c.rrect(cx - d / 2.0, (hf - d) / 2.0, d, d, d / 2.0, ds::KNOB);
            }
        }
        c.blit(item.hdc, rc.left, rc.top);
    }

    /// One history row: date and mode on top, the text below.
    unsafe fn draw_row(&self, item: &DrawItem) {
        let rc = item.rc;
        let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
        let Some(mut c) = Canvas::new(item.hdc, w, h) else {
            return;
        };
        c.fill(ds::FIELD);
        if item.item_id != u32::MAX {
            let s = self.scale;
            let selected = item.item_state & m::ODS_SELECTED != 0;
            if selected {
                c.rrect(
                    2.0 * s,
                    2.0 * s,
                    w as f32 - 4.0 * s,
                    h as f32 - 4.0 * s,
                    8.0 * s,
                    ds::RAISED,
                );
                c.rrect(
                    2.0 * s,
                    h as f32 * 0.3,
                    3.0 * s,
                    h as f32 * 0.4,
                    1.5 * s,
                    ds::BRAND,
                );
            }
            let list = item.hwnd_item;
            let len = SendMessageW(
                list,
                m::LB_GETTEXTLEN,
                WPARAM(item.item_id as usize),
                LPARAM(0),
            )
            .0;
            if len > 0 {
                let mut buf = vec![0u16; len as usize + 1];
                SendMessageW(
                    list,
                    m::LB_GETTEXT,
                    WPARAM(item.item_id as usize),
                    LPARAM(buf.as_mut_ptr() as isize),
                );
                let row = String::from_utf16_lossy(&buf[..len as usize]);
                let (meta, text) = row.split_once('\t').unwrap_or(("", row.as_str()));
                let left = (16.0 * s) as i32;
                let right = w - (12.0 * s) as i32;
                let flags = DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS | DT_VCENTER;
                c.text(
                    meta,
                    RECT {
                        left,
                        top: (6.0 * s) as i32,
                        right,
                        bottom: (24.0 * s) as i32,
                    },
                    ds::FG3,
                    HFONT(self.theme.small.handle as _),
                    flags,
                    0,
                );
                c.text(
                    text,
                    RECT {
                        left,
                        top: (24.0 * s) as i32,
                        right,
                        bottom: h - (6.0 * s) as i32,
                    },
                    if selected { ds::FG } else { 0xC9C9C9 },
                    HFONT(self.theme.body.handle as _),
                    flags,
                    0,
                );
            }
        }
        c.blit(item.hdc, rc.left, rc.top);
    }
}

impl Drop for Skin {
    fn drop(&mut self) {
        for (_, b) in self.brushes.borrow_mut().drain() {
            unsafe {
                let _ = DeleteObject(b);
            }
        }
    }
}

/// The list a dropdown opens: a borderless popup that never takes focus and
/// runs a small modal loop like a menu (mouse capture; keys and the wheel are
/// read from the loop), so Windows' own light menus never show.
mod popup {
    use super::{ds, invalidate, Canvas};
    use std::sync::Once;
    use windows::core::w;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWINDOWATTRIBUTE};
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetCapture, ReleaseCapture, SetCapture};
    use windows::Win32::UI::WindowsAndMessaging::*;

    const ROWS: usize = 9;

    struct List {
        items: Vec<String>,
        selected: Option<usize>,
        hover: Option<usize>,
        offset: usize,
        rows: usize,
        row_h: i32,
        pad: i32,
        scale: f32,
        font: HFONT,
        icons: HFONT,
        done: bool,
        chosen: Option<usize>,
    }

    impl List {
        fn row_at(&self, y: i32) -> Option<usize> {
            if y < self.pad {
                return None;
            }
            let i = ((y - self.pad) / self.row_h) as usize;
            (i < self.rows && self.offset + i < self.items.len()).then_some(self.offset + i)
        }

        fn reveal(&mut self, i: usize) {
            if i < self.offset {
                self.offset = i;
            } else if i >= self.offset + self.rows {
                self.offset = i + 1 - self.rows;
            }
        }

        fn scroll(&mut self, by: i32) {
            let max = self.items.len().saturating_sub(self.rows) as i32;
            self.offset = (self.offset as i32 + by).clamp(0, max) as usize;
        }

        fn key(&mut self, vk: usize) {
            let last = self.items.len().saturating_sub(1);
            let at = self.hover.or(self.selected).unwrap_or(0);
            let next = match vk {
                0x26 => at.saturating_sub(1),         // up
                0x28 => (at + 1).min(last),           // down
                0x21 => at.saturating_sub(self.rows), // page up
                0x22 => (at + self.rows).min(last),   // page down
                0x24 => 0,                            // home
                0x23 => last,                         // end
                0x0D | 0x20 => {
                    // enter, space
                    self.chosen = Some(at);
                    self.done = true;
                    return;
                }
                0x1B | 0x09 => {
                    // esc, tab
                    self.done = true;
                    return;
                }
                _ => return,
            };
            self.hover = Some(next);
            self.reveal(next);
        }

        /// Typing a letter jumps to the next item starting with it.
        fn jump(&mut self, c: char) {
            let c = c.to_lowercase().next().unwrap_or(c);
            let n = self.items.len();
            let start = self.hover.or(self.selected).map_or(0, |i| i + 1);
            for k in 0..n {
                let i = (start + k) % n;
                if self.items[i].to_lowercase().starts_with(c) {
                    self.hover = Some(i);
                    self.reveal(i);
                    return;
                }
            }
        }

        unsafe fn paint(&self, hwnd: HWND) {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut client = RECT::default();
            let _ = GetClientRect(hwnd, &mut client);
            if let Some(mut c) = Canvas::new(hdc, client.right, client.bottom) {
                let s = self.scale;
                let (w, h) = (client.right as f32, client.bottom as f32);
                c.fill(ds::SURFACE);
                c.ring(0.0, 0.0, w, h, 8.0 * s, 1.0, ds::BORDER_STRONG);
                let pad = self.pad as f32;
                for row in 0..self.rows {
                    let i = self.offset + row;
                    let Some(text) = self.items.get(i) else { break };
                    let top = self.pad + row as i32 * self.row_h;
                    if self.hover == Some(i) {
                        c.rrect(
                            pad,
                            top as f32,
                            w - 2.0 * pad,
                            self.row_h as f32,
                            6.0 * s,
                            ds::RAISED,
                        );
                    }
                    let r = RECT {
                        left: self.pad + (10.0 * s) as i32,
                        top,
                        right: client.right - self.pad - (30.0 * s) as i32,
                        bottom: top + self.row_h,
                    };
                    let color = if self.selected == Some(i) || self.hover == Some(i) {
                        ds::FG
                    } else {
                        ds::FG2
                    };
                    c.text(
                        text,
                        r,
                        color,
                        self.font,
                        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
                        0,
                    );
                    if self.selected == Some(i) {
                        let check = RECT {
                            left: client.right - self.pad - (30.0 * s) as i32,
                            right: client.right - self.pad - (6.0 * s) as i32,
                            ..r
                        };
                        c.text(
                            "\u{E73E}",
                            check,
                            ds::BRAND,
                            self.icons,
                            DT_CENTER | DT_SINGLELINE | DT_VCENTER,
                            0,
                        );
                    }
                }
                if self.items.len() > self.rows {
                    // A thin thumb shows where the visible rows are.
                    let track = h - 2.0 * pad;
                    let len = (track * self.rows as f32 / self.items.len() as f32).max(16.0 * s);
                    let max = self.items.len() - self.rows;
                    let top = pad + (track - len) * self.offset as f32 / max as f32;
                    c.rrect(w - 5.0 * s, top, 3.0 * s, len, 1.5 * s, ds::BORDER_STRONG);
                }
                c.blit(hdc, 0, 0);
            }
            let _ = EndPaint(hwnd, &ps);
        }
    }

    unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        let list = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut List;
        if list.is_null() {
            return DefWindowProcW(hwnd, msg, w, l);
        }
        let list = &mut *list;
        let x = (l.0 & 0xFFFF) as u16 as i16 as i32;
        let y = ((l.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
        let mut client = RECT::default();
        let _ = GetClientRect(hwnd, &mut client);
        let inside = x >= 0 && y >= 0 && x < client.right && y < client.bottom;
        match msg {
            WM_PAINT => {
                list.paint(hwnd);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_MOUSEACTIVATE => LRESULT(3), // MA_NOACTIVATE
            WM_MOUSEMOVE => {
                let row = if inside { list.row_at(y) } else { None };
                if row.is_some() && row != list.hover {
                    list.hover = row;
                    invalidate(hwnd.0 as isize);
                }
                LRESULT(0)
            }
            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN => {
                if inside {
                    if let Some(i) = list.row_at(y) {
                        list.chosen = Some(i);
                        list.done = true;
                    }
                } else {
                    list.done = true;
                }
                LRESULT(0)
            }
            WM_CAPTURECHANGED => {
                list.done = true;
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, w, l),
        }
    }

    static REGISTER: Once = Once::new();

    /// Shows the list under `anchor` (screen pixels) and returns the chosen
    /// index, or None when dismissed.
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn run(
        owner: HWND,
        anchor: RECT,
        items: &[String],
        selected: Option<usize>,
        font: HFONT,
        icons: HFONT,
        scale: f32,
    ) -> Option<usize> {
        let Ok(module) = GetModuleHandleW(None) else {
            return None;
        };
        REGISTER.call_once(|| {
            let class = WNDCLASSW {
                style: CS_DROPSHADOW,
                lpfnWndProc: Some(wndproc),
                hInstance: module.into(),
                lpszClassName: w!("HlasDropdown"),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                ..Default::default()
            };
            RegisterClassW(&class);
        });
        let rows = items.len().min(ROWS);
        let row_h = (32.0 * scale).round() as i32;
        let pad = (5.0 * scale).round() as i32;
        let width = anchor.right - anchor.left;
        let height = rows as i32 * row_h + 2 * pad;
        let gap = (4.0 * scale).round() as i32;
        let mut y = anchor.bottom + gap;
        let monitor = MonitorFromRect(&anchor, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(monitor, &mut info).as_bool() && y + height > info.rcWork.bottom {
            y = anchor.top - gap - height;
        }

        let mut list = Box::new(List {
            items: items.to_vec(),
            selected,
            hover: selected,
            offset: 0,
            rows,
            row_h,
            pad,
            scale,
            font,
            icons,
            done: false,
            chosen: None,
        });
        if let Some(i) = selected {
            list.reveal(i);
            list.offset = list
                .offset
                .max(i.saturating_sub(rows / 2))
                .min(items.len().saturating_sub(rows));
        }
        let Ok(hwnd) = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
            w!("HlasDropdown"),
            w!(""),
            WS_POPUP,
            anchor.left,
            y,
            width,
            height,
            owner,
            None,
            module,
            None,
        ) else {
            return None;
        };
        let raw = Box::into_raw(list);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, raw as isize);
        let round = 2u32; // DWMWCP_ROUND
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWINDOWATTRIBUTE(33),
            &round as *const u32 as *const std::ffi::c_void,
            4,
        );
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        SetCapture(hwnd);

        let mut msg = MSG::default();
        while !(*raw).done && GetCapture() == hwnd {
            let got = GetMessageW(&mut msg, None, 0, 0);
            if got.0 <= 0 {
                if got.0 == 0 {
                    PostQuitMessage(msg.wParam.0 as i32);
                }
                break;
            }
            match msg.message {
                WM_KEYDOWN | WM_SYSKEYDOWN => {
                    let vk = msg.wParam.0;
                    (*raw).key(vk);
                    if (0x30..=0x5A).contains(&vk) {
                        // Letters and digits come back as WM_CHAR for jump().
                        let _ = TranslateMessage(&msg);
                    }
                    invalidate(hwnd.0 as isize);
                    continue;
                }
                WM_CHAR => {
                    if let Some(c) =
                        char::from_u32(msg.wParam.0 as u32).filter(|c| c.is_alphanumeric())
                    {
                        (*raw).jump(c);
                        invalidate(hwnd.0 as isize);
                    }
                    continue;
                }
                WM_KEYUP | WM_SYSKEYUP | WM_SYSCHAR => continue,
                WM_MOUSEWHEEL => {
                    let delta = ((msg.wParam.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
                    (*raw).scroll(if delta > 0 { -3 } else { 3 });
                    invalidate(hwnd.0 as isize);
                    continue;
                }
                _ => {}
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        if GetCapture() == hwnd {
            let _ = ReleaseCapture();
        }
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        let _ = DestroyWindow(hwnd);
        Box::from_raw(raw).chosen
    }
}
