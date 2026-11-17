import BlockerKit
import SwiftUI

/// Asks the engine whether a host would be blocked, now or at a chosen time.
struct CheckHostView: View {
    @EnvironmentObject private var store: RuleStore
    @State private var input = ""
    @State private var useCustomTime = false
    @State private var weekday = WeekTime.now.weekday
    @State private var time = Date()
    @State private var result: (host: String, verdict: Verdict)?

    private static let dayNames = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"]

    var body: some View {
        Form {
            Section {
                TextField("Host or URL, e.g. www.reddit.com", text: $input)
                    .keyboardType(.URL)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .onSubmit(check)
                Button("Check", action: check).disabled(input.isEmpty || store.engine == nil)
            }
            Section("When") {
                Toggle("Pick a time", isOn: $useCustomTime)
                if useCustomTime {
                    Picker("Day", selection: $weekday) {
                        ForEach(0..<7, id: \.self) { Text(Self.dayNames[$0]).tag($0) }
                    }
                    DatePicker("Time", selection: $time, displayedComponents: .hourAndMinute)
                }
            }
            if let result {
                Section("Verdict") {
                    LabeledContent("Host", value: result.host)
                    VerdictLabel(verdict: result.verdict)
                }
            }
        }
        .navigationTitle("Check a Host")
    }

    private var weekTime: WeekTime {
        guard useCustomTime else { return .now }
        let c = Calendar.current.dateComponents([.hour, .minute], from: time)
        return WeekTime(weekday: weekday, hour: c.hour ?? 0, minute: c.minute ?? 0)
    }

    private func check() {
        guard let engine = store.engine else { return }
        let host = BlockerEngine.normalizeDomain(input) ?? input
        result = (host, engine.checkHost(input, at: weekTime))
    }
}

struct VerdictLabel: View {
    let verdict: Verdict

    var body: some View {
        switch verdict {
        case .allow:
            Label("Allowed", systemImage: "checkmark.circle.fill").foregroundStyle(.green)
        case .block(let rule, let matched):
            VStack(alignment: .leading, spacing: 4) {
                Label("Blocked", systemImage: "xmark.octagon.fill").foregroundStyle(.red)
                Text("Rule \u{201C}\(rule)\u{201D} matched \(matched)").font(.caption).foregroundStyle(.secondary)
            }
        case .peek:
            Label("Needs inspection", systemImage: "questionmark.circle").foregroundStyle(.orange)
        }
    }
}
