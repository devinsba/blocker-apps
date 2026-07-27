import 'package:flutter/material.dart';
import 'package:blocker_shared/blocker_shared.dart';

void main() {
  runApp(const BlockerApp());
}

class BlockerApp extends StatelessWidget {
  const BlockerApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: AppStrings.appTitle,
      theme: ThemeData(
        colorScheme: ColorScheme.fromSeed(seedColor: Colors.indigo),
        useMaterial3: true,
      ),
      home: const Scaffold(
        body: Center(
          child: Text(AppStrings.mobileSkeletonText),
        ),
      ),
    );
  }
}
