# 用户报告的解析与渲染问题（T6）

来自真实用户文件的解析、渲染、导出问题。每条报告：

1. 把最小复现文件放进 `user-reports/`，命名 `<issue 编号>-<简称>.md`（不含用户隐私内容；必要时改写文字，保留结构）。`cargo test -p inkra-doc --test corpus` 会对其中每个文件做字节级往返、编辑后增量解析与全量解析一致、撤销还原的检查（GFM 与 CommonMark 两种方言）。
2. 渲染类问题在负责绘制的 crate 中补一条测试（`inkra-layout`、`inkra-math`、`inkra-diagram`，或 `crates/inkra-ui/tests/snapshots.rs` 的快照）。
3. 在下表登记，修复后填写提交。

| Issue | 日期 | 范围 | 现象 | 复现文件 | 测试 | 修复提交 | 状态 |
|---|---|---|---|---|---|---|---|
| — | — | — | 暂无（1.0.0-beta.1 发布后开始收集） | — | — | — | — |

## 内部发现（fuzz 与 CI，供参照）

| 日期 | 来源 | 现象 | 测试 | 修复提交 |
|---|---|---|---|---|
| 2026-10-14 | `cargo fuzz` diagram | 同名 subgraph 重复删除占位节点导致下标下溢 panic | `inkra-diagram` `subgraphs_sharing_an_id_share_the_placeholder`、`tests/fuzz-regressions/` | c0e6db4 |
| 2026-10-14 | `cargo fuzz` diagram | `style … fill:#fʿ` 在多字节字符内切片 panic；主题颜色解析同样问题 | `colors_with_non_ascii_bytes_are_ignored`、`colors_and_user_themes` | 01858aa |
| 2026-10-14 | README 截图 | 高于行高的行内公式基线偏高（`\frac{a}{b}` 偏 6 px） | `inkra-layout/tests/inline_math.rs` | a9eca06 |
| 2026-10-14 | `cargo fuzz` diagram | 嵌套/同名 subgraph 互相推开，2.7 KB 流程图宽 1 077 120 px、耗时 0.6 s | `tests/fuzz-regressions/flowchart-nested-subgraphs-width.mmd`（尺寸上限） | 8aade7f |
| 2026-10-14 | `cargo fuzz` math | 未闭合 `\left…\middle` 嵌套布局指数级耗时（1 KB 0.1 s，翻倍 46 s） | `inkra-math/tests/fuzz_regressions.rs` | ddf8cfe |
| 2026-10-14 | `cargo fuzz` diagram | 16 KB 流程图（789 条边）虚线切出数百万段、耗时 1.05 s | `draw::tests::dashes_per_stroke_are_bounded` | 48568ae |
| 2026-10-14 | CI macos-latest | 经符号链接目录打开的文件收不到外部修改通知 | `watch::tests::event_paths_map_back_through_symlinks` | d1ffd14 |
