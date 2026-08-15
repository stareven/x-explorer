# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Project Is

x-explorer is a macOS desktop app for browsing, importing, and exporting files on connected iOS and Android devices. Built with Tauri 2 (Rust backend) + React 19 + TypeScript + Zustand + Tailwind CSS 4.

## Commands

### Development
```bash
npm run tauri dev      # Run full app (starts Vite + compiles Rust + launches Tauri window)
npm run dev            # Vite frontend only (no device features)
npm run build          # TypeScript check + Vite production build
npm run tauri build    # Full production build (.app/.dmg bundle)
```

### Testing
```bash
npx vitest             # Run all frontend tests (watch mode)
npx vitest run         # Run all frontend tests once
npx vitest run src/store/index.test.ts  # Run a single test file
cargo test --manifest-path src-tauri/Cargo.toml  # Run all Rust tests
cargo test --manifest-path src-tauri/Cargo.toml ios_client  # Run a single Rust test module
```

Vite dev server runs on port 1420 (strict — fails if taken). Tauri config expects this port.

## Architecture

### Two-tier: Rust backend shells out to device CLI tools

The Rust backend does NOT link against libimobiledevice or adb libraries. Instead it spawns subprocess calls to bundled binaries in `src-tauri/binaries/`:

| Binary | Used for |
|--------|----------|
| `idevice_id` | iOS device enumeration |
| `ideviceinfo` | iOS trust-state detection (called before every iOS operation) |
| `ideviceinstaller` | iOS app listing + UIFileSharingEnabled filter |
| `afcclient` | All iOS app file ops via `--documents <bundle_id>` (ls, info, get, put, rm) |
| `adb` | All Android operations |

`bin_path.rs` resolves binary paths — checks `src-tauri/binaries/` (dev) or the app bundle's Resources dir (production) first, then falls back to the system PATH; if still missing, the error message includes the Homebrew install command (`libimobiledevice` / `ideviceinstaller` / `android-platform-tools`). The `tauri.conf.json` `bundle.resources` glob (`binaries/*`) ensures they ship inside the .app.

### Rust module responsibilities

| File | Role |
|------|------|
| `lib.rs` | Tauri app entry: registers all `#[tauri::command]`s, starts device_manager thread, creates TransferQueue (max 3 concurrent) |
| `types.rs` | Shared structs: `Device`, `AppInfo`, `FileEntry`, `TransferTask`, `TransferProgress`, `IosFileInfoReady` |
| `device_manager.rs` | Background thread polling iOS + Android every 2s, emits `devices-changed` Tauri event only when device list actually changes |
| `ios_client.rs` | iOS commands: list devices/apps/files, async file-info probing, delete (recursive), download/upload (called by transfer_queue, not directly invokable) |
| `android_client.rs` | Android commands: list devices/apps/files, delete, download/upload. Two access modes: external storage (plain adb) vs app-container (run-as, staging through /data/local/tmp) |
| `file_ops.rs` | Path utilities: `normalize_path`, `join_path`, `sanitize_relative_path` (security: rejects `..` traversal) |
| `transfer_queue.rs` | Async worker pool (3 threads) with Condvar-based queue. Enqueue commands return task IDs; progress flows via `transfer-progress` events. Supports cancellation. |

### Frontend state flow

Single Zustand store (`src/store/index.ts`) holds everything: devices, selected device/app, current path, navigation history (browser-like back stack), files, transfers, view mode.

`src/hooks/useTauri.ts` contains:
- `tauriApi` — typed wrappers around `invoke()` for all Tauri commands
- `useDeviceListener()` — listens for `devices-changed` events
- `useTransferListener()` — listens for `transfer-progress` events
- `useIosFileInfoListener()` — listens for `ios-file-info-ready` events (fallback metadata patching)

### iOS file listing: single `ls -l` call + per-entry fallback

`list_ios_files` runs one `afcclient -- ls -l <dir>` subprocess (~1.2s total), which returns type/size/mtime for every entry at once — so folders show correct icons immediately. Entries whose `ls -l` line can't be parsed come back with placeholder metadata (`modified: None`); the frontend enqueues just those via `enqueue_ios_file_info` (up to 8 concurrent `afcclient info` probes), and each result arrives as an `ios-file-info-ready` event patched into the store. (The old N+1 pattern — plain `ls` plus one 1.2s `info` probe per entry — caused slow type resolution and occasional misclassification when concurrent probes lost the USB/lockdownd race.)

The FileBrowser also implements stale-while-revalidate caching keyed by `platform:deviceId:package:path`, and async reloads use a latest-call-wins sequence so a late-arriving listing never overwrites a newly navigated-to directory.

### iOS vs Android access models

- **iOS**: Only apps with `UIFileSharingEnabled=true` are listed. All file access goes through `afcclient --documents <bundle_id>`, which exposes `/Documents` as the readable root. User-facing paths are relative to `/Documents`.
- **Android external storage**: Plain `adb pull`/`push`/`ls`. No package needed.
- **Android app container**: All ops go through `run-as <pkg>`. Downloads use `run-as cat`. Uploads stage through `/data/local/tmp` then `run-as cp`. Only debuggable apps work.

### Three Tauri events

| Event | Emitted by | Payload | Consumed by |
|-------|-----------|---------|-------------|
| `devices-changed` | device_manager | `Device[]` | useDeviceListener |
| `transfer-progress` | transfer_queue | `TransferProgress` | useTransferListener |
| `ios-file-info-ready` | ios_client (enqueue_ios_file_info) | `IosFileInfoReady` | useIosFileInfoListener |

### Security

`file_ops::sanitize_relative_path` rejects `..` segments in user-supplied relative paths. All iOS and Android file commands call this before constructing device-side paths, preventing directory traversal outside the intended base.

## Conventions

- Error messages shown to users are in Chinese (e.g., "设备待信任或未授权", "该应用未开启调试模式").
- Rust test modules are inline (`#[cfg(test)] mod tests` in each file).
- Frontend tests use Vitest + React Testing Library with jsdom.
- The `tauri.conf.json` has `"csp": null` (no Content Security Policy) — suitable for a local desktop tool.
