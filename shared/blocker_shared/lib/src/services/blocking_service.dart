import '../models/block_rule.dart';
import '../models/block_target.dart';

/// Core blocking logic service - determines what should be blocked
class BlockingService {
  final List<BlockRule> _rules = [];

  /// Add a new blocking rule
  void addRule(BlockRule rule) {
    _rules.removeWhere((r) => r.id == rule.id);
    _rules.add(rule);
  }

  /// Remove a blocking rule
  void removeRule(String ruleId) {
    _rules.removeWhere((r) => r.id == ruleId);
  }

  /// Update an existing rule
  void updateRule(BlockRule rule) {
    final index = _rules.indexWhere((r) => r.id == rule.id);
    if (index != -1) {
      _rules[index] = rule;
    }
  }

  /// Get all rules
  List<BlockRule> getAllRules() {
    return List.unmodifiable(_rules);
  }

  /// Get rules for a specific target type
  List<BlockRule> getRulesByType(BlockTargetType type) {
    return _rules.where((r) => r.target.type == type).toList();
  }

  /// Check if a specific app should be blocked right now
  bool shouldBlockApp(String bundleId) {
    final now = DateTime.now();
    return _rules.any((rule) =>
        rule.target.type == BlockTargetType.application &&
        rule.target.identifier == bundleId &&
        rule.shouldBlock(now));
  }

  /// Check if a specific domain should be blocked right now
  bool shouldBlockDomain(String domain) {
    final now = DateTime.now();
    return _rules.any((rule) =>
        rule.target.type == BlockTargetType.domain &&
        _matchesDomain(rule.target.identifier, domain) &&
        rule.shouldBlock(now));
  }

  /// Get all currently blocked app bundle IDs
  List<String> getBlockedApps() {
    final now = DateTime.now();
    return _rules
        .where((rule) =>
            rule.target.type == BlockTargetType.application &&
            rule.shouldBlock(now))
        .map((rule) => rule.target.identifier)
        .toList();
  }

  /// Get all currently blocked domains
  List<String> getBlockedDomains() {
    final now = DateTime.now();
    return _rules
        .where((rule) =>
            rule.target.type == BlockTargetType.domain && rule.shouldBlock(now))
        .map((rule) => rule.target.identifier)
        .toList();
  }

  /// Check if a domain matches a blocking rule
  /// Supports exact matches and wildcard domains (*.example.com)
  bool _matchesDomain(String rulePattern, String domain) {
    // Exact match
    if (rulePattern == domain) return true;

    // Wildcard match (*.example.com matches sub.example.com)
    if (rulePattern.startsWith('*.')) {
      final baseDomain = rulePattern.substring(2);
      return domain.endsWith('.$baseDomain') || domain == baseDomain;
    }

    // Check if domain is a subdomain of the rule
    return domain.endsWith('.$rulePattern');
  }

  /// Clear all rules
  void clearAllRules() {
    _rules.clear();
  }

  /// Get count of active rules
  int get activeRuleCount {
    final now = DateTime.now();
    return _rules.where((rule) => rule.shouldBlock(now)).length;
  }
}
