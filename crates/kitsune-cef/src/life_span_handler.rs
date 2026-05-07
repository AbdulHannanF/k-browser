use std::sync::{Arc, Mutex};
use std::ffi::c_void;
use crate::{_cef_browser_t, _cef_life_span_handler_t, _cef_base_ref_counted_t};

#[repr(C)]
pub struct KitsuneLifeSpanHandlerBase {
    pub size: usize,
    pub add_ref: Option<unsafe extern "C" fn(self_: *mut _cef_base_ref_counted_t)>,
    pub release: Option<unsafe extern "C" fn(self_: *mut _cef_base_ref_counted_t) -> std::os::raw::c_int>,
    pub has_one_ref: Option<unsafe extern "C" fn(self_: *mut _cef_base_ref_counted_t) -> std::os::raw::c_int>,
    pub has_at_least_one_ref: Option<unsafe extern "C" fn(self_: *mut _cef_base_ref_counted_t) -> std::os::raw::c_int>,
    pub on_before_popup: *const c_void,
    pub on_before_popup_aborted: *const c_void,
    pub on_before_dev_tools_popup: *const c_void,
    pub on_after_created: Option<extern "C" fn(self_: *mut _cef_life_span_handler_t, browser: *mut _cef_browser_t)>,
    pub do_close: *const c_void,
    pub on_before_close: *const c_void,
}

#[repr(C)]
pub struct KitsuneLifeSpanHandler {
    pub base: KitsuneLifeSpanHandlerBase,
    pub browser_ptr: Arc<Mutex<*mut _cef_browser_t>>,
    ref_count: std::os::raw::c_int,
}

impl KitsuneLifeSpanHandler {
    pub fn new() -> Box<Self> {
        let mut handler = Box::new(Self {
            base: unsafe { std::mem::zeroed() },
            browser_ptr: Arc::new(Mutex::new(std::ptr::null_mut())),
            ref_count: 2,
        });

        handler.base.size = std::mem::size_of::<KitsuneLifeSpanHandlerBase>();
        handler.base.add_ref = Some(add_ref);
        handler.base.release = Some(release);
        handler.base.has_one_ref = Some(has_one_ref);
        handler.base.has_at_least_one_ref = Some(has_at_least_one_ref);
        handler.base.on_after_created = Some(on_after_created);

        handler
    }
}

pub extern "C" fn add_ref(self_: *mut _cef_base_ref_counted_t) {
    let handler = unsafe { &mut *(self_ as *mut KitsuneLifeSpanHandler) };
    handler.ref_count += 1;
}

pub extern "C" fn release(self_: *mut _cef_base_ref_counted_t) -> std::os::raw::c_int {
    let handler = unsafe { &mut *(self_ as *mut KitsuneLifeSpanHandler) };
    handler.ref_count -= 1;
    if handler.ref_count == 0 {
        return 1;
    }
    0
}

pub extern "C" fn has_one_ref(self_: *mut _cef_base_ref_counted_t) -> std::os::raw::c_int {
    let handler = unsafe { &mut *(self_ as *mut KitsuneLifeSpanHandler) };
    if handler.ref_count == 1 { 1 } else { 0 }
}

pub extern "C" fn has_at_least_one_ref(self_: *mut _cef_base_ref_counted_t) -> std::os::raw::c_int {
    let handler = unsafe { &mut *(self_ as *mut KitsuneLifeSpanHandler) };
    if handler.ref_count >= 1 { 1 } else { 0 }
}

extern "C" fn on_after_created(
    self_: *mut _cef_life_span_handler_t,
    browser: *mut _cef_browser_t,
) {
    let handler = unsafe { &mut *(self_ as *mut KitsuneLifeSpanHandler) };
    let mut browser_ptr = handler.browser_ptr.lock().unwrap();
    *browser_ptr = browser;
}
