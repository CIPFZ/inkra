# Inkra 路线图

## 1.0 之后（来自 M7 行为清单的 ⚠ 项）

- TH-007：在 Windows/macOS 的 150% / 200% 系统缩放下实测（Linux X11 分数缩放已验证）。
- PR-035：Windows 上无系统 Hunspell 词典时可下载其他语言词典（目前回退到内置 en_US）。
- UI-001 / NF-006：macOS 系统菜单栏与 dmg 在真机上验证。
- UI-033：GNOME 与 macOS 上实测（Windows 已验证）。
- NF-006：原生 Wayland 实测（默认经 XWayland，`INKRA_WAYLAND=1` 强制原生，均未在 Wayland 会话实测）；Flatpak 清单已由 `cargo xtask dist` 生成，`flatpak-builder` 打包未实测；macOS universal dmg（`--target universal-apple-darwin`）未实机构建。
- NF-006 Windows arm64：`cargo xtask dist --target aarch64-pc-windows-msvc` 已实现（MSI `Platform="arm64"`）。测试机上全部 crate 已交叉编译通过（`ring` 需 LLVM clang），最终链接缺 MSVC ARM64 运行库（`msvcrt.lib`，VS 组件 `VC.Tools.ARM64`），测试机无法联网安装；需在装有该组件的机器上出包并在 arm64 设备上验证。
- MD-039 / TH-006：彩色 emoji。egui 文字管线只绘制单色字形，当前用内置 Noto Emoji；需要按位图（CBDT）或 COLR 图层把 emoji 作为行内对象绘制。
- TH-008：Windows 11 最大化按钮的"贴靠布局"浮层需要原生非客户区命中测试（`WM_NCHITTEST`），一体化标题栏暂不提供；可切换为系统标题栏。
- 性能：Windows（虚拟 GPU）冷启动 ~400 ms，高于 300 ms 预算。启动日志分解：引擎就绪 5 ms；DX12 枚举适配器 195 ms（该虚拟机报告 3 个适配器：两个 RTX 3080 与 WARP，wgpu 为每个创建设备探测）；设备创建与 FXC 着色器编译 125 ms；界面与首帧 70 ms。需在单 GPU 实体机上复测；可评估随包分发 DXC（需先确认许可证）以替代 FXC。
- 内存：Windows 空文档工作集 211 MB / 专用 376 MB，主要来自 GPU 驱动；需在实体机上用 VMMap 拆分并评估 GL 后端、较小的帧延迟与字体图集；Linux 匿名内存 57 MB（其中系统 CJK 字体 19 MB 读入内存，`forbid(unsafe)` 下无法 mmap）。
- deb 包未在 Debian/Ubuntu 上实际安装验证。

## 1.x（SPEC 13 P2 汇总）

标签页（FS-050）、Mermaid 全部图表类型、旧 flow/sequence 语法、图片上传服务（ED-094）、视频/音频卡片（ED-095）、全局替换、命令面板（FS-033）、控制通道与 MCP（CM-003）、宏录制、CSS 主题导入（TH-005）、TextBundle（EX-030）、富文本样式复制（EX-010）、tree-sitter 高亮、代码签名与公证、更多语言。
