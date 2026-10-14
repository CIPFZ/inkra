# 控制通道与 MCP 设计（CM-003）

> 状态：**设计草稿，待产品负责人确认后实现**（AGENT.md T7）。版本 0.1，2026-10-14。
> 需求来源：SPEC 11 CM-003（P2）、AD-3（命令系统）、NF-002/NF-004/NF-005/NF-008，按 D-6 在 1.x 中排在 FS-050 之前。

## 1. 目标与非目标

目标：

1. 让本机上的自动化脚本与 AI 代理驱动**正在运行的** Inkra 窗口：执行命令、读取文档状态、截图、模拟点击与按键、打开与保存文件。
2. `inkra-cli mcp` 把这些能力包装为 MCP（Model Context Protocol）工具，供 AI 代理通过 stdio 调用。
3. 与菜单、快捷键、CLI 完全同一条路径：一切修改都经命令注册表（AD-3），因此有相同的撤销分组、可用性判断、参数校验与 `catch_unwind` 保护（NF-002）。

非目标：

- 不开放网络访问（只监听回环地址）；不做远程协作或多用户共享。
- 不提供脚本运行时或插件接口（SPEC 1.5：1.0 不做插件；控制通道只暴露已有命令）。
- 不新增任何"只能通过控制通道做"的编辑能力；控制通道不能绕过命令系统直接改 rope。

## 2. 启用方式

| 方式 | 行为 |
|---|---|
| `inkra --control <port>` | 在 `127.0.0.1:<port>` 与 `[::1]:<port>` 上监听；`<port>` 为 `0` 时由系统分配。 |
| 未指定 | 不监听任何端口（默认关闭，NF-004 无网络行为不变）。 |

是否增加偏好项 `general.controlChannel`（让 GUI 启动时也开启）列为开放问题 Q3，默认**不加**。

启动后写入**连接文件** `<data_dir>/control/<pid>.json`（例如 Linux `~/.local/share/inkra/control/12345.json`）：

```json
{ "version": 1, "pid": 12345, "port": 51734, "token": "<64 位十六进制>", "app": "1.1.0" }
```

- 令牌：256 位随机数（操作系统随机源），每次启动重新生成，不写日志。
- 权限：Unix 下文件 `0600`、目录 `0700`；Windows 下位于用户自己的 `%LOCALAPPDATA%`（默认 ACL 仅本人与管理员）。
- 正常退出时删除文件；残留文件（进程已不存在）由下一次启动与 `inkra-cli mcp` 清理。

## 3. 传输与报文

- **TCP + 换行分隔的 JSON-RPC 2.0**：每行一个请求或响应对象（UTF-8，行内不含换行）。选择理由：无需 HTTP 栈与新依赖（`std::net` + 已有的 `serde_json`），任何语言几行即可实现客户端。HTTP 方案见开放问题 Q1。
- 连接后的第一个请求必须是 `hello`，携带令牌；否则断开。令牌用恒定时间比较。
- 限制：单行最大 16 MiB；同时最多 4 个连接；单连接请求串行处理（一个 `batch` 也是一个请求）。
- 错误码：JSON-RPC 标准码（`-32700` 解析错误、`-32600` 无效请求、`-32601` 方法不存在、`-32602` 参数无效）加应用码：`1` 未认证、`2` 命令不存在、`3` 命令当前不可用、`4` 命令执行失败、`5` 窗口不存在、`6` 超时。命令错误一一对应 `CommandError` 的四个变体。

## 4. 方法

所有方法都可带可选的 `window`（窗口 id，缺省为活动窗口）。

| 方法 | 参数 | 结果 | 实现映射 |
|---|---|---|---|
| `hello` | `{ token, client? }` | `{ version, app, windows: [{ id, path, title, active }] }` | 认证；列出窗口 |
| `commands` | `{}` | `[{ id, title, category, params, shortcut, enabled, checked }]` | 命令注册表（与 `docs/commands.md` 同源），`enabled`/`checked` 按当前窗口计算 |
| `execute` | `{ command, params?, window? }` | `{ outcome: "done" \| "noop" \| { value } }` | `Engine::execute_in(window, id, params)`，随后应用 `take_effects()`（与菜单点击相同） |
| `batch` | `{ calls: [{ command, params? }], window?, stopOnError? }` | `[result \| error]` | 逐条 `execute`，同一帧内完成；`stopOnError` 默认 `true` |
| `inspect_document` | `{ window?, include?: ["text","outline","selection","stats","blocks"] }` | `{ path, dirty, mode, encoding, newline, text?, selection?, outline?, stats?, blocks? }` | 直接读 `AppState`（只读）；`blocks` 为顶层块的种类与源码范围 |
| `screenshot` | `{ window?, scale? }` | `{ png: "<base64>", width, height }` | egui `ViewportCommand::Screenshot`，在下一帧的 `Event::Screenshot` 中取回 |
| `click` | `{ x, y, button?: "primary"\|"secondary", count?: 1\|2\|3, modifiers? }` | `{}` | 注入下一帧的 `PointerMoved` + `PointerButton` 事件；坐标为窗口逻辑像素 |
| `key` | `{ key?, text?, modifiers? }` | `{}` | 注入 `Event::Key` 或 `Event::Text`；键名沿用 `keybindings.json` 的写法（`Ctrl+Shift+K`） |
| `open` | `{ path, window?: "current"\|"new" }` | `{ window }` | `file.open` 命令 |
| `save` | `{ window?, path? }` | `{ path }` | `file.save` / `file.saveAs`（带 `path` 时不弹对话框） |

说明：

- 需要对话框的命令（例如不带 `path` 的 `file.saveAs`）在控制通道中返回错误 `4`（"需要用户交互"），而不是弹出对话框阻塞调用方；带齐参数即可无交互完成。
- `execute` 修改文档时与键盘输入走同一撤销栈；一次 `batch` 不额外合并撤销组（每条命令按自身 `undo_group`）。
- `inspect_document` 是读操作，不经命令系统；`text` 默认不返回（文档可能很大），需显式 `include`。

## 5. 线程模型

```
listener 线程 (std::net::TcpListener, 阻塞 accept)
   └─ 每连接一个线程：读一行 → 解析 → (Request, reply_tx) 送入 channel → ctx.request_repaint()
UI 线程 (Shell::ui 每帧开头)：drain channel → 在 Engine 上执行 → 应用 effects → reply_tx.send(结果)
   screenshot / click / key：登记为"下一帧完成"，在对应事件到达后回复
```

- 无请求时不唤醒 UI，保持空闲 CPU 0%（SPEC 2.5）；只有收到请求才 `request_repaint`。
- 每个请求 30 s 超时（错误 `6`），防止 UI 卡住时客户端永远等待。
- 所有执行都在 UI 线程，与用户输入串行，不需要给 `Engine` 加锁。

## 6. 分层与代码位置

| 内容 | 位置 | 理由 |
|---|---|---|
| 报文类型（请求、响应、错误码）、连接文件读写、令牌生成与比较 | `inkra-engine::control`（L5） | `inkra-ui` 与 `inkra-cli` 都要用；`inkra-cli` 禁止依赖 `inkra-ui`（SPEC 2.2） |
| 监听线程、帧内调度、截图/点击/按键注入 | `inkra-ui::control`（L6） | 需要 egui 上下文与 `Shell` |
| `--control` 参数 | `apps/inkra/src/args.rs` | CM-004 参数解析 |
| 客户端、`inkra-cli mcp` | `apps/inkra-cli` | 只依赖 `inkra-engine::control` |

不需要新依赖：`std::net`、`serde_json`；随机数用 `getrandom`（已在依赖树中，作为直接依赖需加入 `xtask/src/deps.rs` 白名单，许可证 MIT OR Apache-2.0）；base64 编码 PNG 用 30 行自写函数或已在树中的 `base64`（同样需加入白名单）。不涉及 `unsafe`。

## 7. `inkra-cli mcp`

- stdio 上的 MCP 服务器（JSON-RPC 2.0，按行分隔），实现 `initialize`、`tools/list`、`tools/call`、`resources/list`、`resources/read`。
- 连接目标：读取 `<data_dir>/control/*.json`，选择存活的进程（多个时取最新，或 `--pid`）；没有运行中的 Inkra 时，`--launch` 选项以 `inkra --control 0` 启动一个并等待连接文件出现。
- 工具（推荐**少量通用工具**，见开放问题 Q2）：

| MCP 工具 | 对应方法 |
|---|---|
| `inkra_list_commands` | `commands` |
| `inkra_execute` | `execute`（参数 `command`、`params`） |
| `inkra_batch` | `batch` |
| `inkra_inspect` | `inspect_document` |
| `inkra_screenshot` | `screenshot`（返回 MCP image content） |
| `inkra_open` / `inkra_save` | `open` / `save` |
| `inkra_click` / `inkra_key` | `click` / `key` |

- 资源：`inkra://commands`（即 `docs/commands.md` 内容）、`inkra://document/<window>`（当前文档文本，只读）。

## 8. 安全边界

| 威胁 | 措施 |
|---|---|
| 其他机器访问 | 只绑定回环地址；拒绝非回环来源（双重检查 `peer_addr`）。 |
| 本机其他用户或网页 | 必须持有令牌；令牌文件仅本人可读；浏览器无法发起原始 TCP 连接（无 HTTP 即无 CSRF/DNS rebinding 面）。 |
| 令牌泄漏到日志 | 日志只记方法名、窗口 id、耗时与错误码（NF-008：不记录文档内容与参数值）。 |
| 通过控制通道执行任意程序 | 没有"执行程序"的方法；会启动外部进程的命令（Pandoc 导出、打印）与 GUI 中一样只用用户配置的程序（NF-005），且链接打开同样经过 `may_open`。 |
| 资源耗尽 | 行长度上限 16 MiB、连接数上限 4、请求超时 30 s。 |
| 用户内容丢失 | 写操作只能经命令系统；`save` 与 GUI 保存相同（原子写、外部修改冲突检查，NF-003）；未保存的修改照常进入恢复目录。 |
| 用户不知情 | 开启时状态栏显示"控制通道已开启"图标，悬停显示端口与已连接客户端数。 |

## 9. 测试计划

1. `inkra-engine::control`：报文编解码、错误码映射、令牌比较、连接文件读写与权限（单元测试）。
2. `inkra-ui::control`：在 `egui_kittest` 中以内存 channel 驱动 `Shell`，覆盖 `execute`、`batch`、`inspect_document`、`click`、`key`、需要交互的命令返回错误（无网络）。
3. 端到端：CI 的 Linux job 在 Xvfb 下启动 `inkra --control 0`，用 `inkra-cli` 客户端跑一组命令（打开示例、加粗、保存、截图），比较保存结果与截图尺寸。
4. MCP：对 `inkra-cli mcp` 做 stdio 往返测试（`initialize` → `tools/list` → `tools/call inkra_execute`）。

## 10. 开放问题（需产品负责人决策）

| 编号 | 问题 | 推荐 | 其他选项 | 不决策的后果 |
|---|---|---|---|---|
| Q1 | 传输格式 | 换行分隔的 JSON-RPC over TCP：无新依赖、无浏览器攻击面 | HTTP + JSON（便于 curl，但需自写 HTTP 解析或引入依赖，且要防 DNS rebinding） | 按推荐实现 |
| Q2 | MCP 工具粒度 | 少量通用工具（`inkra_execute` + `inkra_list_commands` 等 9 个） | 每个命令一个工具（217 个，占满代理上下文） | 按推荐实现 |
| Q3 | 是否提供偏好项让 GUI 启动时自动开启 | 不提供，仅命令行 `--control` | 增加 `general.controlChannel`（默认关） | 按推荐实现 |
| Q4 | `click`/`key`/`screenshot` 是否放到第一期 | 放入：AI 代理验证界面效果需要截图 | 第一期只做 `execute`/`batch`/`inspect_document`/`open`/`save` | 按推荐实现 |
