# Clearbar — Rust 任务栏透明原型

只做两件事：让 Windows 11 任务栏背景完全透明，以及可选的开机自启。

运行时在右下角通知区域显示蓝色“T”图标；右键可勾选“开机启动”，选择“退出”会停止程序并恢复任务栏背景。
无设置窗口、颜色配置、模糊效果、动态规则或联网功能。
主程序和 Explorer TAP DLL 均由 Rust 编写；DLL 内嵌在 EXE 中，调用 Windows 自带的 Win32 / COM / WinRT 接口。

## 下载

- [直接下载 clearbar.exe](https://github.com/DestinySail/TranslucentTB-rs/releases/latest/download/clearbar.exe)：无需安装，放到固定目录双击运行。
- [最新 Release 与便携 ZIP](https://github.com/DestinySail/TranslucentTB-rs/releases/latest)：含使用说明、许可证和便捷操作脚本。
- [SHA-256 校验文件](https://github.com/DestinySail/TranslucentTB-rs/releases/latest/download/SHA256SUMS.txt)。

适用于 Windows 11 x64；当前仍为原型版本。首次运行默认不开启自启，可在托盘右键菜单中勾选。

## 试用

将 `clearbar.exe` 放到固定目录，直接双击即可，无需手动复制 DLL。
EXE 会将内嵌 DLL 释放到 `%LOCALAPPDATA%\Clearbar\runtime\内容指纹\clearbar_tap.dll`。
先退出原版 TranslucentTB，避免两个程序竞争任务栏外观。
若右下角暂时没看到图标，点击“∧”查看收起的图标，可手动将蓝色“T”拖到常显区域。

| 操作 | 双击文件 |
|---|---|
| 仅本次启动 | `start.cmd` 或 `clearbar.exe` |
| 开关自启 | 右键蓝色“T”图标 → 勾选 / 取消“开机启动” |
| 停止并恢复背景 | 右键蓝色“T”图标 → “退出”，或 `stop.cmd` |
| 启用开机自启并立即运行 | `enable-startup.cmd` |
| 取消自启并停止 | `disable-startup.cmd` |
| 查看运行状态 | `status.cmd` |

首次试用不自动注册开机启动。建议将 EXE 放到固定目录；移动后手动运行一次，已启用的快捷方式会更新到新路径。
勾选后在当前用户的 Windows“启动”文件夹创建 `Clearbar.lnk`，目标为当前 EXE 加 `--startup`，不需要管理员权限。
任务管理器的“启动应用”页可显示 `clearbar.exe`；取消勾选会删除快捷方式，但不停止当前透明效果。
菜单每次打开都会读取系统启用状态；从任务管理器禁用后，菜单显示未勾选，再次勾选即可重新启用。
启用状态通过 Windows 本机 WMI 注册表提供程序读取和清除，不依赖 PowerShell、计划任务或 XML 配置文件。
开机自启在进入 Windows 桌面后运行，不需要软件账号。

也可以从 PowerShell 执行：

```powershell
.\clearbar.exe --enable-startup
.\clearbar.exe --disable-startup
.\clearbar.exe --stop
.\clearbar.exe --status
```

## 启动与恢复机制

- 主程序等待 `Shell_TrayWnd`，按 1、2、4、8、15、30 秒退避，之后保持 30 秒重试间隔。
- 临时线程钩子在任务栏所属 Explorer 中加载 Rust DLL；`DllMain` 只保存模块句柄。
- DLL 连接 Windows XAML Diagnostics，只访问 `Taskbar.TaskbarFrame` 下的
  `BackgroundFill` / `BackgroundStroke` 矩形，将其 `Opacity` 设为 0。
- 图标、文字、输入和任务栏布局不修改。首次捕获的原透明度用于停止时恢复。
- 透明度写入后会读回验证；DLL 每 500 毫秒检查控制器是否存在。
  主程序正常停止或被强制结束时，DLL 都会在 Explorer UI 线程恢复背景。
- Explorer PID 变化后主程序会重新等待并连接新任务栏。
- 本机 Windows 构建存在“背景节点已经枚举，但订阅最终返回 `0x8000FFFF`”的现象。
  原型保留已验证的节点，并每 30 秒重新扫描，避免因此直接退出。
  该兼容路径写入日志，不把 Diagnostics 返回成功当作透明已经生效。

日志和状态：`%LOCALAPPDATA%\Clearbar\app.log`、`tap.log`、`status.txt`。
日志为 UTF-8，约 1 MB 后轮换，不记录窗口标题或文档内容。

## 构建

目标平台：`x86_64-pc-windows-msvc`。需要 Rust 1.82 以上、Visual Studio C++ Build Tools 和 Windows SDK。
依赖版本由 `Cargo.lock` 固定；windows-rs 只启用所需的 Win32 接口。
构建脚本先编译 TAP DLL，再将它编译进主程序；不要用并行 workspace 构建代替这一顺序。

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\build.ps1 -Test
```

产物位于 `dist\Clearbar-0.1.2-时间戳`，同时生成 ZIP。仅 `clearbar.exe` 是运行所必需的文件；其他文件是说明及便捷操作脚本。
本机额外使用了仓库忽略的 `.tools` 微软 SDK/CRT 库和 `.cargo/config.toml`，配合现有 Rust 自带的 LLD 链接。
这些只用于构建，不随发布包分发；正常安装 Build Tools 的电脑不需要它们。

## 范围与已知限制

- 当前仅面向 Windows 11 的 XAML 任务栏与 x64，实机基线为 25H2 / 26200.9457。
- Windows 更新可能改变内部节点，需要重新验证；不支持 Windows 10、旧任务栏替换工具或 ARM64。
- 进入桌面后可能短暂显示正常背景，等待任务栏就绪后再透明；初始化失败时后台重试，不弹窗阻塞操作。
- XAML Diagnostics 会把 DLL 固定在 Explorer 中。停止程序会恢复背景，但 DLL 要到 Explorer 退出才卸载。
  更新 DLL 后建议重启电脑；不要覆盖正在加载的 DLL。
- 当前是原型版本，无签名、安装器和自动更新。实际开机自启、Explorer 真正重启后的恢复需要另做验收。

详细实测结果见仓库 `VALIDATION.md`。
