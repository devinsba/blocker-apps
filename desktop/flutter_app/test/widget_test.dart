import 'package:flutter_test/flutter_test.dart';

import 'package:desktop_flutter_app/main.dart';

void main() {
  testWidgets('displays desktop skeleton text', (tester) async {
    await tester.pumpWidget(const DesktopBlockerApp());

    expect(find.text('Blocker Apps Desktop Skeleton'), findsOneWidget);
  });
}
