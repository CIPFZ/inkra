# Inkra 开发进度

更新时间：2026-10-14
当前阶段：1.0 收尾 — T5 Beta 发布（等待产品负责人确认 release notes，见"开放问题"2）；T7 设计稿待确认
CI 状态：GitHub Actions（`CIPFZ/inkra`）`ci.yml` ubuntu 绿 / windows 绿 / macos 绿（运行 #12 起；另含 bench 同机 A/B、cargo deny + cargo audit、三平台 `cargo xtask dist`）；`release.yml` 最近一次：成功（运行 #6，push 触发的 dry run：全部产物 + 安装验证 + `SHA256SUMS.txt`）。本地 `cargo xtask ci` 绿（Linux）
当前版本：master 为 0.1.0；`1.0.0-beta.1` 的版本号、README、CHANGELOG、release notes 在本地分支 `beta1`（未推送，待确认）

## 本阶段任务进度
- [x] T1 决策落地与文档同步（2026-10-14）：SPEC 2.1/2.5/13 回填 D-1～D-6，ROADMAP 1.x 按 D-6 重排，旧启动 prompt 归档到 `docs/archive/AGENT_INKRA_m0.md`
- [x] T2 DX12 启动优化（2026-10-14）：适配器缓存 `general.graphics.lastAdapter`、WARP→GL 回退（带 `glFallback` 防循环）、`static-dxc` 评估后不启用、关闭间接调用校验；测试机冷启动 ~390 → ~360 ms。**偏离**：验收"命中缓存比首次快 100 ms"无法达成——wgpu 30 安全 API 每次枚举都探测全部适配器，缓存不能跳过探测（见 SPEC 2.1 [回填 2026-10-14]、开放问题 1）
- [x] T3 CI 三平台实跑与 Release 流水线（2026-10-14）：`ci.yml` 三平台绿；`release.yml` 产出 Linux x86_64/arm64（tar.gz、AppImage、deb、rpm、Flatpak）、Windows x64/arm64（zip、MSI）、macOS universal dmg，并在全新 runner 上安装验证（deb + Xvfb 首帧、rpm/Fedora、AppImage、tar.gz、Flatpak、MSI 关联/快捷方式/卸载于 windows-latest 与 windows-11-arm、dmg lipo + `--version`）；tag `v*` 时上传为 Release 草稿。说明：无 token 无法手动 `workflow_dispatch`，改为打包相关文件变更时 push 触发 dry run（等效）。失败日志通过公开的 annotation 读取（`tools/gh_runs.py`）
- [ ] T4 安全与健壮性收口：已完成 cargo deny（许可证白名单 + 具名例外）与 cargo audit 进 CI、外部进程复核（Pandoc/mmdc/自定义导出命令/打印）、`open_external` 统一检查（四类 + Windows 危险协议，带测试）、third-party.md 改为按 `cargo tree` 实际链接集生成、egui 非 OFL 字体移除；fuzz 5 个目标：markdown_parse、markdown_incremental、html_to_markdown 各 2 h 无崩溃；diagram 发现 3 个问题、math 发现 1 个（均已修复并转为回归测试），修复后 math、diagram 的 2 h 复跑进行中（预计 16:40 结束）
- [ ] T5 Beta 发布：版本号、README（截图 6 张、下载表、已知限制、反馈指引）、CHANGELOG、release notes 中英草稿（`docs/release-notes/v1.0.0-beta.1.md`）已在本地分支 `beta1` 完成；**等待确认后**推送、打 tag `v1.0.0-beta.1`、检查草稿并发布（pre-release）
- [x] T6 Dogfooding 与反馈闭环（2026-10-14）：issue 模板（bug / 渲染差异 / 功能请求，bug 模板要求平台、安装方式、版本、`graphics:` 行）、`docs/acceptance/user-reports.md` 与 `user-reports/` 语料测试；README 反馈指引在 `beta1`
- [ ] T7 CM-003：设计稿 `docs/design/control-protocol.md` 已完成（4 个开放问题），**待确认后实现**

## 1.0 收尾中修复的缺陷（2026-10-14）
- 行内公式高于行高时基线偏高（`\frac{a}{b}` 偏 6 px）——README 截图发现，`inkra-layout/tests/inline_math.rs`
- macOS 经符号链接目录（`/var`、`/tmp`）打开的文件收不到外部修改通知（FS-007）——CI macos 发现
- macOS 测试线程上创建原生菜单 panic——CI macos 发现
- Mermaid：同名 subgraph 下标下溢 panic、`#fʿ` 颜色切片 panic（主题颜色解析同样问题）、嵌套 subgraph 宽度失控（1 077 120 px）——fuzz 发现
- 数学：嵌套 `\left…\middle` 指数级布局时间（翻倍输入 46 s → 4.5 ms）——fuzz 发现
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
1. **T2 验收"命中缓存比首次快 100 ms"无法按原设想达成**：wgpu 30 的安全 API 在 DX12 上每次枚举都对每个适配器调用 `D3D12CreateDevice` 探测（`wgpu-hal` 的 `Adapter::expose` 是 crate 私有），缓存无法跳过探测；唯一途径是 fork/patch `wgpu-hal` 增加"按 LUID 只打开一个适配器"并在其中写 unsafe。推荐：不 fork，接受本次 −30 ms（关闭间接调用校验）的收益，在单 GPU 实体机上复测（那里只有 GPU + WARP 两个适配器，枚举成本约为测试机的 2/3）后再判断是否需要。不决策的后果：T2 按"偏离"关闭，冷启动在虚拟 GPU 测试机上仍为 ~360 ms（D-2 下仅作参考）。
2. **T5 发布确认（AGENT.md 第 4 节第 5 条）**：请审阅本地分支 `beta1` 的 release notes 草稿 `docs/release-notes/v1.0.0-beta.1.md`（中英）、README 新增的"下载 / 已知限制 / 反馈问题"段与 CHANGELOG 条目。推荐：确认后推送 `beta1` 到 master、打 tag `v1.0.0-beta.1`，`release.yml` 生成 Release 草稿，检查产物后标记 pre-release 发布。不决策的后果：beta 不发布，其余工作不受影响。
3. **T7 控制通道设计**：`docs/design/control-protocol.md` 第 10 节 Q1～Q4（传输格式、MCP 工具粒度、是否加偏好项、第一期是否含点击/按键/截图），每项附推荐选项。不决策的后果：按推荐方案实现需等确认（AGENT.md T7）。
4. **许可证例外确认**（AGENT.md 第 4 节第 4 条）：依赖树中有只以非白名单许可证发布的 crate，均非本次新增：`nucleo-matcher`、`spellbook`（MPL-2.0，SPEC 2.1 指定的模糊搜索与拼写检查）、`clipboard-win`、`error-code`（BSL-1.0，arboard 的 Windows 剪贴板）、`notify`（CC0-1.0，SPEC 2.1 文件监控）、`webpki-root-certs`（CDLA-Permissive-2.0，更新检查的根证书数据）。已在 `deny.toml` 中逐个列为具名例外并注明理由；推荐保留。另：`ttf-parser` 被 RustSec 标为不再维护（无漏洞），已忽略并列入 ROADMAP 改用 skrifa。
- 原开放问题（启动预算与 Windows 渲染后端）已于 2026-10-13 决策：D-1 保持 DX12 默认，D-2/D-3 改写预算，D-4 三项 DX12 启动优化纳入 T2（见 `AGENT.md` 第 3 节）。

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
- 先看 math、diagram 两个 fuzz 目标的 2 h 复跑结果（`fuzz/logs/{math,diagram}.log`，产物在 `fuzz/artifacts/`；`fuzz/run_all.sh` 可一次跑全部）；有新崩溃则转回归测试后关闭 T4。
- 产品负责人确认 release notes 后：`git checkout beta1 && git rebase master`，重新生成截图（`docs/images/README.md`），`cargo xtask ci`，合入 master 并推送，打 tag `v1.0.0-beta.1`，检查 `release.yml` 生成的 Release 草稿（产物 + `SHA256SUMS.txt`），标记 pre-release 发布；随后在 Windows 测试机上从 Release 下载 MSI 安装运行（T5 验收）。
- T7：按确认后的设计实现 `inkra-engine::control`、`inkra-ui::control`、`inkra --control`、`inkra-cli mcp`。
- 读 Actions 结果：`python3 tools/gh_runs.py`（运行列表）与 `python3 tools/gh_runs.py <run_id>`（annotation 与失败日志尾部）；无 gh CLI、无 token。
