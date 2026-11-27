// Fixture app for tests/mcp/flutter.ts: an email field, a Save button and a long list; a dark
// settings screen (a veiled screen change) whose switch shows a banner that pushes the switch row
// down (the box follows it). On start it asks for contacts (native code in ios/ and android/): the
// recording must grant it up front, so no permission prompt shows, and the test expects
// "Contacts: allowed".
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

void main() {
  runApp(const MaterialApp(debugShowCheckedModeBanner: false, home: Home()));
}

class Home extends StatefulWidget {
  const Home({super.key});
  @override
  State<Home> createState() => _HomeState();
}

class _HomeState extends State<Home> {
  final _email = TextEditingController();
  String? _saved;
  bool? _contacts;

  @override
  void initState() {
    super.initState();
    const MethodChannel('wt/contacts')
        .invokeMethod<bool>('request')
        .then((ok) => setState(() => _contacts = ok ?? false))
        .catchError((_) => setState(() => _contacts = false));
  }

  @override
  Widget build(BuildContext context) => Scaffold(
        appBar: AppBar(title: const Text('Notifications'), actions: [
          IconButton(
            key: const Key('settings'),
            icon: const Icon(Icons.settings),
            onPressed: () => Navigator.of(context).push(MaterialPageRoute(builder: (_) => const Settings())),
          ),
        ]),
        body: ListView(padding: const EdgeInsets.all(16), children: [
          if (_contacts != null) Text('Contacts: ${_contacts! ? 'allowed' : 'not allowed'}'),
          TextField(controller: _email, decoration: const InputDecoration(labelText: 'Email')),
          const SizedBox(height: 12),
          FilledButton(
            onPressed: () => setState(() => _saved = _email.text),
            child: const Text('Save'),
          ),
          if (_saved != null) Text('Saved: $_saved'),
          for (var i = 1; i <= 40; i++) ListTile(title: Text('Wristband $i')),
        ]),
      );
}

class Settings extends StatefulWidget {
  const Settings({super.key});
  @override
  State<Settings> createState() => _SettingsState();
}

class _SettingsState extends State<Settings> {
  bool _alerts = false;
  @override
  Widget build(BuildContext context) => Theme(
        data: ThemeData.dark(),
        child: Scaffold(
          appBar: AppBar(title: const Text('Settings')),
          body: ListView(children: [
            AnimatedSize(
              duration: const Duration(milliseconds: 400),
              child: _alerts
                  ? Container(
                      width: double.infinity,
                      color: Colors.green.shade800,
                      padding: const EdgeInsets.all(24),
                      child: const Text('Email alerts are on'),
                    )
                  : const SizedBox(width: double.infinity),
            ),
            SwitchListTile(
              key: const Key('alerts'),
              title: const Text('Email alerts'),
              value: _alerts,
              onChanged: (v) => setState(() => _alerts = v),
            ),
            const Padding(
              padding: EdgeInsets.all(16),
              child: Text('We send one email for each wristband that is registered to your account, '
                  'and one when a wristband is moved to another person.'),
            ),
          ]),
        ),
      );
}
