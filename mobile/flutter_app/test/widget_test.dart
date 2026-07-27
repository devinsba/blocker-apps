import 'package:flutter_test/flutter_test.dart';

import 'package:flutter_app/main.dart';

void main() {
  testWidgets('renders skeleton text', (tester) async {
    await tester.pumpWidget(const BlockerApp());

    expect(find.text('Blocker Apps Skeleton'), findsOneWidget);
  });
}
