import 'block_target.dart';
import 'block_schedule.dart';

/// Represents a complete blocking rule (target + schedule)
class BlockRule {
  final String id;
  final BlockTarget target;
  final BlockSchedule schedule;
  final DateTime createdAt;
  final DateTime? updatedAt;

  BlockRule({
    required this.id,
    required this.target,
    required this.schedule,
    required this.createdAt,
    this.updatedAt,
  });

  /// Check if this rule should block the target right now
  bool shouldBlock(DateTime now) {
    return schedule.isActiveNow(now);
  }

  BlockRule copyWith({
    String? id,
    BlockTarget? target,
    BlockSchedule? schedule,
    DateTime? createdAt,
    DateTime? updatedAt,
  }) {
    return BlockRule(
      id: id ?? this.id,
      target: target ?? this.target,
      schedule: schedule ?? this.schedule,
      createdAt: createdAt ?? this.createdAt,
      updatedAt: updatedAt ?? this.updatedAt,
    );
  }

  Map<String, dynamic> toJson() {
    return {
      'id': id,
      'target': target.toJson(),
      'schedule': schedule.toJson(),
      'createdAt': createdAt.toIso8601String(),
      'updatedAt': updatedAt?.toIso8601String(),
    };
  }

  factory BlockRule.fromJson(Map<String, dynamic> json) {
    return BlockRule(
      id: json['id'] as String,
      target: BlockTarget.fromJson(json['target'] as Map<String, dynamic>),
      schedule: BlockSchedule.fromJson(json['schedule'] as Map<String, dynamic>),
      createdAt: DateTime.parse(json['createdAt'] as String),
      updatedAt: json['updatedAt'] != null
          ? DateTime.parse(json['updatedAt'] as String)
          : null,
    );
  }

  @override
  bool operator ==(Object other) {
    if (identical(this, other)) return true;
    return other is BlockRule && other.id == id;
  }

  @override
  int get hashCode => id.hashCode;
}
