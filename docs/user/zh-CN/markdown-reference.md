# Markdown 参考

Inkra 支持 CommonMark 与 GitHub 风格 Markdown（GFM），以及数学公式、图表、脚注等扩展。把光标移到下面的示例上即可看到源码。

[TOC]

## 段落与换行

空一行开始新段落。行尾两个空格或 `\` 产生硬换行。

## 标题

# 一级标题
## 二级标题
### 三级标题

## 强调

**粗体**、*斜体*、***粗斜体***、~~删除线~~、`行内代码`。
在偏好设置中开启后还支持 ==高亮==、H~2~O（下标）与 x^2^（上标）。

## 列表

- 无序列表
  - 嵌套项
1. 有序列表
2. 第二项

- [x] 已完成的任务
- [ ] 未完成的任务（点击复选框切换）

## 引用与提示框

> 这是一段引用。

> [!NOTE]
> 提示框：NOTE、TIP、IMPORTANT、WARNING、CAUTION。

## 链接与图片

[Inkra 主页](https://example.com)、<https://example.com>、[跳到“表格”](#表格)。

![替代文字](image.png "标题")

## 代码

```rust
fn main() {
    println!("Hello, Inkra!");
}
```

## 表格

| 左对齐 | 居中 | 右对齐 |
|:------|:----:|------:|
| a     |  b   |     c |

## 数学公式

行内公式 $e^{i\pi} + 1 = 0$（需在偏好设置中开启行内公式）。

$$
\int_0^\infty e^{-x^2}\,dx = \frac{\sqrt{\pi}}{2}
$$

## 图表

```mermaid
flowchart LR
  A[开始] --> B{判断}
  B -->|是| C[完成]
  B -->|否| A
```

还支持 `sequenceDiagram` 与 `pie`；其他类型可配置 Mermaid CLI（`mmdc`）。

## 脚注

这里有一个脚注[^1]。

[^1]: 脚注内容，悬停引用即可查看。

## 目录

单独一行写 `[TOC]` 生成当前文档的目录。

## 分隔线

---

## Emoji

`:smile:` 显示为 :smile:；输入 `:` 加两个字母会弹出补全。

## HTML

支持常用 HTML：<kbd>Ctrl</kbd>、<mark>标记</mark>、<span style="color: #c0392b">彩色文字</span>、`<details>`、`<img>` 等。脚本不会执行。

## Front Matter

文档开头 `---` 包围的 YAML 块，`title` 与 `author` 会用于导出。
