//! Installs and enables the content filter system extension.
//!
//! Two independent pieces of macOS state are involved:
//! 1. the system extension itself (`OSSystemExtensionRequest`), which the
//!    user approves once in System Settings > General > Login Items &
//!    Extensions, and
//! 2. the content filter configuration (`NEFilterManager`), which points
//!    NetworkExtension at that extension and turns filtering on. macOS asks
//!    the user to allow it the first time.
//!
//! Neither creates a VPN configuration, so Tailscale, Twingate and other
//! packet-tunnel VPNs stay connected and in charge of routing and DNS.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use block2::RcBlock;
use dispatch2::DispatchQueue;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, AllocAnyThread, ClassType, DefinedClass};
use objc2_foundation::{NSArray, NSError, NSObject, NSObjectProtocol, NSString};
use objc2_network_extension::{NEFilterManager, NEFilterProviderConfiguration};
use objc2_system_extensions::{
    OSSystemExtensionManager, OSSystemExtensionProperties, OSSystemExtensionReplacementAction,
    OSSystemExtensionRequest, OSSystemExtensionRequestDelegate, OSSystemExtensionRequestResult,
};

pub const FILTER_BUNDLE_ID: &str = "com.briandevins.blocker.filter";
const DESCRIPTION: &str = "Blocker";

/// How long to wait for the user to approve the extension before giving up.
/// The request stays pending in System Settings; re-running picks it up.
const APPROVAL_TIMEOUT: Duration = Duration::from_secs(300);
const PREFS_TIMEOUT: Duration = Duration::from_secs(30);
/// Shorter waits for `status()`, which a UI polls.
const STATUS_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug)]
enum Event {
    NeedsApproval,
    Finished { reboot: bool },
    Failed(String),
    Properties(Vec<ExtensionState>),
}

/// One installed version of the extension, as reported by the system.
#[derive(Debug, Clone)]
pub struct ExtensionState {
    pub version: String,
    pub enabled: bool,
    pub awaiting_approval: bool,
    pub uninstalling: bool,
}

#[derive(Debug, Clone)]
pub struct FilterStatus {
    /// `None` if the properties query failed (e.g. not run from Blocker.app).
    pub extensions: Option<Vec<ExtensionState>>,
    pub configured: bool,
    pub enabled: bool,
}

struct DelegateIvars {
    tx: Mutex<Sender<Event>>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; ivars are Send + Sync
    // via the Mutex and need no special Drop handling.
    #[unsafe(super(NSObject))]
    #[name = "BlockerExtensionRequestDelegate"]
    #[ivars = DelegateIvars]
    struct RequestDelegate;

    unsafe impl NSObjectProtocol for RequestDelegate {}

    unsafe impl OSSystemExtensionRequestDelegate for RequestDelegate {
        #[unsafe(method(request:actionForReplacingExtension:withExtension:))]
        fn replace(
            &self,
            _request: &OSSystemExtensionRequest,
            existing: &OSSystemExtensionProperties,
            ext: &OSSystemExtensionProperties,
        ) -> OSSystemExtensionReplacementAction {
            // SAFETY: plain property getters.
            let (old, new) = unsafe { (existing.bundleVersion(), ext.bundleVersion()) };
            log::info!("replacing extension {old} with {new}");
            OSSystemExtensionReplacementAction::Replace
        }

        #[unsafe(method(requestNeedsUserApproval:))]
        fn needs_approval(&self, _request: &OSSystemExtensionRequest) {
            self.send(Event::NeedsApproval);
        }

        #[unsafe(method(request:didFinishWithResult:))]
        fn finished(
            &self,
            _request: &OSSystemExtensionRequest,
            result: OSSystemExtensionRequestResult,
        ) {
            self.send(Event::Finished {
                reboot: result == OSSystemExtensionRequestResult::WillCompleteAfterReboot,
            });
        }

        #[unsafe(method(request:didFailWithError:))]
        fn failed(&self, _request: &OSSystemExtensionRequest, error: &NSError) {
            self.send(Event::Failed(describe_error(error)));
        }

        #[unsafe(method(request:foundProperties:))]
        fn found(
            &self,
            _request: &OSSystemExtensionRequest,
            properties: &NSArray<OSSystemExtensionProperties>,
        ) {
            let states = properties
                .iter()
                // SAFETY: plain property getters.
                .map(|p| unsafe {
                    ExtensionState {
                        version: p.bundleShortVersion().to_string(),
                        enabled: p.isEnabled(),
                        awaiting_approval: p.isAwaitingUserApproval(),
                        uninstalling: p.isUninstalling(),
                    }
                })
                .collect();
            self.send(Event::Properties(states));
        }
    }
);

impl RequestDelegate {
    fn new(tx: Sender<Event>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(DelegateIvars { tx: Mutex::new(tx) });
        // SAFETY: NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }

    fn send(&self, event: Event) {
        if let Ok(tx) = self.ivars().tx.lock() {
            let _ = tx.send(event);
        }
    }
}

fn describe_error(error: &NSError) -> String {
    let code = error.code();
    let text = error.localizedDescription().to_string();
    let hint = match code {
        // OSSystemExtensionErrorCode values.
        1 => "",
        2 => " (the app is missing the system-extension or network-extension entitlement; see packaging/README.md)",
        3 => " (Blocker.app must be in /Applications)",
        4 => " (the extension is not inside Blocker.app/Contents/Library/SystemExtensions)",
        8 => " (code signature invalid: sign with Developer ID, or enable `systemextensionsctl developer on` for ad-hoc builds)",
        10 => " (blocked by system policy, e.g. MDM)",
        _ => "",
    };
    format!("{text} [code {code}]{hint}")
}

/// Submits a system extension request and returns a receiver for its events.
/// The delegate must outlive the request, so it is returned too.
fn submit(
    make: impl FnOnce(&NSString, &DispatchQueue) -> Retained<OSSystemExtensionRequest>,
) -> (Receiver<Event>, Retained<RequestDelegate>) {
    let (tx, rx) = mpsc::channel();
    let delegate = RequestDelegate::new(tx);
    let queue = DispatchQueue::new("dev.blocker.sysext", None);
    let request = make(&NSString::from_str(FILTER_BUNDLE_ID), &queue);
    // SAFETY: delegate implements the protocol and is kept alive by the caller
    // until the request has finished.
    unsafe {
        request.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        OSSystemExtensionManager::sharedManager().submitRequest(&request);
    }
    (rx, delegate)
}

/// Installs (or upgrades) the extension and enables the content filter.
pub fn activate() -> Result<()> {
    // SAFETY: constructor with a valid identifier and queue.
    let (rx, _delegate) = submit(|id, q| unsafe {
        OSSystemExtensionRequest::activationRequestForExtension_queue(id, q)
    });
    loop {
        match rx.recv_timeout(APPROVAL_TIMEOUT) {
            Ok(Event::NeedsApproval) => eprintln!(
                "Approve \"Blocker\" in System Settings > General > Login Items & Extensions > Network Extensions.\nWaiting..."
            ),
            Ok(Event::Finished { reboot: true }) => {
                eprintln!("Extension will finish installing after a reboot.");
                break;
            }
            Ok(Event::Finished { reboot: false }) => {
                eprintln!("Extension installed.");
                break;
            }
            Ok(Event::Failed(e)) => bail!("activating extension failed: {e}"),
            Ok(Event::Properties(_)) => {}
            Err(_) => bail!("timed out waiting for approval; approve in System Settings and run `blocker activate` again"),
        }
    }
    configure_filter(true)?;
    eprintln!("Content filter enabled.");
    Ok(())
}

/// Disables the filter and removes the extension.
pub fn deactivate() -> Result<()> {
    configure_filter(false)?;
    // SAFETY: constructor with a valid identifier and queue.
    let (rx, _delegate) = submit(|id, q| unsafe {
        OSSystemExtensionRequest::deactivationRequestForExtension_queue(id, q)
    });
    loop {
        match rx.recv_timeout(APPROVAL_TIMEOUT) {
            Ok(Event::NeedsApproval) => eprintln!("Confirm removal in the system prompt..."),
            Ok(Event::Finished { .. }) => return Ok(()),
            Ok(Event::Failed(e)) => bail!("removing extension failed: {e}"),
            Ok(Event::Properties(_)) => {}
            Err(_) => bail!("timed out removing extension"),
        }
    }
}

pub fn status() -> Result<FilterStatus> {
    status_with_timeout(PREFS_TIMEOUT)
}

/// Like [`status`] but gives up quickly; for machine-readable polling.
pub fn status_quick() -> Result<FilterStatus> {
    status_with_timeout(STATUS_TIMEOUT)
}

fn status_with_timeout(timeout: Duration) -> Result<FilterStatus> {
    // SAFETY: constructor with a valid identifier and queue.
    let (rx, _delegate) = submit(|id, q| unsafe {
        OSSystemExtensionRequest::propertiesRequestForExtension_queue(id, q)
    });
    let extensions = match rx.recv_timeout(timeout) {
        Ok(Event::Properties(p)) => Some(p),
        Ok(Event::Failed(e)) => {
            log::debug!("properties request failed: {e}");
            None
        }
        _ => None,
    };
    let manager = load_manager(timeout)?;
    // SAFETY: plain property getters.
    let (configured, enabled) = unsafe {
        (
            manager.providerConfiguration().is_some(),
            manager.isEnabled(),
        )
    };
    Ok(FilterStatus {
        extensions,
        configured,
        enabled,
    })
}

/// Waits for an `NEFilterManager` completion handler.
fn wait_prefs(
    what: &str,
    timeout: Duration,
    start: impl FnOnce(&block2::DynBlock<dyn Fn(*mut NSError)>),
) -> Result<()> {
    let (tx, rx) = mpsc::channel::<Option<String>>();
    let block = RcBlock::new(move |err: *mut NSError| {
        // SAFETY: NetworkExtension passes nil or a valid NSError.
        let msg = unsafe { err.as_ref() }.map(describe_error);
        let _ = tx.send(msg);
    });
    start(&block);
    match rx.recv_timeout(timeout) {
        Ok(None) => Ok(()),
        Ok(Some(e)) => Err(anyhow!("{what}: {e}")),
        Err(_) => bail!("{what}: timed out"),
    }
}

fn load_manager(timeout: Duration) -> Result<Retained<NEFilterManager>> {
    // `+sharedManager` returns nil when the process lacks the content-filter
    // entitlement (e.g. the CLI run from outside Blocker.app); the generated
    // binding would panic on that, so message it directly.
    // SAFETY: class method with no arguments returning an autoreleased object or nil.
    let manager: Option<Retained<NEFilterManager>> =
        unsafe { msg_send![NEFilterManager::class(), sharedManager] };
    let manager = manager.context("content filter unavailable: run the signed /Applications/Blocker.app/Contents/MacOS/blocker")?;
    wait_prefs("loading filter preferences", timeout, |b| unsafe {
        manager.loadFromPreferencesWithCompletionHandler(b)
    })?;
    Ok(manager)
}

fn configure_filter(enable: bool) -> Result<()> {
    let manager = load_manager(PREFS_TIMEOUT)
        .context("is Blocker.app installed in /Applications and signed?")?;
    // SAFETY: configuring a freshly loaded manager from one thread.
    unsafe {
        if enable {
            let conf = NEFilterProviderConfiguration::new();
            conf.setFilterSockets(true);
            conf.setFilterPackets(false);
            conf.setFilterDataProviderBundleIdentifier(Some(&NSString::from_str(FILTER_BUNDLE_ID)));
            conf.setOrganization(Some(&NSString::from_str(DESCRIPTION)));
            manager.setProviderConfiguration(Some(&conf));
            manager.setLocalizedDescription(Some(&NSString::from_str(DESCRIPTION)));
        } else if manager.providerConfiguration().is_none() {
            return Ok(());
        }
        manager.setEnabled(enable);
    }
    wait_prefs("saving filter preferences", PREFS_TIMEOUT, |b| unsafe {
        manager.saveToPreferencesWithCompletionHandler(b)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2::runtime::AnyClass;

    #[test]
    fn delegate_conforms_and_forwards_events() {
        let (tx, rx) = mpsc::channel();
        let d = RequestDelegate::new(tx);
        let cls = AnyClass::get(c"BlockerExtensionRequestDelegate").unwrap();
        // The protocol object only exists once something references it, so
        // check the methods SystemExtensions calls instead.
        for s in [
            objc2::sel!(request:actionForReplacingExtension:withExtension:),
            objc2::sel!(requestNeedsUserApproval:),
            objc2::sel!(request:didFinishWithResult:),
            objc2::sel!(request:didFailWithError:),
            objc2::sel!(request:foundProperties:),
        ] {
            assert!(cls.instance_method(s).is_some(), "{s:?}");
        }
        d.send(Event::NeedsApproval);
        assert!(matches!(rx.try_recv(), Ok(Event::NeedsApproval)));
    }
}
