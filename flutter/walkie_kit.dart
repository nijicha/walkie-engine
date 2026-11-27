// Walkie kit for Flutter on a phone simulator or emulator: the Dart twin of the engine's Demo
// (scripts/demo.ts). Copy it into the app's integration_test/ folder; a walkie test imports it.
//
// The layers are the same as on the web. The page layer is the device's screen recording, made on
// the host. The overlay is logged as one `WTEV {json}` line per event on the test's output, with
// coordinates in screen points, and `wt device collect` turns the lines into events.json. A zoom
// is a `camera` event: the build zooms the recording (it has 3-4x the pixels it is shown at), and
// the box and the taps are logged where the element sits in the zoomed view.
//
// `record` runs it with --dart-define=WALKIE_CONFIG (wt device config): the clip lengths,
// the zoom and the profile's sequence (which cards are on, step numbers).
// ignore_for_file: avoid_print (the overlay events go out on the test output)
import 'dart:convert';
import 'dart:math' as math;

import 'package:flutter/widgets.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';

const _gapMs = 500; // silence after each clip before the next one may start
const _markerMs = 600; // how long the sync square shows; wt mix cuts right after it
const _veilMs = 600; // each half of a screen change's fade (overlay.ts VEIL)

class Demo {
  Demo._(this.tester, Map<String, dynamic> config)
      : durations = (config['durations'] ?? <String, dynamic>{}) as Map<String, dynamic>,
        zoomScale = ((config['zoom']?['scale'] ?? 2.2) as num).toDouble(),
        zoomMs = ((config['zoom']?['ms'] ?? 600) as num).toInt(),
        fadeMs = ((config['fade_ms'] ?? 600) as num).toInt(),
        sequence = (config['sequence'] ??
            <String, dynamic>{'intro': true, 'summary': true, 'step_numbers': true}) as Map<String, dynamic>;

  final WidgetTester tester;
  final Map<String, dynamic> durations;
  final double zoomScale;
  final int zoomMs, fadeMs;
  final Map<String, dynamic> sequence; // the profile's sequence.json
  bool _on(String card) => sequence[card] == true;

  int _t0 = 0, _deadline = 0, _step = 0;
  bool _card = false;
  Finder? _tracked; // the highlighted widget; the box follows it while it moves
  Rect? _shown; // where the box is drawn, in the video's view of the screen
  bool _following = false;
  final _counts = <String, int>{};
  final timeline = <Map<String, Object>>[];
  late Rect _cam = Offset.zero & _screen; // the part of the screen the video shows

  static Demo create(WidgetTester tester) {
    const raw = String.fromEnvironment('WALKIE_CONFIG', defaultValue: '{}');
    return Demo._(tester, jsonDecode(raw) as Map<String, dynamic>);
  }

  Size get _screen => tester.view.physicalSize / tester.view.devicePixelRatio;
  int get _now => DateTime.now().millisecondsSinceEpoch;
  bool get _zoomed => _cam != Offset.zero & _screen;

  void _emit(Map<String, Object?> e) => print('WTEV ${jsonEncode({'ts': _now, ...e})}');
  String _id(String kind) => '$kind-${_counts[kind] = (_counts[kind] ?? 0) + 1}';
  Map<String, double> _json(Rect r) => {'x': r.left, 'y': r.top, 'w': r.width, 'h': r.height};

  // Screen points -> where they show in the video's view of the screen (after a zoom).
  Rect _view(Rect r) {
    final s = _screen.width / _cam.width;
    return Rect.fromLTWH((r.left - _cam.left) * s, (r.top - _cam.top) * s, r.width * s, r.height * s);
  }

  /// Waits in real time, then draws a frame. (tester.pump(duration) can take several times its
  /// duration on a device while a text field has focus.)
  Future<void> wait(int ms) async {
    await Future<void>.delayed(Duration(milliseconds: ms));
    await tester.pump();
  }

  /// Lets animations run out, for at most maxMs. Never pumpAndSettle on a device: a focused text
  /// field's blinking cursor keeps scheduling frames, and it waits for seconds.
  Future<void> settle({int maxMs = 600}) async {
    for (var left = maxMs; left > 0 && tester.binding.hasScheduledFrame; left -= 50) {
      await wait(50);
    }
  }

  // ---- timing -------------------------------------------------------------------------------

  /// Time zero: a magenta square in the corner; the build cuts the recording right after it.
  Future<void> start() async {
    final overlay = tester.state<OverlayState>(find.byType(Overlay).first);
    final entry = OverlayEntry(
        builder: (_) => const Positioned(
            left: 0, top: 0, width: 24, height: 24, child: ColoredBox(color: Color(0xFFFF00FF))));
    overlay.insert(entry);
    await wait(_markerMs);
    entry.remove();
    await wait(50);
    _t0 = _now;
    _emit({'type': 'cursor-visible', 'on': false}); // touch: the dot shows only on a tap
  }

  Future<void> say(String name) async {
    final d = durations[name];
    if (d == null) throw StateError('no voice clip $name: run wt tts');
    await untilQuiet();
    timeline.add({'name': name, 'start': (_now - _t0) / 1000});
    _deadline = _now + ((d as num) * 1000).round() + _gapMs;
  }

  Future<void> untilQuiet() async {
    final left = _deadline - _now;
    if (left > 0) await wait(left);
  }

  Future<void> pause([int ms = 2200]) => wait(ms);

  // ---- sequence -----------------------------------------------------------------------------

  /// Opening card (`.tag` with a <span> per item, h1, p). With intro off in the profile's
  /// sequence, only starts the clock.
  Future<void> intro(String html, [String clip = 'n0']) async {
    if (!_on('intro')) return start();
    _emit({'type': 'card', 'id': _id('card'), 'html': html, 'kind': 'intro', 'instant': true});
    _card = true;
    await start();
    await wait(600);
    await say(clip);
    await untilQuiet();
    await hideCard();
  }

  /// Short card between parts ("ก่อนแก้" / "หลังแก้"). Only when chapter is on.
  Future<void> chapter(String html, [String? clip]) async {
    if (!_on('chapter')) return;
    await _clearStage();
    await _showCard(html, 'chapter');
    if (clip != null) {
      await say(clip);
      await untilQuiet();
    } else {
      await wait(((sequence['chapter_ms'] ?? 1800) as num).toInt());
    }
    await hideCard();
  }

  /// The profile's outro card. Only when outro is on.
  Future<void> outro([String? clip]) async {
    final html = sequence['outro_html'] as String?;
    if (!_on('outro') || html == null || html.isEmpty) return;
    await hideCard();
    await _showCard(html, 'outro');
    if (clip != null) {
      await say(clip);
      await untilQuiet();
    }
    await wait(1500);
  }

  /// Closing card: `ul > li` (li.warn for a caution). Only when summary is on.
  Future<void> summary(String html, String clip) async {
    if (!_on('summary')) return;
    await _clearStage();
    await _showCard(html, 'summary');
    await wait(400);
    await say(clip);
    await untilQuiet();
    await wait(1500);
  }

  Future<void> _showCard(String html, String kind) async {
    _emit({'type': 'card', 'id': _id('card'), 'html': html, 'kind': kind, 'instant': false});
    _card = true;
    await wait(fadeMs);
  }

  Future<void> hideCard() async {
    if (!_card) return;
    _emit({'type': 'card-hide'});
    await wait(fadeMs);
    _emit({'type': 'card-remove'});
    _card = false;
  }

  /// Writes nothing itself: prints the timeline and the screen size for `wt device collect`.
  Future<void> finish() async {
    await untilQuiet();
    print('WTTIMELINE ${jsonEncode(timeline)}');
    print('WTVIEWPORT ${jsonEncode({'width': _screen.width, 'height': _screen.height, 't0': _t0})}');
  }

  // ---- overlay ------------------------------------------------------------------------------

  Future<void> caption(String html) async =>
      _emit({'type': 'caption', if (html.isNotEmpty) 'id': _id('caption'), 'html': html});

  /// Caption for the next step, numbered when the sequence has step_numbers. `<b>` is the emphasis.
  Future<void> stepCaption(String html) async {
    _step++;
    final number = sequence['step_numbers'] == false ? '' : '<span class="step">$_step</span>';
    _emit({'type': 'caption', 'id': 'step-$_step', 'html': '$number$html'});
  }

  /// A screen change behind a veil: the phone fades to the card color, `go` runs (a tap that
  /// opens a screen, a Navigator call), and the new screen fades in. Use it for every change of
  /// screen, or a dark screen cuts straight to a light one (a FLASH). Caption and box go with it.
  Future<void> navigate(Future<void> Function() go) async {
    await _unzoom();
    _emit({'type': 'veil', 'on': true});
    await wait(_veilMs);
    _emit({'type': 'caption', 'html': ''});
    _hideBox();
    await go();
    await settle(maxMs: 1500); // the route transition, under the veil
    _emit({'type': 'veil', 'on': false});
    await wait(_veilMs + 100);
  }

  /// Highlight box around one widget; null hides it. `zoom: true` zooms the video onto it (the
  /// profile's scale, or a number), capped so the widget's height fits. A widget wider than the
  /// zoomed view (most rows and fields on a phone) is shown from its left edge, where its label
  /// and text start, and the box is cut at the view's edges. The next highlight zooms back out.
  Future<void> highlight(Finder? target, {Object zoom = false, bool spotlight = false}) async {
    await _unzoom();
    _hideBox();
    if (target == null) return;
    await _reveal(target);
    final r = _rectOf(target);
    if (zoom != false) await _zoomTo(r, zoom is num ? zoom.toDouble() : zoomScale);
    _shown = _boxRect(target);
    _emit({'type': 'box', 'rect': _json(_shown!), 'spot': spotlight});
    _tracked = target;
    _follow();
  }

  Rect _boxRect(Finder target) => _view(_rectOf(target)).intersect(Offset.zero & _screen);

  void _hideBox() {
    if (_shown != null) _emit({'type': 'box', 'rect': null});
    _shown = null;
    _tracked = null;
  }

  // After every frame while a box shows: when its widget moved (a keyboard opening, a list
  // scrolling, an animation), the box moves with it at once (box-move, no glide).
  void _follow() {
    if (_following) return;
    _following = true;
    void tick(Duration _) {
      final target = _tracked, shown = _shown;
      if (target == null || shown == null) {
        _following = false;
        return;
      }
      final found = target.evaluate();
      final ro = found.length == 1 ? found.single.renderObject : null;
      if (ro is RenderBox && ro.attached && ro.hasSize) {
        final r = _boxRect(target);
        final moved = (r.left - shown.left).abs() + (r.top - shown.top).abs() +
            (r.width - shown.width).abs() + (r.height - shown.height).abs();
        if (moved > 0.5) {
          _emit({'type': 'box-move', 'rect': _json(r)});
          _shown = r;
        }
      }
      WidgetsBinding.instance.addPostFrameCallback(tick);
    }

    WidgetsBinding.instance.addPostFrameCallback(tick);
  }

  // The widget's place; for a Text, only the painted text (a Text in a row fills the row).
  // Reads the render object itself, not tester.getRect: the box follows its widget from a frame
  // callback, and the tester's guarded calls refuse to run while a pump is under way.
  Rect _rectOf(Finder target) {
    final ro = target.evaluate().single.renderObject! as RenderBox;
    final r = ro.localToGlobal(Offset.zero) & ro.size;
    if (ro is RenderParagraph) {
      // textSize is the full row under the default TextWidthBasis.parent; the glyphs' boxes are not.
      final length = ro.text.toPlainText(includeSemanticsLabels: false).length;
      final boxes = ro.getBoxesForSelection(TextSelection(baseOffset: 0, extentOffset: length));
      if (boxes.isEmpty) return r;
      final local = boxes.map((b) => b.toRect()).reduce((a, b) => a.expandToInclude(b));
      return local.shift(ro.localToGlobal(Offset.zero));
    }
    return r;
  }

  // Scrolls only when the widget is not in full view, smoothly, to a third of the way down. The
  // keyboard counts as covering the screen (in landscape it can cover most of it).
  Future<void> _reveal(Finder target, {bool strict = false}) async {
    final r = tester.getRect(target);
    final sc = _screen;
    final keyboard = tester.view.viewInsets.bottom / tester.view.devicePixelRatio;
    final bottom = math.min(strict ? sc.height : sc.height * 0.85, sc.height - keyboard - 8);
    final inView = r.top >= (strict ? 0 : sc.height * 0.1) && r.bottom <= bottom;
    if (!inView && Scrollable.maybeOf(tester.element(target)) != null) {
      final done = Scrollable.ensureVisible(tester.element(target),
          alignment: 0.35, duration: const Duration(milliseconds: 800), curve: Curves.easeInOut);
      await wait(850);
      await done;
    }
    await settle();
  }

  Future<void> _zoomTo(Rect r, double scale) async {
    final sc = _screen;
    final s = math.max(1.0, math.min(scale, sc.height * 0.6 / r.height));
    final w = sc.width / s, h = sc.height / s;
    // The widget a bit above the middle, never past an edge of the screen; a wide one from its left.
    final x = (r.width <= w * 0.92 ? r.center.dx - w / 2 : r.left - 12).clamp(0.0, sc.width - w);
    final y = (r.center.dy - h * 0.42).clamp(0.0, sc.height - h);
    _cam = Rect.fromLTWH(x, y, w, h);
    _emit({'type': 'camera', 'rect': _json(_cam), 'ms': zoomMs});
    await wait(zoomMs + 50);
  }

  Future<void> _unzoom() async {
    if (!_zoomed) return;
    _hideBox();
    _cam = Offset.zero & _screen;
    _emit({'type': 'camera', 'rect': _json(_cam), 'ms': zoomMs});
    await wait(zoomMs + 50);
  }

  Future<void> _clearStage() async {
    await caption('');
    await highlight(null);
  }

  // ---- acting -------------------------------------------------------------------------------

  /// A tap: scrolls the widget into view when it is not (under the keyboard, below the fold),
  /// then the dot appears on it, presses, and fades.
  Future<void> tap(Finder target) async {
    await _reveal(target, strict: true);
    final c = _view(tester.getRect(target)).center;
    _emit({'type': 'cursor', 'x': c.dx, 'y': c.dy});
    _emit({'type': 'cursor-visible', 'on': true});
    await wait(450);
    _emit({'type': 'press', 'down': true});
    await tester.tap(target);
    await wait(180);
    _emit({'type': 'press', 'down': false});
    await wait(300);
    _emit({'type': 'cursor-visible', 'on': false});
    await settle();
  }

  /// Types like a person, one character at a time.
  Future<void> type(Finder target, String text, {int msPerChar = 70}) async {
    for (var i = 1; i <= text.length; i++) {
      await tester.enterText(target, text.substring(0, i));
      await wait(msPerChar);
    }
  }

  /// Scrolls a list down by dy points, smoothly. Zooms out first. (Not a drag: the test binding
  /// draws a crosshair under a test finger while it is down, and it would be in the video.)
  Future<void> scroll(Finder scrollable, double dy, {int ms = 1200}) async {
    await _unzoom();
    _hideBox();
    final state = tester.state<ScrollableState>(
        find.descendant(of: scrollable, matching: find.byType(Scrollable), matchRoot: true).first);
    final p = state.position;
    final to = (p.pixels + dy).clamp(p.minScrollExtent, p.maxScrollExtent);
    final done = p.animateTo(to, duration: Duration(milliseconds: ms), curve: Curves.easeInOut);
    await wait(ms + 50);
    await done;
    await settle();
  }
}
