# Inkra 路线图

## 1.0 之后（来自 M7 行为清单的 ⚠ 项）

- TH-007：在 Windows/macOS 的 150% / 200% 系统缩放下实测（Linux X11 分数缩放已验证）。
- PR-035：Windows 上无系统 Hunspell 词典时可下载其他语言词典（目前回退到内置 en_US）。
- UI-001 / NF-006：macOS 系统菜单栏与 dmg 在真机上验证。
- UI-033：GNOME 与 macOS 上实测（Windows 已验证）。
- NF-006：Wayland 实测；Flatpak 清单；Windows arm64 构建。
- 性能：Windows（虚拟 GPU）冷启动 ~390 ms，高于 300 ms 预算；DX12 设备创建约占 330 ms，需在实体机上复测并评估延迟创建表面。
- 内存：Windows 空文档工作集 211 MB / 专用 376 MB，主要来自 GPU 驱动；需在实体机上用 VMMap 拆分并评估 GL 后端、较小的帧延迟与字体图集；Linux 匿名内存 57 MB（其中系统 CJK 字体 19 MB 读入内存，`forbid(unsafe)` 下无法 mmap）。
- deb 包未在 Debian/Ubuntu 上实际安装验证。

## 1.x（SPEC 13 P2 汇总）

标签页（FS-050）、Mermaid 全部图表类型、旧 flow/sequence 语法、图片上传服务（ED-094）、视频/音频卡片（ED-095）、全局替换、命令面板（FS-033）、控制通道与 MCP（CM-003）、宏录制、CSS 主题导入（TH-005）、TextBundle（EX-030）、富文本样式复制（EX-010）、tree-sitter 高亮、代码签名与公证、更多语言。
