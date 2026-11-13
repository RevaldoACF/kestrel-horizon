use crate::config::{XrayKey, ZoomKey};
use crate::control_bridge::{send_cmd, ControlCmd};
use crate::state::{UiKey, STATE};
use std::ffi::{c_char, c_int, c_uint, c_ulong, c_void};
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

pub type Display = c_void;
pub type Window = c_ulong;
pub type Cursor = c_ulong;
pub type Time = c_ulong;
pub type Bool = c_int;
pub type KeySym = c_ulong;
pub type XcbConnection = c_void;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct XGenericEventCookie {
    pub r#type: c_int,
    pub _pad0: c_int,
    pub serial: c_ulong,
    pub send_event: Bool,
    pub display: *mut Display,
    pub extension: c_int,
    pub evtype: c_int,
    pub cookie: c_uint,
    pub _pad1: c_uint,
    pub data: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct XKeyEvent {
    pub r#type: c_int,
    pub _pad0: c_int,
    pub serial: c_ulong,
    pub send_event: Bool,
    pub display: *mut Display,
    pub window: Window,
    pub root: Window,
    pub subwindow: Window,
    pub time: Time,
    pub x: c_int,
    pub y: c_int,
    pub x_root: c_int,
    pub y_root: c_int,
    pub state: c_uint,
    pub keycode: c_uint,
    pub same_screen: Bool,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct XButtonEvent {
    pub r#type: c_int,
    pub _pad0: c_int,
    pub serial: c_ulong,
    pub send_event: Bool,
    pub display: *mut Display,
    pub window: Window,
    pub root: Window,
    pub subwindow: Window,
    pub time: Time,
    pub x: c_int,
    pub y: c_int,
    pub x_root: c_int,
    pub y_root: c_int,
    pub state: c_uint,
    pub button: c_uint,
    pub same_screen: Bool,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct XMotionEvent {
    pub r#type: c_int,
    pub _pad0: c_int,
    pub serial: c_ulong,
    pub send_event: Bool,
    pub display: *mut Display,
    pub window: Window,
    pub root: Window,
    pub subwindow: Window,
    pub time: Time,
    pub x: c_int,
    pub y: c_int,
    pub x_root: c_int,
    pub y_root: c_int,
    pub state: c_uint,
    pub is_hint: c_char,
    pub same_screen: Bool,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union XEvent {
    pub r#type: c_int,
    pub xkey: XKeyEvent,
    pub xbutton: XButtonEvent,
    pub xmotion: XMotionEvent,
    pub xcookie: XGenericEventCookie,
    pub pad: [CLongPad; 24],
}
type CLongPad = libc::c_long;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct XIDeviceEvent {
    pub r#type: c_int,
    pub _pad0: c_int,
    pub serial: c_ulong,
    pub send_event: Bool,
    pub display: *mut Display,
    pub extension: c_int,
    pub evtype: c_int,
    pub time: Time,
    pub deviceid: c_int,
    pub sourceid: c_int,
    pub detail: c_int,
    pub _pad1: c_int,
    pub root: Window,
    pub event: Window,
    pub child: Window,
    pub root_x: f64,
    pub root_y: f64,
    pub event_x: f64,
    pub event_y: f64,
    pub flags: c_int,
    pub _pad2: c_int,
}

const KEY_PRESS: c_int = 2;
const KEY_RELEASE: c_int = 3;
const BUTTON_PRESS: c_int = 4;
const BUTTON_RELEASE: c_int = 5;
const MOTION_NOTIFY: c_int = 6;
const GENERIC_EVENT: c_int = 35;

const XI_BUTTON_PRESS: c_int = 4;
const XI_BUTTON_RELEASE: c_int = 5;
const XI_MOTION: c_int = 6;
const XI_RAW_KEY_PRESS: c_int = 13;
const XI_RAW_KEY_RELEASE: c_int = 14;
const XI_RAW_BUTTON_PRESS: c_int = 15;
const XI_RAW_BUTTON_RELEASE: c_int = 16;
const XI_RAW_MOTION: c_int = 17;

const XK_GRAVE: KeySym = 0x0060;
const XK_ASCIITILDE: KeySym = 0x007e;
const XK_ESCAPE: KeySym = 0xff1b;
const XK_TAB: KeySym = 0xff09;
const XK_ISO_LEFT_TAB: KeySym = 0xfe20;
const XK_RETURN: KeySym = 0xff0d;
const XK_KP_ENTER: KeySym = 0xff8d;
const XK_BACKSPACE: KeySym = 0xff08;
const XK_SPACE: KeySym = 0x0020;
const XK_LEFT: KeySym = 0xff51;
const XK_UP: KeySym = 0xff52;
const XK_RIGHT: KeySym = 0xff53;
const XK_DOWN: KeySym = 0xff54;
const XK_SHIFT_L: KeySym = 0xffe1;
const XK_SHIFT_R: KeySym = 0xffe2;
const XK_ALT_L: KeySym = 0xffe9;
const XK_ALT_R: KeySym = 0xffea;

pub type FnXCheckIfEvent = unsafe extern "C" fn(
    *mut Display,
    *mut XEvent,
    Option<unsafe extern "C" fn(*mut Display, *mut XEvent, *mut c_char) -> Bool>,
    *mut c_char,
) -> Bool;
type FnXGetEventData = unsafe extern "C" fn(*mut Display, *mut XGenericEventCookie) -> Bool;
type FnXFreeEventData = unsafe extern "C" fn(*mut Display, *mut XGenericEventCookie);
type FnXLookupString = unsafe extern "C" fn(
    *mut XKeyEvent,
    *mut c_char,
    c_int,
    *mut KeySym,
    *mut c_void,
) -> c_int;
type FnXKeycodeToKeysym = unsafe extern "C" fn(*mut Display, c_uint, c_int) -> KeySym;
type FnXUngrabPointer = unsafe extern "C" fn(*mut Display, Time) -> c_int;
type FnXGrabPointer = unsafe extern "C" fn(
    *mut Display,
    Window,
    Bool,
    c_uint,
    c_int,
    c_int,
    Window,
    Cursor,
    Time,
) -> c_int;
type FnXDefineCursor = unsafe extern "C" fn(*mut Display, Window, Cursor) -> c_int;
type FnXCreateFontCursor = unsafe extern "C" fn(*mut Display, c_uint) -> Cursor;
type FnXFlush = unsafe extern "C" fn(*mut Display) -> c_int;
type FnXGetXCBConnection = unsafe extern "C" fn(*mut Display) -> *mut XcbConnection;
type FnXcbGrabPointer = unsafe extern "C" fn(
    *mut XcbConnection,
    u8,
    u32,
    u16,
    u8,
    u8,
    u32,
    u32,
    u32,
) -> u32;
type FnXcbUngrabPointer = unsafe extern "C" fn(*mut XcbConnection, u32) -> u32;
type FnXDefaultScreen = unsafe extern "C" fn(*mut Display) -> c_int;
type FnXRootWindow = unsafe extern "C" fn(*mut Display, c_int) -> Window;
type FnXCreateBitmapFromData =
    unsafe extern "C" fn(*mut Display, Window, *const c_char, c_uint, c_uint) -> c_ulong;
type FnXCreatePixmapCursor = unsafe extern "C" fn(
    *mut Display,
    c_ulong,
    c_ulong,
    *mut c_void,
    *mut c_void,
    c_uint,
    c_uint,
) -> Cursor;
type FnXFreePixmap = unsafe extern "C" fn(*mut Display, c_ulong) -> c_int;
type FnXcbChangeWindowAttributes =
    unsafe extern "C" fn(*mut XcbConnection, u32, u32, *const u32) -> u32;
type FnXcbFlush = unsafe extern "C" fn(*mut XcbConnection) -> c_int;

struct X11Fns {
    x_check_if_event: Option<FnXCheckIfEvent>,
    x_get_event_data: Option<FnXGetEventData>,
    x_free_event_data: Option<FnXFreeEventData>,
    x_lookup_string: Option<FnXLookupString>,
    x_keycode_to_keysym: Option<FnXKeycodeToKeysym>,
    x_ungrab_pointer: Option<FnXUngrabPointer>,
    x_grab_pointer: Option<FnXGrabPointer>,
    x_define_cursor: Option<FnXDefineCursor>,
    x_create_font_cursor: Option<FnXCreateFontCursor>,
    x_default_screen: Option<FnXDefaultScreen>,
    x_root_window: Option<FnXRootWindow>,
    x_create_bitmap_from_data: Option<FnXCreateBitmapFromData>,
    x_create_pixmap_cursor: Option<FnXCreatePixmapCursor>,
    x_free_pixmap: Option<FnXFreePixmap>,
    x_flush: Option<FnXFlush>,
    x_get_xcb_conn: Option<FnXGetXCBConnection>,
    xcb_grab_pointer: Option<FnXcbGrabPointer>,
    xcb_ungrab_pointer: Option<FnXcbUngrabPointer>,
    xcb_change_window_attributes: Option<FnXcbChangeWindowAttributes>,
    xcb_change_window_attributes_checked: Option<FnXcbChangeWindowAttributes>,
    xcb_flush: Option<FnXcbFlush>,
}

static X11_FNS: OnceLock<X11Fns> = OnceLock::new();

fn resolve_x11() -> &'static X11Fns {
    X11_FNS.get_or_init(|| unsafe {
        let libx11 = libc::dlopen(c"libX11.so.6".as_ptr(), libc::RTLD_LAZY | libc::RTLD_NOLOAD);
        let libx11 = if libx11.is_null() {
            libc::dlopen(c"libX11.so.6".as_ptr(), libc::RTLD_LAZY)
        } else {
            libx11
        };
        let libx11_xcb =
            libc::dlopen(c"libX11-xcb.so.1".as_ptr(), libc::RTLD_LAZY | libc::RTLD_NOLOAD);
        let libxcb = libc::dlopen(c"libxcb.so.1".as_ptr(), libc::RTLD_LAZY | libc::RTLD_NOLOAD);

        macro_rules! sym {
            ($lib:expr, $name:literal, $ty:ty) => {{
                let mut p = if !$lib.is_null() {
                    crate::real_dlsym($lib, $name.as_ptr() as *const c_char)
                } else {
                    ptr::null_mut()
                };
                if p.is_null() {
                    p = crate::real_dlsym(libc::RTLD_NEXT, $name.as_ptr() as *const c_char);
                }
                if p.is_null() {
                    None
                } else {
                    Some(std::mem::transmute::<*mut c_void, $ty>(p))
                }
            }};
        }

        X11Fns {
            x_check_if_event: sym!(libx11, c"XCheckIfEvent", FnXCheckIfEvent),
            x_get_event_data: sym!(libx11, c"XGetEventData", FnXGetEventData),
            x_free_event_data: sym!(libx11, c"XFreeEventData", FnXFreeEventData),
            x_lookup_string: sym!(libx11, c"XLookupString", FnXLookupString),
            x_keycode_to_keysym: sym!(libx11, c"XKeycodeToKeysym", FnXKeycodeToKeysym),
            x_ungrab_pointer: sym!(libx11, c"XUngrabPointer", FnXUngrabPointer),
            x_grab_pointer: sym!(libx11, c"XGrabPointer", FnXGrabPointer),
            x_define_cursor: sym!(libx11, c"XDefineCursor", FnXDefineCursor),
            x_create_font_cursor: sym!(libx11, c"XCreateFontCursor", FnXCreateFontCursor),
            x_default_screen: sym!(libx11, c"XDefaultScreen", FnXDefaultScreen),
            x_root_window: sym!(libx11, c"XRootWindow", FnXRootWindow),
            x_create_bitmap_from_data: sym!(libx11, c"XCreateBitmapFromData", FnXCreateBitmapFromData),
            x_create_pixmap_cursor: sym!(libx11, c"XCreatePixmapCursor", FnXCreatePixmapCursor),
            x_free_pixmap: sym!(libx11, c"XFreePixmap", FnXFreePixmap),
            x_flush: sym!(libx11, c"XFlush", FnXFlush),
            x_get_xcb_conn: sym!(libx11_xcb, c"XGetXCBConnection", FnXGetXCBConnection),
            xcb_grab_pointer: sym!(libxcb, c"xcb_grab_pointer", FnXcbGrabPointer),
            xcb_ungrab_pointer: sym!(libxcb, c"xcb_ungrab_pointer", FnXcbUngrabPointer),
            xcb_change_window_attributes: sym!(
                libxcb,
                c"xcb_change_window_attributes",
                FnXcbChangeWindowAttributes
            ),
            xcb_change_window_attributes_checked: sym!(
                libxcb,
                c"xcb_change_window_attributes_checked",
                FnXcbChangeWindowAttributes
            ),
            xcb_flush: sym!(libxcb, c"xcb_flush", FnXcbFlush),
        }
    })
}

static LAST_DISPLAY: AtomicUsize = AtomicUsize::new(0);
static LAST_WINDOW: AtomicUsize = AtomicUsize::new(0);
static LAST_XCB_CONN: AtomicUsize = AtomicUsize::new(0);
static ARROW_CURSOR: AtomicUsize = AtomicUsize::new(0);
static EMPTY_CURSOR: AtomicUsize = AtomicUsize::new(0);
static SAVED_GAME_CURSOR: AtomicUsize = AtomicUsize::new(0);
static GAME_WANTS_GRAB: AtomicBool = AtomicBool::new(false);
static GRAVE_DOWN: AtomicBool = AtomicBool::new(false);
static SHIFT_DOWN: AtomicBool = AtomicBool::new(false);
pub static UI_CAPTURING: AtomicBool = AtomicBool::new(false);
pub static PASSIVE_KEYS_MASK: AtomicU8 = AtomicU8::new(0);
pub static FREECAM_ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn set_freecam_active(active: bool) {
    FREECAM_ACTIVE.store(active, Ordering::Relaxed);
}

pub fn set_ui_capturing(capturing: bool) {
    UI_CAPTURING.store(capturing, Ordering::Relaxed);
}

#[inline(always)]
fn ui_capturing() -> bool {
    UI_CAPTURING.load(Ordering::Relaxed)
}

fn ensure_arrow_cursor(dpy: *mut Display) -> Cursor {
    let cached = ARROW_CURSOR.load(Ordering::Relaxed) as Cursor;
    if cached != 0 {
        return cached;
    }
    if let Some(f) = resolve_x11().x_create_font_cursor {
        let cur = unsafe { f(dpy, 68 /* XC_left_ptr */) };
        ARROW_CURSOR.store(cur as usize, Ordering::Relaxed);
        return cur;
    }
    0
}

fn ensure_empty_cursor(dpy: *mut Display) -> Cursor {
    let cached = EMPTY_CURSOR.load(Ordering::Relaxed) as Cursor;
    if cached != 0 {
        return cached;
    }
    let fns = resolve_x11();
    if let (Some(ds), Some(rw), Some(cb), Some(cpc), Some(fp)) = (
        fns.x_default_screen,
        fns.x_root_window,
        fns.x_create_bitmap_from_data,
        fns.x_create_pixmap_cursor,
        fns.x_free_pixmap,
    ) {
        unsafe {
            let screen = ds(dpy);
            let root = rw(dpy, screen);
            let data: [c_char; 1] = [0];
            let pixmap = cb(dpy, root, data.as_ptr(), 1, 1);
            if pixmap != 0 {
                let mut dummy = [0u8; 32];
                let cur = cpc(
                    dpy,
                    pixmap,
                    pixmap,
                    dummy.as_mut_ptr() as *mut c_void,
                    dummy.as_mut_ptr() as *mut c_void,
                    0,
                    0,
                );
                fp(dpy, pixmap);
                EMPTY_CURSOR.store(cur as usize, Ordering::Relaxed);
                return cur;
            }
        }
    }
    0
}

fn unlock_cursor_for_ui(dpy: *mut Display, win: Window) {
    let fns = resolve_x11();
    if dpy.is_null() {
        return;
    }
    unsafe {
        if let Some(f) = fns.x_ungrab_pointer {
            f(dpy, 0);
        }
        let mut conn = LAST_XCB_CONN.load(Ordering::Relaxed) as *mut XcbConnection;
        if conn.is_null()
            && let Some(get_xcb) = fns.x_get_xcb_conn
        {
            conn = get_xcb(dpy);
        }
        if !conn.is_null() {
            if let Some(xcb_ungrab) = fns.xcb_ungrab_pointer {
                xcb_ungrab(conn, 0);
            }
            if win != 0
                && let Some(cwa) = fns.xcb_change_window_attributes
            {
                let arrow = ensure_arrow_cursor(dpy) as u32;
                if arrow != 0 {
                    cwa(conn, win as u32, 1 << 14, &arrow);
                }
            }
            if let Some(xcb_fl) = fns.xcb_flush {
                xcb_fl(conn);
            }
        }
        if win != 0
            && let Some(def_cur) = fns.x_define_cursor
        {
            let arrow = ensure_arrow_cursor(dpy);
            if arrow != 0 {
                def_cur(dpy, win, arrow);
            }
        }
        if let Some(flush) = fns.x_flush {
            flush(dpy);
        }
    }
}

fn restore_cursor_for_game(dpy: *mut Display, win: Window) {
    let fns = resolve_x11();
    if dpy.is_null() || win == 0 {
        return;
    }
    let in_world_playing = {
        let st = STATE.lock();
        st.game.in_world && st.game.screen == "playing"
    };
    let wants_grab = GAME_WANTS_GRAB.load(Ordering::Relaxed) || in_world_playing;

    unsafe {
        let mut target_cursor = SAVED_GAME_CURSOR.load(Ordering::Relaxed) as Cursor;
        if target_cursor == 0 && wants_grab {
            target_cursor = ensure_empty_cursor(dpy);
        }
        let mut conn = LAST_XCB_CONN.load(Ordering::Relaxed) as *mut XcbConnection;
        if conn.is_null()
            && let Some(get_xcb) = fns.x_get_xcb_conn
        {
            conn = get_xcb(dpy);
        }
        if target_cursor != 0 {
            if let Some(def_cur) = fns.x_define_cursor {
                def_cur(dpy, win, target_cursor);
            }
            if !conn.is_null()
                && let Some(cwa) = fns.xcb_change_window_attributes
            {
                let cid = target_cursor as u32;
                cwa(conn, win as u32, 1 << 14, &cid);
            }
        }
        if wants_grab {
            let mask: u16 = (1 << 2) | (1 << 3) | (1 << 6); // ButtonPress | ButtonRelease | PointerMotion
            if !conn.is_null()
                && let Some(xcb_grab) = fns.xcb_grab_pointer
            {
                xcb_grab(conn, 1, win as u32, mask, 1, 1, win as u32, target_cursor as u32, 0);
            } else if let Some(grab) = fns.x_grab_pointer {
                grab(dpy, win, 1, mask as c_uint, 1, 1, win, target_cursor, 0);
            }
        }
        if !conn.is_null()
            && let Some(xcb_fl) = fns.xcb_flush
        {
            xcb_fl(conn);
        }
        if let Some(flush) = fns.x_flush {
            flush(dpy);
        }
    }
}

fn toggle_menu(dpy: *mut Display, win: Window) {
    let (opened, should_restore) = {
        let mut st = STATE.lock();
        if st.hud_edit_mode {
            st.hud_edit_mode = false;
            st.dragging_widget = None;
            let _ = st.config.save();
            UI_CAPTURING.store(false, Ordering::Relaxed);
            (false, true)
        } else {
            st.menu_open = !st.menu_open;
            st.active_slider = None;
            st.active_text_target = None;
            UI_CAPTURING.store(st.menu_open, Ordering::Relaxed);
            (st.menu_open, !st.menu_open)
        }
    };
    if opened {
        unlock_cursor_for_ui(dpy, win);
    } else if should_restore {
        restore_cursor_for_game(dpy, win);
    }
}

fn handle_key_in_ui(sym: KeySym, pressed: bool, text_buf: &str) {
    if sym == XK_SHIFT_L || sym == XK_SHIFT_R {
        SHIFT_DOWN.store(pressed, Ordering::Relaxed);
    }
    if !pressed {
        return;
    }

    let mut restore = false;
    {
        let mut st = STATE.lock();
        match sym {
            XK_ESCAPE => {
                if st.hud_edit_mode {
                    st.hud_edit_mode = false;
                    st.dragging_widget = None;
                    let _ = st.config.save();
                    UI_CAPTURING.store(false, Ordering::Relaxed);
                    restore = true;
                } else if st.active_text_target.is_some() {
                    st.pending_keys.push(UiKey::Escape);
                } else {
                    st.menu_open = false;
                    UI_CAPTURING.store(false, Ordering::Relaxed);
                    restore = true;
                }
            }
            XK_TAB => {
                if SHIFT_DOWN.load(Ordering::Relaxed) {
                    st.pending_keys.push(UiKey::ShiftTab);
                } else {
                    st.pending_keys.push(UiKey::Tab);
                }
            }
            XK_ISO_LEFT_TAB => st.pending_keys.push(UiKey::ShiftTab),
            XK_RETURN | XK_KP_ENTER => st.pending_keys.push(UiKey::Enter),
            XK_BACKSPACE => st.pending_keys.push(UiKey::Backspace),
            XK_SPACE => st.pending_keys.push(UiKey::Space),
            XK_LEFT => st.pending_keys.push(UiKey::Left),
            XK_RIGHT => st.pending_keys.push(UiKey::Right),
            XK_UP => st.pending_keys.push(UiKey::Up),
            XK_DOWN => st.pending_keys.push(UiKey::Down),
            _ => {
                for ch in text_buf.chars() {
                    if !ch.is_control() {
                        st.pending_chars.push(ch);
                    }
                }
            }
        }
    }
    if restore {
        let dpy = LAST_DISPLAY.load(Ordering::Relaxed) as *mut Display;
        let win = LAST_WINDOW.load(Ordering::Relaxed) as Window;
        restore_cursor_for_game(dpy, win);
    }
}

#[inline]
fn set_mask_bit(bit: u8, on: bool) {
    if on {
        PASSIVE_KEYS_MASK.fetch_or(1 << bit, Ordering::Relaxed);
    } else {
        PASSIVE_KEYS_MASK.fetch_and(!(1 << bit), Ordering::Relaxed);
    }
}

fn handle_key_passive_fast(dpy: *mut Display, keycode: c_uint, pressed: bool) -> bool {
    let fns = resolve_x11();
    let sym = if let Some(k2s) = fns.x_keycode_to_keysym {
        unsafe { k2s(dpy, keycode, 0) }
    } else {
        0
    };
    let lower = if (0x41..=0x5A).contains(&sym) {
        sym + 0x20
    } else {
        sym
    };

    let freecam = FREECAM_ACTIVE.load(Ordering::Relaxed);
    match lower {
        0x77 => {
            set_mask_bit(0, pressed);
            return freecam;
        }
        0x61 => {
            set_mask_bit(1, pressed);
            return freecam;
        }
        0x73 => {
            set_mask_bit(2, pressed);
            return freecam;
        }
        0x64 => {
            set_mask_bit(3, pressed);
            return freecam;
        }
        XK_SPACE => {
            set_mask_bit(4, pressed);
            return freecam;
        }
        _ if sym == XK_SHIFT_L || sym == XK_SHIFT_R => {
            set_mask_bit(7, pressed);
            return freecam;
        }
        _ => {}
    }

    if !matches!(lower, 0x63 | 0x76 | 0x78 | 0x62) && sym != XK_ALT_L && sym != XK_ALT_R {
        return false;
    }
    let mut zoom_change: Option<(bool, u32)> = None;
    {
        let mut st = STATE.lock();
        if st.config.zoom_enabled {
            let match_zoom = match st.config.zoom_key {
                ZoomKey::C => lower == 0x63,
                ZoomKey::V => lower == 0x76,
                ZoomKey::Alt => sym == XK_ALT_L || sym == XK_ALT_R,
            };
            if match_zoom && st.zoom_held != pressed {
                st.zoom_held = pressed;
                zoom_change = Some((pressed, st.config.zoom_fov));
            }
        }

        let match_xray = match st.config.xray_key {
            XrayKey::X => lower == 0x78,
            XrayKey::B => lower == 0x62,
            XrayKey::None => false,
        };
        if match_xray {
            if pressed && !st.xray_key_down {
                st.xray_key_down = true;
                st.config.xray_ores = !st.config.xray_ores;
                let _ = st.config.save();
            } else if !pressed {
                st.xray_key_down = false;
            }
        }
    }
    if let Some((active, fov)) = zoom_change {
        send_cmd(ControlCmd::SetZoom(active, fov));
    }
    false
}

fn handle_mouse_button_passive(detail: c_uint, pressed: bool) {
    if detail != 1 && detail != 3 {
        return;
    }
    set_mask_bit(if detail == 1 { 5 } else { 6 }, pressed);
    if pressed {
        let mut st = STATE.lock();
        let now = Instant::now();
        if detail == 1 {
            if !st.lmb_down {
                st.lmb_times.push_back(now);
            }
            st.lmb_down = true;
        } else {
            if !st.rmb_down {
                st.rmb_times.push_back(now);
            }
            st.rmb_down = true;
        }
    } else {
        let mut st = STATE.lock();
        if detail == 1 {
            st.lmb_down = false;
        } else {
            st.rmb_down = false;
        }
    }
}

fn handle_mouse_ui(detail: c_int, pressed: bool, x: Option<f32>, y: Option<f32>) {
    let mut st = STATE.lock();
    if let (Some(px), Some(py)) = (x, y) {
        st.cursor_pos = [px, py];
    }
    match detail {
        1 => {
            if pressed {
                if !st.mouse_down {
                    st.mouse_just_pressed = true;
                }
                st.mouse_down = true;
            } else {
                if st.mouse_down {
                    st.mouse_just_released = true;
                }
                st.mouse_down = false;
            }
        }
        4 if pressed => st.scroll_delta += 1.0,
        5 if pressed => st.scroll_delta -= 1.0,
        _ => {}
    }
}

pub unsafe extern "C" fn hook_XCheckIfEvent(
    display: *mut Display,
    event_return: *mut XEvent,
    predicate: Option<unsafe extern "C" fn(*mut Display, *mut XEvent, *mut c_char) -> Bool>,
    arg: *mut c_char,
) -> Bool {
    let fns = resolve_x11();
    let Some(real) = fns.x_check_if_event else {
        return 0;
    };

    if !display.is_null() {
        LAST_DISPLAY.store(display as usize, Ordering::Relaxed);
    }

    loop {
        let got = unsafe { real(display, event_return, predicate, arg) };
        if got == 0 || event_return.is_null() {
            return 0;
        }

        let ev = unsafe { &mut *event_return };
        let ev_type = unsafe { ev.r#type };
        let capturing = ui_capturing();

        // Fast pass-through for all non-menu motion/generic events when playing!
        if !capturing && ev_type != KEY_PRESS && ev_type != KEY_RELEASE && ev_type != BUTTON_PRESS && ev_type != BUTTON_RELEASE {
            return 1;
        }

        if ev_type == KEY_PRESS || ev_type == KEY_RELEASE {
            let win = unsafe { ev.xkey.window };
            if win != 0 {
                LAST_WINDOW.store(win as usize, Ordering::Relaxed);
            }
            let pressed = ev_type == KEY_PRESS;
            let keycode = unsafe { ev.xkey.keycode };

            // Fast check for ` / ~ keycode (49) or keysym before full XLookupString
            let quick_sym = fns
                .x_keycode_to_keysym
                .map_or(0, |f| unsafe { f(display, keycode, 0) });
            if keycode == 49 || quick_sym == XK_GRAVE || quick_sym == XK_ASCIITILDE {
                if pressed {
                    if !GRAVE_DOWN.swap(true, Ordering::Relaxed) {
                        toggle_menu(display, win);
                    }
                } else {
                    GRAVE_DOWN.store(false, Ordering::Relaxed);
                }
                continue;
            }

            if capturing {
                let mut sym: KeySym = 0;
                let mut buf = [0i8; 32];
                let mut text = String::new();
                if let Some(lookup) = fns.x_lookup_string {
                    let len = unsafe {
                        lookup(
                            &mut ev.xkey,
                            buf.as_mut_ptr(),
                            buf.len() as c_int - 1,
                            &mut sym,
                            ptr::null_mut(),
                        )
                    };
                    if len > 0 {
                        let bytes = unsafe {
                            std::slice::from_raw_parts(buf.as_ptr() as *const u8, len as usize)
                        };
                        if let Ok(s) = std::str::from_utf8(bytes) {
                            text.push_str(s);
                        }
                    }
                }
                handle_key_in_ui(sym, pressed, &text);
                continue;
            } else {
                if handle_key_passive_fast(display, keycode, pressed) {
                    continue;
                }
                return 1;
            }
        }

        if ev_type == BUTTON_PRESS || ev_type == BUTTON_RELEASE {
            let pressed = ev_type == BUTTON_PRESS;
            let btn = unsafe { ev.xbutton.button };
            if capturing {
                let px = unsafe { ev.xbutton.x as f32 };
                let py = unsafe { ev.xbutton.y as f32 };
                handle_mouse_ui(btn as c_int, pressed, Some(px), Some(py));
                continue;
            } else {
                handle_mouse_button_passive(btn, pressed);
                return 1;
            }
        }

        if ev_type == MOTION_NOTIFY {
            let px = unsafe { ev.xmotion.x as f32 };
            let py = unsafe { ev.xmotion.y as f32 };
            let mut st = STATE.lock();
            st.cursor_pos = [px, py];
            continue;
        }

        if ev_type == GENERIC_EVENT {
            let xev_type = unsafe { ev.xcookie.evtype };
            if matches!(xev_type, XI_MOTION | XI_BUTTON_PRESS | XI_BUTTON_RELEASE) {
                if let (Some(get_data), Some(free_data)) =
                    (fns.x_get_event_data, fns.x_free_event_data)
                    && unsafe { get_data(display, &mut ev.xcookie) } != 0
                    && !unsafe { ev.xcookie.data }.is_null()
                {
                    let dev = unsafe { &*(ev.xcookie.data as *const XIDeviceEvent) };
                    if dev.event != 0 {
                        LAST_WINDOW.store(dev.event as usize, Ordering::Relaxed);
                    }
                    match xev_type {
                        XI_MOTION => {
                            let mut st = STATE.lock();
                            st.cursor_pos = [dev.event_x as f32, dev.event_y as f32];
                        }
                        XI_BUTTON_PRESS | XI_BUTTON_RELEASE => {
                            handle_mouse_ui(
                                dev.detail,
                                xev_type == XI_BUTTON_PRESS,
                                Some(dev.event_x as f32),
                                Some(dev.event_y as f32),
                            );
                        }
                        _ => {}
                    }
                    unsafe { free_data(display, &mut ev.xcookie) };
                }
                continue;
            }
            if matches!(
                xev_type,
                XI_RAW_MOTION
                    | XI_RAW_BUTTON_PRESS
                    | XI_RAW_BUTTON_RELEASE
                    | XI_RAW_KEY_PRESS
                    | XI_RAW_KEY_RELEASE
            ) {
                continue;
            }
            return 1;
        }

        return 1;
    }
}

pub unsafe extern "C" fn hook_XGetEventData(
    display: *mut Display,
    cookie: *mut XGenericEventCookie,
) -> Bool {
    let Some(real) = resolve_x11().x_get_event_data else {
        return 0;
    };
    let ok = unsafe { real(display, cookie) };
    if ok != 0 && !cookie.is_null() && !ui_capturing() {
        let c = unsafe { &*cookie };
        if !c.data.is_null() && (c.evtype == XI_BUTTON_PRESS || c.evtype == XI_BUTTON_RELEASE) {
            let dev = unsafe { &*(c.data as *const XIDeviceEvent) };
            if dev.event != 0 {
                LAST_WINDOW.store(dev.event as usize, Ordering::Relaxed);
            }
            handle_mouse_button_passive(dev.detail as c_uint, c.evtype == XI_BUTTON_PRESS);
        }
    }
    ok
}

pub unsafe extern "C" fn hook_XDefineCursor(
    display: *mut Display,
    window: Window,
    cursor: Cursor,
) -> c_int {
    let fns = resolve_x11();
    if !display.is_null() {
        LAST_DISPLAY.store(display as usize, Ordering::Relaxed);
    }
    if window != 0 {
        LAST_WINDOW.store(window as usize, Ordering::Relaxed);
    }
    if cursor != ARROW_CURSOR.load(Ordering::Relaxed) as Cursor {
        SAVED_GAME_CURSOR.store(cursor as usize, Ordering::Relaxed);
    }
    if ui_capturing() {
        return 1;
    }
    if let Some(real) = fns.x_define_cursor {
        unsafe { real(display, window, cursor) }
    } else {
        1
    }
}
pub unsafe extern "C" fn hook_xcb_change_window_attributes(
    conn: *mut XcbConnection,
    window: u32,
    value_mask: u32,
    value_list: *const c_void,
) -> u32 {
    if !conn.is_null() {
        LAST_XCB_CONN.store(conn as usize, Ordering::Relaxed);
    }
    if window != 0 {
        LAST_WINDOW.store(window as usize, Ordering::Relaxed);
    }
    const CW_CURSOR: u32 = 1 << 14;
    if (value_mask & CW_CURSOR) != 0 && !value_list.is_null() {
        let idx = (value_mask & (CW_CURSOR - 1)).count_ones() as usize;
        let cur = unsafe { *(value_list as *const u32).add(idx) };
        if cur != ARROW_CURSOR.load(Ordering::Relaxed) as u32 {
            SAVED_GAME_CURSOR.store(cur as usize, Ordering::Relaxed);
        }
        if ui_capturing() {
            // Keep arrow cursor visible while UI is open
            return 0;
        }
    }
    if let Some(real) = resolve_x11().xcb_change_window_attributes {
        unsafe { real(conn, window, value_mask, value_list as *const u32) }
    } else {
        0
    }
}

pub unsafe extern "C" fn hook_xcb_change_window_attributes_checked(
    conn: *mut XcbConnection,
    window: u32,
    value_mask: u32,
    value_list: *const c_void,
) -> u32 {
    if !conn.is_null() {
        LAST_XCB_CONN.store(conn as usize, Ordering::Relaxed);
    }
    if window != 0 {
        LAST_WINDOW.store(window as usize, Ordering::Relaxed);
    }
    const CW_CURSOR: u32 = 1 << 14;
    if (value_mask & CW_CURSOR) != 0 && !value_list.is_null() {
        let idx = (value_mask & (CW_CURSOR - 1)).count_ones() as usize;
        let cur = unsafe { *(value_list as *const u32).add(idx) };
        if cur != ARROW_CURSOR.load(Ordering::Relaxed) as u32 {
            SAVED_GAME_CURSOR.store(cur as usize, Ordering::Relaxed);
        }
        if ui_capturing() {
            return 0;
        }
    }
    if let Some(real) = resolve_x11().xcb_change_window_attributes_checked {
        unsafe { real(conn, window, value_mask, value_list as *const u32) }
    } else {
        0
    }
}


pub unsafe extern "C" fn hook_XGrabPointer(
    display: *mut Display,
    grab_window: Window,
    owner_events: Bool,
    event_mask: c_uint,
    pointer_mode: c_int,
    keyboard_mode: c_int,
    confine_to: Window,
    cursor: Cursor,
    time: Time,
) -> c_int {
    GAME_WANTS_GRAB.store(true, Ordering::Relaxed);
    if ui_capturing() {
        return 0;
    }
    if let Some(real) = resolve_x11().x_grab_pointer {
        unsafe {
            real(
                display,
                grab_window,
                owner_events,
                event_mask,
                pointer_mode,
                keyboard_mode,
                confine_to,
                cursor,
                time,
            )
        }
    } else {
        0
    }
}

pub unsafe extern "C" fn hook_XUngrabPointer(display: *mut Display, time: Time) -> c_int {
    GAME_WANTS_GRAB.store(false, Ordering::Relaxed);
    if let Some(real) = resolve_x11().x_ungrab_pointer {
        unsafe { real(display, time) }
    } else {
        1
    }
}

pub unsafe extern "C" fn hook_xcb_grab_pointer(
    conn: *mut XcbConnection,
    owner_events: u8,
    grab_window: u32,
    event_mask: u16,
    pointer_mode: u8,
    keyboard_mode: u8,
    confine_to: u32,
    cursor: u32,
    time: u32,
) -> u32 {
    if !conn.is_null() {
        LAST_XCB_CONN.store(conn as usize, Ordering::Relaxed);
    }
    if grab_window != 0 {
        LAST_WINDOW.store(grab_window as usize, Ordering::Relaxed);
    }
    GAME_WANTS_GRAB.store(true, Ordering::Relaxed);
    if ui_capturing() {
        return 0;
    }
    if let Some(real) = resolve_x11().xcb_grab_pointer {
        unsafe {
            real(
                conn,
                owner_events,
                grab_window,
                event_mask,
                pointer_mode,
                keyboard_mode,
                confine_to,
                cursor,
                time,
            )
        }
    } else {
        0
    }
}

pub unsafe extern "C" fn hook_xcb_ungrab_pointer(conn: *mut XcbConnection, time: u32) -> u32 {
    GAME_WANTS_GRAB.store(false, Ordering::Relaxed);
    if let Some(real) = resolve_x11().xcb_ungrab_pointer {
        unsafe { real(conn, time) }
    } else {
        0
    }
}

pub fn sync_cursor_from_ui_transition() {
    if !ui_capturing() {
        let dpy = LAST_DISPLAY.load(Ordering::Relaxed) as *mut Display;
        let win = LAST_WINDOW.load(Ordering::Relaxed) as Window;
        if !dpy.is_null() && win != 0 {
            restore_cursor_for_game(dpy, win);
        }
    }
}
