# Android APK checklist

Desktop is the guaranteed demo path. Two Android phones is the stretch goal —
this checklist is written so anyone can produce the APK and run the demo.

Deploy the backend first and put it behind a
[Cloudflare tunnel](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/)
pointed at port 8080; phones reach the server at the tunnel URL
(HTTPS at the tunnel edge is what makes GPS + SSE work on real devices).

## One-time setup

1. **Android SDK + NDK** — install [Android Studio](https://developer.android.com/studio)
   (or the command-line tools) and, via *SDK Manager → SDK Tools*, tick:
   - Android SDK Platform (API 34), SDK Platform-Tools, SDK Build-Tools
   - **NDK (Side by side)** — current LTS version
   - CMake (if offered alongside the NDK)
2. **Rust Android targets** (on a typical 64-bit ARM phone):

   ```sh
   rustup target add aarch64-linux-android
   ```

3. **Environment variables** — put in your shell profile, with versions
   matching your SDK Manager choices:

   ```sh
   export ANDROID_HOME="$HOME/Android/Sdk"                 # macOS: ~/Library/Android/sdk
   export NDK_HOME="$ANDROID_HOME/ndk/<version>"           # e.g. 28.2.13676358
   export PATH="$PATH:$ANDROID_HOME/platform-tools"        # gives you `adb`
   ```

4. Java 17+ (bundled with Android Studio) — set `JAVA_HOME` if `cargo tauri`
   complains.

## Build the APK

```sh
nix develop                          # or your local Rust + cargo-tauri install
cargo tauri android init             # once: generates src-tauri/gen/android
cargo tauri android build --debug    # produces an installable debug APK
```

The APK lands under `src-tauri/gen/android/app/build/outputs/apk/universal/debug/`
(`--debug`; swap for `build` for a release APK, which needs signing setup).

## Install & demo on two phones

1. Enable *Developer options → USB debugging* on both phones, plug in via
   USB, accept the fingerprint dialog.
2. Install: `adb install <path-to>.apk` (or `adb install -s` to target a
   specific device when both are plugged in; `adb devices` lists them).
3. **Point the app at the server**: the app defaults to `http://127.0.0.1:8080`,
   which is wrong on a phone. In the app, set the server URL to the
   **tunnel URL** from the deployment step, e.g. `https://lana.example.com`.
4. Demo flow: phone A sends an SOS from the map; phone B (registered as a
   helper, nearby or on schedule) receives it via SSE and responds; phone A
   sees "someone is coming"; A resolves (or cancels).

If Android blocks at any step, fall back to the desktop path — `just dev`
against a deployed server gives the identical flow on two laptop windows.
