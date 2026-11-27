// Fixture for tests/mcp/flutter.ts: the walkie of tests/flutter/main.dart.
import 'package:flutter/widgets.dart';
import 'package:flutter/material.dart' show TextField, FilledButton, ListView, SwitchListTile;
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:wt_fixture/main.dart' as app;
import 'walkie_kit.dart';

void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  binding.framePolicy = LiveTestWidgetsFlutterBindingFramePolicy.fullyLive; // smooth on video

  testWidgets('walkie', (tester) async {
    app.main();
    final demo = Demo.create(tester);
    await demo.wait(1500); // first frames
    // record granted the app's permissions up front: no prompt, and the app has access.
    expect(find.text('Contacts: allowed'), findsOneWidget);
    await demo.intro('<div class="tag"><span>FLUTTER</span><span>iOS</span></div>'
        '<h1>ตั้งค่าการแจ้งเตือน ทางอีเมล</h1><p>ตัวอย่างจากแอปมือถือ</p>', 'n0');

    final email = find.widgetWithText(TextField, 'Email');
    await demo.highlight(email, zoom: 1.6); // Save stays in view
    await demo.stepCaption('กรอกอีเมล แล้วกด <b>“Save”</b>');
    await demo.say('n1');
    await demo.tap(email);
    await demo.type(email, 'name@example.com');
    await demo.tap(find.widgetWithText(FilledButton, 'Save'));
    expect(find.text('Saved: name@example.com'), findsOneWidget);
    FocusManager.instance.primaryFocus?.unfocus();
    await demo.untilQuiet();

    await demo.highlight(null);
    await demo.stepCaption('เลื่อนลงไปดู <b>Wristband 12</b>');
    await demo.say('n2');
    await demo.scroll(find.byType(ListView), 450);
    await demo.highlight(find.text('Wristband 12'), zoom: true);
    expect(find.text('Wristband 12'), findsOneWidget);
    await demo.untilQuiet();
    await demo.pause(1500); // let the zoomed row be seen

    // A dark screen behind the veil; a spotlight on the switch; the banner pushes the switch row
    // down and the box follows it.
    await demo.navigate(() => tester.tap(find.byKey(const Key('settings'))));
    final alerts = find.byType(SwitchListTile);
    await demo.highlight(alerts, spotlight: true);
    await demo.stepCaption('เปิด <b>Email alerts</b>');
    await demo.say('n3');
    await demo.tap(alerts);
    await demo.wait(800); // the banner grows; the box follows the row
    expect(find.text('Email alerts are on'), findsOneWidget);
    await demo.untilQuiet();

    await demo.summary('<div class="tag"><span>สรุป</span></div><ul><li>กรอกอีเมล แล้วกด Save</li>'
        '<li>รายการ ริสต์แบนด์ เลื่อนดูได้</li><li class="warn">ตัวอย่าง ไม่ใช่แอปจริง</li></ul>', 'n9');
    await demo.outro();
    await demo.finish();
  });
}
