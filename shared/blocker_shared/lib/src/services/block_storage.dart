import 'dart:convert';
import 'dart:io';
import 'package:path_provider/path_provider.dart';
import '../models/block_rule.dart';

/// Handles persistence of blocking rules
class BlockStorage {
  static const String _fileName = 'block_rules.json';
  File? _storageFile;

  /// Initialize storage
  Future<void> initialize() async {
    final directory = await getApplicationDocumentsDirectory();
    _storageFile = File('${directory.path}/$_fileName');
  }

  /// Save all rules to disk
  Future<void> saveRules(List<BlockRule> rules) async {
    if (_storageFile == null) {
      await initialize();
    }

    final jsonList = rules.map((rule) => rule.toJson()).toList();
    final jsonString = jsonEncode(jsonList);
    await _storageFile!.writeAsString(jsonString);
  }

  /// Load all rules from disk
  Future<List<BlockRule>> loadRules() async {
    if (_storageFile == null) {
      await initialize();
    }

    if (!await _storageFile!.exists()) {
      return [];
    }

    try {
      final jsonString = await _storageFile!.readAsString();
      final jsonList = jsonDecode(jsonString) as List<dynamic>;
      return jsonList
          .map((json) => BlockRule.fromJson(json as Map<String, dynamic>))
          .toList();
    } catch (e) {
      // If file is corrupted, return empty list
      return [];
    }
  }

  /// Clear all saved rules
  Future<void> clearRules() async {
    if (_storageFile == null) {
      await initialize();
    }

    if (await _storageFile!.exists()) {
      await _storageFile!.delete();
    }
  }

  /// Export rules to a JSON string
  Future<String> exportRules(List<BlockRule> rules) async {
    final jsonList = rules.map((rule) => rule.toJson()).toList();
    return jsonEncode(jsonList);
  }

  /// Import rules from a JSON string
  Future<List<BlockRule>> importRules(String jsonString) async {
    try {
      final jsonList = jsonDecode(jsonString) as List<dynamic>;
      return jsonList
          .map((json) => BlockRule.fromJson(json as Map<String, dynamic>))
          .toList();
    } catch (e) {
      throw Exception('Failed to import rules: Invalid JSON format');
    }
  }
}
