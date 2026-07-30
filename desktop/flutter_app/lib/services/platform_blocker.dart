import 'package:flutter/services.dart';
import 'package:blocker_shared/blocker_shared.dart';

/// Platform channel for communicating with native macOS/Windows blocking
class PlatformBlocker {
  static const MethodChannel _channel = MethodChannel('com.blocker/blocking');

  /// Enable blocking for the given apps
  Future<bool> enableAppBlocking(List<String> bundleIds) async {
    try {
      final result = await _channel.invokeMethod('enableAppBlocking', {
        'bundleIds': bundleIds,
      });
      return result as bool;
    } catch (e) {
      print('Error enabling app blocking: $e');
      return false;
    }
  }

  /// Disable blocking for the given apps
  Future<bool> disableAppBlocking(List<String> bundleIds) async {
    try {
      final result = await _channel.invokeMethod('disableAppBlocking', {
        'bundleIds': bundleIds,
      });
      return result as bool;
    } catch (e) {
      print('Error disabling app blocking: $e');
      return false;
    }
  }

  /// Enable blocking for the given domains
  Future<bool> enableDomainBlocking(List<String> domains) async {
    try {
      final result = await _channel.invokeMethod('enableDomainBlocking', {
        'domains': domains,
      });
      return result as bool;
    } catch (e) {
      print('Error enabling domain blocking: $e');
      return false;
    }
  }

  /// Disable blocking for the given domains
  Future<bool> disableDomainBlocking(List<String> domains) async {
    try {
      final result = await _channel.invokeMethod('disableDomainBlocking', {
        'domains': domains,
      });
      return result as bool;
    } catch (e) {
      print('Error disabling domain blocking: $e');
      return false;
    }
  }

  /// Update all blocking rules at once
  Future<bool> updateAllRules(List<BlockRule> rules) async {
    final now = DateTime.now();
    final activeAppRules = rules
        .where((r) =>
            r.target.type == BlockTargetType.application && r.shouldBlock(now))
        .map((r) => r.target.identifier)
        .toList();

    final activeDomainRules = rules
        .where((r) =>
            r.target.type == BlockTargetType.domain && r.shouldBlock(now))
        .map((r) => r.target.identifier)
        .toList();

    try {
      final result = await _channel.invokeMethod('updateAllRules', {
        'apps': activeAppRules,
        'domains': activeDomainRules,
      });
      return result as bool;
    } catch (e) {
      print('Error updating all rules: $e');
      return false;
    }
  }

  /// Check if the platform supports native blocking
  Future<bool> isPlatformSupported() async {
    try {
      final result = await _channel.invokeMethod('isPlatformSupported');
      return result as bool;
    } catch (e) {
      print('Platform not supported or error: $e');
      return false;
    }
  }

  /// Request necessary permissions for blocking
  Future<bool> requestPermissions() async {
    try {
      final result = await _channel.invokeMethod('requestPermissions');
      return result as bool;
    } catch (e) {
      print('Error requesting permissions: $e');
      return false;
    }
  }
}
