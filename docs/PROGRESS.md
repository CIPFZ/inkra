# Inkra 开发进度

更新时间：2026-10-15
当前阶段：**beta 反馈期**（2026-10-15 起，不开新功能）：等待产品负责人实机测试 issue；阻塞写作的 bug 立即修，其余汇总到下方"beta 反馈"等一起定 1.0 范围
CI 状态：GitHub Actions `ci.yml` ubuntu 绿 / windows 绿 / macos 绿（含 bench 同机 A/B、cargo deny + cargo audit（无忽略项）、三平台打包、Linux 控制通道端到端）；`release.yml` 最近一次：成功（运行 #12，tag `v1.0.0-beta.1`）
当前版本：1.0.0-beta.1（[Release](https://github.com/CIPFZ/inkra/releases/tag/v1.0.0-beta.1)，pre-release，15 个安装包 + `SHA256SUMS.txt`）

## 本阶段任务进度
- [x] T1 决策落地与文档同步（2026-10-14）：SPEC 2.1/2.5/13 回填 D-1～D-6，ROADMAP 1.x 按 D-6 重排，旧启动 prompt 归档到 `docs/archive/AGENT_INKRA_m0.md`
- [x] T2 DX12 启动优化（2026-10-14，偏离已接受）：适配器缓存 `general.graphics.lastAdapter`（多适配器时直接选用上次成功者；不能跳过探测，代码注释已注明）、WARP→GL 回退（带 `glFallback` 防循环）、`static-dxc` 评估后不启用、关闭间接调用校验；测试机冷启动 ~390 → ~360 ms。"命中缓存快 100 ms"不达成：wgpu 30 安全 API 每次枚举都探测全部适配器；产品负责人决定不 fork wgpu，实体机数据由 beta 用户日志（`graphics:` 行）回收
- [x] T3 CI 三平台实跑与 Release 流水线（2026-10-14）：`ci.yml` 三平台绿；`release.yml` 产出 Linux x86_64/arm64（tar.gz、AppImage、deb、rpm、Flatpak）、Windows x64/arm64（zip、MSI）、macOS universal dmg，并在全新 runner 上安装验证（deb + Xvfb 首帧、rpm/Fedora、AppImage、tar.gz、Flatpak、MSI 关联/快捷方式/卸载于 windows-latest 与 windows-11-arm、dmg lipo + `--version`）；tag `v*` 时上传为 Release 草稿。说明：无 token 无法手动 `workflow_dispatch`，改为打包相关文件变更时 push 触发 dry run（等效）。失败日志通过公开的 annotation 读取（`tools/gh_runs.py`）
- [x] T4 安全与健壮性收口（2026-10-14）：cargo deny（许可证白名单 + 具名例外）与 cargo audit 进 CI；外部进程复核（Pandoc/mmdc/自定义导出命令/打印）；`open_external` 统一检查（`javascript:`/`vbscript:`/`data:`/指向程序的 `file:` 及 Windows 危险协议，带测试）；third-party.md 按 `cargo tree` 实际链接集生成；egui 非 OFL 字体移除。fuzz（`fuzz/`，各 ≥ 2 h，修复后复跑）：markdown_parse 206 万次、markdown_incremental 142 万次、html_to_markdown 875 万次无问题；diagram 2 个 panic + 3 个超慢/超大输入、math 2 个指数级布局，全部修复并转为回归测试，最终 diagram 317 万次、math 273 万次无崩溃。数学 fuzz 曾因 ASan 隔离区触发 RSS 上限（非泄漏：无 ASan 时 2.8 万次渲染峰值 24 MB）
- [x] T5 Beta 发布（2026-10-15）：release notes 按产品负责人清单补齐（beta 声明与备份建议、三平台下载表与 SHA256 校验方法、已知限制含 macOS 未签名与 arm64 未真机验证、反馈指引、许可证；中英双语）；`beta1` 推送后 CI #28 与打包演练 #10 绿，快进合入 master，tag `v1.0.0-beta.1` 触发 release.yml #12，以 pre-release 直接发布（无 token 无法把草稿转为发布，故 pre-release tag 直接发布、正式版 tag 仍为草稿）。验收：从 Release 下载 Linux AppImage，`sha256sum -c` 通过，`--version` 与 Xvfb 首帧（159 ms）正常；Windows 测试机从 Release 下载 x64 MSI，SHA256 一致，`msiexec /qn` 退出码 0，`.md` → `Inkra.Markdown`、打开命令、开始菜单快捷方式、文件版本 1.0.0-beta.1 均正确，冷启动 365 ms，经文件关联打开含公式与图表的文档渲染正常（截图）
- [x] T6 Dogfooding 与反馈闭环（2026-10-14）：issue 模板（bug / 渲染差异 / 功能请求，bug 模板要求平台、安装方式、版本、`graphics:` 行）、`docs/acceptance/user-reports.md` 与 `user-reports/` 语料测试；README 反馈指引在 `beta1`
- [x] T7 CM-003（2026-10-15）：`inkra --control <port>`（回环 JSON-RPC、令牌连接文件、hello/commands/execute/batch/inspect_document/open/save/screenshot/click/key、对话框命令返回错误、注入记日志不含文本、截图不落盘、状态栏"自动化"标记、被杀进程的连接文件自动清理）；`inkra-cli mcp` 无头模式（读写根目录默认当前目录，越界与危险命令拒绝）与 `--attach [--pid] [--config]`；单元测试 + Shell 端到端测试 + CI Linux 真实进程端到端（`.github/scripts/control-e2e.py`）。说明：注入输入的帧视为窗口获得焦点（egui 在未聚焦窗口中忽略按键，而自动化通常在后台进行）
- [x] T8 AD-5（2026-10-15）：`inkra-diagram::background` 工作线程布局、按源码+调色板缓存、编辑防抖 150 ms、源码变化取消旧布局（流程图各阶段与边循环检查取消标志）、可见待定块每帧保持需要、1 s 无人需要即放弃；结果到达重绘，期间显示上一次结果。验收：10 万字文档中在一张同步布局需 149 ms 的图内连续输入，p95 4.3 ms（`typing_in_a_large_diagram_stays_responsive`，release 下断言 < 16 ms）
- [x] T9 ttf-parser → skrifa（2026-10-15）：inkra-math、inkra-diagram、inkra-export 改用 skrifa 0.48 / read-fonts 0.45（树中原有的 0.42/0.44 没有 MATH 表，故新增该版本，许可证 MIT OR Apache-2.0）；字形框沿用 ttf-parser 定义（全部轮廓点含控制点、截断取整）。验收：233 个公式的 474 次渲染场景哈希（含轮廓）完全一致；35 个图表栅格化像素完全一致；PNG 快照不变；ttf-parser 已不在依赖树中，`deny.toml` 与 `cargo audit` 的 RUSTSEC-2026-0192 忽略项删除

## beta 反馈（分诊规则与汇总）
- 来源：GitHub Issues（模板 bug / 渲染差异 / 功能请求）。`python3 tools/triage_issues.py` 读取 issue，提取版本、平台、安装方式、`graphics:` 行，重写 `docs/acceptance/user-reports.md` 的自动生成段（`--fetch-logs` 另读附件中的 inkra.log；本机出口 IP 的匿名 API 配额常被耗尽，可设 `GH_TOKEN`，或在 Windows 测试机上取 JSON 后用 `--input`）。
- 分诊：bug 模板按 平台 / 版本 / 后端（`graphics:` 行）/ 复现步骤 / 最小 .md 核对；**阻塞写作**（崩溃、丢内容、无法输入/保存/打开/启动、数据损坏）立即修复并发布修复版（`1.0.0-beta.N`）；其余记入下表，等产品负责人一起定 1.0 范围。
- 当前：0 个 issue（2026-10-15）。

| Issue | 平台 / 版本 | 分诊 | 结论 | 处理 |
|---|---|---|---|---|
| — | — | — | 暂无 | — |

## beta 期间的维护记录（2026-10-15）
- Release 对象核对（API，经 Windows 测试机出口查询）：`v1.0.0-beta.1` 存在，`draft=false`、`prerelease=true`、16 个文件、发布于 2026-10-15T08:25:37Z。`releases/latest` 返回 404 是 GitHub 的设计（该端点不含 pre-release 与草稿），不是发布失败。修复与加固：(1) 正式版构建在只有 pre-release 时查 `releases/latest` 得到 404，手动检查更新会报错，现按"无更新"处理；(2) release.yml 发布后经 API 自检 Release 对象（存在、draft/prerelease 标志、文件数与 `SHA256SUMS.txt`），用本次真实 JSON 验证过检查逻辑。
- 代码签名与公证调研：`docs/design/code-signing.md`（未执行；第 6 节 5 个待决事项）。

## 1.0 收尾中修复的缺陷（2026-10-14）
- 行内公式高于行高时基线偏高（`\frac{a}{b}` 偏 6 px）——README 截图发现，`inkra-layout/tests/inline_math.rs`
- macOS 经符号链接目录（`/var`、`/tmp`）打开的文件收不到外部修改通知（FS-007）——CI macos 发现
- macOS 测试线程上创建原生菜单 panic——CI macos 发现
- Mermaid：同名 subgraph 下标下溢 panic、`#fʿ` 颜色切片 panic（主题颜色解析同样问题）、嵌套 subgraph 宽度失控（1 077 120 px）——fuzz 发现
- 数学：嵌套 `\left…\middle` 指数级布局时间（翻倍输入 46 s → 4.5 ms），aligned 单元格克隆导致同类问题（8.3 s → 11 ms）——fuzz 发现
- Mermaid：大量虚线边切出数百万段（1.16 s → 55 ms，每条描边 ≤ ~300 段）——fuzz 发现
- 数学/图表布局 panic 改为降级显示源码（缓存边界 `catch_unwind`），不再每帧失败
- 更新检查指向占位仓库 `inkra-app/inkra`，且 beta 版看不到更新的 beta——改为 `CIPFZ/inkra` 与 SemVer 预发布比较
- Windows 打印把文件名拼进 PowerShell 脚本（`’` 可越出字符串）——改为环境变量传参
- Pandoc 临时文件名可预测（共享临时目录的符号链接风险）——改为 `create_new` 私有文件
- 许可证：egui 默认字体中的 Ubuntu Light（UFL）与 Hack 随二进制分发，违反"字体只用 OFL"——移除，改为自带 Noto Emoji（OFL）与 emoji-icon-font（MIT，界面图标字体）
- deb/rpm 缺 `libxkbcommon-x11` 依赖（X11 下无法开窗）——补依赖；预发布版本在 deb/rpm 中写作 `1.0.0~beta.1`

## 已完成的需求编号
- M0：AD-3（命令注册表）、CM-001（框架 + 3 个样例命令）、PR-001、PR-003、PR-018（键）、UI-001（菜单骨架，macOS muda 原生菜单）、UI-002、UI-010（默认快捷键表）、UI-050、NF-002（命令边界 catch_unwind）、NF-008、CM-004（`--version` / `--help` / `--measure-startup` / `--config` 等参数解析）
- M7：第 9 章偏好全部项、UI-020 快捷键编辑、PR-002/PR-021、UI-030 AccessKit、UI-033、UI-041、FS-009/010/011、FS-024、FS-041、ED-017、ED-093、WM-004、WM-005、TH-007、PR-020 更新检查、CM-002 CLI、NF-006 安装包与文件关联、帮助文档
- M6：EX-001～EX-006、EX-020、EX-021（HTML 两种、PDF、图片、Pandoc 导出/导入、页面设置、打印、复制为 HTML、自定义导出项）
- M5：MD-008（数学块、自动编号、`\tag`/`\label`/`\eqref`）、MD-035（行内公式）、MD-004、MD-010、MD-013、MD-014、MD-039/040、MD-064、MD-068、MD-078、MD-100～MD-102（Mermaid flowchart/sequence/pie、mmdc 兜底、主题配色）、ED-018、ED-041、ED-102～ED-105、PR-031/032/035/036/041
- M4：ED-080～ED-085（表格呈现、单元格编辑、工具条、命令、拖拽列宽与重排、源码对齐）、FS-020～FS-025（大纲、文章列表、文件树、当前文件定位、搜索入口、字数详情）、FS-030（快速打开）、FS-031（全局搜索）、PR-011（`current` 选项）、PR-014、PR-038、PR-039、PR-043、窗口状态记忆
- M3：ED-001～ED-006、ED-012～ED-014（实时模式语义）、ED-016 规则 1（粘贴位图）、ED-020、ED-030/031/033/034/039/040、ED-050/051/052/053/054/056～061/063/070、ED-090/091/092（本地图片、相对路径、`./<doc>.assets/` 复制）、ED-130（右键菜单 P0 项，表格菜单随 M4）、FS-001～FS-008、FS-040（视图菜单：侧边栏、源码模式、专注、打字机、状态栏、菜单栏、全屏、总在最前、缩放 50%～300%、性能叠层）、TH-001～TH-003（六个原创主题 + Auto + 热重载）、WM-001、WM-002、WM-003、MD-075、PR-010～PR-012、PR-016、PR-017、PR-030
- M2：ED-010（输入、IME 预编辑与候选窗定位）、ED-011（拖拽/Shift 扩展/双击词/三击块/Ctrl+A 两段式）、ED-012（源码模式下按字/词/行首删除）、ED-013（源码模式换行延续：列表、任务、引用、代码缩进、空项退出、嵌套空项先提升）、ED-014、ED-015、ED-016（复制 Markdown+HTML、粘贴 HTML→Markdown、URL 粘贴为链接、纯文本粘贴、AI 公式分隔符转换、文件列表）、ED-020（闪烁 530 ms、自动滚动留 2 行边距、PageUp/Down）、ED-100、ED-101（自动续写与有序列表重编号）、ED-110、ED-111、ED-120（切换命令与状态栏按钮）、ED-121（等宽、行号、Markdown 高亮、软换行）、FS-025（状态栏字数，基础）、FS-040（性能叠层）
- M1：AD-1、AD-2、MD-001～MD-015 的块级结构（含 Front Matter、TOC、脚注定义、Alerts、数学块）、MD-030～MD-043 行内解析（M2 内容一并完成）、MD-091（未修改字节逐字保留）、ED-015（撤销分组）、ED-106（编码与换行检测、原样回写）、TS-001、TS-002、TS-003

## 验收数据
### M0
| 验收项 | 结果 |
|---|---|
| 三平台 CI 绿 | Linux、Windows 本地 `cargo xtask ci` 绿；macOS 交叉 clippy 通过但未实机；三平台以 GitHub Actions（`.github/workflows/ci.yml`）为准，结果见文件头"CI 状态"（D-7） |
| `inkra --version` | 通过：输出 `Inkra 0.1.0`；Windows 下 GUI 子系统程序通过 AttachConsole 输出到控制台（cmd 提示符先返回，属 Windows 常规行为） |
| 空窗口启动 < 300 ms | **偏离**：Windows（RTX 3080 直通云主机，Release）DX12 ≈ 400 ms，其中显卡驱动初始化（适配器枚举 + 设备创建）≈ 330 ms；GL 后端 ≈ 260 ms；Linux（Xvfb + llvmpipe GL）≈ 120 ms。见开放问题 1 |
| 窗口、任务栏图标 | Windows 通过（截图确认标题栏与任务栏均为 Inkra 图标，exe 内嵌 ico 与版本信息）；Linux/macOS 未目视验证 |
### M1
| 验收项 | 结果 |
|---|---|
| CommonMark 块级例子 ≥ 95% | 296/296（100%）；全部 652 例通过（ParseOptions::commonmark） |
| GFM 表格 / 任务例子 100% | 表格 8/8、任务 2/2；GFM 扩展例子 19/19 全部通过 |
| 1 MB 文档单字符编辑重解析 < 2 ms | 平均 8.7 µs，最坏 48 µs（Release，Linux）；1 MB 全量解析 47 ms |
| proptest 无 panic | 随机 Markdown（含 2 KB 随机字节）不 panic；增量 = 全量解析 200,000 例 × 两种方言通过 |
| 任意文件 round-trip 字节一致 | 1673 个真实世界文件（rust-lang book/reference/nomicon/cargo/rbe、两个中文教程）解码→编码字节一致；插入编辑后增量 = 全量，撤销后字节一致 |

### M2
| 验收项 | 结果 |
|---|---|
| CommonMark 行内例子 ≥ 95% | 356/356（100%） |
| 源码模式完成 ED-010～016、ED-100、ED-101、ED-110、ED-111 | 通过：engine 单测 37 项 + egui_kittest 交互测试 12 项；Windows 实机验证微软拼音输入（预编辑下划线、候选窗跟随光标）、HTML 剪贴板转换、Ctrl+End、查找条 |
| 输入延迟 p95 < 16 ms（10 万字） | Windows 实机（性能叠层）：p50 1.29 ms、p95 2.36 ms；无头 CPU：p50 0.65 ms、p95 0.75 ms |
| 偏离 | ED-016 规则 1（粘贴位图）依赖 M3 的图片插入流程（ED-092），在 M3 实现；拼写检查为 P1 |

### M3
见 `docs/acceptance/m3.md`：10 个验收用例全部通过（7 个 egui_kittest 自动化脚本 + 3 个 Linux/Windows 实机手工验证）。

### M4
见 `docs/acceptance/m4.md`：全部验收项通过（快速打开 1 万文件 2 ms；全局搜索 1 万文件首批 1 ms）。

### M5
见 `docs/acceptance/m5.md`：数学语料 233 个公式、图表语料 35 个全部渲染。

### M6
见 `docs/acceptance/m6.md`：134 页 PDF 0.32 s；书签、可搜索文字、内外链接、表头重复；docx 往返。

### M7
见 `docs/acceptance/m7.md` 与 `docs/acceptance/behavior-checklist.md`（203 条，通过率 98.0%）。

### SPEC 审计补齐（2026-10-13）
逐章对照 SPEC 第 3～10 章补齐遗漏：主题字段全部生效与系统字体（TH-001/TH-006）、Windows 一体化标题栏（TH-008）、`\[ \]`/`\( \)` 公式与 `\eqref` 跳转、行内公式提示、HTML 样式子集与 `<details>` 折叠、跨文件锚点链接、会话恢复侧边栏/滚动/全屏、关闭最后窗口保持运行、快捷导出、导出错误对话框、Emoji 与符号、AccessKit 块节点、对话框 Enter、重新加载窗口、打字延迟基准与 p95 门槛、1 MB 随机输入测试、XWayland 回退、arm64/universal 打包参数、Flatpak 清单。行为清单扩充到 203 条（✅ 199、⚠ 4，98.0%）。测试策略补齐：TS-004 每个 Markdown 元素与三套主题的 PNG 快照（`crates/inkra-ui/tests/snapshots.rs`，Linux llvmpipe 与 Windows DX12 在同一阈值下一致），TS-006 基准增加打字延迟与 PDF/HTML 导出（121 页 PDF 0.11 s）。

## 已知问题
- AGENT.md 与仓库的小出入：默认分支是 `master`（AGENT.md 5.4 写 `main`）；`inkra --sample` 参数不存在（5.2 速查表）；`choose_backends` 函数不存在（T2 中参照的风格，已按同样思路写成 `apps/inkra/src/graphics.rs` 的纯函数）。
- 仍有超大但合法的 Mermaid 图（上百行、多层 subgraph）可能布局较宽，宽度上限为"全部节点并排"的自然宽度。
- 增量解析器修复：表格从段落中拆分出来的块不能作为重新同步点（proptest 发现，已加回归测试）。
- 标题栏文件名点击重命名（FS-008）：一体化标题栏（Windows 默认）点击文件名打开重命名框；系统原生标题栏上无法实现，改由 文件 → 重命名（FS-010）提供。
- Windows GUI 子系统下 `inkra --version` 的输出出现在 cmd 提示符之后（系统行为）；脚本场景可用 `inkra-cli`。
- egui-winit 在剪贴板无文本时不产生粘贴事件；Inkra 自行接管 winit 事件循环捕获 `Ctrl+V`（`apps/inkra/src/main.rs` 的 `Hooked`）。
- Wayland 下窗口图标依赖安装 `.desktop` 与 hicolor 图标：`cargo xtask dist` 的 tar.gz/deb/rpm/AppImage 均已包含；Wayland 未实测。

## 开放问题（需产品负责人决策）
- 已决策（2026-10-14）：(1) T2 偏离接受、不 fork wgpu；(2) T5 批准发布（release notes 满足清单即发布）；(3) T7 Q1～Q4 按推荐并补两条安全约束；(4) 六个许可证例外批准，MPL-2.0（仅限未修改链接）、BSL-1.0、CC0-1.0、CDLA-Permissive-2.0 加入 AGENT.md 第 4 节白名单，RUSTSEC-2026-0192 忽略接受、skrifa 迁移列为 T9。
- 代码签名（`docs/design/code-signing.md` 第 6 节）：签名主体（个人/组织）、Windows 方案（SignPath Foundation 免费但证书主体为 SignPath Foundation / Azure Artifact Signing 需主体在支持地区 / Certum + 自建 runner）、预算（Apple 99 美元/年）、时间点（1.0 前或维持 P2）、是否现在就加免费的 GitHub Artifact Attestations。
- 原开放问题（启动预算与 Windows 渲染后端）已于 2026-10-13 决策：D-1 保持 DX12 默认，D-2/D-3 改写预算，D-4 三项 DX12 启动优化纳入 T2。

## 回填到 SPEC 的决策
- 2026-10-12：2.2 层内子层（L1a/L1b、L3a/L3b）、i18n 横切、apps 与 xtask 的位置
- 2026-10-12：AD-6 唯一 unsafe 例外（apps/inkra Windows FFI）
- 2026-10-12：CM-004 `--measure-startup` 输出格式与退出、`--config <dir>` 语义
- 2026-10-12：10.2 快捷键冲突裁决（mac `Cmd+Shift+O`、`Ctrl+Shift+Backspace`、`Alt+Up/Down`）
- 2026-10-12：M3 主题改为 Paper/Ink（与 TH-002/NF-011 一致）
- 2026-10-12：附录 D-1 依赖系统 CJK 字体
- 2026-10-12：PR-018 键名 `general.language`
- 2026-10-13：ED-020 光标 30 s 无输入后停止闪烁（空闲 CPU 0%）
- 2026-10-13：数学字体选用 STIX Two Math（OFL，SPEC 2.1 允许的两者之一）
- 2026-10-13：PR-011 键 `general.openFileIn` 增加 `current`
- 2026-10-13：TH-002 字体名映射到随包字体（Inter / Source Serif 4 / JetBrains Mono），用户主题目录格式，选择主题时同步 Auto 明暗槽位
- 2026-10-13：2.1 Windows 后端保持 DX12 与 DX12 启动优化三项（D-1、D-4）
- 2026-10-13：2.5 冷启动只在单 GPU 实体机考核、内存预算分平台（D-2、D-3）
- 2026-10-14：11 CM-003 协议与安全边界（无头模式读写根目录、注入日志、截图不落盘）
- 2026-10-14：2.4 图表后台布局（T8：防抖、取消、过期）
- 2026-10-14：2.1 T2 实施结论（缓存不能跳过探测、`static-dxc` 不启用、关闭间接调用校验）
- 2026-10-13：13 章 1.0 收尾以 `1.0.0-beta.1` 公开 beta，彩色 emoji / 贴靠布局 / 原生 Wayland 不阻塞（D-5）；1.x 顺序 CM-003 先于 FS-050（D-6）

## 性能记录（实体机 / 虚拟 GPU 分开记）
| 指标 | 环境 | 值 | 日期 |
|---|---|---|---|
| 冷启动 | 单 GPU 实体机 | 未测（D-2 的考核环境） | |
| 冷启动 | Windows Server 2022 VM，3 适配器（2×RTX 3080 + WARP），DX12，Release | ~360 ms（枚举 ~207、设备与渲染器 ~88、界面与首帧 ~65） | 2026-10-14 |
| 冷启动 | 同上，T2 之前 | ~390 ms（设备与渲染器 ~124） | 2026-10-14 |
| 冷启动 | 同上，`WGPU_BACKEND=gl` | ~252 ms | 2026-10-14 |
| 冷启动 | Linux，Xvfb + llvmpipe GL | ~120 ms | 2026-10-12 |
| 着色器编译 DXC vs FXC | 同 VM（4 个着色器） | 120.9 vs 123.9 ms 设备与渲染器耗时，差 ~3 ms | 2026-10-14 |
| 1 MB Markdown 全量解析 / 单字符增量 | Linux | 47 ms / 平均 8.7 µs、最坏 48 µs | 2026-10-12 |
| 10 万字源码模式输入延迟 | Windows VM | p50 1.29 ms / p95 2.36 ms | 2026-10-12 |
| 10 万字实时模式输入 / 滚动帧 | 无头 CPU | p50 2.7 / p95 3.0 ms；滚动 p95 0.14 ms | 2026-10-13 |
| 快速打开 / 全局搜索（1 万文件） | Linux | 1.7～2.1 ms / 首批 1 ms、全部 40 ms | 2026-10-12 |
| 空闲 CPU | Linux | 0%（5 s 内 0 tick） | 2026-10-12 |
| 内存（空文档） | Windows VM | 工作集 211 MB / 专用 376 MB（含 GPU 驱动；D-3 按私有字节且不计驱动，待 VMMap 拆分） | 2026-10-13 |
| 内存（空文档） | Linux | 匿名内存 57 MB | 2026-10-13 |

## 测试环境备注
- Windows 测试机经代理上网，crates.io 索引不可达：依赖通过 `cargo vendor` 上传（`D:\work\vendor`，`~/.cargo/config.toml` 源替换）。C 盘空间不足，工作目录在 `D:\work`。
- Windows 生成工具：Visual Studio Build Tools 2026（VCTools + Windows SDK）；构建前通过 `vcvars64.bat` 导入环境。
- 语料：`cargo xtask corpus` 拉取固定提交的 7 个仓库到 `target/corpus`（不入库）。

## 下次会话从这里继续
- 收集 beta 反馈：Issues（模板）→ `docs/acceptance/user-reports.md` 与 `user-reports/` 语料；用户日志中的 `graphics:` 行用于实体机冷启动数据（D-2，T2 偏离的后续）。
- 依据反馈定 1.0 正式版范围；正式版 tag（无 `-`）由 release.yml 生成草稿，发布前按 AGENT.md 第 4 节第 5 条给出 release notes 确认。
- ROADMAP 余项：TH-007 高 DPI 实测、PR-035 Windows 词典、UI-001/UI-033 macOS/GNOME 实测、原生 Wayland、彩色 emoji、贴靠布局浮层、内存实体机 VMMap 拆分；1.x 顺序见 ROADMAP（CM-003 已完成，下一项 FS-050 标签页）。
- fuzz：`fuzz/run_all.sh [秒]`；读 Actions：`python3 tools/gh_runs.py [run_id]`。
