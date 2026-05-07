use crate::{_cef_browser_t, _cef_client_t, _cef_base_ref_counted_t, _cef_life_span_handler_t};
use crate::life_span_handler::KitsuneLifeSpanHandler;
use std::ffi::c_void;
use std::sync::{Arc, Mutex};

#[repr(C)]
pub struct KitsuneCefClientBase {
    pub size: usize,
    pub add_ref: Option<unsafe extern "C" fn(self_: *mut _cef_base_ref_counted_t)>,
    pub release: Option<unsafe extern "C" fn(self_: *mut _cef_base_ref_counted_t) -> std::os::raw::c_int>,
    pub has_one_ref: Option<unsafe extern "C" fn(self_: *mut _cef_base_ref_counted_t) -> std::os::raw::c_int>,
    pub has_at_least_one_ref: Option<unsafe extern "C" fn(self_: *mut _cef_base_ref_counted_t) -> std::os::raw::c_int>,
    pub get_audio_handler: *const c_void,
    pub get_command_handler: *const c_void,
    pub get_context_menu_handler: *const c_void,
    pub get_dialog_handler: *const c_void,
    pub get_display_handler: *const c_void,
    pub get_download_handler: *const c_void,
    pub get_drag_handler: *const c_void,
    pub get_find_handler: *const c_void,
    pub get_focus_handler: *const c_void,
    pub get_frame_handler: *const c_void,
    pub get_permission_handler: *const c_void,
    pub get_jsdialog_handler: *const c_void,
    pub get_keyboard_handler: *const c_void,
    pub get_life_span_handler: Option<extern "C" fn(self_: *mut _cef_client_t) -> *mut _cef_life_span_handler_t>,
    pub get_load_handler: *const c_void,
    pub get_print_handler: *const c_void,
    pub get_render_handler: *const c_void,
    pub get_request_handler: *const c_void,
    pub on_process_message_received: *const c_void,
}

#[repr(C)]
pub struct KitsuneCefClient {
    pub base: KitsuneCefClientBase,
    life_span_handler: Box<KitsuneLifeSpanHandler>,
    ref_count: std::os::raw::c_int,
}

impl KitsuneCefClient {
    pub fn new() -> Box<Self> {
        let mut client = Box::new(Self {
            base: unsafe { std::mem::zeroed() },
            life_span_handler: KitsuneLifeSpanHandler::new(),
            ref_count: 2,
        });

        client.base.size = std::mem::size_of::<KitsuneCefClientBase>();
        client.base.add_ref = Some(add_ref);
        client.base.release = Some(release);
        client.base.has_one_ref = Some(has_one_ref);
        client.base.has_at_least_one_ref = Some(has_at_least_one_ref);
        client.base.get_life_span_handler = Some(get_life_span_handler);

        client
    }

    pub fn get_browser_ptr(&self) -> *mut _cef_browser_t {
        *self.life_span_handler.browser_ptr.lock().unwrap()
    }

    pub fn browser_ptr_handle(&self) -> Arc<Mutex<*mut _cef_browser_t>> {
        Arc::clone(&self.life_span_handler.browser_ptr)
    }
}

pub extern "C" fn add_ref(self_: *mut _cef_base_ref_counted_t) {
    let client = unsafe { &mut *(self_ as *mut KitsuneCefClient) };
    client.ref_count += 1;
}

pub extern "C" fn release(self_: *mut _cef_base_ref_counted_t) -> std::os::raw::c_int {
    let client = unsafe { &mut *(self_ as *mut KitsuneCefClient) };
    client.ref_count -= 1;
    if client.ref_count == 0 {
        return 1;
    }
    0
}

pub extern "C" fn has_one_ref(self_: *mut _cef_base_ref_counted_t) -> std::os::raw::c_int {
    let client = unsafe { &mut *(self_ as *mut KitsuneCefClient) };
    if client.ref_count == 1 { 1 } else { 0 }
}

pub extern "C" fn has_at_least_one_ref(self_: *mut _cef_base_ref_counted_t) -> std::os::raw::c_int {
    let client = unsafe { &mut *(self_ as *mut KitsuneCefClient) };
    if client.ref_count >= 1 { 1 } else { 0 }
}

extern "C" fn get_life_span_handler(self_: *mut _cef_client_t) -> *mut _cef_life_span_handler_t {
    let client = unsafe { &mut *(self_ as *mut KitsuneCefClient) };
    &mut client.life_span_handler.base as *mut _ as *mut _cef_life_span_handler_t
}
