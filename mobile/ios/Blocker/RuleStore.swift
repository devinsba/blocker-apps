import BlockerKit
import Foundation

/// Owns the rules, persists them to the App Group and keeps a compiled engine.
@MainActor
final class RuleStore: ObservableObject {
    @Published private(set) var rules: [RuleModel] = []
    @Published private(set) var engine: BlockerEngine?
    @Published private(set) var activeRuleNames: Set<String> = []
    @Published var lastError: String?

    init() {
        load()
    }

    /// Loads rules.json, seeding the sample rules (and config.toml) on first launch.
    func load() {
        do {
            if let data = try? Data(contentsOf: SharedConfig.rulesURL) {
                rules = try JSONDecoder().decode([RuleModel].self, from: data)
            } else {
                rules = RuleModel.sample
                try persist()
            }
            if SharedConfig.readConfigTOML() == nil {
                try persist()
            }
            try recompile()
        } catch {
            lastError = error.localizedDescription
        }
    }

    func add(_ rule: RuleModel) throws {
        guard !rules.contains(where: { $0.name == rule.name }) else {
            throw BlockerError("A rule named \u{201C}\(rule.name)\u{201D} already exists.")
        }
        try commit(rules + [rule])
    }

    func update(_ rule: RuleModel) throws {
        guard let i = rules.firstIndex(where: { $0.name == rule.name }) else { return }
        if rules[i].locked && activeRuleNames.contains(rule.name) {
            throw BlockerError("\u{201C}\(rule.name)\u{201D} is locked while it is active.")
        }
        var next = rules
        next[i] = rule
        try commit(next)
    }

    func delete(at offsets: IndexSet) throws {
        let doomed = offsets.map { rules[$0] }
        if let locked = doomed.first(where: { $0.locked && activeRuleNames.contains($0.name) }) {
            throw BlockerError("\u{201C}\(locked.name)\u{201D} is locked while it is active.")
        }
        var next = rules
        next.remove(atOffsets: offsets)
        try commit(next)
    }

    func refreshActive() {
        activeRuleNames = Set(engine?.activeRules() ?? [])
    }

    // MARK: - Private

    /// Validates the new rules by rendering and compiling them before anything is saved.
    private func commit(_ next: [RuleModel]) throws {
        let toml = RuleModel.renderConfigTOML(next)
        _ = try BlockerEngine(configTOML: toml)
        let previous = rules
        rules = next
        do {
            try persist()
            try recompile()
        } catch {
            rules = previous
            throw error
        }
        lastError = nil
        // TODO: tell running extensions to reload: update the NEFilterManager
        // vendorConfiguration (content filter) / send a provider message (tunnel).
    }

    private func persist() throws {
        try FileManager.default.createDirectory(at: SharedConfig.directory, withIntermediateDirectories: true)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        try encoder.encode(rules).write(to: SharedConfig.rulesURL, options: .atomic)
        try SharedConfig.writeConfigTOML(RuleModel.renderConfigTOML(rules))
    }

    private func recompile() throws {
        engine = try SharedConfig.loadEngine()
        refreshActive()
    }
}
