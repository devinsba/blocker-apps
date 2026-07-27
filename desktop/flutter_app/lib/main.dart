import 'package:flutter/material.dart';

void main() {
  runApp(const DesktopBlockerApp());
}

class DesktopBlockerApp extends StatelessWidget {
  const DesktopBlockerApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Blocker Apps Desktop Skeleton',
      theme: ThemeData(
        colorScheme: ColorScheme.fromSeed(seedColor: Colors.blueGrey),
        useMaterial3: true,
      ),
      home: const Scaffold(
        body: Center(
          child: Text('Blocker Apps Desktop Skeleton'),
        ),
      ),
    );
  }
}
