# Inkra 路线图

## 1.0 之后（来自 M7 行为清单的 ⚠ 项）

- TH-007：在 Windows/macOS 的 150% / 200% 系统缩放下实测（Linux X11 分数缩放已验证）。
- PR-035：Windows 上无系统 Hunspell 词典时可下载其他语言词典（目前回退到内置 en_US）。
- UI-001：macOS 系统菜单栏在真机上目视验证（dmg 已由 CI 验证，见下）。
- UI-033：GNOME 与 macOS 上实测（Windows 已验证）。
- NF-006：原生 Wayland 实测（默认经 XWayland，`INKRA_WAYLAND=1` 强制原生，均未在 Wayland 会话实测；D-5 不阻塞 beta）。
- [已关闭 2026-10-14，`release.yml` 运行 #6] Flatpak：ubuntu 22.04 x86_64/arm64 上 `flatpak-builder`（清单 `appstream-compose: false`）打包，`flatpak install --bundle` 后 `flatpak run io.inkra.Inkra --version` 通过；macOS universal dmg：macos-latest 构建，挂载后 `lipo` 含 x86_64 与 arm64、`--version` 通过（未签名，跳过 `codesign --verify`）。
- [已关闭 2026-10-14，`release.yml` 运行 #6] NF-006 Windows arm64：windows-latest 交叉构建 MSI 与 zip，windows-11-arm 上 `msiexec /i /qn` 安装、`.md` 关联（HKLM `Inkra.Markdown`）、开始菜单快捷方式、`inkra-cli --version`、卸载全部通过。
- MD-039 / TH-006：彩色 emoji。egui 文字管线只绘制单色字形，当前用内置 Noto Emoji；需要按位图（CBDT）或 COLR 图层把 emoji 作为行内对象绘制。
- TH-008：Windows 11 最大化按钮的"贴靠布局"浮层需要原生非客户区命中测试（`WM_NCHITTEST`），一体化标题栏暂不提供；可切换为系统标题栏。
- 性能（D-2、D-4）：冷启动门槛按 SPEC 2.5 [回填 2026-10-13] 只在单 GPU 实体机上考核；虚拟 GPU 测试机（Windows Server 2022，3 个适配器）仅作参考。T2 后测试机 ~360 ms：引擎就绪 4 ms；窗口与 DX12 枚举 ~207 ms（两个 RTX 3080 与 WARP，wgpu 为每个创建设备探测）；设备与渲染器 ~88 ms（原 124 ms，关闭间接调用校验后少编译两个着色器）；界面与首帧 ~65 ms。适配器缓存因 wgpu 30 无法只打开单个 DX12 适配器而不能跳过探测（PROGRESS 开放问题 1）；`static-dxc` 收益 ~3 ms，不启用。待办：单 GPU 实体机复测；可选把字体加载（~16 ms）移到与 GPU 初始化并行的后台线程。
- 内存（D-3）：门槛改为 Linux 空文档 < 120 MB RSS、Windows 空文档私有字节 < 150 MB（GPU 驱动占用不计入）。Windows 测试机空文档工作集 211 MB / 专用 376 MB，主要来自 GPU 驱动，需在实体机上用 VMMap 拆分后再判定；Linux 匿名内存 57 MB（其中系统 CJK 字体 19 MB 读入内存，`forbid(unsafe)` 下无法 mmap）。
- [已关闭 2026-10-14，`release.yml` 运行 #6] deb/rpm：ubuntu-latest（x86_64）与 ubuntu-24.04-arm 上 `apt install ./inkra_*.deb`、`.desktop`/MIME/图标就位、Xvfb 下 `--measure-startup` 出首帧；Fedora 容器 `dnf install` rpm 后 `--version` 通过。
- ttf-parser 已不再维护（RUSTSEC-2026-0192，无漏洞）：`inkra-math` 与 `inkra-diagram` 改用 skrifa（已随 krilla 在依赖树中）读取 MATH 表与字形轮廓；`deny.toml` 中的忽略项随之删除。

## 1.x（SPEC 13 P2 汇总，按 D-6 排序）

按以下顺序执行（D-6：控制通道提前到标签页之前）：

1. 控制通道与 MCP（CM-003）：先出设计文档 `docs/design/control-protocol.md`，经确认后实现。
2. 标签页（FS-050）。
3. Mermaid 全部图表类型；旧 flow/sequence 语法。
4. 图片上传服务（ED-094）。
5. 命令面板（FS-033）。
6. 视频/音频卡片（ED-095）、全局替换、宏录制。
7. CSS 主题导入（TH-005）、TextBundle（EX-030）、富文本样式复制（EX-010）。
8. tree-sitter 高亮、代码签名与公证、更多语言。
