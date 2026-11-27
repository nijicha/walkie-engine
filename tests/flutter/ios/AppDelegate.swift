// Fixture for tests/mcp/flutter.ts: the app asks for contacts on start (a permission prompt the
// recording must never show), through the "wt/contacts" channel.
import Contacts
import Flutter
import UIKit

@main
@objc class AppDelegate: FlutterAppDelegate, FlutterImplicitEngineDelegate {
  override func application(
    _ application: UIApplication,
    didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
  ) -> Bool {
    return super.application(application, didFinishLaunchingWithOptions: launchOptions)
  }

  func didInitializeImplicitFlutterEngine(_ engineBridge: FlutterImplicitEngineBridge) {
    GeneratedPluginRegistrant.register(with: engineBridge.pluginRegistry)
    guard let registrar = engineBridge.pluginRegistry.registrar(forPlugin: "WtContacts") else { return }
    FlutterMethodChannel(name: "wt/contacts", binaryMessenger: registrar.messenger())
      .setMethodCallHandler { _, result in
        CNContactStore().requestAccess(for: .contacts) { granted, _ in
          DispatchQueue.main.async { result(granted) }
        }
      }
  }
}
