<p align="center"><img src="assets/brand/inkra-128.png" width="96" alt="Inkra"></p>

# Inkra（墨刻）

原生、轻量的所见即所得 Markdown 编辑器。100% Rust：界面由 egui 在 GPU 上自绘，排版、语法高亮、数学公式、图表、PDF 生成全部为原生实现，不内嵌浏览器内核，不依赖 JavaScript 运行时。

- 单视图实时渲染：光标所在块显示 Markdown 标记，其他块显示排版结果；磁盘上始终是标准 `.md`。
- 文档的唯一真相是源码 rope，所有编辑都是可撤销的 Transaction。
- 每个动作都是一个命令（`docs/commands.md`），菜单、快捷键、CLI 都只是入口。

需求与设计见 [`docs/SPEC.md`](docs/SPEC.md)，进度见 [`docs/PROGRESS.md`](docs/PROGRESS.md)。

## 构建

需要 Rust stable（2024 edition，≥ 1.98）。

```sh
cargo run -p inkra                 # 启动桌面应用
cargo run -p inkra -- --version
cargo run --release -p inkra -- --measure-startup   # 打印到首帧呈现的耗时
cargo run -p inkra-cli -- --help
cargo xtask ci                     # fmt + clippy -D warnings + test + layering + 生成文档检查
cargo xtask docs                   # 重新生成 docs/commands.md、preferences.md、keybindings.md
cargo xtask bundle-macos --release # 生成 target/release/Inkra.app
cargo xtask dist                   # 本平台安装包到 target/dist（Linux：tar.gz/deb/rpm/AppImage；Windows：zip/MSI；macOS：app/dmg）
```

命令行（CM-002）：

```sh
inkra-cli convert doc.md doc.pdf          # 也支持 .html .png .jpg；装有 Pandoc 时 .docx .odt .epub .tex 等
inkra-cli inspect doc.md --json           # 标题、链接、图片、字数
inkra-cli lint doc.md                     # 断链、缺图、未闭合围栏、未定义脚注
inkra-cli fmt doc.md --write              # 规范化表格、编号、空白
inkra-cli run doc.md --cmd edit.selectAll --cmd format.bold --save out.md
```

打包工具（可选）：Linux `rpmbuild`、`appimagetool`；Windows WiX Toolset 3（`candle`/`light` 在 PATH 或 `WIX` 指向其目录）；macOS `hdiutil`。

平台依赖：

- Linux：`libxkbcommon`、`libwayland`、X11 开发库，以及 Vulkan 或 GL 驱动（Mesa 即可）。
- Windows：MSVC 生成工具与 Windows SDK（嵌入图标资源需要 `rc.exe`）。
- macOS：Xcode 命令行工具。

## 目录结构

```
crates/
  inkra-text        L0  rope 封装、偏移换算、Unicode 分词、编码与换行符
  inkra-i18n        L0  fluent 资源加载（横切，可被任意层依赖）
  inkra-markdown    L1a 自研增量 Markdown 解析器、AST、无损序列化
  inkra-doc         L1b 文档模型：rope + AST 缓存、Transaction、撤销栈
  inkra-highlight   L2  syntect 代码高亮
  inkra-math        L2  TeX 数学排版
  inkra-diagram     L2  Mermaid 子集
  inkra-html        L2  HTML ↔ Markdown
  inkra-layout      L3a 文档排版、命中测试、布局缓存
  inkra-render      L3b 布局树 → egui Shape、离屏位图
  inkra-export      L4  PDF / HTML / 图片导出、Pandoc 桥接
  inkra-workspace   L4  路径、原子写入、日志、文件树、监控、搜索、会话
  inkra-engine      L5  命令系统、编辑器状态机、偏好设置、主题、快捷键、菜单模型
  inkra-ui          L6  egui 前端
apps/
  inkra             桌面应用
  inkra-cli         命令行
xtask/              ci、layering、文档生成、打包
assets/brand/       应用图标包（由 tools/build_icons.py 生成）
docs/               SPEC、架构、命令目录、偏好与快捷键表、进度
```

分层规则（`cargo xtask layering` 机器检查）：上层可依赖下层，下层禁止依赖上层，同层禁止互相依赖；详见 [`docs/architecture.md`](docs/architecture.md)。

## 许可证

MIT OR Apache-2.0，见 `LICENSE-MIT` 与 `LICENSE-APACHE`。应用图标为项目原创，随代码以相同许可证发布。
