// Copy to <work_dir>/<name>/demo_test.dart. `record` copies it and the kit into the app's
// integration_test/walkie/ for the run and removes them after, so the app's own tests stay as
// they are. One scene per step; one narration clip per scene.
// Start the app and get past login or seed data before demo.intro(): the build cuts everything
// before it. Never call tester.pumpAndSettle() (see the kit): use demo.wait() or demo.settle().
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'package:APP/main.dart' as app; // the app's package name, from its pubspec.yaml

import 'walkie_kit.dart';

void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  binding.framePolicy = LiveTestWidgetsFlutterBindingFramePolicy.fullyLive; // every frame, for the video

  testWidgets('walkie', (tester) async {
    app.main();
    final demo = Demo.create(tester);
    await demo.wait(1500); // first frames

    // Cards and captions: in the narration's language (walkie:start's Voice line), UI words as the UI spells them.
    await demo.intro(
      '<div class="tag"><span>TICKET</span><span>PR #NNNN</span><span>FOR …</span></div>'
      '<h1>What changed</h1>'
      '<p>One short line</p>',
      'n0',
    );

    // Scene 1. Finders as in widget tests: find.text, find.byKey, find.widgetWithText ...
    final save = find.text('Save');
    await demo.highlight(save, zoom: true); // zoom: the phone shows small; zoom on small widgets
    await demo.stepCaption('Press <b>Save</b>'); // UI words in <b>, spelled as the UI shows them
    await demo.say('n1');
    await demo.tap(save);
    expect(find.text('Saved'), findsOneWidget); // assert the result, so a broken demo fails
    await demo.untilQuiet();

    await demo.summary(
      '<div class="tag"><span>SUMMARY</span><span>PR #NNNN</span></div>'
      '<ul><li>What changed</li><li class="warn">What to watch out for</li></ul>',
      'n9',
    );
    await demo.finish(); // prints the timeline and screen size for `record`
  });
}
