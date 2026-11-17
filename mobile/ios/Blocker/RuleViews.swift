import BlockerKit
import SwiftUI

struct RuleListView: View {
    @EnvironmentObject private var store: RuleStore
    @Binding var selection: RuleModel.ID?
    @State private var showingAdd = false

    var body: some View {
        List(selection: $selection) {
            Section {
                ForEach(store.rules) { rule in
                    NavigationLink(value: rule.id) {
                        RuleRow(rule: rule, active: store.activeRuleNames.contains(rule.name))
                    }
                }
                .onDelete { offsets in
                    do { try store.delete(at: offsets) } catch { store.lastError = error.localizedDescription }
                }
            } footer: {
                if let warnings = store.engine?.warnings, !warnings.isEmpty {
                    VStack(alignment: .leading) {
                        ForEach(warnings, id: \.self) { Label($0, systemImage: "exclamationmark.triangle") }
                    }
                }
            }
        }
        .overlay {
            if store.rules.isEmpty {
                Text("No rules yet. Tap + to add one.").foregroundStyle(.secondary)
            }
        }
        .navigationTitle("Rules")
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button { showingAdd = true } label: { Label("Add Rule", systemImage: "plus") }
            }
            ToolbarItem(placement: .navigationBarLeading) { EditButton() }
        }
        .refreshable { store.load() }
        .sheet(isPresented: $showingAdd) {
            NavigationStack { AddRuleView() }
        }
    }
}

struct RuleRow: View {
    let rule: RuleModel
    let active: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(rule.name).font(.headline)
                if rule.locked { Image(systemName: "lock.fill").foregroundStyle(.secondary) }
                Spacer()
                if !rule.enabled {
                    Text("Off").font(.caption).foregroundStyle(.secondary)
                } else if active {
                    Text("Active").font(.caption.bold()).foregroundStyle(.red)
                }
            }
            Text(rule.domains.joined(separator: ", "))
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .lineLimit(1)
            Text(scheduleDescription(rule.window))
                .font(.caption)
                .foregroundStyle(.tertiary)
        }
        .padding(.vertical, 2)
    }
}

struct RuleDetailView: View {
    @EnvironmentObject private var store: RuleStore
    let rule: RuleModel

    var body: some View {
        Form {
            Section("Status") {
                Toggle("Enabled", isOn: Binding(
                    get: { rule.enabled },
                    set: { newValue in
                        var r = rule
                        r.enabled = newValue
                        do { try store.update(r) } catch { store.lastError = error.localizedDescription }
                    }
                ))
                LabeledContent("Active now", value: store.activeRuleNames.contains(rule.name) ? "Yes" : "No")
                LabeledContent("Schedule", value: scheduleDescription(rule.window))
                if rule.locked { Label("Locked while active", systemImage: "lock.fill") }
            }
            Section("Domains") {
                if rule.domains.isEmpty { Text("None").foregroundStyle(.secondary) }
                ForEach(rule.domains, id: \.self) { Text($0) }
            }
            if !rule.categories.isEmpty {
                Section("Categories") { ForEach(rule.categories, id: \.self) { Text($0) } }
            }
            Section {
                // TODO(screen-time): app blocking on iOS uses FamilyActivityPicker tokens,
                // not bundle ids; see ScreenTimeAuthorization.swift.
                Text("App blocking uses Screen Time; see Protection.")
                    .foregroundStyle(.secondary)
            } header: {
                Text("Apps")
            }
        }
        .navigationTitle(rule.name)
    }
}

/// Adds a rule. Domains are normalized with blocker-core so the app shows
/// exactly what the engine will match.
struct AddRuleView: View {
    @EnvironmentObject private var store: RuleStore
    @Environment(\.dismiss) private var dismiss

    @State private var name = ""
    @State private var domainInput = ""
    @State private var domains: [String] = []
    @State private var domainError: String?
    @State private var scheduled = false
    @State private var days: Set<Int> = [0, 1, 2, 3, 4]
    @State private var start = Calendar.current.date(from: DateComponents(hour: 9)) ?? Date()
    @State private var end = Calendar.current.date(from: DateComponents(hour: 17)) ?? Date()
    @State private var locked = false
    @State private var saveError: String?

    private static let dayLabels = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]

    var body: some View {
        Form {
            Section("Name") {
                TextField("e.g. focus", text: $name)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
            }
            Section {
                HStack {
                    TextField("Domain or URL", text: $domainInput)
                        .keyboardType(.URL)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .onSubmit(addDomain)
                    Button("Add", action: addDomain).disabled(domainInput.isEmpty)
                }
                if let domainError {
                    Text(domainError).font(.caption).foregroundStyle(.red)
                } else if let preview = BlockerEngine.normalizeDomain(domainInput), preview != domainInput {
                    Text("Will block \(preview) and its subdomains").font(.caption).foregroundStyle(.secondary)
                }
                ForEach(domains, id: \.self) { Text($0) }
                    .onDelete { domains.remove(atOffsets: $0) }
            } header: {
                Text("Domains")
            }
            Section("Schedule") {
                Toggle("Only at certain times", isOn: $scheduled)
                if scheduled {
                    HStack {
                        ForEach(0..<7, id: \.self) { day in
                            Button(Self.dayLabels[day]) {
                                if days.contains(day) { days.remove(day) } else { days.insert(day) }
                            }
                            .buttonStyle(.bordered)
                            .tint(days.contains(day) ? .accentColor : .gray)
                            .font(.caption)
                        }
                    }
                    DatePicker("Start", selection: $start, displayedComponents: .hourAndMinute)
                    DatePicker("End", selection: $end, displayedComponents: .hourAndMinute)
                }
            }
            Section {
                Toggle("Lock while active", isOn: $locked)
            } footer: {
                Text("A locked rule can't be edited or removed while it is blocking.")
            }
            if let saveError {
                Section { Text(saveError).foregroundStyle(.red) }
            }
        }
        .navigationTitle("New Rule")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
            ToolbarItem(placement: .confirmationAction) {
                Button("Save", action: save)
                    .disabled(name.trimmingCharacters(in: .whitespaces).isEmpty || domains.isEmpty)
            }
        }
    }

    private func addDomain() {
        guard let domain = BlockerEngine.normalizeDomain(domainInput) else {
            domainError = "\u{201C}\(domainInput)\u{201D} isn't a valid domain."
            return
        }
        domainError = nil
        if !domains.contains(domain) { domains.append(domain) }
        domainInput = ""
    }

    private func save() {
        if !domainInput.isEmpty { addDomain() }
        let window = scheduled
            ? RuleModel.DailyWindow(days: days.sorted(), startMinute: minutes(start), endMinute: minutes(end))
            : nil
        let rule = RuleModel(
            name: name.trimmingCharacters(in: .whitespaces),
            locked: locked,
            domains: domains,
            window: window
        )
        do {
            try store.add(rule)
            dismiss()
        } catch {
            saveError = error.localizedDescription
        }
    }

    private func minutes(_ date: Date) -> Int {
        let c = Calendar.current.dateComponents([.hour, .minute], from: date)
        return (c.hour ?? 0) * 60 + (c.minute ?? 0)
    }
}

func scheduleDescription(_ window: RuleModel.DailyWindow?) -> String {
    guard let w = window else { return "Always" }
    let names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
    let days = w.days.isEmpty || w.days.count == 7
        ? "Every day"
        : w.days.filter { (0...6).contains($0) }.map { names[$0] }.joined(separator: " ")
    let fmt = { (m: Int) in String(format: "%02d:%02d", m / 60, m % 60) }
    return "\(days) \(fmt(w.startMinute))\u{2013}\(fmt(w.endMinute))"
}
