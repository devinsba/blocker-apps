/// Represents what should be blocked (an app or domain)
class BlockTarget {
  final String id;
  final BlockTargetType type;
  final String identifier; // Bundle ID for apps, domain for websites
  final String displayName;

  BlockTarget({
    required this.id,
    required this.type,
    required this.identifier,
    required this.displayName,
  });

  Map<String, dynamic> toJson() {
    return {
      'id': id,
      'type': type.name,
      'identifier': identifier,
      'displayName': displayName,
    };
  }

  factory BlockTarget.fromJson(Map<String, dynamic> json) {
    return BlockTarget(
      id: json['id'] as String,
      type: BlockTargetType.values.firstWhere(
        (e) => e.name == json['type'],
      ),
      identifier: json['identifier'] as String,
      displayName: json['displayName'] as String,
    );
  }

  @override
  bool operator ==(Object other) {
    if (identical(this, other)) return true;
    return other is BlockTarget &&
        other.id == id &&
        other.type == type &&
        other.identifier == identifier;
  }

  @override
  int get hashCode => Object.hash(id, type, identifier);
}

enum BlockTargetType {
  application, // Native apps
  domain, // Websites/domains
}
