# x-explorer 设计文档

**日期：** 2026-08-14
**状态：** 已确认

---

## 概述

x-explorer 是一个 macOS 原生桌面应用，用于查看连接的 iPhone 和 Android 设备，浏览 App 容器文件系统，支持文件的导入、导出和拖拽操作。

---

## 技术栈

- **框架：** Tauri（Rust 后端 + React/TypeScript 前端）
- **前端：** React + TypeScript + Zustand（状态管理）+ Vitest（测试）
- **后端：** Rust，通过内置二进制与设备通信
- **内置工具：** adb（Android）、idevice_id / ideviceinfo / ideviceinstaller / afcclient（iOS，均随 libimobiledevice 提供），打包进 .app bundle

---

## 功能范围

### iOS 支持
- 检测 USB / WiFi 连接的 iOS 设备
- 列出已安装 App（bundle id + 名称），**只展示 `UIFileSharingEnabled=true` 的 App**——这是 iOS 平台限制：`house_arrest` 服务的 `VendDocuments` 操作只对声明了该 entitlement 的 App 开放，`VendContainer`（完整容器访问）即使该 flag 为 true 也会因缺少开发者签名/描述文件而返回 `InstallationLookupFailed`，因此不使用 `--container`，只使用 `--documents`
- 通过 `afcclient --documents <bundle_id>` 访问 App 的 `Documents` 目录（浏览根目录固定为 `/Documents`，不暴露 `--documents` 模式下 `/` 本身——该路径本身不可 `ls`，会返回 `Permission denied`，属于 house_arrest 的已知行为，只有 `/Documents` 及其子目录可读写）
- 文件上传、下载、删除（已在真实设备上验证 put/get/rm 均正常工作）

### Android 支持
- 检测 adb 连接的 Android 设备
- 列出已安装 App
- 访问 App 数据目录（`/data/data/<package>`），**必须通过 `run-as <package>` 桥接**，因为 SELinux 禁止 adb shell 直接读写其他 App 的私有目录（非 root 设备上 `adb pull /data/data/<pkg>/...` 会失败）
  - 目标 App 必须是 debuggable（`android:debuggable="true"`）或设备已 root，否则 `run-as` 本身会失败——需在 UI 上明确提示这一限制
- 访问外部存储（`/sdcard`、`/storage/emulated/0`），**不需要 run-as**，直接用 `adb pull`/`adb push`
- 不支持 root 级别系统目录
- UI 上"外部存储"是独立于 App 选择的固定入口（不依赖选中某个 App）

### 文件操作
- 列目录、读文件、创建、删除、重命名
- 批量操作（多选 Cmd+点击 / Shift+点击 / Cmd+A）
- 导入（Mac → 设备）、导出（设备 → Mac）
- 双向拖拽：从设备拖出到 Finder/桌面，从 Mac 拖入设备目录

---

## 架构

```
React 前端  ⇄ (Tauri IPC)  ⇄  Rust 后端  ⇄ (subprocess)  ⇄  内置二进制  ⇄  设备
```

前端通过 Tauri `invoke()` 调用 Rust Command，Rust 调用内置工具并返回结构化数据给前端渲染。Tauri Event 用于设备热插拔通知和传输进度推送。

---

## 后端模块

### `device_manager`
- 每 2 秒轮询 `adb devices` 和 `idevice_id -l`
- 设备连接/断开时通过 Tauri Event 推送前端

### `ios_client`
- 调用 `idevice_id -l` 检测设备
- 调用 `ideviceinstaller -u <udid> list -a CFBundleIdentifier -a UIFileSharingEnabled` 列出 App，前端只展示 `UIFileSharingEnabled=true` 的条目
- **无状态 subprocess 调用**：不再维护挂载生命周期，每次文件操作都是一次独立的 `afcclient -u <udid> --documents <bundle_id> <cmd> <path>` 子进程调用，调用结束进程即退出，没有需要管理的挂载点/清理逻辑
  - 列目录：`afcclient -u <udid> --documents <bundle_id> ls <path>`（`path` 相对于 `/Documents`，UI 层拼接 `/Documents` 前缀）
  - 下载：`afcclient -u <udid> --documents <bundle_id> get <remote_path> <local_path>`
  - 上传：`afcclient -u <udid> --documents <bundle_id> put <local_path> <remote_path>`
  - 删除：`afcclient -u <udid> --documents <bundle_id> rm -rf <remote_path>`
- **信任状态检测**：`idevice_id -l` 只返回已信任设备的 UDID；未信任设备通过 `idevicepair validate` 或尝试 `ideviceinfo` 检测失败来识别（返回 "Trust" 相关的 stderr 关键字），据此在 UI 上区分"已连接"和"待信任"状态
- **house_arrest 特有错误处理**：`InstallationLookupFailed`（对应 App 未设置 `UIFileSharingEnabled` 或使用了 `--container`）与 AFC 状态码 10 / `Permission denied`（对 `--documents` 根路径 `/` 本身发起操作）需要分别识别并转换成明确的中文提示，而不是把原始 stderr 抛给用户

### `android_client`
- 调用 `adb devices` 检测设备
- 调用 `adb shell pm list packages` 列出 App
- **外部存储路径**（`/sdcard/...`）：直接 `adb shell ls -la <path>`、`adb pull`/`adb push`
- **App 容器路径**（`/data/data/<pkg>/...`）：所有操作都通过 `run-as <pkg> <cmd>` 桥接：
  - 列目录：`adb shell run-as <pkg> ls -la <path>`
  - 下载文件：`adb shell run-as <pkg> cat <remote_path>`，将 stdout 写入本地文件（因为 `run-as` 输出无法直接被 `adb pull` 访问，需走 stdout 管道）
  - 上传文件：先 `adb push <local> /data/local/tmp/<tmpname>`（外部临时区，无需 run-as），再 `adb shell run-as <pkg> cp /data/local/tmp/<tmpname> <remote_path>`，最后 `adb shell rm /data/local/tmp/<tmpname>` 清理
  - 删除：`adb shell run-as <pkg> rm -rf <path>`
  - 若 `run-as` 返回 `run-as: package not debuggable` 等错误，直接返回明确的错误信息给前端（对应错误处理表新增条目）

### `file_ops`
- 统一的文件增删改查接口（iOS/Android 共用上层接口）
- 支持批量操作和文件夹递归操作
- 提供 `normalize_path`，Android/iOS 路径拼接统一走此函数，避免手工字符串拼接导致的双斜杠等问题

### `transfer_queue`
- 所有导入/导出操作必须通过队列执行，前端不直接 await 单次 `invoke` 完成整个传输
- 队列内部为每个任务派生一个后台线程/tokio task 执行实际的 adb/idevice 调用
- 大文件下载/上传按分块（如 256KB）读写并在每个分块后 `emit` 一次 `transfer-progress`，暂不做真正的分块流式 API（`run-as cat`/`afcclient get/put` 输出本身是整体的，进度以"已完成的文件数 / 总文件数"为粒度，而非字节级精确进度）
- 支持并发上限（默认 3 个任务并行），暂停、取消、错误重试

---

## 前端组件

```
App
├── DevicePanel（左栏）
│   ├── DeviceList — 设备卡片，连接状态徽标（已连接 / 待信任 / 未授权）
│   └── AppList — 已安装 App 列表 + 固定的「外部存储」入口（Android 仅有）
├── FileBrowser（右侧主区）
│   ├── BreadcrumbBar — 路径导航，可点击跳转
│   ├── Toolbar — 视图切换 / 导入导出 / 多选操作按钮
│   ├── FileGrid — 网格视图（图标 + 文件名）
│   └── FileList — 列表视图（名称 / 大小 / 修改时间）
└── TransferPanel（底部浮层）
    └── TransferItem — 进度条 + 取消按钮
```

### 拖拽交互
- **导出（设备 → Mac）：** 由于设备上的文件不在本机文件系统中，Tauri `startDrag` 无法直接拖拽远程路径。实际流程为两步：
  1. `dragstart` 触发时先将选中文件下载到临时目录（`$TMPDIR/x-explorer-drag/`）
  2. 下载完成后立即对本地临时文件路径调用 Tauri `startDrag`，交给系统接管拖拽
  - 若文件较大导致下载有明显延迟，拖拽起始处显示 loading 光标，避免用户以为拖拽未响应
- **导入（Mac → Sdcard/App 容器）：** FileBrowser 监听 `ondrop`，捕获本地路径，调用 Rust 上传命令，加入 transfer_queue

### 多选
- Cmd+点击：逐个选中
- Shift+点击：范围选中
- Cmd+A：全选
- 选中后 Toolbar 显示批量操作：导出、删除
- 拖拽选中项整体传输（多文件一次性）

### 状态管理（Zustand）
全局 store 管理：设备列表、当前选中设备/App、当前路径、文件列表缓存、传输队列状态。

---

## 错误处理

| 场景 | 处理方式 |
|------|----------|
| 设备断开 | 前端自动刷新设备列表，清除当前浏览状态 |
| iOS 信任弹窗未确认 | 检测到 "Trust" 相关错误后，设备状态显示为「待信任」，提示用户在手机上点「信任此电脑」 |
| iOS App 不支持文件共享 | `afcclient` 返回 `InstallationLookupFailed` 时，提示「该应用未开启文件共享，无法访问其文档目录」；App 列表默认已按 `UIFileSharingEnabled` 过滤，此错误主要用于兜底 |
| adb unauthorized | 提示用户开启 USB 调试并在手机上授权 |
| `run-as` 失败（App 非 debuggable） | 提示「该应用未开启调试模式，无法访问其数据目录」，不崩溃，外部存储浏览仍可用 |
| 传输中断 | 标记任务失败，支持重试 |
| 权限不足（App 数据目录） | 显示明确错误原因，不崩溃 |
| 目标路径已存在 | 弹窗提示：覆盖 / 跳过 / 重命名 |

---

## 测试策略

### Rust 单元测试
- adb / idevice 命令输出解析逻辑
- 文件路径规范化
- 传输队列调度逻辑

### 前端测试（Vitest）
- 组件逻辑 + Zustand store
- Mock Tauri `invoke`，不依赖真实设备
- 拖拽交互用 `userEvent` 模拟

---

## 项目目录结构

```
x-explorer/
├── src-tauri/                  # Rust 后端
│   ├── src/
│   │   ├── device_manager.rs
│   │   ├── ios_client.rs
│   │   ├── android_client.rs
│   │   ├── file_ops.rs
│   │   ├── transfer_queue.rs
│   │   └── main.rs
│   └── binaries/               # 内置 adb、idevice 系列工具（含 afcclient，macOS arm64/x86_64）
└── src/                        # React 前端
    ├── components/
    │   ├── DevicePanel/
    │   ├── FileBrowser/
    │   └── TransferPanel/
    ├── store/                  # Zustand store
    └── hooks/
```
