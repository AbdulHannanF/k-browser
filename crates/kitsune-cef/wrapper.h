#undef OS_WIN
#define OS_WIN 1
#include "include/capi/cef_app_capi.h"
#include "include/capi/cef_client_capi.h"
#include "include/capi/cef_v8_capi.h"
#include "include/capi/cef_browser_capi.h"
#include "include/capi/cef_life_span_handler_capi.h"

// Let's force it
#undef CEF_CALLBACK
#define CEF_CALLBACK
