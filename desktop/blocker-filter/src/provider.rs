//! `BlockerFilterDataProvider`, the `NEFilterDataProvider` subclass named in
//! the extension's Info.plist.

use std::net::IpAddr;

use block2::DynBlock;
use blocker_core::engine::{normalize_app_id, Proto};
use blocker_core::sni::{self, Peek};
use blocker_core::{FlowInfo, Verdict, WeekTime};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, ClassType};
use objc2_foundation::{NSData, NSError, NSObject, NSUInteger};
#[allow(deprecated)] // NWHostEndpoint: the nw_endpoint_t replacement has no binding yet
use objc2_network_extension::{
    NEFilterDataProvider, NEFilterDataVerdict, NEFilterFlow, NEFilterNewFlowVerdict,
    NEFilterProvider, NEFilterSocketFlow, NEProvider, NEProviderStopReason, NWHostEndpoint,
};

use crate::state;

const IPPROTO_TCP: i32 = 6;
const IPPROTO_UDP: i32 = 17;

/// Bytes requested on the first peek: enough for a typical ClientHello or
/// HTTP request line + Host header. Larger hellos ask for more via `NeedMore`.
const FIRST_PEEK: usize = 2048;

define_class!(
    // SAFETY: NEFilterDataProvider has no subclassing requirements beyond
    // overriding the methods below with matching signatures, and we hold no
    // ivars that need Drop.
    #[unsafe(super(NEFilterDataProvider, NEFilterProvider, NEProvider, NSObject))]
    #[name = "BlockerFilterDataProvider"]
    pub struct FilterProvider;

    impl FilterProvider {
        #[unsafe(method(startFilterWithCompletionHandler:))]
        fn start_filter(&self, completion: &DynBlock<dyn Fn(*mut NSError)>) {
            log::info!("filter started");
            completion.call((std::ptr::null_mut(),));
        }

        #[unsafe(method(stopFilterWithReason:completionHandler:))]
        fn stop_filter(&self, reason: NEProviderStopReason, completion: &DynBlock<dyn Fn()>) {
            log::info!("filter stopping, reason {}", reason.0);
            completion.call(());
        }

        #[unsafe(method_id(handleNewFlow:))]
        fn handle_new_flow(&self, flow: &NEFilterFlow) -> Retained<NEFilterNewFlowVerdict> {
            new_flow_verdict(flow)
        }

        #[unsafe(method_id(handleOutboundDataFromFlow:readBytesStartOffset:readBytes:))]
        fn handle_outbound(
            &self,
            flow: &NEFilterFlow,
            _offset: NSUInteger,
            bytes: &NSData,
        ) -> Retained<NEFilterDataVerdict> {
            outbound_data_verdict(flow, &bytes.to_vec())
        }

        #[unsafe(method_id(handleInboundDataFromFlow:readBytesStartOffset:readBytes:))]
        fn handle_inbound(
            &self,
            _flow: &NEFilterFlow,
            _offset: NSUInteger,
            _bytes: &NSData,
        ) -> Retained<NEFilterDataVerdict> {
            // Never requested; allow defensively.
            unsafe { NEFilterDataVerdict::allowVerdict() }
        }

        #[unsafe(method_id(handleOutboundDataCompleteForFlow:))]
        fn outbound_complete(&self, _flow: &NEFilterFlow) -> Retained<NEFilterDataVerdict> {
            unsafe { NEFilterDataVerdict::allowVerdict() }
        }

        #[unsafe(method_id(handleInboundDataCompleteForFlow:))]
        fn inbound_complete(&self, _flow: &NEFilterFlow) -> Retained<NEFilterDataVerdict> {
            unsafe { NEFilterDataVerdict::allowVerdict() }
        }
    }
);

fn new_flow_verdict(flow: &NEFilterFlow) -> Retained<NEFilterNewFlowVerdict> {
    let verdict = with_flow(flow, |engine, info| {
        engine.decide(info, None, WeekTime::now())
    });
    // SAFETY: class constructors with no preconditions.
    unsafe {
        match verdict {
            Verdict::Allow => NEFilterNewFlowVerdict::allowVerdict(),
            Verdict::Block(reason) => {
                log_block(flow, &reason);
                NEFilterNewFlowVerdict::dropVerdict()
            }
            Verdict::Peek => {
                NEFilterNewFlowVerdict::filterDataVerdictWithFilterInbound_peekInboundBytes_filterOutbound_peekOutboundBytes(
                    false,
                    0,
                    true,
                    FIRST_PEEK as NSUInteger,
                )
            }
        }
    }
}

fn outbound_data_verdict(flow: &NEFilterFlow, bytes: &[u8]) -> Retained<NEFilterDataVerdict> {
    let (peek, ech) = match bytes.first() {
        Some(0x16) => sni::peek_tls(bytes),
        _ => (sni::peek_host(bytes), false),
    };
    let host = match peek {
        Peek::NeedMore { want } if want > bytes.len() && want <= sni::MAX_PEEK => {
            // SAFETY: class constructor with no preconditions. Passing 0 bytes
            // keeps the flow held at offset 0 so the next call sees the whole,
            // longer prefix.
            return unsafe {
                NEFilterDataVerdict::dataVerdictWithPassBytes_peekBytes(0, want as NSUInteger)
            };
        }
        Peek::Host(h) => Some(h),
        Peek::NeedMore { .. } | Peek::NoHost | Peek::Unknown => None,
    };
    let verdict = with_flow(flow, |engine, info| {
        let info = FlowInfo { ech, ..*info };
        engine.decide(&info, Some(host.as_deref()), WeekTime::now())
    });
    // SAFETY: class constructors with no preconditions.
    unsafe {
        match verdict {
            Verdict::Block(reason) => {
                log_block(flow, &reason);
                NEFilterDataVerdict::dropVerdict()
            }
            Verdict::Allow | Verdict::Peek => NEFilterDataVerdict::allowVerdict(),
        }
    }
}

/// Forces the class to be registered with the Objective-C runtime.
pub fn register() {
    let _ = FilterProvider::class();
}

/// Plain-Rust view of an `NEFilterFlow`.
struct Extracted {
    hostname: Option<String>,
    remote_ip: Option<IpAddr>,
    remote_port: u16,
    proto: Proto,
    app_id: Option<String>,
}

fn extract(flow: &NEFilterFlow) -> Extracted {
    // SAFETY: plain property getters on a live flow object.
    let app_id =
        unsafe { flow.sourceAppIdentifier() }.map(|s| normalize_app_id(&s.to_string()).to_string());
    let mut out = Extracted {
        hostname: None,
        remote_ip: None,
        remote_port: 0,
        proto: Proto::Other,
        app_id,
    };
    let any: &AnyObject = flow.as_ref();
    let Some(socket) = any.downcast_ref::<NEFilterSocketFlow>() else {
        return out; // browser flows (NEFilterBrowserFlow) are iOS-only
    };
    // SAFETY: plain property getters on a live flow object.
    unsafe {
        out.proto = match socket.socketProtocol() {
            IPPROTO_TCP => Proto::Tcp,
            IPPROTO_UDP => Proto::Udp,
            _ => Proto::Other,
        };
        out.hostname = socket.remoteHostname().map(|h| h.to_string());
        #[allow(deprecated)] // the nw_endpoint_t replacement has no binding yet
        if let Some(endpoint) = socket.remoteEndpoint() {
            let any: &AnyObject = endpoint.as_ref();
            if let Some(host) = any.downcast_ref::<NWHostEndpoint>() {
                let h = host.hostname().to_string();
                out.remote_port = host.port().to_string().parse().unwrap_or(0);
                match h
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .split('%')
                    .next()
                    .unwrap_or("")
                    .parse()
                {
                    Ok(ip) => out.remote_ip = Some(ip),
                    Err(_) if out.hostname.is_none() => out.hostname = Some(h),
                    Err(_) => {}
                }
            }
        }
    }
    out
}

/// Runs `f` with the current engine; flows are allowed if none is loaded.
fn with_flow(
    flow: &NEFilterFlow,
    f: impl FnOnce(&blocker_core::Engine, &FlowInfo) -> Verdict,
) -> Verdict {
    let Some(engine) = state::engine() else {
        return Verdict::Allow;
    };
    let x = extract(flow);
    let info = FlowInfo {
        hostname: x.hostname.as_deref(),
        remote_ip: x.remote_ip,
        remote_port: x.remote_port,
        proto: x.proto,
        app_id: x.app_id.as_deref(),
        ech: false,
    };
    f(&engine, &info)
}

fn log_block(flow: &NEFilterFlow, reason: &blocker_core::engine::Reason) {
    let x = extract(flow);
    log::info!(
        "blocked {} {}:{} from {}: {reason}",
        match x.proto {
            Proto::Tcp => "tcp",
            Proto::Udp => "udp",
            Proto::Other => "flow",
        },
        x.hostname.as_deref().unwrap_or("?"),
        x.remote_port,
        x.app_id.as_deref().unwrap_or("?"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2::runtime::{AnyClass, Sel};
    use objc2::sel;

    #[test]
    fn registers_as_filter_data_provider_subclass() {
        register();
        let cls = AnyClass::get(c"BlockerFilterDataProvider").expect("class registered");
        assert_eq!(cls.superclass().unwrap().name(), c"NEFilterDataProvider");
        let selectors: [Sel; 7] = [
            sel!(startFilterWithCompletionHandler:),
            sel!(stopFilterWithReason:completionHandler:),
            sel!(handleNewFlow:),
            sel!(handleOutboundDataFromFlow:readBytesStartOffset:readBytes:),
            sel!(handleInboundDataFromFlow:readBytesStartOffset:readBytes:),
            sel!(handleOutboundDataCompleteForFlow:),
            sel!(handleInboundDataCompleteForFlow:),
        ];
        for s in selectors {
            let ours = cls.instance_method(s).expect("overridden");
            let theirs = cls
                .superclass()
                .unwrap()
                .instance_method(s)
                .expect("exists on superclass");
            // Same selector, different IMP: we really override it.
            assert_ne!(
                ours.implementation() as usize,
                theirs.implementation() as usize,
                "{s:?}"
            );
        }
    }
}
