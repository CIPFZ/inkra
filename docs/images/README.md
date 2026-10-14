# README 截图

由 `ui_shot`（NF-010）从 `src/` 中的文档离屏渲染，不依赖窗口与显卡驱动：

```sh
S="cargo run -q -p inkra-ui --example ui_shot --"
P="--pref editor.spellcheck=false"
$S docs/images/src/writing.md  docs/images/light.png   --size 1100x620 --theme Paper $P
$S docs/images/src/writing.md  docs/images/dark.png    --size 1100x620 --theme Ink $P
$S docs/images/src/writing.md  docs/images/sidebar.png --size 1100x620 --theme Slate --sidebar $P
$S docs/images/src/tables.md   docs/images/tables.png  --size 1100x380 --theme Paper $P
$S docs/images/src/math.md     docs/images/math.png    --size 1100x520 --theme Journal $P --pref markdown.inlineMath=true
$S docs/images/src/diagram.md  docs/images/diagram.png --size 1100x740 --theme Paper $P
```
