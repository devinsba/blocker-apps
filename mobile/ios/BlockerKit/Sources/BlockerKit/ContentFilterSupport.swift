import Foundation

/// Decides whether the NEFilterDataProvider content filter (BlockerFilter) can
/// run on this device, and which website-blocking method the app should use.
///
/// iOS only runs content filters on **supervised** devices (or where an MDM
/// profile installed the filter configuration), and there is no public API
/// that says whether a device is supervised. So the app combines signals:
///
/// 1. A filter configuration already exists (`NEFilterManager` has a
///    `providerConfiguration`): an MDM profile installed it, or a previous
///    enable from this app succeeded. The filter works here.
/// 2. The MDM said so in managed app configuration
///    (`UserDefaults` key `com.apple.configuration.managed`), key
///    ``managedConfigSupervisedKey``. MDMs (Fleet, Jamf, Intune, ...) can set
///    it per device.
/// 3. The outcome of the last attempt to save a filter configuration on this
///    iOS version. A permission/invalid-configuration failure means the device
///    is not supervised; it is re-tried after an OS update or once the MDM
///    signal appears.
///
/// With no signal the support is `.unknown`: the app keeps the DNS tunnel as
/// the default and offers to try the filter. The content filter is never
/// required.
///
/// This type is pure (no NetworkExtension import) so it is unit-tested on macOS.
public enum ContentFilterSupport: Equatable, Sendable {
    case available(Evidence)
    case unknown
    case unavailable(reason: String)

    public enum Evidence: Equatable, Sendable {
        /// A filter configuration is installed (MDM profile or an earlier enable).
        case configurationInstalled
        /// Managed app configuration from the MDM says the device is supervised.
        case managedConfiguration
        /// Saving a filter configuration succeeded on this OS version.
        case probeSucceeded
    }

    /// Managed app configuration key an MDM sets to `true` on supervised devices.
    public static let managedConfigSupervisedKey = "supervised"
    /// `UserDefaults` key iOS stores managed app configuration under.
    public static let managedConfigDefaultsKey = "com.apple.configuration.managed"

    public var isAvailable: Bool {
        if case .available = self { return true }
        return false
    }

    /// Whether the UI should offer to try enabling the filter.
    public var canAttempt: Bool {
        switch self {
        case .available, .unknown: return true
        case .unavailable: return false
        }
    }

    /// Everything known about the device that bears on filter support.
    public struct Signals: Equatable, Sendable {
        /// `NEFilterManager.shared().providerConfiguration != nil` after loading.
        public var filterConfigurationInstalled: Bool
        /// The managed-config `supervised` flag, if the MDM sent one.
        public var managedSupervised: Bool?
        /// The last recorded attempt to save a filter configuration.
        public var lastProbe: ProbeRecord?
        /// Current OS version, e.g. `ProcessInfo.processInfo.operatingSystemVersionString`.
        public var osVersion: String

        public init(
            filterConfigurationInstalled: Bool,
            managedSupervised: Bool?,
            lastProbe: ProbeRecord?,
            osVersion: String
        ) {
            self.filterConfigurationInstalled = filterConfigurationInstalled
            self.managedSupervised = managedSupervised
            self.lastProbe = lastProbe
            self.osVersion = osVersion
        }
    }

    /// Outcome of trying to save a filter configuration, persisted by the app.
    public struct ProbeRecord: Codable, Equatable, Sendable {
        public var succeeded: Bool
        public var osVersion: String
        public var message: String?

        public init(succeeded: Bool, osVersion: String, message: String? = nil) {
            self.succeeded = succeeded
            self.osVersion = osVersion
            self.message = message
        }
    }

    public static func evaluate(_ s: Signals) -> ContentFilterSupport {
        if s.filterConfigurationInstalled {
            return .available(.configurationInstalled)
        }
        if s.managedSupervised == true {
            return .available(.managedConfiguration)
        }
        // A probe only counts for the OS version it ran on.
        if let probe = s.lastProbe, probe.osVersion == s.osVersion {
            if probe.succeeded {
                return .available(.probeSucceeded)
            }
            return .unavailable(reason: probe.message ?? "This device is not supervised.")
        }
        if s.managedSupervised == false {
            return .unavailable(reason: "Your device management reports this device is not supervised.")
        }
        return .unknown
    }

    /// Reads the managed-config `supervised` flag from a managed app
    /// configuration dictionary. Accepts a Bool, a number, or "true"/"false"/"yes"/"no"/"1"/"0".
    public static func managedSupervisedFlag(from managedConfig: [String: Any]?) -> Bool? {
        guard let value = managedConfig?[managedConfigSupervisedKey] else { return nil }
        switch value {
        case let b as Bool: return b
        case let n as NSNumber: return n.boolValue
        case let s as String:
            switch s.trimmingCharacters(in: .whitespaces).lowercased() {
            case "true", "yes", "1": return true
            case "false", "no", "0": return false
            default: return nil
            }
        default: return nil
        }
    }

    /// How a failed `NEFilterManager.saveToPreferences()` should be treated.
    public enum SaveFailure: Equatable, Sendable {
        /// The device can't run content filters (not supervised): record it.
        case unsupported
        /// Transient or unrelated: report it but don't record a probe.
        case transient
    }

    /// `NEFilterErrorDomain`, spelled out so this file needs no NetworkExtension.
    public static let filterErrorDomain = "NEFilterErrorDomain"

    /// Classifies an error from saving a filter configuration.
    ///
    /// `NEFilterManagerError` codes: 1 configurationInvalid, 2 configurationDisabled,
    /// 3 configurationStale, 4 configurationCannotBeRemoved,
    /// 5 configurationPermissionDenied, 6 configurationInternalError.
    /// Unsupervised devices report invalid (1) or permission denied (5);
    /// disabled (2) means a restriction/profile forbids it — also not usable.
    public static func classifySaveError(domain: String, code: Int) -> SaveFailure {
        guard domain == filterErrorDomain else { return .transient }
        switch code {
        case 1, 2, 5: return .unsupported
        default: return .transient
        }
    }
}

/// How the app blocks websites on this device.
public enum WebsiteProtectionMethod: String, Equatable, Sendable {
    /// NEFilterDataProvider: full flow inspection with SNI peeking; supervised only.
    /// Doesn't use the VPN slot, so it coexists with VPN apps.
    case contentFilter
    /// NEPacketTunnelProvider answering DNS: works anywhere, occupies the VPN slot.
    case dnsTunnel

    /// The filter when the device supports it, otherwise the DNS tunnel.
    public static func preferred(for support: ContentFilterSupport) -> WebsiteProtectionMethod {
        support.isAvailable ? .contentFilter : .dnsTunnel
    }
}
