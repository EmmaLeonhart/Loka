import 'dart:io' show Platform;

/// Desktop / mobile implementation.
///
/// Reads `LOKA_ENDPOINT` (set by the MCP `launch_studio` tool, or by
/// `flutter run` with the env var exported). Selected over the web
/// stub via the conditional import in `main.dart`, so `dart:io` is
/// only compiled into non-web targets.
String? readEnvEndpoint() => Platform.environment['LOKA_ENDPOINT'];
