// Fixture for tests/mcp/flutter.ts: the app asks for contacts on start (a permission prompt the
// recording must never show), through the "wt/contacts" channel.
package com.example.wt_fixture

import android.Manifest
import android.content.pm.PackageManager
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

class MainActivity : FlutterActivity() {
  override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
    super.configureFlutterEngine(flutterEngine)
    MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "wt/contacts").setMethodCallHandler { _, result ->
      if (checkSelfPermission(Manifest.permission.READ_CONTACTS) == PackageManager.PERMISSION_GRANTED) {
        result.success(true)
      } else {
        requestPermissions(arrayOf(Manifest.permission.READ_CONTACTS), 1)
        result.success(false)
      }
    }
  }
}
