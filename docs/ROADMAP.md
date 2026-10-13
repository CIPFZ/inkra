# Inkra 路线图

## 1.0 之后（来自 M7 行为清单的 ⚠ 项）

- EX-001：HTML 导出的 MathML 公式输出选项（目前为 SVG 或 TeX）。
- TH-007：在 125% / 150% / 200% 系统缩放下实测（测试环境为 100%）。
- PR-035：除 en_US 外的拼写词典（按系统语言选择）。
- UI-001 / NF-006：macOS 系统菜单栏与 dmg 在真机上验证。
- UI-033：在开启"减少动画"的系统上实测。
- UI-040：插入链接对话框（目前 `Ctrl+K` 直接插入并取剪贴板地址）。
- NF-006：Wayland 实测；Flatpak 清单；Windows arm64 构建。
- 性能：Windows（虚拟 GPU）冷启动 ~390 ms，高于 300 ms 预算；DX12 设备创建约占 330 ms，需在实体机上复测并评估延迟创建表面。
- 内存：Windows 空文档工作集 211 MB / 专用 376 MB，主要来自 GPU 驱动；需在实体机上用 VMMap 拆分并评估 GL 后端、较小的帧延迟与字体图集；Linux 匿名内存 57 MB（其中系统 CJK 字体 19 MB 读入内存，`forbid(unsafe)` 下无法 mmap）。
- deb 包未在 Debian/Ubuntu 上实际安装验证。

## 1.x（SPEC 13 P2 汇总）

标签页（FS-050）、Mermaid 全部图表类型、旧 flow/sequence 语法、图片上传服务（ED-094）、视频/音频卡片（ED-095）、全局替换、命令面板（FS-033）、控制通道与 MCP（CM-003）、宏录制、CSS 主题导入（TH-005）、TextBundle（EX-030）、富文本样式复制（EX-010）、tree-sitter 高亮、代码签名与公证、更多语言。
