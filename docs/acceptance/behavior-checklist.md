# 产品行为清单（M7 验收）

由 SPEC 第 3 至 10 章全部 P0/P1 需求生成的可观察行为。每条给出验证方式：**自动**（测试名，`cargo xtask ci` 在 Linux 与 Windows 上运行）、**手工 L / W**（Linux Xvfb / Windows 10 实机截图核对）。结果：✅ 通过、⚠ 部分通过（偏离写在备注，剩余项列入 ROADMAP）、❌ 未通过。P2 与已声明的非目标（ED-019 多光标）不计入。

日期：2026-10-13。

## 3. 编辑器

| # | 需求 | 可观察行为 | 验证 | 结果 |
|---|---|---|---|---|
| 1 | ED-001 | 单一视图中 Markdown 直接显示为排版结果 | 自动 `m3_case1_heading_marker_shows_only_in_active_heading` | ✅ |
| 2 | ED-002 | 光标所在块显示源标记，离开后隐藏 | 自动 `m3_case2_bold_markers_follow_the_caret` | ✅ |
| 3 | ED-002 | 图片、公式、图表在光标进入时显示可编辑源码 | 手工 L（图片、公式块、mermaid） | ✅ |
| 4 | ED-003 | 标记显隐不引起正文跳动（同一行宽度保持） | 手工 L | ✅ |
| 5 | ED-004 | 输入 `# `、`- `、`> `、```` ``` ````、`---` 后立即渲染为对应块 | 自动 `live_enter_expands_syntax`、`editor_typing_lists_and_undo` | ✅ |
| 6 | ED-005 | 上下方向键跨块移动保持列位置 | 自动 `editor_vertical_movement_keeps_column` | ✅ |
| 7 | ED-005 | 从代码块末行按下方向键离开代码块 | 自动 `m3_case4_code_block_highlight_and_leaving_with_down` | ✅ |
| 8 | ED-006 | 文档末尾为表格/代码块时点击其下方新建段落 | 自动 `typing_after_block_on_blank_line_starts_new_paragraph`；手工 L | ✅ |
| 9 | ED-010 | 中文输入法组合文本显示、提交后写入 | 自动 `editor_ime_preedit_and_commit`；手工 W（微软拼音） | ✅ |
| 10 | ED-011 | 双击选词、三击选块、Shift 扩展选区 | 自动 `editor_arrows_and_selection`、`selection_commands` | ✅ |
| 11 | ED-012 | Backspace/Delete 按字、词、行删除 | 自动 `word_and_line_deletion`、`live_backspace_semantics`、`live_delete_forward_merges` | ✅ |
| 12 | ED-012 | 在列表项/引用/标题开头 Backspace 先移除标记 | 自动 `live_backspace_semantics` | ✅ |
| 13 | ED-013 | Enter 延续列表、空项退出、代码块内换行 | 自动 `list_continuation_and_exit`、`code_and_quote_continuation`、`live_enter_semantics` | ✅ |
| 14 | ED-013 | 嵌套空项 Enter 先减少缩进 | 自动 `live_enter_on_empty_nested_item_outdents` | ✅ |
| 15 | ED-014 | Tab/Shift+Tab 嵌套与提升列表项、表格内移动单元格 | 自动 `tab_nests_and_shift_tab_outdents`、`table_tab_walks_cells_in_order` | ✅ |
| 16 | ED-015 | 撤销按词与停顿分组，重做恢复 | 自动 `typing_groups_by_word_and_pause`、`typing_and_undo_groups` | ✅ |
| 17 | ED-016 | 复制写入 Markdown 与 HTML，粘贴 HTML 转 Markdown | 自动 `paste_rules` | ✅ |
| 18 | ED-016 | 粘贴位图保存为图片文件并插入 | 手工 W | ✅ |
| 19 | ED-016 | 复制为 Markdown / HTML / 纯文本 / HTML 代码 | 手工 L；`clipboard_html` 共用导出生成器 | ✅ |
| 20 | ED-017 | 拖动选中文本移动，按住 Ctrl 复制 | 自动 `drag_moves_and_copies_text`；手工 L | ✅ |
| 21 | ED-017 | 从系统拖入图片插入、`.md` 打开（偏好 `general.dropMarkdown` 可改为插入链接）、其他文件插入链接 | 手工 L/W | ✅ |
| 22 | ED-017 | 从文件树拖入文档插入相对链接或图片 | 自动 `drag_moves_and_copies_text`（`edit.insertFileLink`） | ✅ |
| 23 | ED-018 | Alt+Up/Down 移动段落、列表项、表格行 | 自动 `clear_scope_and_move`、`table_editing` | ✅ |
| 24 | ED-020 | 光标闪烁 530 ms，空闲 30 s 后停止；打字时滚动跟随 | 手工 L；CPU 空闲 0% | ✅ |
| 25 | ED-030..037 | 加粗、斜体、下划线、代码、删除线、高亮、上下标切换 | 自动 `bold_toggles`、`targeted_edits` | ✅ |
| 26 | ED-039 | `Ctrl+K` 生成链接，剪贴板为 URL 时作为地址 | 自动 `targeted_edits` | ✅ |
| 27 | ED-040 | `Ctrl+Shift+I` 打开文件选择器插入图片 | 手工 W | ✅ |
| 28 | ED-041 | 清除样式移除选区内所有行内标记 | 自动 `clear_scope_and_move` | ✅ |
| 29 | ED-042 | 行内公式切换 | 自动 `targeted_edits` | ✅ |
| 30 | ED-044 | 插入脚注引用与定义 | 自动 `alerts_and_footnotes` | ✅ |
| 31 | ED-050/051 | 标题 1–6 与正文互转 | 自动 `headings_and_paragraph` | ✅ |
| 32 | ED-052 | 提升/降低标题级别 | 自动 `headings_and_paragraph` | ✅ |
| 33 | ED-053 | 插入表格对话框（行/列） | 自动 `m4_table_dialog_tab_rows_align_sort` | ✅ |
| 34 | ED-054..063 | 代码块、公式块、引用、有序/无序/任务列表、任务状态、缩进、列表样式、水平线 | 自动 `code_fences_rule_table_paragraphs`、`quotes_and_lists` | ✅ |
| 35 | ED-064/065 | 插入 `[TOC]` 与 Front Matter | 自动 `code_fences_rule_table_paragraphs` | ✅ |
| 36 | ED-067/069 | 插入提示框、脚注定义 | 自动 `alerts_and_footnotes` | ✅ |
| 37 | ED-070 | 在当前块前/后插入空段落 | 自动 `code_fences_rule_table_paragraphs` | ✅ |
| 38 | ED-080 | 表格渲染为网格，宽表横向滚动 | 手工 L/W；M4 验收 | ✅ |
| 39 | ED-081 | 单元格内编辑，`|` 自动转义 | 自动 `table_editing` | ✅ |
| 40 | ED-082 | 光标在表格内显示表格工具条（对齐、增删行列、调整大小） | 手工 L | ✅ |
| 41 | ED-083 | 表格命令：插入/删除/移动行列、对齐、排序、复制为 CSV | 自动 `table_editing`、`m4_table_dialog_tab_rows_align_sort` | ✅ |
| 42 | ED-084 | 拖动列边框调整列宽、拖动行列手柄重排 | 手工 L | ✅ |
| 43 | ED-085 | 源码中表格按显示宽度对齐（CJK 计 2） | 自动 `m4_table_dialog_tab_rows_align_sort` | ✅ |
| 44 | ED-090 | 图片按原始尺寸显示、不超过正文宽度、加载失败显示占位 | 手工 L/W | ✅ |
| 45 | ED-091 | 相对路径相对文档目录解析，`file://` 与百分号编码可用 | 自动 `images::tests::resolves_paths` | ✅ |
| 46 | ED-092 | 插入图片按偏好复制到 `./文件名.assets` 或指定目录 | 自动 `targets_and_relative_paths`；手工 W | ✅ |
| 47 | ED-093 | 右键图片：复制图片、复制地址、在文件夹中显示、默认程序打开、重新加载、转为本地、缩放、编辑、删除 | 自动 `finds_resizes_edits_and_deletes`；手工 L | ✅ |
| 48 | ED-093 | 选中图片显示边框，拖动右下角手柄调整宽度（写入 `<img width>`） | 手工 L 截图 | ✅ |
| 49 | ED-093 | 双击图片用系统默认查看器打开 | 手工 L | ✅ |
| 50 | ED-100 | 括号引号自动配对、成对删除、跳过闭合符 | 自动 `auto_pair_brackets_and_quotes` | ✅ |
| 51 | ED-100 | 代码中不配对 Markdown 符号；输入 ```` ``` ```` 不重复 | 自动 `no_markdown_pairing_in_code`、`typing_a_fence_does_not_double_backticks` | ✅ |
| 52 | ED-101 | 有序列表自动编号与重编号 | 自动 `ordered_lists_renumber` | ✅ |
| 53 | ED-102 | 智能引号、破折号、省略号；Backspace 撤回替换 | 自动 `smart_punctuation` | ✅ |
| 54 | ED-103 | 输入 `:sm` 弹出 Emoji 补全 | 手工 L | ✅ |
| 55 | ED-104 | 拼写错误显示波浪线，右键给出建议、添加到词典、忽略 | 自动 `misspelled_words_get_squiggles`；手工 L | ✅ |
| 56 | ED-105 | `[` 后补全标题链接，`[^` 补全脚注 | 手工 L | ✅ |
| 57 | ED-106 | 检测与保留换行符和编码，可转换 LF/CRLF，可设保存编码 | 自动 `line_ending_conversion`、`round_trip_bytes` | ✅ |
| 58 | ED-110 | 查找栏：大小写、整词、正则，匹配高亮与计数 | 自动 `editor_find_bar` | ✅ |
| 59 | ED-111 | 替换与全部替换 | 自动 `editor_find_bar` | ✅ |
| 60 | ED-112 | 跨越标记的替换按源码执行后重新解析 | 自动 `editor_find_bar` | ✅ |
| 61 | ED-120 | `Ctrl+/` 切换源码模式，光标位置保持 | 手工 L/W | ✅ |
| 62 | ED-121 | 源码模式语法着色、行号、软换行 | 手工 L | ✅ |
| 63 | ED-122 | 源码模式下大纲可用，格式命令直接编辑源码 | 手工 L | ✅ |
| 64 | ED-130 | 右键菜单随位置变化（链接、任务、代码块、表格、图片、拼写、公式/图表） | 手工 L | ✅ |

## 4. Markdown

| # | 需求 | 可观察行为 | 验证 | 结果 |
|---|---|---|---|---|
| 65 | MD-001..011 | CommonMark 块级例子 | 自动 `commonmark_conformance`（偏离见 M1 报告） | ✅ |
| 66 | MD-005/006 | GFM 列表与任务列表 | 自动 `gfm_extensions_conformance` | ✅ |
| 67 | MD-004 | GitHub Alerts 五种类型带图标与颜色 | 手工 L | ✅ |
| 68 | MD-007 | 围栏代码块语法高亮（语言别名） | 自动 `m3_case4_code_block_highlight_and_leaving_with_down` | ✅ |
| 69 | MD-008 | 数学块排版、自动编号、`\tag`、`\eqref` | 自动 `tags_and_numbers`、`inkra-math` 语料测试 | ✅ |
| 70 | MD-009 | GFM 表格（对齐、转义、行内格式） | 自动 `gfm_examples` | ✅ |
| 71 | MD-010 | 脚注按首次引用编号、悬停气泡、点击跳转 | 自动 `m5_toc_and_footnotes_render` | ✅ |
| 72 | MD-012 | Front Matter 显示为灰色等宽块 | 手工 L | ✅ |
| 73 | MD-013 | `[TOC]` 随标题实时更新、可点击 | 自动 `m5_toc_and_footnotes_render` | ✅ |
| 74 | MD-014 | HTML 块子集渲染，脚本不执行 | 自动 `flattens_common_readme_html`；手工 L | ✅ |
| 75 | MD-015 | 链接引用定义 | 自动 `commonmark_examples` | ✅ |
| 76 | MD-030..034 | 强调、行内代码、删除线、链接、图片 | 自动 `commonmark_examples` | ✅ |
| 77 | MD-035 | 行内公式（偏好开启） | 手工 L/W | ✅ |
| 78 | MD-036/037/038 | 上下标、高亮、下划线（偏好开启） | 自动 `gfm_extensions_conformance` | ✅ |
| 79 | MD-039 | `:shortcode:` 显示为 Emoji | 自动 `emoji::tests::table` | ✅ |
| 80 | MD-040 | 行内 HTML（kbd、mark、颜色、注释淡灰） | 手工 L 截图 | ✅ |
| 81 | MD-041/042 | 转义与硬换行 | 自动 `commonmark_examples`、`soft_break_and_crlf` | ✅ |
| 82 | MD-060..080 | Markdown 偏好全部可在偏好窗口修改并立即生效 | 自动 `preferences_window_changes_settings` | ✅ |
| 83 | MD-064 | 关闭图表后 mermaid 显示为代码块 | 手工 L | ✅ |
| 84 | MD-071/072 | 代码块行号；不换行时横向滚动 | 手工 L 截图 | ✅ |
| 85 | MD-074 | 保留连续空行的显示 | 手工 L 截图 | ✅ |
| 86 | MD-075 | 命令按首选粗体/斜体/列表符号生成 | 自动 `bold_toggles` | ✅ |
| 87 | MD-090 | 编辑保留原书写风格（符号、围栏字符） | 自动 `targeted_edits` | ✅ |
| 88 | MD-091 | 打开并保存未修改文件字节一致；编辑只改受影响范围 | 自动 `round_trip_bytes`、`m3_case7_save_changes_only_the_edited_bytes`、`corpus_round_trip_and_edits` | ✅ |
| 89 | MD-092 | 保存时规范化（表格、编号、空白、末尾换行） | 自动 `normalizes_tables_lists_and_whitespace` | ✅ |
| 90 | MD-100 | mermaid 渲染、错误行号、保留上次结果、编辑时下方预览 | 自动 `inkra-diagram` 语料测试；手工 L | ✅ |
| 91 | MD-100 | 图表右键复制 SVG/PNG、导出图片 | 自动 `blocks_copy_and_export` | ✅ |
| 92 | MD-102 | 图表随主题配色 | 手工 L（Paper / Ink） | ✅ |

## 5. 文件与工作区

| # | 需求 | 可观察行为 | 验证 | 结果 |
|---|---|---|---|---|
| 93 | FS-001 | 每个窗口一个文档；新建窗口、窗口列表切换 | 手工 L/W | ✅ |
| 94 | FS-002 | 打开对话框、命令行参数、拖入文件打开；识别扩展名列表 | 手工 W | ✅ |
| 95 | FS-003 | 保存、另存为；未命名文档建议文件名来自首个标题 | 自动 `m3_case7_save_changes_only_the_edited_bytes` | ✅ |
| 96 | FS-004 | 恢复区定时写入；崩溃后提示恢复 | 手工 W（结束进程后重启） | ✅ |
| 97 | FS-004 | 自动保存（偏好）在停顿与失焦后保存 | 手工 L | ✅ |
| 98 | FS-005 | 关闭修改过的窗口询问保存/不保存/取消 | 自动 `m3_case9_closing_a_modified_window_asks` | ✅ |
| 99 | FS-005 | 退出关闭所有窗口 | 自动 `quit_closes_viewport` | ✅ |
| 100 | FS-006 | 重启恢复窗口、文件、光标、侧边栏 | 手工 W | ✅ |
| 101 | FS-007 | 外部修改：无本地修改时重载，有修改时横幅提示 | 手工 L | ✅ |
| 102 | FS-008 | 标题栏 `● 名称 - 文件夹 - Inkra` | 手工 W 截图 | ✅ |
| 103 | FS-009 | 属性：路径、大小、时间、编码、换行符、字数，可改编码与换行符 | 自动 `m7_rename_and_properties_of_the_current_file` | ✅ |
| 104 | FS-010 | 文件菜单：在文件夹中显示、重命名、移动到、删除（回收站） | 自动 `m7_rename_and_properties_of_the_current_file`；手工 L | ✅ |
| 105 | FS-011 | 只读文件标题显示"只读"，保存转为另存为 | 自动 `saving_a_read_only_file_asks_for_another_name`；手工 L（标题 `ro.md [Read-only]`） | ✅ |
| 106 | FS-020 | 大纲：层级、点击跳转、当前标题高亮 | 自动 `m4_outline_click_jumps_to_heading` | ✅ |
| 107 | FS-021 | 文章列表：摘要、排序、递归 | 手工 L | ✅ |
| 108 | FS-022 | 文件树：新建、重命名（F2）、拖拽移动、删除、键盘导航 | 自动 `m4_quick_open_and_file_tree_move` | ✅ |
| 109 | FS-023 | 打开文件时文件树定位 | 手工 L | ✅ |
| 110 | FS-024 | 侧边栏顶部搜索入口进入全局搜索 | 手工 L | ✅ |
| 111 | FS-025 | 状态栏字数，悬停/点击显示明细与阅读时间 | 手工 L | ✅ |
| 112 | FS-030 | 快速打开模糊匹配，1 万文件 < 50 ms | 自动 `ten_thousand_paths_under_50ms` | ✅ |
| 113 | FS-031 | 全局搜索（正则、大小写、.gitignore），结果点击定位 | 自动 `ten_thousand_files`、`finds_matches_with_options` | ✅ |
| 114 | FS-040 | 视图菜单：侧边栏、源码、专注、打字机、状态栏、菜单栏、全屏、置顶、缩放、窗口 | 自动 `toggle_sidebar_via_menu` | ✅ |
| 115 | FS-041 | 工具栏：段落类型、格式按钮，状态随光标 | 手工 L 截图 | ✅ |

## 6. 写作模式

| # | 需求 | 可观察行为 | 验证 | 结果 |
|---|---|---|---|---|
| 116 | WM-001 | 专注模式淡化当前块以外内容 | 手工 L | ✅ |
| 117 | WM-002 | 打字机模式光标行保持垂直居中 | 手工 L | ✅ |
| 118 | WM-003 | F11 全屏 | 手工 W | ✅ |
| 119 | WM-004 | 正文最大宽度、宽屏居中、自动隐藏滚动条偏好 | 手工 L | ✅ |
| 120 | WM-005 | 启动无动画；欢迎面板默认关闭、可开启 | 手工 L | ✅ |
| 121 | WM-006 | 空文档不显示占位文案 | 手工 L | ✅ |

## 7. 主题

| # | 需求 | 可观察行为 | 验证 | 结果 |
|---|---|---|---|---|
| 122 | TH-001 | TOML 主题、继承 `base`、错误时保持当前主题并提示 | 自动 `builtins_load`、`colors_and_user_themes` | ✅ |
| 123 | TH-002 | 内置 Paper、Ink、Journal、Mono、Slate、Serif | 手工 L | ✅ |
| 124 | TH-003 | 主题菜单切换、跟随系统深浅色、主题文件热重载 | 手工 L | ✅ |
| 125 | TH-004 | 导出使用指定主题；深色主题无背景时改用浅色 | 自动 `export_commands_build_jobs_that_run` | ✅ |
| 126 | TH-006 | 中文回退到系统字体（Windows 微软雅黑），缺字不崩溃 | 手工 W 截图 | ✅ |
| 127 | TH-007 | 界面字号偏好；125%/150% 分数缩放清晰 | 手工 L（`WINIT_X11_SCALE_FACTOR=1.25/1.5` 截图：正文、公式纹理、代码均清晰） | ✅ |
| 128 | TH-008 | 外观偏好：侧边栏字号行高、菜单栏、状态栏、窗口尺寸记忆 | 自动 `preferences_window_changes_settings` | ✅ |

## 8. 导入导出

| # | 需求 | 可观察行为 | 验证 | 结果 |
|---|---|---|---|---|
| 129 | EX-001 | HTML（含样式）离线可看、仅含用到的样式 | 自动 `exports_structure_math_code_and_toc` | ✅ |
| 130 | EX-001 | HTML（不含样式）片段 | 自动 `exports_structure_math_code_and_toc` | ✅ |
| 131 | EX-001 | 标题锚点、TOC、脚注聚合、代码高亮、SVG 公式/图表、Alerts | 自动 `exports_structure_math_code_and_toc` | ✅ |
| 132 | EX-001 | 图片路径：保持/绝对/内嵌 | 自动 `images_embed_and_absolute` | ✅ |
| 133 | EX-001 | 公式输出可选 SVG / MathML / TeX | 自动 `exports_structure_math_code_and_toc`、`common_constructs`（数学语料全部生成配对完整的 MathML） | ✅ |
| 134 | EX-002 | PDF 分页、书签、可选文字、内外链接、页眉页脚、元数据 | 自动 `exports_a_pdf_with_text_links_and_outline`、`pagination_rules`；M6 报告 | ✅ |
| 135 | EX-002 | 页面设置对话框 | 自动 `unimplemented_items_are_disabled_and_about_opens` | ✅ |
| 136 | EX-002 | 打印：PDF 交给系统 | 手工 W | ✅ |
| 137 | EX-003 | 长图 PNG、按页 PNG/JPEG、倍率 | 自动 `long_and_paged_images` | ✅ |
| 138 | EX-004 | Pandoc 导出（docx 等），缺失时置灰并引导 | 自动 `docx_round_trip_with_images`；手工 W 截图 | ✅ |
| 139 | EX-004 | 自定义导出命令 | 自动 `custom_export_runs_the_template` | ✅ |
| 140 | EX-005 | 导出设置页（偏好 → 导出） | 手工 L | ✅ |
| 141 | EX-006 | 复制为 HTML 与导出共用生成器 | 代码审查 | ✅ |
| 142 | EX-020 | 导入 docx 等（Pandoc），HTML 无 Pandoc 也可导入，媒体进入 assets | 自动 `docx_round_trip_with_images`、`html_import_without_pandoc_and_word_splitting` | ✅ |
| 143 | EX-021 | 拖入可导入文件时询问导入 | 手工 L | ✅ |

## 9. 偏好设置

| # | 需求 | 可观察行为 | 验证 | 结果 |
|---|---|---|---|---|
| 144 | PR-001 | 配置文件路径与原子写入；未知键保留 | 自动 `round_trip_preserves_unknown_keys` | ✅ |
| 145 | PR-001 | `Ctrl+,` 打开独立偏好窗口，修改即时生效 | 自动 `preferences_window_changes_settings`；手工 L 截图 | ✅ |
| 146 | PR-002 | 导出/导入设置文件 | 自动 `export_import_and_reset_settings` | ✅ |
| 147 | PR-003 | 配置损坏时备份并提示 | 自动 `corrupt_file_is_backed_up` | ✅ |
| 148 | PR-010 | 启动：恢复会话 / 新文档 / 文件夹 | 手工 W | ✅ |
| 149 | PR-011 | 打开文件：复用空白窗口 / 新窗口 / 当前窗口 | 手工 L | ✅ |
| 150 | PR-012/017 | 自动保存、外部修改自动重载 | 手工 L | ✅ |
| 151 | PR-013 | 保存时清除尾随空格 / 规范化 | 自动 `normalizes_tables_lists_and_whitespace` | ✅ |
| 152 | PR-014/015/016 | 默认扩展名、扩展名列表、默认编码与换行符 | 手工 L | ✅ |
| 153 | PR-018 | 界面语言（重启生效） | 自动 `menu_bar_shows_localized_top_level_menus` | ✅ |
| 154 | PR-019 | 无遥测 | 代码审查（网络仅图片与更新检查） | ✅ |
| 155 | PR-020 | 启动检查 GitHub Release，不下载 | 自动 `versions_and_schemes`；手工 L（日志） | ✅ |
| 156 | PR-021 | 高级：打开配置/主题/快捷键/日志文件夹、重置设置 | 自动 `export_import_and_reset_settings`（重置）；手工 L | ✅ |
| 157 | PR-030..036 | 编辑器偏好：默认视图、配对、智能标点、首行缩进、拼写、补全 | 手工 L 截图 | ✅ |
| 158 | PR-035 | 拼写检查语言跟随系统（`editor.spellcheckLanguage`）：使用系统已安装的 Hunspell 词典，否则内置 en_US | 自动 `finds_installed_dictionaries_by_language` | ✅ |
| 159 | PR-037 | 复制时纯文本为 Markdown 或纯文本 | 手工 L | ✅ |
| 160 | PR-038/039 | 专注/打字机记忆、Ctrl+滚轮缩放并记忆 | 手工 L | ✅ |
| 161 | PR-040 | 点击链接：Ctrl+单击 / 单击 | 手工 L | ✅ |
| 162 | PR-041/042/043 | Tab 宽度与空格、光标闪烁、平滑滚动、状态栏字数 | 手工 L | ✅ |

## 10. 菜单、快捷键与可访问性

| # | 需求 | 可观察行为 | 验证 | 结果 |
|---|---|---|---|---|
| 163 | UI-001 | Windows/Linux 自绘菜单栏，可隐藏，Alt 临时显示 | 自动 `menu_bar_shows_localized_top_level_menus` | ✅ |
| 164 | UI-001 | macOS 使用系统菜单栏 | 代码已实现，测试环境无 macOS | ⚠ 未实测 |
| 165 | UI-002 | 菜单显示快捷键、置灰、勾选；全部映射命令 | 自动 `unimplemented_items_are_disabled_and_about_opens`、`every_command_has_a_title_in_both_languages` | ✅ |
| 166 | UI-010 | 默认快捷键表（`docs/keybindings.md`） | 自动 `defaults_parse_on_all_platforms`、`no_conflicting_defaults` | ✅ |
| 167 | UI-020 | `keybindings.json` 覆盖与移除默认绑定 | 自动 `user_keybindings` | ✅ |
| 168 | UI-020 | 偏好 → 快捷键：搜索、录入、冲突提示、重置 | 手工 L 截图 | ✅ |
| 169 | UI-030 | AccessKit：菜单、按钮、输入框、编辑区文本与光标位置 | 自动 `m7_document_is_exposed_to_assistive_technology` | ✅ |
| 170 | UI-031 | 输入法候选窗跟随光标，切换输入法不被拦截 | 手工 W | ✅ |
| 171 | UI-032 | 对话框 Esc 关闭、Enter 确认；侧边栏键盘导航 | 手工 L | ✅ |
| 172 | UI-033 | 系统"减少动画"时关闭平滑滚动与淡化 | 手工 W：关闭"显示动画"（SPI_SETCLIENTAREAANIMATION）后启动日志 `system asks for reduced motion`，恢复后不再出现 | ✅ |
| 173 | UI-040 | 模态对话框：表格、图片属性、重命名、保存确认、恢复、外部修改横幅 | 手工 L | ✅ |
| 174 | UI-040 | 链接对话框：编辑链接文字与地址，或为选区插入链接（`Ctrl+K` 按 ED-039 直接插入） | 自动 `link_dialog_edits_and_inserts` | ✅ |
| 175 | UI-041 | 右下角 toast 3 s：保存成功（可关）、导出完成（点击打开）、更新可用 | 手工 L（已保存）、W（已导出）截图 | ✅ |
| 176 | UI-050 | 窗口、任务栏、安装包图标 | 手工 W（任务栏、开始菜单） | ✅ |
| 177 | UI-051 | Linux symbolic 单色图标随包安装 | `cargo xtask dist` 包内容 | ✅ |
| 178 | UI-052 | 关于对话框：256 px 图标、Inkra、墨刻、版本、许可证；欢迎面板 64 px 图标 | 自动 `unimplemented_items_are_disabled_and_about_opens` | ✅ |

## 平台与发布（NF-006、M7）

| # | 需求 | 可观察行为 | 验证 | 结果 |
|---|---|---|---|---|
| 179 | NF-006 | Windows MSI 安装、开始菜单快捷方式、`.md` 关联、双击打开 | 手工 W（`msiexec /qn`，`assoc .md`，`Start-Process doc.md`） | ✅ |
| 180 | NF-006 | Windows 便携 zip | `cargo xtask dist` | ✅ |
| 181 | NF-006 | Linux tar.gz（install.sh）、deb、rpm、AppImage | 手工 L（tar.gz 安装运行、AppImage 运行、rpm 列表） | ✅ |
| 182 | NF-006 | macOS dmg | xtask 任务已写，测试环境无 macOS | ⚠ 未实测 |
| 183 | NF-006 | Wayland | 测试环境仅 X11 | ⚠ 未实测 |
| 184 | CM-002 | `inkra-cli convert/inspect/lint/fmt/run/commands` 与退出码 | 自动 `inspect_lint_fmt_run_convert` | ✅ |
| 185 | CM-004 | `--new`、`--source-mode`、`--theme`、`--config`、`--version`、`--measure-startup`、`--no-session` | 手工 L/W | ✅ |

## 汇总

共 185 条：✅ 182、⚠ 3、❌ 0，通过率 98.4%。⚠ 项进入 `docs/ROADMAP.md`。
