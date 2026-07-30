import 'package:flutter/material.dart';
import 'package:blocker_shared/blocker_shared.dart';

class AddBlockRuleScreen extends StatefulWidget {
  const AddBlockRuleScreen({super.key});

  @override
  State<AddBlockRuleScreen> createState() => _AddBlockRuleScreenState();
}

class _AddBlockRuleScreenState extends State<AddBlockRuleScreen> {
  final _formKey = GlobalKey<FormState>();
  final _displayNameController = TextEditingController();
  final _identifierController = TextEditingController();

  BlockTargetType _targetType = BlockTargetType.application;
  ScheduleType _scheduleType = ScheduleType.always;
  TimeOfDay? _startTime;
  TimeOfDay? _endTime;
  Set<int> _selectedDays = {};

  @override
  void dispose() {
    _displayNameController.dispose();
    _identifierController.dispose();
    super.dispose();
  }

  Future<void> _selectTime(BuildContext context, bool isStart) async {
    final time = await showTimePicker(
      context: context,
      initialTime: const TimeOfDay(hour: 9, minute: 0),
    );

    if (time != null) {
      setState(() {
        final customTime = TimeOfDay(hour: time.hour, minute: time.minute);
        if (isStart) {
          _startTime = customTime;
        } else {
          _endTime = customTime;
        }
      });
    }
  }

  void _createRule() {
    if (!_formKey.currentState!.validate()) return;

    if (_scheduleType != ScheduleType.always) {
      if (_startTime == null || _endTime == null) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('Please select start and end times')),
        );
        return;
      }
    }

    if (_scheduleType == ScheduleType.scheduled && _selectedDays.isEmpty) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('Please select at least one day')),
      );
      return;
    }

    final target = BlockTarget(
      id: DateTime.now().millisecondsSinceEpoch.toString(),
      type: _targetType,
      identifier: _identifierController.text.trim(),
      displayName: _displayNameController.text.trim(),
    );

    final schedule = BlockSchedule(
      id: DateTime.now().millisecondsSinceEpoch.toString(),
      name: 'Default Schedule',
      enabled: true,
      type: _scheduleType,
      daysOfWeek: _scheduleType == ScheduleType.scheduled
          ? _selectedDays.toList()..sort()
          : null,
      startTime: _scheduleType != ScheduleType.always ? _startTime : null,
      endTime: _scheduleType != ScheduleType.always ? _endTime : null,
    );

    final rule = BlockRule(
      id: DateTime.now().millisecondsSinceEpoch.toString(),
      target: target,
      schedule: schedule,
      createdAt: DateTime.now(),
    );

    Navigator.pop(context, rule);
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Add Blocking Rule'),
      ),
      body: Form(
        key: _formKey,
        child: ListView(
          padding: const EdgeInsets.all(24),
          children: [
            _buildTargetTypeSection(),
            const SizedBox(height: 24),
            _buildTargetDetailsSection(),
            const SizedBox(height: 24),
            _buildScheduleSection(),
            const SizedBox(height: 32),
            _buildActionButtons(),
          ],
        ),
      ),
    );
  }

  Widget _buildTargetTypeSection() {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'What to Block',
              style: Theme.of(context).textTheme.titleMedium?.copyWith(
                    fontWeight: FontWeight.bold,
                  ),
            ),
            const SizedBox(height: 16),
            SegmentedButton<BlockTargetType>(
              segments: const [
                ButtonSegment(
                  value: BlockTargetType.application,
                  label: Text('Application'),
                  icon: Icon(Icons.apps),
                ),
                ButtonSegment(
                  value: BlockTargetType.domain,
                  label: Text('Website'),
                  icon: Icon(Icons.web),
                ),
              ],
              selected: {_targetType},
              onSelectionChanged: (Set<BlockTargetType> newSelection) {
                setState(() {
                  _targetType = newSelection.first;
                  _identifierController.clear();
                });
              },
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildTargetDetailsSection() {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            TextFormField(
              controller: _displayNameController,
              decoration: const InputDecoration(
                labelText: 'Display Name',
                hintText: 'e.g., Safari, YouTube',
                border: OutlineInputBorder(),
              ),
              validator: (value) {
                if (value == null || value.trim().isEmpty) {
                  return 'Please enter a display name';
                }
                return null;
              },
            ),
            const SizedBox(height: 16),
            TextFormField(
              controller: _identifierController,
              decoration: InputDecoration(
                labelText: _targetType == BlockTargetType.application
                    ? 'Bundle ID'
                    : 'Domain',
                hintText: _targetType == BlockTargetType.application
                    ? 'e.g., com.apple.Safari'
                    : 'e.g., youtube.com or *.youtube.com',
                border: const OutlineInputBorder(),
                helperText: _targetType == BlockTargetType.domain
                    ? 'Use *.domain.com to block all subdomains'
                    : null,
              ),
              validator: (value) {
                if (value == null || value.trim().isEmpty) {
                  return _targetType == BlockTargetType.application
                      ? 'Please enter a bundle ID'
                      : 'Please enter a domain';
                }
                return null;
              },
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildScheduleSection() {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'When to Block',
              style: Theme.of(context).textTheme.titleMedium?.copyWith(
                    fontWeight: FontWeight.bold,
                  ),
            ),
            const SizedBox(height: 16),
            SegmentedButton<ScheduleType>(
              segments: const [
                ButtonSegment(
                  value: ScheduleType.always,
                  label: Text('Always'),
                ),
                ButtonSegment(
                  value: ScheduleType.timeRange,
                  label: Text('Time Range'),
                ),
                ButtonSegment(
                  value: ScheduleType.scheduled,
                  label: Text('Scheduled'),
                ),
              ],
              selected: {_scheduleType},
              onSelectionChanged: (Set<ScheduleType> newSelection) {
                setState(() {
                  _scheduleType = newSelection.first;
                });
              },
            ),
            if (_scheduleType != ScheduleType.always) ...[
              const SizedBox(height: 16),
              _buildTimeRangeSelector(),
            ],
            if (_scheduleType == ScheduleType.scheduled) ...[
              const SizedBox(height: 16),
              _buildDaySelector(),
            ],
          ],
        ),
      ),
    );
  }

  Widget _buildTimeRangeSelector() {
    return Row(
      children: [
        Expanded(
          child: OutlinedButton.icon(
            onPressed: () => _selectTime(context, true),
            icon: const Icon(Icons.access_time),
            label: Text(_startTime?.toString() ?? 'Start Time'),
          ),
        ),
        const SizedBox(width: 16),
        Expanded(
          child: OutlinedButton.icon(
            onPressed: () => _selectTime(context, false),
            icon: const Icon(Icons.access_time),
            label: Text(_endTime?.toString() ?? 'End Time'),
          ),
        ),
      ],
    );
  }

  Widget _buildDaySelector() {
    const days = [
      (1, 'Mon'),
      (2, 'Tue'),
      (3, 'Wed'),
      (4, 'Thu'),
      (5, 'Fri'),
      (6, 'Sat'),
      (7, 'Sun'),
    ];

    return Wrap(
      spacing: 8,
      children: days.map((day) {
        final isSelected = _selectedDays.contains(day.$1);
        return FilterChip(
          label: Text(day.$2),
          selected: isSelected,
          onSelected: (selected) {
            setState(() {
              if (selected) {
                _selectedDays.add(day.$1);
              } else {
                _selectedDays.remove(day.$1);
              }
            });
          },
        );
      }).toList(),
    );
  }

  Widget _buildActionButtons() {
    return Row(
      mainAxisAlignment: MainAxisAlignment.end,
      children: [
        OutlinedButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        const SizedBox(width: 16),
        ElevatedButton.icon(
          onPressed: _createRule,
          icon: const Icon(Icons.add),
          label: const Text('Add Rule'),
        ),
      ],
    );
  }
}
