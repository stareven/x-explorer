# Device List Name and ID Display Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show real device names as the main title in the device list and show the device id as a gray subtitle.

**Architecture:** Populate `Device.name` with real device names in the backend before events reach the store, then render each item as a compact two-line row in `DeviceList`: name on top, id below. iOS reads `DeviceName` from `ideviceinfo`; Android first tries `adb devices -l` and parses `model:` from the long device listing, then falls back to `adb -s <id> shell getprop ro.product.model` when the long output does not carry a model string.

**Tech Stack:** Rust, Tauri 2, React 19, TypeScript, Zustand, Tailwind CSS, Vitest, React Testing Library, Cargo tests

---

### Task 1: Populate backend device names

**Files:**
- Modify: `src-tauri/src/ios_client.rs`
- Modify: `src-tauri/src/android_client.rs`
- Modify: `src-tauri/src/device_manager.rs`

- [ ] **Step 1: Write failing Rust tests**

Add an iOS parser test for `DeviceName` and an Android parser test for `adb devices -l` long output.

```rust
#[test]
fn test_parse_ideviceinfo_device_name() {
    let output = "DeviceName: Alice's iPhone\nProductType: iPhone15,2\n";
    assert_eq!(parse_ideviceinfo_device_name(output), Some("Alice's iPhone".to_string()));
}

#[test]
fn test_parse_adb_devices_reads_model_from_long_output() {
    let output = "List of devices attached\n0123456789ABCDEF\tdevice usb:1-1 product:panther model:Pixel_7 device:panther transport_id:1\n";
    let devices = parse_adb_devices(output);
    assert_eq!(devices[0].name, "Pixel 7");
}
```

- [ ] **Step 2: Run tests and confirm RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ios_client -- --nocapture`

Expected: FAIL before implementation because `parse_ideviceinfo_device_name` is missing.

- [ ] **Step 3: Implement device name extraction**

Add `parse_ideviceinfo_device_name`, use it in `list_ios_devices`, update `parse_adb_devices` to read `model:`, and change `device_manager` to call `adb devices -l`.

```rust
fn parse_ideviceinfo_device_name(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        if key.trim() == "DeviceName" {
            let name = value.trim();
            if name.is_empty() { None } else { Some(name.to_string()) }
        } else {
            None
        }
    })
}
```

- [ ] **Step 4: Verify backend tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`

Expected: PASS, including the new iOS and Android device name tests.

---

### Task 2: Render name and id in the device list

**Files:**
- Modify: `src/components/DevicePanel/DeviceList.tsx`
- Modify: `src/components/DevicePanel/DevicePanel.test.tsx`

- [ ] **Step 1: Write the failing frontend test**

Add assertions that a device row renders both the device name and the device id.

```ts
it("renders device names and ids", () => {
  render(<DeviceList />);
  expect(screen.getByText("iPhone 15")).toBeInTheDocument();
  expect(screen.getByText("iphone-1")).toBeInTheDocument();
  expect(screen.getByText("Pixel 7")).toBeInTheDocument();
  expect(screen.getByText("pixel-1")).toBeInTheDocument();
});
```

- [ ] **Step 2: Update the list item markup**

Render the device label as a two-line text block while leaving the surrounding button intact.

```tsx
<span className="min-w-0 flex-1">
  <span className="block truncate">{device.name}</span>
  <span className="block truncate text-xs text-gray-400">{device.id}</span>
</span>
```

- [ ] **Step 3: Verify frontend tests**

Run: `npx vitest run src/components/DevicePanel/DevicePanel.test.tsx`

Expected: PASS, with the existing click-selection and unauthorized-badge assertions still passing.
