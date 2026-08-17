# x-explorer

x-explorer is a macOS desktop app for browsing, importing, and exporting files on connected iOS and Android devices.

## Features

- Browse iOS app containers and Android external storage from one UI
- Import files from the Mac to a device and export files back to the Mac
- Track connected devices and refresh the file browser as device state changes
- Show transfer progress for long-running copy operations
- Cache directory listings for faster navigation while metadata is refreshed in the background

## Requirements

- macOS
- Homebrew
- `libimobiledevice`
- `ideviceinstaller`
- `android-platform-tools`

The backend shells out to `idevice_id`, `ideviceinfo`, `ideviceinstaller`, `afcclient`, and `adb`. If one is missing, the app shows the matching Homebrew install command.

## Development

```bash
npm run tauri dev      # Run the full app (starts Vite + compiles Rust + launches the Tauri window)
npm run dev            # Vite frontend only (no device features)
npm run build          # TypeScript check + Vite production build
npm run tauri build    # Full production build (.app/.dmg bundle)
```

## Testing

```bash
npx vitest             # Run all frontend tests in watch mode
npx vitest run         # Run all frontend tests once
npx vitest run src/store/index.test.ts  # Run a single test file
cargo test --manifest-path src-tauri/Cargo.toml  # Run all Rust tests
cargo test --manifest-path src-tauri/Cargo.toml ios_client  # Run a single Rust test module
```

Vite dev server runs on port 1420 and Tauri expects that port to be available.

## Architecture Notes

- Rust commands are registered in `src-tauri/src/lib.rs` and the device polling loop lives in `device_manager.rs`
- Shared frontend state is managed in `src/store/index.ts` with Zustand
- The file browser owns directory navigation, selection, caching, and transfer actions
- Frontend listeners for `devices-changed`, `transfer-progress`, and `ios-file-info-ready` live in `src/hooks/useTauri.ts`
