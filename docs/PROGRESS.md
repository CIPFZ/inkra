# Inkra 开发进度

更新时间：2026-10-13
当前里程碑：M4 表格、侧边栏与搜索（M0～M3 已完成，P0 主体已交付）
CI 状态：绿（`cargo xtask ci`：Linux x86_64 本机、Windows Server 2022 测试机均通过；macOS 仅做交叉 `cargo clippy --target aarch64-apple-darwin`，未实机运行；无 GitHub 远程仓库，Actions 未实际运行）

## 已完成的需求编号
- M0：AD-3（命令注册表）、CM-001（框架 + 3 个样例命令）、PR-001、PR-003、PR-018（键）、UI-001（菜单骨架，macOS muda 原生菜单）、UI-002、UI-010（默认快捷键表）、UI-050、NF-002（命令边界 catch_unwind）、NF-008、CM-004（`--version` / `--help` / `--measure-startup` / `--config` 等参数解析）
- M3：ED-001～ED-006、ED-012～ED-014（实时模式语义）、ED-016 规则 1（粘贴位图）、ED-020、ED-030/031/033/034/039/040、ED-050/051/052/053/054/056～061/063/070、ED-090/091/092（本地图片、相对路径、`./<doc>.assets/` 复制）、ED-130（右键菜单 P0 项，表格菜单随 M4）、FS-001～FS-008、FS-040（视图菜单：侧边栏、源码模式、专注、打字机、状态栏、菜单栏、全屏、总在最前、缩放 50%～300%、性能叠层）、TH-001～TH-003（六个原创主题 + Auto + 热重载）、WM-001、WM-002、WM-003、MD-075、PR-010～PR-012、PR-016、PR-017、PR-030
- M2：ED-010（输入、IME 预编辑与候选窗定位）、ED-011（拖拽/Shift 扩展/双击词/三击块/Ctrl+A 两段式）、ED-012（源码模式下按字/词/行首删除）、ED-013（源码模式换行延续：列表、任务、引用、代码缩进、空项退出、嵌套空项先提升）、ED-014、ED-015、ED-016（复制 Markdown+HTML、粘贴 HTML→Markdown、URL 粘贴为链接、纯文本粘贴、AI 公式分隔符转换、文件列表）、ED-020（闪烁 530 ms、自动滚动留 2 行边距、PageUp/Down）、ED-100、ED-101（自动续写与有序列表重编号）、ED-110、ED-111、ED-120（切换命令与状态栏按钮）、ED-121（等宽、行号、Markdown 高亮、软换行）、FS-025（状态栏字数，基础）、FS-040（性能叠层）
- M1：AD-1、AD-2、MD-001～MD-015 的块级结构（含 Front Matter、TOC、脚注定义、Alerts、数学块）、MD-030～MD-043 行内解析（M2 内容一并完成）、MD-091（未修改字节逐字保留）、ED-015（撤销分组）、ED-106（编码与换行检测、原样回写）、TS-001、TS-002、TS-003

## 验收数据
### M0
| 验收项 | 结果 |
|---|---|
| 三平台 CI 绿 | Linux、Windows 本地 `cargo xtask ci` 绿；macOS 交叉 clippy 通过但未实机；GitHub Actions 工作流已写好（`.github/workflows/ci.yml`），因无远程仓库未运行 |
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

## 进行中
- [ ] M4：表格编辑（ED-080～085 全部 P0/P1）、侧边栏三视图（FS-020～FS-025）、快速打开（FS-030）、全局搜索（FS-031）、字数详情、外观偏好、窗口状态记忆

## 已知问题
- 第二个及以后的窗口（egui immediate viewport）不记忆位置尺寸；根窗口由 eframe persistence 记忆。
- 标题栏文件名点击重命名（FS-008）无法在系统原生标题栏上实现，改由 文件 → 重命名（FS-010，P1）提供。
- Windows GUI 子系统下 `inkra --version` 的输出出现在 cmd 提示符之后（系统行为）；脚本场景可用 `inkra-cli`。
- egui-winit 在剪贴板无文本时不产生粘贴事件；Inkra 自行接管 winit 事件循环捕获 `Ctrl+V`（`apps/inkra/src/main.rs` 的 `Hooked`）。
- egui 内置界面主题暂为默认外观，品牌主题（Paper/Ink）随 M3 TH-001～TH-003 交付。
- Wayland 下窗口图标依赖安装 `.desktop` 与 hicolor 图标（`apps/inkra/linux/`），打包在 M7。

## 开放问题（需产品负责人决策）
1. **启动预算与 Windows 渲染后端**：SPEC 2.1 规定 Windows 默认 DX12；在测试机上 DX12 驱动初始化即占 ~330 ms，冷启动 ~400 ms，超出 300 ms 预算；GL 后端 ~260 ms 达标但稳定性不如 DX12。当前保持 DX12 默认（可用 `WGPU_BACKEND=gl` 覆盖），需决策是否改为"GL 优先、DX12 回退"或放宽该项预算。

## 回填到 SPEC 的决策
- 2026-10-12：2.2 层内子层（L1a/L1b、L3a/L3b）、i18n 横切、apps 与 xtask 的位置
- 2026-10-12：AD-6 唯一 unsafe 例外（apps/inkra Windows FFI）
- 2026-10-12：CM-004 `--measure-startup` 输出格式与退出、`--config <dir>` 语义
- 2026-10-12：10.2 快捷键冲突裁决（mac `Cmd+Shift+O`、`Ctrl+Shift+Backspace`、`Alt+Up/Down`）
- 2026-10-12：M3 主题改为 Paper/Ink（与 TH-002/NF-011 一致）
- 2026-10-12：附录 D-1 依赖系统 CJK 字体
- 2026-10-12：PR-018 键名 `general.language`
- 2026-10-13：ED-020 光标 30 s 无输入后停止闪烁（空闲 CPU 0%）
- 2026-10-13：TH-002 字体名映射到随包字体（Inter / Source Serif 4 / JetBrains Mono），用户主题目录格式，选择主题时同步 Auto 明暗槽位

## 性能记录
- 空窗口冷启动（Release，Windows Server 2022，RTX 3080，DX12）：~400 ms（热）/ ~690 ms（首次）；GL：~260 ms
- 空窗口冷启动（Release，Linux，Xvfb + llvmpipe GL）：~120 ms
- 1 MB Markdown 全量解析：47 ms；单字符增量编辑：平均 8.7 µs / 最坏 48 µs
- 10 万字源码模式输入延迟（Windows 实机）：p50 1.29 ms / p95 2.36 ms
- 10 万字实时模式输入（无头 CPU）：p50 2.7 ms / p95 3.0 ms；滚动帧 p95 0.14 ms
- 空闲 CPU：0%（Linux 实测 5 s 内 0 tick）

## 测试环境备注
- Windows 测试机经代理上网，crates.io 索引不可达：依赖通过 `cargo vendor` 上传（`D:\work\vendor`，`~/.cargo/config.toml` 源替换）。C 盘空间不足，工作目录在 `D:\work`。
- Windows 生成工具：Visual Studio Build Tools 2026（VCTools + Windows SDK）；构建前通过 `vcvars64.bat` 导入环境。
- 语料：`cargo xtask corpus` 拉取固定提交的 7 个仓库到 `target/corpus`（不入库）。

## 下次会话从这里继续
- M4：先做表格（inkra-layout 表格网格已能显示，需单元格编辑、Tab 导航、表格命令与工具条、源码对齐格式化），再做侧边栏（大纲、文章列表、文件树）、快速打开（nucleo）与全局搜索（grep-searcher + ignore）。
