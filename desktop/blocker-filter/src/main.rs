//! Blocker content filter: a Network Extension system extension
//! (`NEFilterDataProvider`) written in Rust.
//!
//! The provider sees every new socket flow before it reaches any VPN tunnel
//! and answers allow / drop / "show me the first bytes". It never alters
//! routes, DNS settings or packets, so it runs alongside Tailscale, Twingate
//! and other packet-tunnel VPNs instead of competing with them.
//!
//! All decisions are made by `blocker_core::Engine`; this crate only adapts
//! NetworkExtension objects to `FlowInfo` and verdicts back.

#[cfg(target_os = "macos")]
mod provider;
#[cfg(target_os = "macos")]
mod state;

#[cfg(target_os = "macos")]
fn main() {
    let _ = oslog::OsLogger::new(state::SUBSYSTEM)
        .level_filter(log::LevelFilter::Info)
        .init();
    log::info!("blocker filter starting");

    // Register the Objective-C class before NetworkExtension looks it up by
    // the name in Info.plist (NEProviderClasses).
    provider::register();
    state::start_reloader();

    // SAFETY: called once from the main thread, as NetworkExtension requires.
    unsafe { objc2_network_extension::NEProvider::startSystemExtensionMode() };
    dispatch2::dispatch_main();
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("blocker-filter: the content filter is a macOS Network Extension and is macOS-only");
    std::process::exit(1);
}
