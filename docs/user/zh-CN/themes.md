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
| `[fonts]` | `body` `heading` `mono` | 字体族列表，按顺序取第一个可用的：内置 Inter、Source Serif 4、JetBrains Mono，通用名 `sans-serif` `serif` `monospace`，或系统已安装的字体名（如 `"Noto Serif CJK SC"`） |
|  | `math` | 公式字体；目前只使用内置 Latin Modern Math |
|  | `cjk_latin_spacing` | `true` 时在中日韩文字与西文、数字之间显示一点间隙（不改动文件） |
|  | `size` `line_height` `paragraph_spacing` | 正文字号（pt）、行高倍数、段落间距（em） |
| `[colors]` | `bg` `fg` `fg_muted` `accent` `selection` `cursor` | 背景、文字、次要文字、强调色（链接等）、选区、光标 |
|  | `code_bg` `code_border` `quote_border` `quote_fg` `hr` | 代码块、引用、分隔线 |
|  | `table_border` `table_header_bg` `table_row_alt_bg` | 表格 |
|  | `highlight_bg` `mark_fg` `search_match_bg` `search_current_bg` `spell_error` | 高亮、隐藏标记、搜索、拼写 |
|  | `sidebar_bg` `sidebar_fg` `sidebar_active_bg` `toolbar_bg` `statusbar_fg` | 侧边栏与状态栏 |
|  | `alert_note` `alert_tip` `alert_important` `alert_warning` `alert_caution` | 提示框颜色 |
| `[headings]` | `h1`…`h6` | `{ size, weight, margin_top, margin_bottom, border_bottom, color }`，`color` 可写颜色或颜色键名 |
|  | `auto_number` | `true` 时在标题前显示 `1.`、`1.2.` 编号（只显示，不写入文件） |
| `[code]` | `syntax_theme` `radius` `padding` `line_numbers_fg` | 代码高亮主题（syntect 内置：InspiredGitHub、base16-ocean.dark 等）、圆角、内边距。主题文件夹中的 `syntax.tmTheme`（TextMate 配色）会自动使用；也可写 `syntax_theme = "我的.tmTheme"` |
| `[layout]` | `max_width` `padding_x` `padding_y` `list_indent` `quote_indent` `focus_dim_opacity` | 正文最大宽度、边距、缩进、专注模式淡化程度 |
|  | `radius` `image_radius` | 表格圆角、图片圆角 |
|  | `task_done` | 已完成任务的文字：`"muted"`（变灰，默认）、`"strike"`（删除线）、`"both"` |
| `[diagram]` | `primary` `primary_border` `line` `text` | 图表节点填充、节点边框、连线、文字颜色（默认由主题颜色推出） |

颜色写成 `#rrggbb` 或带透明度的 `#rrggbbaa`。主题文件有错误时，Inkra 保持当前主题并在提示中给出错误行。

## 导出

导出 HTML 与 PDF 时使用当前主题（或偏好设置“导出”中指定的主题），颜色、字体、标题样式都会带入导出结果。
