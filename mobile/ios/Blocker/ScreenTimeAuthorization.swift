import Foundation

/// App blocking on iOS goes through Screen Time (FamilyControls +
/// ManagedSettings + DeviceActivity), not bundle ids: blocker-core's
/// `bundle_ids` can't be applied directly because iOS only hands out opaque
/// `ApplicationToken`s chosen by the user in `FamilyActivityPicker`.
///
/// STUB. Everything below is a placeholder until the
/// `com.apple.developer.family-controls` entitlement is granted by Apple
/// (the development variant works for local builds) and added to
/// Blocker.entitlements.
@MainActor
final class ScreenTimeAuthorization: ObservableObject {
    enum Status: Equatable {
        case notDetermined, approved, denied, unavailable
    }

    @Published private(set) var status: Status = .unavailable
    @Published private(set) var lastError: String?

    /// TODO(screen-time): replace with
    ///
    ///     import FamilyControls
    ///     try await AuthorizationCenter.shared.requestAuthorization(for: .individual)
    ///     status = AuthorizationCenter.shared.authorizationStatus == .approved ? .approved : .denied
    ///
    /// then present `FamilyActivityPicker(selection:)`, persist the
    /// `FamilyActivitySelection` (it is Codable) in the App Group, and apply it with
    ///
    ///     ManagedSettingsStore().shield.applications = selection.applicationTokens
    ///
    /// For schedules, register `DeviceActivitySchedule`s mirroring each rule's
    /// window with `DeviceActivityCenter().startMonitoring(...)` and add a
    /// DeviceActivityMonitor extension that sets/clears the shield in
    /// intervalDidStart/intervalDidEnd.
    func requestAuthorization() async {
        lastError = "Screen Time integration is not implemented yet (needs the Family Controls entitlement)."
        status = .unavailable
    }
}
