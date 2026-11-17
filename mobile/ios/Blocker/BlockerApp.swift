import BlockerKit
import SwiftUI

@main
struct BlockerApp: App {
    @StateObject private var store = RuleStore()
    @StateObject private var screenTime = ScreenTimeAuthorization()
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            RootView()
                .environmentObject(store)
                .environmentObject(screenTime)
        }
        .onChange(of: scenePhase) { phase in
            if phase == .active { store.refreshActive() }
        }
    }
}

enum Destination: Hashable, CaseIterable, Identifiable {
    case rules, check, protection
    var id: Self { self }

    var title: String {
        switch self {
        case .rules: return "Rules"
        case .check: return "Check a Host"
        case .protection: return "Protection"
        }
    }

    var systemImage: String {
        switch self {
        case .rules: return "list.bullet.rectangle"
        case .check: return "magnifyingglass"
        case .protection: return "shield.lefthalf.filled"
        }
    }
}

/// Sidebar + content + detail on iPad (and large iPhones in landscape);
/// collapses to a navigation stack on iPhone.
struct RootView: View {
    @EnvironmentObject private var store: RuleStore
    @State private var destination: Destination? = .rules
    @State private var selectedRule: RuleModel.ID?
    @State private var columnVisibility: NavigationSplitViewVisibility = .automatic

    var body: some View {
        NavigationSplitView(columnVisibility: $columnVisibility) {
            List(Destination.allCases, selection: $destination) { item in
                NavigationLink(value: item) {
                    Label(item.title, systemImage: item.systemImage)
                }
            }
            .navigationTitle("Blocker")
        } content: {
            switch destination ?? .rules {
            case .rules:
                RuleListView(selection: $selectedRule)
            case .check:
                CheckHostView()
            case .protection:
                ProtectionView()
            }
        } detail: {
            if destination == .rules, let name = selectedRule,
               let rule = store.rules.first(where: { $0.id == name }) {
                RuleDetailView(rule: rule)
            } else {
                Text(destination == .rules ? "Select a rule" : "")
                    .foregroundStyle(.secondary)
            }
        }
        .alert("Error", isPresented: Binding(
            get: { store.lastError != nil },
            set: { if !$0 { store.lastError = nil } }
        )) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(store.lastError ?? "")
        }
    }
}
