import Foundation
import Testing
@testable import BlockerKit

@Suite struct ContentFilterSupportTests {
    private func signals(
        installed: Bool = false,
        managed: Bool? = nil,
        probe: ContentFilterSupport.ProbeRecord? = nil,
        os: String = "18.1"
    ) -> ContentFilterSupport.Signals {
        .init(filterConfigurationInstalled: installed, managedSupervised: managed, lastProbe: probe, osVersion: os)
    }

    @Test func noSignalsIsUnknownAndUsesTunnel() {
        let support = ContentFilterSupport.evaluate(signals())
        #expect(support == .unknown)
        #expect(support.canAttempt)
        #expect(WebsiteProtectionMethod.preferred(for: support) == .dnsTunnel)
    }

    @Test func installedConfigurationWins() {
        let failed = ContentFilterSupport.ProbeRecord(succeeded: false, osVersion: "18.1")
        let support = ContentFilterSupport.evaluate(signals(installed: true, managed: false, probe: failed))
        #expect(support == .available(.configurationInstalled))
        #expect(WebsiteProtectionMethod.preferred(for: support) == .contentFilter)
    }

    @Test func managedSupervisedOverridesFailedProbe() {
        let failed = ContentFilterSupport.ProbeRecord(succeeded: false, osVersion: "18.1")
        #expect(ContentFilterSupport.evaluate(signals(managed: true, probe: failed)) == .available(.managedConfiguration))
    }

    @Test func probeOnlyCountsForItsOSVersion() {
        let ok = ContentFilterSupport.ProbeRecord(succeeded: true, osVersion: "18.1")
        #expect(ContentFilterSupport.evaluate(signals(probe: ok)) == .available(.probeSucceeded))
        let failed = ContentFilterSupport.ProbeRecord(succeeded: false, osVersion: "18.1", message: "nope")
        #expect(ContentFilterSupport.evaluate(signals(probe: failed)) == .unavailable(reason: "nope"))
        #expect(ContentFilterSupport.evaluate(signals(probe: failed, os: "18.2")) == .unknown)
    }

    @Test func managedNotSupervisedIsUnavailable() {
        let support = ContentFilterSupport.evaluate(signals(managed: false))
        #expect(!support.canAttempt)
        #expect(WebsiteProtectionMethod.preferred(for: support) == .dnsTunnel)
    }

    @Test func managedFlagParsing() {
        let key = ContentFilterSupport.managedConfigSupervisedKey
        #expect(ContentFilterSupport.managedSupervisedFlag(from: nil) == nil)
        #expect(ContentFilterSupport.managedSupervisedFlag(from: [:]) == nil)
        #expect(ContentFilterSupport.managedSupervisedFlag(from: [key: true]) == true)
        #expect(ContentFilterSupport.managedSupervisedFlag(from: [key: NSNumber(value: 0)]) == false)
        #expect(ContentFilterSupport.managedSupervisedFlag(from: [key: " YES "]) == true)
        #expect(ContentFilterSupport.managedSupervisedFlag(from: [key: "false"]) == false)
        #expect(ContentFilterSupport.managedSupervisedFlag(from: [key: "maybe"]) == nil)
    }

    @Test func saveErrorClassification() {
        let domain = ContentFilterSupport.filterErrorDomain
        #expect(ContentFilterSupport.classifySaveError(domain: domain, code: 1) == .unsupported)
        #expect(ContentFilterSupport.classifySaveError(domain: domain, code: 5) == .unsupported)
        #expect(ContentFilterSupport.classifySaveError(domain: domain, code: 3) == .transient)
        #expect(ContentFilterSupport.classifySaveError(domain: NSCocoaErrorDomain, code: 1) == .transient)
    }
}
