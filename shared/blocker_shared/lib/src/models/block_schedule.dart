/// Represents when blocking should be active
class BlockSchedule {
  final String id;
  final String name;
  final bool enabled;
  final ScheduleType type;
  final List<int>? daysOfWeek; // 1-7 (Monday-Sunday)
  final TimeOfDay? startTime;
  final TimeOfDay? endTime;

  BlockSchedule({
    required this.id,
    required this.name,
    required this.enabled,
    required this.type,
    this.daysOfWeek,
    this.startTime,
    this.endTime,
  });

  bool isActiveNow(DateTime now) {
    if (!enabled) return false;

    switch (type) {
      case ScheduleType.always:
        return true;
      case ScheduleType.timeRange:
        return _isInTimeRange(now);
      case ScheduleType.scheduled:
        return _isInSchedule(now);
    }
  }

  bool _isInTimeRange(DateTime now) {
    if (startTime == null || endTime == null) return false;

    final currentMinutes = now.hour * 60 + now.minute;
    final startMinutes = startTime!.hour * 60 + startTime!.minute;
    final endMinutes = endTime!.hour * 60 + endTime!.minute;

    return currentMinutes >= startMinutes && currentMinutes <= endMinutes;
  }

  bool _isInSchedule(DateTime now) {
    if (daysOfWeek == null || !daysOfWeek!.contains(now.weekday)) {
      return false;
    }
    return _isInTimeRange(now);
  }

  Map<String, dynamic> toJson() {
    return {
      'id': id,
      'name': name,
      'enabled': enabled,
      'type': type.name,
      'daysOfWeek': daysOfWeek,
      'startTime': startTime?.toJson(),
      'endTime': endTime?.toJson(),
    };
  }

  factory BlockSchedule.fromJson(Map<String, dynamic> json) {
    return BlockSchedule(
      id: json['id'] as String,
      name: json['name'] as String,
      enabled: json['enabled'] as bool,
      type: ScheduleType.values.firstWhere((e) => e.name == json['type']),
      daysOfWeek: (json['daysOfWeek'] as List<dynamic>?)?.cast<int>(),
      startTime: json['startTime'] != null
          ? TimeOfDay.fromJson(json['startTime'] as Map<String, dynamic>)
          : null,
      endTime: json['endTime'] != null
          ? TimeOfDay.fromJson(json['endTime'] as Map<String, dynamic>)
          : null,
    );
  }
}

enum ScheduleType {
  always, // Block 24/7
  timeRange, // Block during specific time range
  scheduled, // Block on specific days and times
}

class TimeOfDay {
  final int hour; // 0-23
  final int minute; // 0-59

  TimeOfDay({required this.hour, required this.minute});

  Map<String, dynamic> toJson() {
    return {
      'hour': hour,
      'minute': minute,
    };
  }

  factory TimeOfDay.fromJson(Map<String, dynamic> json) {
    return TimeOfDay(
      hour: json['hour'] as int,
      minute: json['minute'] as int,
    );
  }

  @override
  String toString() {
    final h = hour.toString().padLeft(2, '0');
    final m = minute.toString().padLeft(2, '0');
    return '$h:$m';
  }
}
