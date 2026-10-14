# Clearbar 验证记录

时间：2026-10-14（Asia/Shanghai）。

## 0.1.2 托盘开机启动开关

- 原生托盘右键菜单新增带勾选状态的“开机启动”，保留“退出”。
- 自启改为当前用户“启动”文件夹中的 `Clearbar.lnk`，目标为本次交付 EXE，参数 `--startup`。
- 最初使用 Run 注册值时，程序直接读取成功，但系统 WMI 查询和用户任务管理器均未发现它。
  本机直接注册表访问与系统提供程序观察到的状态存在差异，底层原因尚未确定。
  改用启动快捷方式后，用户提供截图确认任务管理器可见；测试产生的重复 Run 项已清除。
- 系统启用状态采用原生 COM 调用本机 `StdRegProv`，明确访问当前进程用户 SID 下的 Clearbar 自有值。
  不调用 PowerShell，不使用计划任务，不写其他应用的启动设置。
- 实际打开原生菜单，读取 HMENU 检查勾选状态，通过同一命令处理函数验证开关。
  每次开关同时检查系统 `Win32_StartupCommand`：取消为 0 条，启用为 1 条。
- 检查快捷方式目标与参数；取消自启后进程仍运行、任务栏仍透明。
- 通过系统提供程序仅对 `Clearbar.lnk` 设置禁用状态，菜单变为未勾选；重新勾选清除禁用状态。
- 本次测试最终保留开机启动已启用，系统只保留一个 Clearbar 启动项。
- 本版本的 Rust 静态检查、Release 编译与 3 项既有测试通过。
- 未执行实际关机、重启或重新进入桌面；启动项可见不等于已完成真实开机自启验收。

## 0.1.1 单 EXE 与托盘

- `cargo check --workspace`、Release 编译和 `cargo test --workspace --locked` 均通过，3 项测试通过。
- 在仅放置一个 `clearbar.exe` 的新目录直接启动成功，自动释放内嵌 DLL，任务栏保持透明。
- 已记录 `Tray icon added`，托盘图标使用独立的蓝色“T”，悬停文案为“Clearbar · 任务栏透明化（右键退出）”。
- 实际打开原生右键菜单，检查可见菜单项“退出”；已记录 `Exit selected from tray menu`。
- 选择退出后主进程结束，记录 `restoration acknowledged=true`、`Restored 4/4 backgrounds` 和 `Tray icon removed`。
- 修正了同一次右键同时处理旧版鼠标消息与版本 4 上下文消息导致菜单重复弹出的问题。
- 增加窗口消息泵与 `TaskbarCreated` 图标恢复；未通过真实 Explorer 重启验证。
- 首次试用仍不自动启用开机自启；自启接口保留不变。

以下是 0.1.0 透明底座的验证记录，原有兼容性限制继续适用。

## 环境

- Windows 11 家庭版中文 / 25H2 / 26200.9457，x64。
- Rust 1.88.0，目标 `x86_64-pc-windows-msvc`，使用 Rust 自带 LLD 和本地提取的微软 SDK / CRT。
- 参考项目保持原样；新项目是独立 Git 仓库。初次实测时仅保存在本地，随后按用户要求发布源码与 v0.1.2 包体到 GitHub。

## 已验证

| 项目 | 结果与证据 |
|---|---|
| Rust 静态检查 | `cargo check --workspace` 通过 |
| 最终单元测试 | `cargo test --workspace --locked`：3 项通过；覆盖真实命名事件操作及清理、协议命名、Windows x64 ABI 布局 |
| 最终 Release 构建 | `cargo build --release --workspace --locked` 通过，无警告 |
| 真实透明效果 | 已截图对比；原深色背景变为能看到桌面壁纸，图标及文字仍可见 |
| 背景属性验证 | 捕获两个任务栏的共 4 个背景/边线矩形，原 Opacity=1；修改后读回 0 |
| 正常停止 | `--stop` 返回 0，DLL 记录 `Restored 4/4 backgrounds`；恢复后的截图为原深色背景 |
| 异常退出 | 强制结束本次启动的 Clearbar 主进程；约 2 秒后的日志与截图确认恢复；交付版本重复验证通过 |
| 重复启动 | 第二次启动返回 0，主进程数保持 1 |
| 再次启动 | 停止或强制结束后再次启动，状态恢复为 `running=true`、`transparent=true` |
| 定期扫描兜底 | 最终版本已完成第 2 次扫描，透明效果持续，之后正常停止仍恢复 4/4 |
| 自启注册 | 注册值精确为带引号的 EXE 路径加 `--startup`，核对后通过 `--disable-startup` 删除 |
| 自启模式入口 | 交付版本以 `--startup` 启动成功；这不等于已经通过真实重启后的自启验收 |
| Explorer 稳定性 | 整个实测期间主 Explorer PID 6956 及启动时间不变，没有重启 Explorer |

## 当前兼容路径

本机 `InitializeXamlDiagnosticsEx` 返回 S_OK。
`AdviseVisualTreeChange` 在已经交付任务栏节点后返回 `0x8000FFFF`；其根因尚未确定。
原型不会因此结束，而是保留已捕获背景，验证真实 Opacity，每 30 秒重新扫描。
仅更新任务栏背景；停止和主进程异常退出时，都恢复首次捕获值。

这证明当前桌面会话下的透明及恢复有效，不代表已经修复用户此前原版弹窗的具体根因。

## 未验证

- 实际关机/重启、重新进入桌面后的自启与启动竞争时序。
- Explorer 真正重启后的自动连接；已实现 PID 变化检测，但本次没有重启用户桌面。
- Windows 更新、其他构建、ARM64、Windows 10、第三方任务栏修改工具。
- 锁屏/睡眠唤醒、显示器热插拔及不同 DPI 的长期稳定性。
- 长时间资源趋势、正式签名或发行安装验收。

## 0.1.0 历史交付状态

- 试用程序当前运行，任务栏透明；开机自启没有保留启用。
- EXE SHA-256：`5EA0C59B214B52A04B3C43E1F9CDA5319830F83CEB52CDD0B914A516F4D8D19F`
- DLL SHA-256：`2AE5E609521118F53AD04126601A8F534B8CD24D1615D95FD7440D4DC9BE7F38`
- DLL 由 XAML 固定在 Explorer 中；关闭 Clearbar 恢复外观，DLL 在 Explorer 退出时才卸载。
