import 'package:flutter/material.dart';
import 'package:blocker_shared/blocker_shared.dart';
import 'add_block_rule_screen.dart';

class HomeScreen extends StatefulWidget {
  const HomeScreen({super.key});

  @override
  State<HomeScreen> createState() => _HomeScreenState();
}

class _HomeScreenState extends State<HomeScreen> {
  final BlockingService _blockingService = BlockingService();
  final BlockStorage _storage = BlockStorage();
  bool _isLoading = true;

  @override
  void initState() {
    super.initState();
    _loadRules();
  }

  Future<void> _loadRules() async {
    setState(() => _isLoading = true);
    await _storage.initialize();
    final rules = await _storage.loadRules();
    for (final rule in rules) {
      _blockingService.addRule(rule);
    }
    setState(() => _isLoading = false);
  }

  Future<void> _saveRules() async {
    await _storage.saveRules(_blockingService.getAllRules());
  }

  Future<void> _addRule() async {
    final rule = await Navigator.push<BlockRule>(
      context,
      MaterialPageRoute(
        builder: (context) => const AddBlockRuleScreen(),
      ),
    );

    if (rule != null) {
      setState(() {
        _blockingService.addRule(rule);
      });
      await _saveRules();
    }
  }

  Future<void> _deleteRule(String ruleId) async {
    setState(() {
      _blockingService.removeRule(ruleId);
    });
    await _saveRules();
  }

  @override
  Widget build(BuildContext context) {
    if (_isLoading) {
      return const Scaffold(
        body: Center(child: CircularProgressIndicator()),
      );
    }

    final rules = _blockingService.getAllRules();
    final appRules = rules.where((r) => r.target.type == BlockTargetType.application).toList();
    final domainRules = rules.where((r) => r.target.type == BlockTargetType.domain).toList();

    return Scaffold(
      appBar: AppBar(
        title: const Text('Blocker'),
        actions: [
          IconButton(
            icon: const Icon(Icons.add),
            onPressed: _addRule,
            tooltip: 'Add blocking rule',
          ),
        ],
      ),
      body: rules.isEmpty
          ? _buildEmptyState()
          : Row(
              children: [
                Expanded(
                  child: _buildRuleList('Blocked Apps', appRules, Icons.apps),
                ),
                const VerticalDivider(width: 1),
                Expanded(
                  child: _buildRuleList('Blocked Sites', domainRules, Icons.web),
                ),
              ],
            ),
    );
  }

  Widget _buildEmptyState() {
    return Center(
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          Icon(Icons.block, size: 64, color: Colors.grey[400]),
          const SizedBox(height: 16),
          Text(
            'No blocking rules yet',
            style: Theme.of(context).textTheme.headlineSmall,
          ),
          const SizedBox(height: 8),
          Text(
            'Click + to add apps or sites to block',
            style: Theme.of(context).textTheme.bodyMedium?.copyWith(
                  color: Colors.grey[600],
                ),
          ),
          const SizedBox(height: 24),
          ElevatedButton.icon(
            onPressed: _addRule,
            icon: const Icon(Icons.add),
            label: const Text('Add Blocking Rule'),
          ),
        ],
      ),
    );
  }

  Widget _buildRuleList(String title, List<BlockRule> rules, IconData icon) {
    return Column(
      children: [
        Padding(
          padding: const EdgeInsets.all(16.0),
          child: Row(
            children: [
              Icon(icon, size: 20),
              const SizedBox(width: 8),
              Text(
                title,
                style: Theme.of(context).textTheme.titleMedium?.copyWith(
                      fontWeight: FontWeight.bold,
                    ),
              ),
              const Spacer(),
              Text(
                '${rules.length}',
                style: Theme.of(context).textTheme.bodySmall?.copyWith(
                      color: Colors.grey[600],
                    ),
              ),
            ],
          ),
        ),
        const Divider(height: 1),
        Expanded(
          child: rules.isEmpty
              ? Center(
                  child: Text(
                    'No rules',
                    style: TextStyle(color: Colors.grey[500]),
                  ),
                )
              : ListView.builder(
                  itemCount: rules.length,
                  itemBuilder: (context, index) {
                    final rule = rules[index];
                    return _buildRuleCard(rule);
                  },
                ),
        ),
      ],
    );
  }

  Widget _buildRuleCard(BlockRule rule) {
    final isActive = rule.shouldBlock(DateTime.now());

    return Card(
      margin: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
      child: ListTile(
        leading: CircleAvatar(
          backgroundColor: isActive ? Colors.red[100] : Colors.grey[300],
          child: Icon(
            rule.target.type == BlockTargetType.application
                ? Icons.apps
                : Icons.web,
            color: isActive ? Colors.red[700] : Colors.grey[600],
          ),
        ),
        title: Text(
          rule.target.displayName,
          style: const TextStyle(fontWeight: FontWeight.bold),
        ),
        subtitle: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              rule.target.identifier,
              style: TextStyle(
                fontSize: 12,
                color: Colors.grey[600],
              ),
            ),
            const SizedBox(height: 4),
            Row(
              children: [
                Icon(
                  isActive ? Icons.block : Icons.schedule,
                  size: 14,
                  color: isActive ? Colors.red : Colors.grey[600],
                ),
                const SizedBox(width: 4),
                Text(
                  _getScheduleText(rule.schedule, isActive),
                  style: TextStyle(
                    fontSize: 12,
                    color: isActive ? Colors.red : Colors.grey[600],
                  ),
                ),
              ],
            ),
          ],
        ),
        trailing: IconButton(
          icon: const Icon(Icons.delete_outline),
          onPressed: () => _showDeleteConfirmation(rule),
          tooltip: 'Delete rule',
        ),
      ),
    );
  }

  String _getScheduleText(BlockSchedule schedule, bool isActive) {
    if (!schedule.enabled) return 'Disabled';
    if (isActive) return 'Blocking now';

    switch (schedule.type) {
      case ScheduleType.always:
        return 'Always blocked';
      case ScheduleType.timeRange:
        return '${schedule.startTime} - ${schedule.endTime}';
      case ScheduleType.scheduled:
        final days = schedule.daysOfWeek?.map((d) => _getDayName(d)).join(', ') ?? '';
        return '$days ${schedule.startTime} - ${schedule.endTime}';
    }
  }

  String _getDayName(int day) {
    const days = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];
    return days[day - 1];
  }

  Future<void> _showDeleteConfirmation(BlockRule rule) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Delete Rule'),
        content: Text(
          'Are you sure you want to delete the blocking rule for "${rule.target.displayName}"?',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Cancel'),
          ),
          TextButton(
            onPressed: () => Navigator.pop(context, true),
            style: TextButton.styleFrom(foregroundColor: Colors.red),
            child: const Text('Delete'),
          ),
        ],
      ),
    );

    if (confirmed == true) {
      await _deleteRule(rule.id);
    }
  }
}
