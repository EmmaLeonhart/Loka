/// Default / web implementation.
///
/// A browser has no process environment, so there is no
/// `LOKA_ENDPOINT` to read — return null and let
/// [ConnectionProvider] fall back to its default endpoint (or the
/// auth screen). The matching desktop/mobile implementation in
/// `env_endpoint_io.dart` is selected via a conditional import so
/// `dart:io` never reaches the web build.
String? readEnvEndpoint() => null;
