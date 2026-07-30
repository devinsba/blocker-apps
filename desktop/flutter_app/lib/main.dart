import 'package:flutter/material.dart';
import 'package:blocker_shared/blocker_shared.dart';
import 'screens/home_screen.dart';

void main() {
  runApp(const DesktopBlockerApp());
}

class DesktopBlockerApp extends StatelessWidget {
  const DesktopBlockerApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: AppStrings.desktopTitle,
      theme: ThemeData(
        colorScheme: ColorScheme.fromSeed(seedColor: Colors.blueGrey),
        useMaterial3: true,
      ),
      home: const HomeScreen(),
    );
  }
}
