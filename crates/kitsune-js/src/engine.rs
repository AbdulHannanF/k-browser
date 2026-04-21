use crate::{JsError, JsResult};
use kitsune_html::dom::DomTree;
use std::sync::Once;
use tracing::{debug, error, warn};

/// JavaScript engine — wraps V8.
///
/// # Isolate lifetime rules
///
/// V8 requires that `OwnedIsolate` instances are **dropped in the reverse order
/// they were created**. The old code recreated an isolate inside `execute()` on
/// every call, which violates this rule when the isolate is moved into the
/// `JsEngine` struct and later replaced (e.g., after a timeout reset).
///
/// The fix: create **one isolate per `JsEngine`**, and create a **fresh
/// `Context`** inside each `execute()` call. This is the correct V8 usage —
/// contexts are lightweight and disposable; isolates are heavy-weight
/// singletons per-engine.
use std::ffi::c_void;
use std::ptr::null_mut;

pub struct JsEngine {
    isolate: v8::OwnedIsolate,
    host_data: *mut c_void, 
}


// Safety: JsEngine is only held behind a `tokio::sync::Mutex` in the pipeline
// and is accessed from a single task at a time.
unsafe impl Send for JsEngine {}

impl JsEngine {
    pub fn new() -> Self {
        // V8 platform must be initialized exactly once globally.
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            let platform = v8::new_default_platform(0, false).make_shared();
            v8::V8::initialize_platform(platform);
            v8::V8::initialize();
        });

        // One isolate per engine instance. Do NOT store the isolate in a
        // Vec or recreate it mid-execution — that violates V8's invariant.
        let isolate = v8::Isolate::new(v8::CreateParams::default());
        Self { isolate, host_data: null_mut() }
    }

    /// Execute a script string in an isolated context.
    ///
    /// A new `Context` is created for every call so scripts cannot share
    /// state between invocations. The `OwnedIsolate` is reused (required by V8).
    pub fn execute(&mut self, script: &str, mut dom: DomTree) -> (DomTree, JsResult<()>) {
        self.host_data = &mut dom as *mut _ as *mut c_void;
        self.isolate.set_data(0, self.host_data);

        // Create a new HandleScope over the existing isolate.
        let handle_scope = &mut v8::HandleScope::new(&mut self.isolate);

        // Fresh context — each script gets an isolated global.
        let context = v8::Context::new(handle_scope);
        let scope = &mut v8::ContextScope::new(handle_scope, context);

        let global = context.global(scope);

        // Register `__kitsune_native` callback.
        let func_template = v8::FunctionTemplate::new(scope, Self::kitsune_native_callback);
        let func = match func_template.get_function(scope) {
            Some(f) => f,
            None => return (dom, Err(JsError::ExecutionError("Failed to create function from template".into()))),
        };
        let name_str = match v8::String::new(scope, "__kitsune_native") {
            Some(s) => s,
            None => return (dom, Err(JsError::ExecutionError("OOM".into()))),
        };
        global.set(scope, name_str.into(), func.into());

        // Inject security shims and DOM polyfills.
        if let Err(e) = Self::inject_bindings(scope) {
            return (dom, Err(e));
        }

        // Compile and run the user script.
        let source = match v8::String::new(scope, script) {
            Some(s) => s,
            None => return (dom, Err(JsError::ExecutionError("Invalid script UTF-8 encoding".into()))),
        };

        let mut try_catch = v8::TryCatch::new(scope);
        let compiled_script = match v8::Script::compile(&mut try_catch, source, None) {
            Some(s) => s,
            None => {
                let msg = Self::extract_error(&mut try_catch);
                warn!("[JS] Syntax error: {}", msg);
                return (dom, Err(JsError::ExecutionError(msg)));
            }
        };

        if compiled_script.run(&mut try_catch).is_none() {
            let msg = Self::extract_error(&mut try_catch);
            warn!("[JS] Runtime error: {}", msg);
            return (dom, Err(JsError::ExecutionError(msg)));
        }

        (dom, Ok(()))
    }

    fn extract_error(try_catch: &mut v8::TryCatch<v8::HandleScope>) -> String {
        try_catch
            .exception()
            .map(|e| e.to_rust_string_lossy(try_catch))
            .unwrap_or_else(|| "Unknown JS error".to_string())
    }

    fn inject_bindings(scope: &mut v8::HandleScope<'_>) -> JsResult<()> {
        let shim = r#"
            // Console passthrough
            globalThis.console = {
                log:   (...args) => __kitsune_native('log',   args.join(' ')),
                warn:  (...args) => __kitsune_native('warn',  args.join(' ')),
                error: (...args) => __kitsune_native('error', args.join(' ')),
                info:  (...args) => __kitsune_native('log',   args.join(' ')),
                debug: (...args) => __kitsune_native('log',   args.join(' ')),
            };

            // Security hard-blocks (Invariant 6)
            globalThis.eval = () => { throw new Error("SecurityError: eval is disabled"); };
            globalThis.fetch = () => Promise.resolve({ ok: true, json: () => Promise.resolve({}), text: () => Promise.resolve('') });
            globalThis.XMLHttpRequest = class {
                constructor() { this.readyState = 4; this.status = 200; this.responseText = ''; this.onreadystatechange = null; this.onload = null; this.onerror = null; }
                open() {}
                send() { if (this.onload) this.onload(); else if (this.onreadystatechange) this.onreadystatechange(); }
                setRequestHeader() {}
            };
            globalThis.open = () => { throw new Error("SecurityError: popup windows blocked"); };
            globalThis.importScripts = () => { throw new Error("SecurityError: importScripts blocked"); };

            // Anti-fingerprinting
            Object.defineProperty(globalThis, 'navigator', {
                value: {
                    userAgent: 'KitsuneEngine/0.1',
                    language: 'en-US',
                    languages: ['en-US'],
                    onLine: true,
                    cookieEnabled: false,
                    doNotTrack: '1',
                    hardwareConcurrency: 4,
                    platform: 'Win64',
                },
                writable: false, configurable: false,
            });

            // Minimal DOM stub
            class DOMElement {
                constructor(tag) {
                    this.tagName = tag;
                    this.id = '';
                    this.className = '';
                    this.style = {};
                    this.childNodes = [];
                    this.children = [];
                    this.dataset = {};
                    this.classList = { add() {}, remove() {}, contains() { return false; }, toggle() {} };
                    this.nodeType = 1;
                    this.ownerDocument = _document;
                }
                appendChild(child) { this.children.push(child); if (this.childNodes) this.childNodes.push(child); return child; }
                removeChild(child) { return child; }
                insertBefore(n, r) { return n; }
                replaceChild(n, o) { return o; }
                addEventListener(type, fn) {}
                removeEventListener(type, fn) {}
                get textContent() { return ''; }
                set textContent(v) { __kitsune_native('setTextContent', JSON.stringify({ id: this.id, text: v })); }
                get innerHTML() { return ''; }
                set innerHTML(v) {
                    let clean = v.replace(/<script[\s\S]*?<\/script>/gi, '');
                    __kitsune_native('setInnerHTML', JSON.stringify({ id: this.id, html: clean }));
                }
                setAttribute(k, v) { __kitsune_native('setAttribute', JSON.stringify({ id: this.id, k, v })); }
                getAttribute(k) { return null; }
                hasAttribute(k) { return false; }
                removeAttribute(k) { }
                remove() { }
                getBoundingClientRect() { return { x:0, y:0, width:400, height:200, top:0, left:0, bottom:0, right:0 }; }
                matches(sel) { return false; }
                closest(sel) { return null; }
                querySelector(sel) { return null; }
                querySelectorAll(sel) { return []; }
                getElementsByTagName(tag) { return []; }
                getElementsByClassName(cls) { return []; }
            }

            globalThis.Node = DOMElement;
            globalThis.Element = DOMElement;
            globalThis.HTMLElement = DOMElement;
            globalThis.Event = class {};
            globalThis.CustomEvent = class extends globalThis.Event {};
            globalThis.MessageChannel = class { constructor() { this.port1 = {}; this.port2 = {}; } };
            globalThis.MessagePort = class {};

            const _document = {
                readyState: 'complete',
                cookie: '',
                title: '',
                body: new DOMElement('body'),
                head: new DOMElement('head'),
                documentElement: new DOMElement('html'),
                getElementById: (id) => {
                    let res = __kitsune_native('getElementById', id);
                    if (res === 'true') { let el = new DOMElement('div'); el.id = id; return el; }
                    return null;
                },
                getElementsByTagName: (tag) => [],
                getElementsByClassName: (cls) => [],
                querySelector: (sel) => null,
                querySelectorAll: (sel) => [],
                createElement: (tag) => new DOMElement(tag),
                createTextNode: (text) => { let el = new DOMElement('#text'); el._text = text; el.nodeType = 3; return el; },
                createDocumentFragment: () => { let el = new DOMElement('#fragment'); el.nodeType = 11; return el; },
                createComment: () => { let el = new DOMElement('#comment'); el.nodeType = 8; return el; },
                createEvent: () => new globalThis.Event(),
                createRange: () => ({ setStart(){}, setEnd(){}, commonAncestorContainer: new DOMElement('div'), collapse(){} }),
                addEventListener: (type, fn) => { if (type === 'DOMContentLoaded' || type === 'load') { try { fn(); } catch(e) {} } },
                removeEventListener: () => {},
                dispatchEvent: () => true,
            };
            Object.defineProperty(_document, 'cookie', {
                get: () => '',
                set: () => { throw new Error('SecurityError: cookie writes blocked'); }
            });
            globalThis.document = _document;

            globalThis.window = globalThis;
            globalThis.self = globalThis;
            globalThis.top = globalThis;
            globalThis.parent = globalThis;
            globalThis.location = { href: 'http://kitsune.local/', origin: 'http://kitsune.local', protocol: 'http:', host: 'kitsune.local', hostname: 'kitsune.local', port: '', pathname: '/', search: '', hash: '' };
            globalThis.navigator = { userAgent: 'Kitsune/1.0', platform: 'Win32', language: 'en-US', onLine: true, clipboard: { readText: () => Promise.resolve(''), writeText: () => Promise.resolve() } };
            globalThis.history = { pushState() {}, replaceState() {}, back() {}, forward() {} };
            globalThis.screen = { width: 1280, height: 800, colorDepth: 24 };
            globalThis.performance = { now: () => 0, timing: {} };

            globalThis.setTimeout  = (fn, ms) => { __kitsune_native('setTimeout', String(ms)); try { fn(); } catch(e) {} return 0; };
            globalThis.setInterval = (fn, ms) => { __kitsune_native('setInterval', String(ms)); return 0; };
            globalThis.clearTimeout  = () => {};
            globalThis.clearInterval = () => {};
            globalThis.requestAnimationFrame = (fn) => { try { fn(); } catch(e) {} return 0; };
            globalThis.cancelAnimationFrame  = () => {};

            globalThis.addEventListener    = (type, fn) => { if (type === 'load' || type === 'DOMContentLoaded') { try { fn(); } catch(e) {} } };
            globalThis.removeEventListener = () => {};
            globalThis.dispatchEvent       = () => true;

            const noopProxy = new Proxy(() => {}, {
                get: (target, prop) => {
                    if (prop === 'then') return undefined;
                    if (prop === Symbol.toPrimitive) return (hint) => hint === 'number' ? 0 : '';
                    if (prop === 'toString' || prop === 'valueOf') return () => '';
                    return typeof prop === 'string' && prop.match(/^[A-Z]/) ? noopProxy : noopProxy;
                },
                apply: () => noopProxy,
                construct: () => noopProxy
            });

            // Stub popular tracker globals so analytics scripts don't throw ReferenceError
            globalThis.ga         = noopProxy;
            globalThis.gtag       = noopProxy;
            globalThis.dataLayer  = { push: () => {} };
            globalThis.fbq        = noopProxy;
            globalThis._gaq       = { push: () => {} };
            globalThis.google     = noopProxy;
            globalThis.grecaptcha = { render: () => 0, execute: () => Promise.resolve('') };
            globalThis.__tcfapi   = noopProxy;
            globalThis.__uspapi   = noopProxy;
            globalThis.Intercom   = noopProxy;
            globalThis.Sentry     = { init() {}, captureException() {}, captureMessage() {} };
            globalThis._          = noopProxy; // Google Closure / underscore leaker
            globalThis.__         = noopProxy;

            globalThis.MutationObserver = class { constructor(cb){} observe(){} disconnect(){} takeRecords(){ return []; } };
            globalThis.ResizeObserver   = class { constructor(cb){} observe(){} unobserve(){} disconnect(){} };
            globalThis.IntersectionObserver = class { constructor(cb,cfg){} observe(){} unobserve(){} disconnect(){} };
            globalThis.PerformanceObserver  = class { constructor(cb){} observe(){} disconnect(){} };

            globalThis.localStorage  = { getItem: () => null, setItem: () => {}, removeItem: () => {}, clear: () => {}, length: 0, key: () => null };
            globalThis.sessionStorage = { getItem: () => null, setItem: () => {}, removeItem: () => {}, clear: () => {}, length: 0, key: () => null };

            // Image stub
            globalThis.Image = class { constructor() { this.src = ''; this.onload = null; this.onerror = null; } };

            // Crypto stub (many ad scripts call this)
            globalThis.crypto = { getRandomValues: (a) => a, randomUUID: () => '00000000-0000-0000-0000-000000000000', subtle: {} };

            // Node / CommonJS shim for scripts that leak globals
            globalThis.module    = { exports: {} };
            globalThis.exports   = globalThis.module.exports;
            globalThis.require   = () => ({});
            globalThis.process   = { env: {}, nextTick: (fn) => { try { fn(); } catch(e) {} } };
            globalThis.global    = globalThis;
        "#;

        let source = v8::String::new(scope, shim)
            .ok_or_else(|| JsError::ExecutionError("OOM allocating shim".into()))?;
        let mut try_catch = v8::TryCatch::new(scope);
        if let Some(script) = v8::Script::compile(&mut try_catch, source, None) {
            if script.run(&mut try_catch).is_none() {
                let msg = try_catch
                    .exception()
                    .map(|e| e.to_rust_string_lossy(&mut try_catch))
                    .unwrap_or_else(|| "shim error".into());
                warn!("[JS] Shim error (non-fatal): {}", msg);
                // Non-fatal: continue even if shim partially fails.
            }
        }
        Ok(())
    }

    fn kitsune_native_callback(
        scope: &mut v8::HandleScope,
        args: v8::FunctionCallbackArguments,
        mut rv: v8::ReturnValue,
    ) {
        let op  = args.get(0).to_rust_string_lossy(scope);
        let arg = args.get(1).to_rust_string_lossy(scope);

        let isolate: &mut v8::Isolate = scope.as_mut();
        let dom = unsafe { &mut *(isolate.get_data(0) as *mut DomTree) };

        match op.as_str() {
            "log"   => debug!("[JS] {}", arg),
            "warn"  => warn!("[JS] {}", arg),
            "error" => error!("[JS] {}", arg),
            "getElementById" => {
                if let Some(_) = dom.get_element_by_id(&arg) {
                    // For now, just confirming it exists is enough.
                    // We'll return a full DOMElement object later.
                    let ret = v8::String::new(scope, "true").unwrap();
                    rv.set(ret.into());
                } else {
                    let ret = v8::null(scope);
                    rv.set(ret.into());
                }
            }
            "setAttribute" => {
                #[derive(serde::Deserialize)]
                struct SetAttributeArgs { id: String, k: String, v: String }
                if let Ok(args) = serde_json::from_str::<SetAttributeArgs>(&arg) {
                    if let Some(node_id) = dom.get_element_by_id(&args.id) {
                        if let Some(node) = dom.get_node_mut(node_id) {
                            if let Some(element) = node.as_element_mut() {
                                element.attributes.insert(args.k, args.v);
                            }
                        }
                    }
                }
            }
            "setTextContent" => {
                #[derive(serde::Deserialize)]
                struct SetTextContentArgs { id: String, text: String }
                if let Ok(args) = serde_json::from_str::<SetTextContentArgs>(&arg) {
                    if let Some(node_id) = dom.get_element_by_id(&args.id) {
                        dom.set_text_content(node_id, &args.text);
                    }
                }
            }
            "setInnerHTML" => {
                #[derive(serde::Deserialize)]
                struct SetInnerHTMLArgs { id: String, html: String }
                if let Ok(args) = serde_json::from_str::<SetInnerHTMLArgs>(&arg) {
                    if let Some(node_id) = dom.get_element_by_id(&args.id) {
                        dom.set_inner_html(node_id, &args.html);
                    }
                }
            }
            "setTimeout" | "setInterval" => {
                let ret = v8::Integer::new(scope, 0);
                rv.set(ret.into());
            }
            _ => {
                // Return undefined for any unhandled native calls.
            }
        }
    }
}

impl Default for JsEngine {
    fn default() -> Self {
        Self::new()
    }
}
