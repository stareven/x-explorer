# Bundled Binaries

Place platform binaries here before building:

- adb                (Android Debug Bridge, from Android SDK platform-tools)
- idevice_id         (from libimobiledevice)
- ideviceinfo        (from libimobiledevice — used for trust-state detection)
- ideviceinstaller
- afcclient          (from libimobiledevice — used for all iOS app file access via `--documents`)

Download libimobiledevice tools: brew install libimobiledevice
Download adb: brew install android-platform-tools

Note: `ifuse`/macFUSE is no longer required. iOS file access now goes
through `afcclient --documents <bundle_id>`, a one-shot subprocess call
that doesn't need a kernel extension or user-approved mount. This only
works for apps with `UIFileSharingEnabled=true` in their Info.plist — this
is an iOS platform restriction on the house_arrest service, not specific
to afcclient (ifuse had the exact same restriction).
