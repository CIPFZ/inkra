# Inkra 开发进度

更新时间：2026-10-13
当前里程碑：M2 行内解析与源码模式编辑器（M0、M1 已完成）
CI 状态：绿（`cargo xtask ci`：Linux x86_64 本机、Windows Server 2022 测试机均通过；macOS 仅做交叉 `cargo clippy --target aarch64-apple-darwin`，未实机运行；无 GitHub 远程仓库，Actions 未实际运行）

## 已完成的需求编号
- M0：AD-3（命令注册表）、CM-001（框架 + 3 个样例命令）、PR-001、PR-003、PR-018（键）、UI-001（菜单骨架，macOS muda 原生菜单）、UI-002、UI-010（默认快捷键表）、UI-050、NF-002（命令边界 catch_unwind）、NF-008、CM-004（`--version` / `--help` / `--measure-startup` / `--config` 等参数解析）
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

## 进行中
- [ ] M2：源码模式编辑器（inkra-layout 文本运行与命中测试、inkra-render、inkra-ui 编辑画布、IME、选区、剪贴板、查找替换、Markdown 高亮、行号、自动配对、列表延续）

## 已知问题
- Windows GUI 子系统下 `inkra --version` 的输出出现在 cmd 提示符之后（系统行为）；脚本场景可用 `inkra-cli`。
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

## 性能记录
- 空窗口冷启动（Release，Windows Server 2022，RTX 3080，DX12）：~400 ms（热）/ ~690 ms（首次）；GL：~260 ms
- 空窗口冷启动（Release，Linux，Xvfb + llvmpipe GL）：~120 ms
- 1 MB Markdown 全量解析：47 ms；单字符增量编辑：平均 8.7 µs / 最坏 48 µs

## 测试环境备注
- Windows 测试机经代理上网，crates.io 索引不可达：依赖通过 `cargo vendor` 上传（`C:\work\vendor`，`~/.cargo/config.toml` 源替换）。
- Windows 生成工具：Visual Studio Build Tools 2026（VCTools + Windows SDK）。
- 语料：`cargo xtask corpus` 拉取固定提交的 7 个仓库到 `target/corpus`（不入库）。

## 下次会话从这里继续
- 进入 M2：先实现 inkra-layout 的通用"文本运行 + 源码映射 + 命中测试"框架（源码模式与 M3 实时渲染共用），再做 inkra-ui 编辑画布与 inkra-engine 编辑器状态机。
