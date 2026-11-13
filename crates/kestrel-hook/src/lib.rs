#![allow(non_snake_case)]

mod config;
mod control_bridge;
mod draw;
mod font;
mod hooks_vk;
mod hooks_x11;
mod hud;
mod overlay_vk;
mod state;
mod ui;

use std::ffi::{c_char, c_void, CStr};
use std::sync::OnceLock;

type FnDlsym = unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void;

static REAL_DLSYM: OnceLock<FnDlsym> = OnceLock::new();

unsafe extern "C" {
    fn dlvsym(handle: *mut c_void, symbol: *const c_char, version: *const c_char) -> *mut c_void;
}

pub unsafe fn real_dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void {
    let f = *REAL_DLSYM.get_or_init(|| unsafe {
        let mut p = dlvsym(libc::RTLD_NEXT, c"dlsym".as_ptr(), c"GLIBC_2.34".as_ptr());
        if p.is_null() {
            p = dlvsym(libc::RTLD_NEXT, c"dlsym".as_ptr(), c"GLIBC_2.2.5".as_ptr());
        }
        std::mem::transmute::<*mut c_void, FnDlsym>(p)
    });
    unsafe { f(handle, symbol) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void {
    if symbol.is_null() {
        return std::ptr::null_mut();
    }
    let sym = unsafe { CStr::from_ptr(symbol) }.to_bytes();
    let real = unsafe { real_dlsym(handle, symbol) };

    match sym {
        b"dlsym" => return dlsym as *mut c_void,
        b"vkGetInstanceProcAddr" => {
            if !real.is_null() {
                hooks_vk::set_real_gipa(real);
                return hooks_vk::hook_vkGetInstanceProcAddr as *mut c_void;
            }
        }
        b"vkGetDeviceProcAddr" => {
            if !real.is_null() {
                hooks_vk::set_real_gdpa(real);
                return hooks_vk::hook_vkGetDeviceProcAddr as *mut c_void;
            }
        }
        b"XCheckIfEvent" => {
            if !real.is_null() {
                return hooks_x11::hook_XCheckIfEvent as *mut c_void;
            }
        }
        b"XGetEventData" => {
            if !real.is_null() {
                return hooks_x11::hook_XGetEventData as *mut c_void;
            }
        }
        b"XDefineCursor" => {
            if !real.is_null() {
                return hooks_x11::hook_XDefineCursor as *mut c_void;
            }
        }
        b"XGrabPointer" => {
            if !real.is_null() {
                return hooks_x11::hook_XGrabPointer as *mut c_void;
            }
        }
        b"XUngrabPointer" => {
            if !real.is_null() {
                return hooks_x11::hook_XUngrabPointer as *mut c_void;
            }
        }
        b"xcb_grab_pointer" => {
            if !real.is_null() {
                return hooks_x11::hook_xcb_grab_pointer as *mut c_void;
            }
        }
        b"xcb_change_window_attributes" => {
            if !real.is_null() {
                return hooks_x11::hook_xcb_change_window_attributes as *mut c_void;
            }
        }
        b"xcb_change_window_attributes_checked" => {
            if !real.is_null() {
                return hooks_x11::hook_xcb_change_window_attributes_checked as *mut c_void;
            }
        }
        b"xcb_ungrab_pointer" => {
            if !real.is_null() {
                return hooks_x11::hook_xcb_ungrab_pointer as *mut c_void;
            }
        }
        _ => {}
    }

    real
}

#[used]
#[unsafe(link_section = ".init_array")]
static KESTREL_INIT: extern "C" fn() = kestrel_ctor;

extern "C" fn kestrel_ctor() {
    // Only activate inside horizon-client (do not interfere if LD_PRELOAD leaks to child sh/zenity)
    let is_horizon = std::fs::read_link("/proc/self/exe")
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .is_some_and(|name| name.contains("horizon-client"));
    if is_horizon {
        let _ = &*state::STATE;
        control_bridge::start_bridge();
    }
}
