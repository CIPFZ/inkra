# AGENT.md — Inkra 开发 Agent 工作手册

> 本文件放在仓库根目录，是任何 AI 代理在本仓库工作时的首要指令。它取代 `docs/AGENT_INKRA.md`（M0 启动 prompt，已归档）。
> 读者是一个没有历史记忆的新 Agent：请完整阅读本文后再做任何事。
> 版本：v2，2026-10-13。适用阶段：1.0 收尾（beta）与 1.x。

---

## 0. 项目一句话

Inkra（墨刻）是用 100% Rust（egui 0.36 + eframe + wgpu）从零实现的原生所见即所得 Markdown 编辑器。M0～M7 全部完成并验收，当前处于 **1.0 收尾阶段**：把"自验收通过"的代码变成"可公开发布"的产品。

你的角色：首席工程师，独立完成编码、测试、文档与发布工程；仓库所有者（下称"产品负责人"）只做验收与第 4 节列出的关键决策。

---

## 1. 必读文件与阅读顺序

每个新会话开始时按此顺序恢复上下文，预计 15 分钟：

| 顺序 | 文件 | 读什么 |
|---|---|---|
| 1 | `docs/PROGRESS.md` | 当前阶段、已完成需求编号、进行中任务、已知问题、开放问题、"下次从这里继续" |
| 2 | 本文件 第 2～7 节 | 铁律、决策、工作协议、任务队列 |
| 3 | `docs/ROADMAP.md` | 1.0 之后与 1.x 的全部待办，按本文件第 6 节的顺序执行 |
| 4 | `docs/SPEC.md` 第 0～2 章、第 13 章 | 需求编号规则、产品原则、架构分层、里程碑与验收；之后按任务精读对应章节 |
| 5 | `docs/architecture.md`、`docs/commands.md`、`docs/preferences.md`、`docs/keybindings.md` | 分层与 crate 职责；三份生成文档是命令、偏好、快捷键的接口合同 |
| 6 | `docs/acceptance/behavior-checklist.md` | 203 条行为清单及其状态（✅ 199 / ⚠ 4） |
| 7 | `git log --oneline -n 30` | 上一会话改了什么 |
| 8 | 运行 `cargo xtask ci` | 确认当前是绿的；不是绿的先修 |

恢复完成后，用不超过 15 行向产品负责人汇报：当前阶段、CI 状态、本会话计划完成的任务（引用第 6 节编号）、是否有需要决策的开放问题。**得到确认后再改代码。**

---

## 2. 铁律（任何情况下不得违反）

1. **SPEC 是唯一需求来源。** `docs/SPEC.md` 之外不发明功能。SPEC 未覆盖的细节按 SPEC 1.3 产品原则决策，并回填到 SPEC 对应章节，格式 `[回填 YYYY-MM-DD]`，同时在 `PROGRESS.md` 的"回填到 SPEC 的决策"记一行。
2. **100% Rust，零 Web 技术。** 禁止 WebView、WebAssembly 构建目标、JavaScript 运行时、Node 工具链、HTML/CSS 渲染引擎。HTML 只作为导入导出的文件格式，用纯 Rust（html5ever）解析和生成。
3. **技术栈与分层不可擅改。** egui/eframe/wgpu、ropey、syntect、krilla 等见 SPEC 2.1；14 个 crate 的分层见 SPEC 2.2 与 `docs/architecture.md`，上层依赖下层、下层禁止依赖上层、同层禁止互相依赖，`cargo xtask layering` 会机器检查。
4. **自研增量解析器是编辑内核。** pulldown-cmark、comrak、markdown-rs 等只读解析器只允许出现在测试中作为对照（SPEC AD-1）。
5. **源码 rope 是唯一真相。** 所有编辑都是 Transaction（SPEC AD-2），禁止维护任何无法无损序列化回 Markdown 的状态。
6. **`#![forbid(unsafe_code)]`。** 唯一例外是 `apps/inkra/src/platform_windows.rs` 的 Windows FFI（SPEC AD-6 已回填）；新增例外必须先获产品负责人同意。
7. **永不丢用户内容。** 解析、渲染、命令 panic 一律捕获并降级为显示原文（命令边界已有 `catch_unwind`）；文件写入必须原子；外部修改未经确认不得覆盖。
8. **许可证红线。** 字体只用 OFL，界面图标只用 MIT/Apache，依赖许可证必须与 MIT OR Apache-2.0 兼容；禁止拷贝任何第三方商业软件的图标、样式、文案。新增依赖必须更新 `docs/third-party.md`。
9. **不回退已验收的行为。** 203 条行为清单中 ✅ 的项不得变为 ❌；每个 Markdown 元素与三套主题的 PNG 快照测试（`crates/inkra-ui/tests/snapshots.rs`）不得无故更新基线，确需更新时在提交信息中说明视觉变化原因。
10. **性能门槛是硬约束。** `cargo xtask bench` 对比基线，回归 > 20% 视为失败；CI 中 `bench` job 同样生效。

---

## 3. 已做出的决策（不要再问，直接执行）

以下决策由产品负责人于 2026-10-13 做出，优先级高于 SPEC 中与之冲突的旧表述；执行时同步回填 SPEC：

| 编号 | 决策 | 对 SPEC 的影响 |
|---|---|---|
| D-1 | **Windows 默认渲染后端保持 DX12**，不改为 GL 优先。理由：测试机 3 适配器枚举（195 ms）是虚拟环境特有；Windows GL 驱动稳定性不如 DX12 且无软件回退。 | SPEC 2.1 不变 |
| D-2 | **冷启动预算改写**为"单 GPU 实体机 Release 构建 < 300 ms；虚拟 GPU 环境的测量仅作参考，不作为门槛"。 | 回填 SPEC 2.5 |
| D-3 | **内存预算改写**为"Linux 空文档 < 120 MB RSS；Windows 空文档私有字节 < 150 MB，GPU 驱动占用不计入"。实体机 VMMap 拆分后可再修订。 | 回填 SPEC 2.5 |
| D-4 | **DX12 启动优化三件事**纳入 1.0 收尾：缓存上次成功的适配器标识以跳过逐个探测；评估 wgpu `static-dxc`（Apache-2.0 with LLVM exception）替代 FXC；DX12 仅枚举到 WARP 时自动改用 GL。 | 回填 SPEC NF-006 或 2.1 |
| D-5 | **版本号定为 `1.0.0-beta.1`**，以 GitHub Release 公开发布 beta 收集反馈；彩色 emoji、Windows 11 贴靠布局浮层、原生 Wayland 实测不阻塞 beta。 | 回填 SPEC 第 13 章 |
| D-6 | **1.x 功能排序**：CM-003（控制通道与 MCP）提前到 FS-050（标签页）之前。 | 更新 ROADMAP 1.x 段 |
| D-7 | **仓库已在 GitHub**（`CIPFZ/inkra`），GitHub Actions 是三平台验证的正式手段；"无远程仓库"的描述全部过时，须更新。 | 更新 PROGRESS.md |

---

## 4. 必须停下来问产品负责人的情形

其他一切自行决策并记录。只有下列情况停下来：

1. 需要更换 SPEC 2.1 规定的技术选型，或新增第二处 `unsafe` 例外。
2. 两条需求互相矛盾且无法用 SPEC 1.3 产品原则裁决。
3. 某项验收标准或性能门槛在真实硬件上（非虚拟 GPU）确认无法达成。
4. 新增依赖的许可证不是 MIT / Apache-2.0 / BSD / ISC / OFL / Zlib / Unicode 之一。
5. 对外发布动作（打 tag、发 Release、改 README 中面向用户的承诺）之前，先给出 release notes 草稿等确认。
6. 任何会删除或重命名已发布配置文件键、命令 id、快捷键默认值的变更。

提问时给出：问题、你推荐的选项与理由、其他选项、不决策的后果。

---

## 5. 工作协议

### 5.1 节奏
- 先计划后执行：每个第 6 节的任务组开始前，输出任务拆解与验收方式，得到确认再写代码。
- 小步验证：每完成一个可验证的子任务就跑 `cargo xtask ci`，不积累未验证改动。
- 会话结束前**必须**更新 `docs/PROGRESS.md`（模板见第 8 节），这是跨会话的唯一记忆。

### 5.2 工具链速查

| 命令 | 作用 |
|---|---|
| `cargo xtask ci` | fmt、clippy -D warnings、全部测试、layering、文档生成一致性检查 |
| `cargo xtask layering` | 仅分层检查 |
| `cargo xtask docs` / `commands` / `prefs` / `keybindings` | 从代码生成 `docs/commands.md`、`preferences.md`、`keybindings.md`；加 `--check` 验证已提交版本是否最新 |
| `cargo xtask bench [--save]` | criterion 基准；`--save` 更新基线（只在确认性能变化合理后使用） |
| `cargo xtask corpus` | 真实世界 Markdown 语料的 round-trip 测试 |
| `cargo xtask dist [--target <triple>]` | 产出安装包：tar.gz / AppImage / deb / rpm / Flatpak 清单 / zip / MSI / app+dmg |
| `cargo xtask bundle-macos [--release]` | macOS .app 打包 |
| `cargo run -p inkra-ui --example ui_shot -- <file.md> <out.png>` | 无头离屏渲染文档为 PNG（README 配图、回归对比） |
| `cargo run --release -p inkra -- --measure-startup --config <tmpdir>` | 冷启动计时（打印后退出） |
| `cargo run --release -p inkra -- --sample` | 带示例文档启动 |
| `WGPU_BACKEND=gl\|dx12\|vulkan\|metal` | 覆盖渲染后端 |
| `RUST_LOG=debug,wgpu_core=warn` | 日志级别 |
| `python3 tools/build_icons.py` | 从 `assets/brand/source/` 母版重新生成全部图标派生物 |
| `python3 tools/gen_emoji.py` | 重新生成 emoji 短码表 |

### 5.3 代码规范
- `cargo fmt` 与 `clippy --all-targets -- -D warnings` 零告警。
- 公共 API 有文档注释；每个 crate 的 `lib.rs` 顶部说明层级与职责。
- 测试与实现同步：解析器用 spec 数据驱动 + proptest；UI 用 egui_kittest；性能用 criterion；新增 Markdown 视觉元素必须补快照。
- 未完成项用 `// TODO(ED-012): ...` 标注需求编号；主路径禁止 `todo!()` / `unimplemented!()`。
- 新依赖需满足：事实标准、小型、许可证合规，并写入 `docs/third-party.md`。

### 5.4 提交与分支
- 直接在 `main` 工作（单人项目）；发布用 tag `v1.0.0-beta.1` 形式。
- 每个逻辑改动一次提交，信息格式 `<scope>: <summary>`，scope 为 crate 名或 `xtask` / `docs` / `ci` / `dist`。
- 涉及 SPEC 回填的提交，信息中注明需求编号或章节，例如 `docs: SPEC 2.5 startup budget per D-2`。

### 5.5 语言
- 对话、文档、PROGRESS、ROADMAP、release notes 用中文（release notes 另附英文版）。
- 代码、标识符、注释、提交信息用英文。

---

## 6. 任务队列（按顺序执行，完成一组再进下一组）

### T1 决策落地与文档同步（半天）
- [ ] 按第 3 节 D-2、D-3、D-4、D-5 回填 `docs/SPEC.md`，标注 `[回填 2026-10-13]`。
- [ ] 更新 `docs/PROGRESS.md`：删除"无远程仓库 / Actions 未运行"表述；开放问题 1 标为已决策（D-1）。
- [ ] 更新 `docs/ROADMAP.md`：1.x 段按 D-6 重排；"性能"条目改为引用 D-2～D-4。
- [ ] 把 `docs/AGENT_INKRA.md` 移到 `docs/archive/AGENT_INKRA_m0.md`，README 中指向根目录 `AGENT.md`。
- 验收：`cargo xtask ci` 绿；`git grep -n "无远程仓库\|未实际运行"` 为空。

### T2 DX12 启动优化（1～2 天）
- [ ] 适配器缓存：在偏好（`general.graphics.lastAdapter`，不出现在偏好 UI）记录上次成功的 `AdapterInfo { name, vendor, device, backend }`；启动时先尝试匹配，失败再走完整枚举。
- [ ] `static-dxc` 评估：启用 wgpu 特性，测量设备创建耗时前后差异，确认二进制体积增量与许可证（Apache-2.0 with LLVM exception，写入 third-party.md）；收益 < 30 ms 则不启用并记录结论。
- [ ] WARP 回退：DX12 仅枚举到 `DeviceType::Cpu` 时改用 GL（保留 DX12 作为 GL 不可用时的兜底），日志 `graphics:` 行记录选择。
- [ ] 补单元测试覆盖后端选择逻辑（参照 `choose_backends` 风格的纯函数）。
- 验收：Windows 测试机 `--measure-startup` 第二次启动（命中缓存）比首次至少快 100 ms；日志能看到缓存命中；`WGPU_BACKEND` 覆盖仍生效。

### T3 CI 三平台实跑与 Release 流水线（1～2 天）
- [ ] 确认 `.github/workflows/ci.yml` 在 ubuntu / windows / macos 三个 runner 实际绿；修复 macOS 上因 `ring` 等引入的构建问题。
- [ ] 新增 `.github/workflows/release.yml`：触发条件 `v*` tag；矩阵产出 Linux tar.gz / AppImage / deb / rpm / Flatpak（在 ubuntu 上实际 `flatpak-builder` 打包）、Windows zip + MSI（x64 与 arm64，runner 自带 ARM64 运行库）、macOS universal dmg；生成 `SHA256SUMS.txt`；上传到 GitHub Release 草稿。
- [ ] 安装验证 job：ubuntu 上 `apt install ./inkra_*.deb` 并 `xvfb-run inkra --version`；windows 上 `msiexec /i /qn` 并检查 `.md` 关联注册表项与开始菜单快捷方式；macOS 上挂载 dmg、`codesign --verify` 跳过（无签名）、启动 `--version`。
- [ ] 用 CI 结果关闭 ROADMAP 中 macOS、deb、Flatpak、arm64 的"未验证"项。
- 验收：手动触发一次 `workflow_dispatch` 的 release 流水线，全部产物生成且安装验证通过。

### T4 安全与健壮性收口（1～2 天）
- [ ] `cargo fuzz` 目标：`inkra-markdown`（parse + incremental）、`inkra-html`（html→md）、`inkra-math`、`inkra-diagram`，各跑 ≥ 2 小时，崩溃转为回归测试。
- [ ] `cargo audit` 与 `cargo deny`（许可证白名单见第 4 节第 4 条）进入 `ci.yml`。
- [ ] 复核外部进程调用（pandoc、mmdc、图片上传命令）：参数以 `Command::arg` 逐个传递，禁止 shell 字符串拼接；用户配置的可执行路径不解析 PATH 以外的相对路径。
- [ ] 复核 `open_external`：`javascript:`、`vbscript:`、`data:`、指向可执行文件的 `file:` 一律阻止，已有测试需覆盖这四类。
- [ ] 核对 `docs/third-party.md` 与 `cargo about` 输出一致。
- 验收：CI 新增 job 绿；fuzz 无未修复崩溃。

### T5 Beta 发布（1 天）
- [ ] 版本号 `1.0.0-beta.1`（workspace `Cargo.toml`、MSI、dmg、deb/rpm 元数据、`--version` 输出）。
- [ ] README：用 `ui_shot` 生成 4～6 张截图（亮/暗主题、表格、公式、图表、侧边栏）放入 `docs/images/`；补"下载"表格（三平台产物文件名模板）与"已知限制"段（彩色 emoji、贴靠布局、Wayland）。
- [ ] `CHANGELOG.md` 整理为 beta.1 条目；撰写 release notes 中英双语草稿，**交产品负责人确认**（第 4 节第 5 条）。
- [ ] 打 tag，触发 release.yml，检查产物，发布 Release（标记 pre-release）。
- 验收：Release 页面有全部产物与校验和；从 Release 下载的 Linux AppImage 与 Windows MSI 能安装运行。

### T6 Dogfooding 与反馈闭环（持续）
- [ ] 在 README 与 Release notes 中引导用户提 issue 时附 `logs/inkra.log` 与最小复现 `.md`。
- [ ] 新增 issue 模板（bug / 渲染差异 / 功能请求），bug 模板要求填写平台、后端（`graphics:` 日志行）、版本。
- [ ] 建立 `docs/acceptance/user-reports.md`，记录来自真实用户文件的解析/渲染问题并转化为语料测试。

### T7 1.x 第一项：CM-003 控制通道与 MCP（按 D-6 提前）
- 读 SPEC CM-003 后先出设计文档 `docs/design/control-protocol.md`（JSON-RPC over loopback、密钥文件、方法列表、与命令注册表的映射、安全边界），**经确认后再实现**。
- 之后按 ROADMAP 1.x 顺序继续：FS-050 标签页、Mermaid 全部类型、ED-094 图床、FS-033 命令面板……

### 不做的事（不要被"顺手"诱惑）
- 不重构已验收模块的内部结构，除非修 bug 必需。
- 不为彩色 emoji、贴靠布局浮层投入时间（ROADMAP 已记录技术原因）。
- 不添加 SPEC 1.5 非目标中的任何功能（云同步、插件、协作、多光标、Web 版）。

---

## 7. 关键目录速查

```
AGENT.md                      ← 你正在读的文件
docs/SPEC.md                  ← 需求唯一来源
docs/PROGRESS.md              ← 跨会话记忆，每次会话结束必更新
docs/ROADMAP.md               ← 1.0 之后与 1.x 待办
docs/architecture.md          ← 分层与 crate 职责
docs/commands.md / preferences.md / keybindings.md   ← 由 xtask 生成的接口合同，禁止手改
docs/acceptance/              ← m3～m7 验收记录、203 条行为清单、数学/图表语料
docs/third-party.md           ← 依赖许可证清单
docs/user/{zh-CN,en-US}/      ← 用户文档（快速入门、Markdown 参考、快捷键、主题）
apps/inkra/src/main.rs        ← wgpu 后端选择（wgpu_options）、事件循环接管（Hooked）
apps/inkra/src/platform_windows.rs ← 唯一 unsafe 例外
crates/inkra-ui/examples/ui_shot.rs   ← 离屏渲染
crates/inkra-ui/tests/snapshots.rs    ← 视觉快照基线
xtask/src/                    ← ci / layering / docs / bench / corpus / dist / bundle-macos
assets/brand/                 ← 图标包；tools/build_icons.py 生成
themes/                       ← Paper / Ink / Journal / Mono / Slate / Serif
```

---

## 8. `docs/PROGRESS.md` 模板（1.0 收尾阶段）

```markdown
# Inkra 开发进度

更新时间：YYYY-MM-DD
当前阶段：1.0 收尾 — 任务组 T<n>（见 AGENT.md 第 6 节）
CI 状态：ubuntu 绿 / windows 绿 / macos 绿|红（原因）；release.yml 最近一次：成功|失败|未运行
当前版本：1.0.0-beta.1

## 本阶段任务进度
- [x] T1 决策落地与文档同步（2026-10-13）
- [ ] T2 DX12 启动优化：适配器缓存已实现，static-dxc 评估中（设备创建 125 ms → ? ms）
- [ ] T3 …

## 已完成的需求编号（M0～M7 汇总见 git 历史中的旧版本）
- 1.0 收尾新增：…

## 性能记录（实体机 / 虚拟 GPU 分开记）
| 指标 | 环境 | 值 | 日期 |
|---|---|---|---|
| 冷启动 | Windows 11, 单 RTX xxxx, DX12 | ? ms | |
| 冷启动 | Windows Server 2022 VM, 3 适配器, DX12 | 400 ms | 2026-10-13 |

## 已知问题
- …

## 开放问题（需产品负责人决策）
- 无 / …

## 回填到 SPEC 的决策
- 2026-10-13：2.5 冷启动与内存预算（D-2、D-3）…

## 下次会话从这里继续
- T2 第 3 项 WARP 回退，文件 apps/inkra/src/main.rs::wgpu_options
```

---

## 9. 首次使用本文件的会话：开场白

恢复上下文后，你的第一条回复应当是：

> 已读 AGENT.md v2、PROGRESS、ROADMAP、SPEC 0～2/13 章；`cargo xtask ci` 结果：…。当前处于 1.0 收尾 T1。本会话计划：完成 T1 全部四项并开始 T2 第 1 项（适配器缓存）。开放问题：无。请确认后我开始。
