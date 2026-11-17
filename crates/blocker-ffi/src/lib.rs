//! C ABI over [`blocker_core`], declared in `include/blocker.h`.
//!
//! The mobile apps link the static library this crate builds:
//! iOS/iPadOS through `BlockerFFI.xcframework` (`import BlockerFFI` in Swift),
//! Android through a C JNI shim compiled by the NDK.
//!
//! Rules every export follows:
//! - never unwind across the boundary (panics become errors);
//! - returned strings come from [`CString::into_raw`] and go back through
//!   [`blocker_string_free`];
//! - failures return NULL / false and set a thread-local error message.

#![allow(clippy::missing_safety_doc)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{c_char, CStr, CString};
use std::net::IpAddr;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;

use blocker_core::engine::Proto;
use blocker_core::schedule::Day;
use blocker_core::sni::{self, Peek};
use blocker_core::{Config, Engine, FlowInfo, Store, Verdict, WeekTime};

/// Opaque engine handle (`BlockerEngine` in C).
pub struct BlockerEngine {
    engine: Engine,
    warnings: Vec<String>,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockerVerdictKind {
    Allow = 0,
    Block = 1,
    Peek = 2,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockerProto {
    Tcp = 0,
    Udp = 1,
    Other = 2,
}

#[repr(C)]
#[derive(Debug)]
pub struct BlockerVerdict {
    pub kind: BlockerVerdictKind,
    pub rule: *mut c_char,
    pub matched: *mut c_char,
}

#[repr(C)]
pub struct BlockerFlow {
    pub hostname: *const c_char,
    pub remote_ip: *const c_char,
    pub remote_port: u16,
    pub proto: BlockerProto,
    pub app_id: *const c_char,
    pub ech: bool,
}

thread_local! {
    static LAST_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
}

fn set_error(msg: impl Into<String>) {
    LAST_ERROR.with(|e| *e.borrow_mut() = Some(msg.into()));
}

fn clear_error() {
    LAST_ERROR.with(|e| *e.borrow_mut() = None);
}

/// Runs `f`, turning a panic into `fallback` plus an error message.
fn guarded<T>(fallback: T, f: impl FnOnce() -> T) -> T {
    clear_error();
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(v) => v,
        Err(_) => {
            set_error("internal panic in blocker-core");
            fallback
        }
    }
}

fn to_c(s: impl Into<Vec<u8>>) -> *mut c_char {
    match CString::new(s) {
        Ok(c) => c.into_raw(),
        Err(_) => ptr::null_mut(),
    }
}

unsafe fn from_c<'a>(p: *const c_char) -> Option<&'a str> {
    if p.is_null() {
        return None;
    }
    CStr::from_ptr(p).to_str().ok()
}

fn week_time(day: i32, minute: u16) -> WeekTime {
    let day = match day {
        0 => Day::Mon,
        1 => Day::Tue,
        2 => Day::Wed,
        3 => Day::Thu,
        4 => Day::Fri,
        5 => Day::Sat,
        6 => Day::Sun,
        _ => return WeekTime::now(),
    };
    WeekTime {
        day,
        minute: minute.min(24 * 60 - 1),
    }
}

fn verdict_to_c(v: Verdict) -> BlockerVerdict {
    match v {
        Verdict::Allow => BlockerVerdict {
            kind: BlockerVerdictKind::Allow,
            rule: ptr::null_mut(),
            matched: ptr::null_mut(),
        },
        Verdict::Peek => BlockerVerdict {
            kind: BlockerVerdictKind::Peek,
            rule: ptr::null_mut(),
            matched: ptr::null_mut(),
        },
        Verdict::Block(reason) => BlockerVerdict {
            kind: BlockerVerdictKind::Block,
            rule: to_c(reason.rule),
            matched: to_c(reason.matched),
        },
    }
}

const ALLOW: BlockerVerdict = BlockerVerdict {
    kind: BlockerVerdictKind::Allow,
    rule: ptr::null_mut(),
    matched: ptr::null_mut(),
};

fn json(value: serde_json::Value) -> *mut c_char {
    to_c(value.to_string())
}

#[no_mangle]
pub extern "C" fn blocker_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr().cast()
}

#[no_mangle]
pub extern "C" fn blocker_last_error() -> *mut c_char {
    LAST_ERROR.with(|e| e.borrow().as_deref().map_or(ptr::null_mut(), to_c))
}

#[no_mangle]
pub unsafe extern "C" fn blocker_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

#[no_mangle]
pub unsafe extern "C" fn blocker_verdict_free(verdict: *mut BlockerVerdict) {
    if let Some(v) = verdict.as_mut() {
        blocker_string_free(v.rule);
        blocker_string_free(v.matched);
        v.rule = ptr::null_mut();
        v.matched = ptr::null_mut();
    }
}

fn parse_config(text: &str) -> Result<Config, String> {
    toml::from_str(text).map_err(|e| format!("parsing config: {e}"))
}

#[no_mangle]
pub unsafe extern "C" fn blocker_engine_from_toml(
    config_toml: *const c_char,
) -> *mut BlockerEngine {
    guarded(ptr::null_mut(), || {
        let Some(text) = from_c(config_toml) else {
            set_error("config_toml is NULL or not UTF-8");
            return ptr::null_mut();
        };
        let result = parse_config(text)
            .and_then(|config| Engine::new(&config, &HashMap::new()).map_err(|e| format!("{e:#}")));
        match result {
            Ok((engine, warnings)) => Box::into_raw(Box::new(BlockerEngine { engine, warnings })),
            Err(e) => {
                set_error(e);
                ptr::null_mut()
            }
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn blocker_engine_open_dir(dir: *const c_char) -> *mut BlockerEngine {
    guarded(ptr::null_mut(), || {
        let Some(dir) = from_c(dir) else {
            set_error("dir is NULL or not UTF-8");
            return ptr::null_mut();
        };
        match Store::new(dir).load_engine() {
            Ok((engine, warnings)) => Box::into_raw(Box::new(BlockerEngine { engine, warnings })),
            Err(e) => {
                set_error(format!("{e:#}"));
                ptr::null_mut()
            }
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn blocker_engine_free(engine: *mut BlockerEngine) {
    if !engine.is_null() {
        drop(Box::from_raw(engine));
    }
}

#[no_mangle]
pub unsafe extern "C" fn blocker_engine_warnings_json(engine: *const BlockerEngine) -> *mut c_char {
    guarded(ptr::null_mut(), || match engine.as_ref() {
        Some(e) => json(serde_json::json!(e.warnings)),
        None => {
            set_error("engine is NULL");
            ptr::null_mut()
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn blocker_engine_is_idle(
    engine: *const BlockerEngine,
    day: i32,
    minute: u16,
) -> bool {
    guarded(true, || {
        engine
            .as_ref()
            .is_none_or(|e| e.engine.is_idle(week_time(day, minute)))
    })
}

#[no_mangle]
pub unsafe extern "C" fn blocker_engine_active_rules_json(
    engine: *const BlockerEngine,
    day: i32,
    minute: u16,
) -> *mut c_char {
    guarded(ptr::null_mut(), || match engine.as_ref() {
        Some(e) => json(serde_json::json!(e
            .engine
            .active_rules(week_time(day, minute)))),
        None => {
            set_error("engine is NULL");
            ptr::null_mut()
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn blocker_engine_blocked_apps_json(
    engine: *const BlockerEngine,
    day: i32,
    minute: u16,
) -> *mut c_char {
    guarded(ptr::null_mut(), || match engine.as_ref() {
        Some(e) => {
            let apps = e.engine.blocked_apps(week_time(day, minute));
            json(serde_json::json!({
                "bundle_ids": apps.bundle_ids,
                "process_names": apps.process_names,
            }))
        }
        None => {
            set_error("engine is NULL");
            ptr::null_mut()
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn blocker_engine_check_host(
    engine: *const BlockerEngine,
    host: *const c_char,
    day: i32,
    minute: u16,
) -> BlockerVerdict {
    guarded(ALLOW, || {
        let (Some(e), Some(host)) = (engine.as_ref(), from_c(host)) else {
            set_error("engine or host is NULL");
            return ALLOW;
        };
        verdict_to_c(e.engine.check_host(host, week_time(day, minute)))
    })
}

#[no_mangle]
pub unsafe extern "C" fn blocker_engine_decide(
    engine: *const BlockerEngine,
    flow: *const BlockerFlow,
    peeked: bool,
    peeked_host: *const c_char,
    day: i32,
    minute: u16,
) -> BlockerVerdict {
    guarded(ALLOW, || {
        let (Some(e), Some(f)) = (engine.as_ref(), flow.as_ref()) else {
            set_error("engine or flow is NULL");
            return ALLOW;
        };
        let info = FlowInfo {
            hostname: from_c(f.hostname),
            remote_ip: from_c(f.remote_ip).and_then(|s| s.parse::<IpAddr>().ok()),
            remote_port: f.remote_port,
            proto: match f.proto {
                BlockerProto::Tcp => Proto::Tcp,
                BlockerProto::Udp => Proto::Udp,
                BlockerProto::Other => Proto::Other,
            },
            app_id: from_c(f.app_id),
            ech: f.ech,
        };
        let peeked = peeked.then(|| from_c(peeked_host));
        verdict_to_c(e.engine.decide(&info, peeked, week_time(day, minute)))
    })
}

#[no_mangle]
pub unsafe extern "C" fn blocker_normalize_domain(input: *const c_char) -> *mut c_char {
    guarded(ptr::null_mut(), || {
        from_c(input)
            .and_then(blocker_core::domain::normalize)
            .map_or(ptr::null_mut(), to_c)
    })
}

#[no_mangle]
pub unsafe extern "C" fn blocker_peek_host(
    bytes: *const u8,
    len: usize,
    want_more: *mut usize,
) -> *mut c_char {
    guarded(ptr::null_mut(), || {
        if let Some(w) = want_more.as_mut() {
            *w = 0;
        }
        if bytes.is_null() || len == 0 {
            return ptr::null_mut();
        }
        let buf = std::slice::from_raw_parts(bytes, len.min(sni::MAX_PEEK));
        match sni::peek_host(buf) {
            Peek::Host(h) => to_c(h),
            Peek::NeedMore { want } => {
                if let Some(w) = want_more.as_mut() {
                    *w = want;
                }
                ptr::null_mut()
            }
            Peek::NoHost | Peek::Unknown => ptr::null_mut(),
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn blocker_config_validate(config_toml: *const c_char) -> bool {
    guarded(false, || {
        let Some(text) = from_c(config_toml) else {
            set_error("config_toml is NULL or not UTF-8");
            return false;
        };
        match parse_config(text) {
            Ok(_) => true,
            Err(e) => {
                set_error(e);
                false
            }
        }
    })
}
