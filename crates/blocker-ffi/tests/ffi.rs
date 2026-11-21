use std::ffi::{CStr, CString};
use std::ptr;

use blocker_ffi::*;

const CONFIG: &str = r#"
[[rule]]
name = "focus"
domains = ["youtube.com"]
bundle_ids = ["com.example.game"]
schedule = { type = "windows", windows = [{ days = ["mon"], start = "09:00", end = "17:00" }] }
"#;

fn take(p: *mut std::ffi::c_char) -> Option<String> {
    if p.is_null() {
        return None;
    }
    let s = unsafe { CStr::from_ptr(p) }.to_str().unwrap().to_owned();
    unsafe { blocker_string_free(p) };
    Some(s)
}

fn engine() -> *mut BlockerEngine {
    let toml = CString::new(CONFIG).unwrap();
    let e = unsafe { blocker_engine_from_toml(toml.as_ptr()) };
    assert!(!e.is_null(), "{:?}", take(blocker_last_error()));
    e
}

#[test]
fn check_host_respects_schedule() {
    let e = engine();
    let host = CString::new("www.youtube.com").unwrap();
    // Monday 10:00 -> blocked.
    let mut v = unsafe { blocker_engine_check_host(e, host.as_ptr(), 0, 600) };
    assert_eq!(v.kind, BlockerVerdictKind::Block);
    assert_eq!(take(v.rule), Some("focus".into()));
    v.rule = ptr::null_mut();
    unsafe { blocker_verdict_free(&mut v) };
    // Tuesday 10:00 -> allowed.
    let v = unsafe { blocker_engine_check_host(e, host.as_ptr(), 1, 600) };
    assert_eq!(v.kind, BlockerVerdictKind::Allow);
    assert_eq!(
        take(unsafe { blocker_engine_active_rules_json(e, 0, 600) }).unwrap(),
        r#"["focus"]"#
    );
    assert!(unsafe { blocker_engine_is_idle(e, 6, 0) });
    let apps = take(unsafe { blocker_engine_blocked_apps_json(e, 0, 600) }).unwrap();
    assert!(apps.contains("com.example.game"));
    unsafe { blocker_engine_free(e) };
}

#[test]
fn decide_peeks_then_blocks() {
    let e = engine();
    let flow = BlockerFlow {
        hostname: ptr::null(),
        remote_ip: ptr::null(),
        remote_port: 443,
        proto: BlockerProto::Tcp,
        app_id: ptr::null(),
        ech: false,
    };
    let v = unsafe { blocker_engine_decide(e, &flow, false, ptr::null(), 0, 600) };
    assert_eq!(v.kind, BlockerVerdictKind::Peek);
    let host = CString::new("youtube.com").unwrap();
    let mut v = unsafe { blocker_engine_decide(e, &flow, true, host.as_ptr(), 0, 600) };
    assert_eq!(v.kind, BlockerVerdictKind::Block);
    unsafe { blocker_verdict_free(&mut v) };
    unsafe { blocker_engine_free(e) };
}

#[test]
fn bad_config_sets_error() {
    let toml = CString::new("[[rule]]\nname = 3").unwrap();
    assert!(unsafe { blocker_engine_from_toml(toml.as_ptr()) }.is_null());
    assert!(take(blocker_last_error())
        .unwrap()
        .contains("parsing config"));
    assert!(!unsafe { blocker_config_validate(toml.as_ptr()) });
}

#[test]
fn normalize_and_peek() {
    let url = CString::new("https://WWW.Example.com/path").unwrap();
    assert_eq!(
        take(unsafe { blocker_normalize_domain(url.as_ptr()) }),
        Some("www.example.com".into())
    );
    let req = b"GET / HTTP/1.1\r\nHost: example.org\r\n\r\n";
    let mut want = 99usize;
    let h = take(unsafe { blocker_peek_host(req.as_ptr(), req.len(), &mut want) });
    assert_eq!(h, Some("example.org".into()));
    assert_eq!(want, 0);
}

#[test]
fn header_declares_every_export() {
    let header = include_str!("../include/blocker.h");
    let src = include_str!("../src/lib.rs");
    let exports: Vec<&str> = src
        .split("extern \"C\" fn ")
        .skip(1)
        .map(|s| s.split('(').next().unwrap())
        .collect();
    assert!(exports.len() >= 15);
    for name in exports {
        assert!(
            header.contains(&format!("{name}(")),
            "blocker.h is missing {name}"
        );
    }
}
