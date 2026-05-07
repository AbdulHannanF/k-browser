use std::ffi::c_void;
use crate::{_cef_browser_t, _cef_frame_t, _cef_browser_host_t, _cef_string_utf16_t};

// _cef_browser_t
pub unsafe fn browser_get_host(browser: *mut _cef_browser_t) -> *mut _cef_browser_host_t {
    let ptr = browser as *const *const c_void;
    let func: extern "system" fn(*mut _cef_browser_t) -> *mut _cef_browser_host_t = std::mem::transmute(*ptr.add(6));
    func(browser)
}

pub unsafe fn browser_go_back(browser: *mut _cef_browser_t) {
    let ptr = browser as *const *const c_void;
    let func: extern "system" fn(*mut _cef_browser_t) = std::mem::transmute(*ptr.add(8));
    func(browser)
}

pub unsafe fn browser_go_forward(browser: *mut _cef_browser_t) {
    let ptr = browser as *const *const c_void;
    let func: extern "system" fn(*mut _cef_browser_t) = std::mem::transmute(*ptr.add(10));
    func(browser)
}

pub unsafe fn browser_reload(browser: *mut _cef_browser_t) {
    let ptr = browser as *const *const c_void;
    let func: extern "system" fn(*mut _cef_browser_t) = std::mem::transmute(*ptr.add(12));
    func(browser)
}

pub unsafe fn browser_stop_load(browser: *mut _cef_browser_t) {
    let ptr = browser as *const *const c_void;
    let func: extern "system" fn(*mut _cef_browser_t) = std::mem::transmute(*ptr.add(14));
    func(browser)
}

pub unsafe fn browser_get_main_frame(browser: *mut _cef_browser_t) -> *mut _cef_frame_t {
    let ptr = browser as *const *const c_void;
    let func: extern "system" fn(*mut _cef_browser_t) -> *mut _cef_frame_t = std::mem::transmute(*ptr.add(19));
    func(browser)
}

// _cef_frame_t
pub unsafe fn frame_load_url(frame: *mut _cef_frame_t, url: *const _cef_string_utf16_t) {
    let ptr = frame as *const *const c_void;
    // is_valid(5), undo(6), redo(7), cut(8), copy(9), paste(10), paste_and_match_style(11), del(12), select_all(13)
    // view_source(14), get_source(15), get_text(16), load_request(17), load_url(18)
    let func: extern "system" fn(*mut _cef_frame_t, *const _cef_string_utf16_t) = std::mem::transmute(*ptr.add(18));
    func(frame, url)
}

pub unsafe fn frame_execute_java_script(frame: *mut _cef_frame_t, code: *const _cef_string_utf16_t, script_url: *const _cef_string_utf16_t, start_line: std::os::raw::c_int) {
    let ptr = frame as *const *const c_void;
    // execute_java_script is after load_url (18) -> 19
    let func: extern "system" fn(*mut _cef_frame_t, *const _cef_string_utf16_t, *const _cef_string_utf16_t, std::os::raw::c_int) = std::mem::transmute(*ptr.add(19));
    func(frame, code, script_url, start_line)
}

// _cef_browser_host_t
pub unsafe fn browser_host_get_window_handle(host: *mut _cef_browser_host_t) -> *mut c_void {
    let ptr = host as *const *const c_void;
    // get_browser(5), close_browser(6), try_close_browser(7), is_ready_to_be_closed(8), set_focus(9), get_window_handle(10)
    let func: extern "system" fn(*mut _cef_browser_host_t) -> *mut c_void = std::mem::transmute(*ptr.add(10));
    func(host)
}
