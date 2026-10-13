# 自定义主题

主题是一个文件夹，里面有一个 `theme.toml`。把文件夹放进主题目录（“主题 → 打开主题文件夹”），它就会出现在“主题”菜单中；编辑 `theme.toml` 并保存后立即生效，无需重启。

## 从现有主题继承

最简单的方式是继承内置主题，只改需要的颜色：

```toml
[meta]
name = "My Theme"
author = "我"
version = "1.0"
dark = false
base = "Paper"

[colors]
accent = "#0b6e4f"
cursor = "#e4572e"
```

内置主题：Paper（默认）、Ink（深色）、Journal、Mono、Slate、Serif。

## 各节说明

| 节 | 键 | 说明 |
|---|---|---|
| `[meta]` | `name` `author` `version` `dark` `base` | `dark` 决定深色界面；`base` 为继承的主题名 |
| `[fonts]` | `body` `heading` `mono` | 字体族列表，按顺序回退；内置 Inter、Source Serif 4、JetBrains Mono |
|  | `size` `line_height` `paragraph_spacing` | 正文字号（pt）、行高倍数、段落间距（em） |
| `[colors]` | `bg` `fg` `fg_muted` `accent` `selection` `cursor` | 背景、文字、次要文字、强调色（链接等）、选区、光标 |
|  | `code_bg` `code_border` `quote_border` `quote_fg` `hr` | 代码块、引用、分隔线 |
|  | `table_border` `table_header_bg` `table_row_alt_bg` | 表格 |
|  | `highlight_bg` `mark_fg` `search_match_bg` `search_current_bg` `spell_error` | 高亮、隐藏标记、搜索、拼写 |
|  | `sidebar_bg` `sidebar_fg` `sidebar_active_bg` `toolbar_bg` `statusbar_fg` | 侧边栏与状态栏 |
|  | `alert_note` `alert_tip` `alert_important` `alert_warning` `alert_caution` | 提示框颜色 |
| `[headings]` | `h1`…`h6` | `{ size, weight, margin_top, margin_bottom, border_bottom, color }`，`color` 可写颜色或颜色键名 |
| `[code]` | `syntax_theme` `radius` `padding` `line_numbers_fg` | 代码高亮主题（syntect 内置：InspiredGitHub、base16-ocean.dark 等）、圆角、内边距 |
| `[layout]` | `max_width` `padding_x` `padding_y` `list_indent` `quote_indent` `focus_dim_opacity` | 正文最大宽度、边距、缩进、专注模式淡化程度 |

颜色写成 `#rrggbb` 或带透明度的 `#rrggbbaa`。主题文件有错误时，Inkra 保持当前主题并在提示中给出错误行。

## 导出

导出 HTML 与 PDF 时使用当前主题（或偏好设置“导出”中指定的主题），颜色、字体、标题样式都会带入导出结果。
