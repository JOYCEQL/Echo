# 第三方来源

- Rune 官方 Metal 颜色空间修复：回移植 [#902](https://github.com/anomalyco/rune/pull/902)，提交 `53baebed87f3a64d081c3cebedb0482aed191558`。配置和缩放窗口时保留 CAMetalLayer 的 sRGB 标记，避免像素被按显示器色域解释。

- Windows 窗口和任务栏图标使用 Rune [#600](https://github.com/anomalyco/rune/pull/600) 的可执行文件图标资源接口。首帧显示和激活采用 Rune [#619](https://github.com/anomalyco/rune/pull/619) 与 [#846](https://github.com/anomalyco/rune/pull/846) 的窗口处理。窗口在首帧显示时按系统前台权限请求焦点，包含所需的遮罩状态转换，省略新版框架专用的性能埋点。单实例监听器只创建一个管道实例，后续启动连接到已有监听器。

- Rune, MIT: https://github.com/anomalyco/rune ，源码快照 `dece2aff3a254ffd6f6a0a49baa82b75664b3190`，见 `vendor/rune/REVISION` 与 `vendor/rune/LICENSE`。为避免 dev 分支更新导致接口变化，交付包内附固定源码快照；缩减了 workspace 成员列表，并应用 Echo 本地补丁：新增 Heading / Task 段落属性、HTML 往返保存、标题字号与富文本强调字重、复选框绘制与点击、相关回车和退格行为。补丁文件见 `patches/live-markdown.patch`，已经应用在随附源码中。
- Feather Icons, MIT: https://github.com/feathericons/feather ，线性界面图标。仅调整描边颜色与粗细。
- Tabler Icons, MIT: https://github.com/tabler/tabler-icons ，hand-stop、bulb 图标。仅调整描边颜色与粗细。
- 建筑照片、手写便笺、设计参考图均来自本次用户提供的设计稿；照片与便笺从原图裁取，用于本地还原示例。侧栏颗粒由 Rune 官方 `rune-fx::Fx::grain` 的 WGSL 程序生成，不使用纹理图片。
- 其余 Rust 依赖及精确版本见 Cargo.lock。

- 新增对齐、链接、复制、剪切图标来自 Feather v4.29.2（MIT）：https://github.com/feathericons/feather/tree/v4.29.2/icons 。

- Rune 空闲唤醒修复：无 Live 订阅的 `live_only` 事件也清除已消费的 dirty 标记，避免静态窗口连续重绘；见 `patches/rune-idle-live-only.patch`。已在随附源码中应用，Echo 回归测试覆盖布局回调与百万字文档静置。

- 结构化待办控件复用 `rune-kit::Checkbox`。侧栏文件夹已统一至下述 Lucide 图标；画布富文本勾选图形 `vendor/rune/crates/rune/src/edit/task-check.svg` 同样来自 Lucide。富文本待办的背景与勾选色由 Echo 显式传入，以支持全局明暗外观。

## Yab Look editor

The native Look editor in `src/look.rs` and OKLCH gamut/harmony calculations in `src/material.rs` adapt the design and algorithms of [Yab](https://github.com/yetanotherbrowser/yab), including `SpaceLook.swift`, `LookHarmony.swift`, `LookSlider.swift`, `Gamut.swift`, and `YabDesign/Design.swift`. Copyright 2026 Serafim Korablev. Source revision `a4661df137ad98122831539f20d18e62887fb79c`. Licensed under Apache-2.0; see `licenses/yab-Apache-2.0.txt` and `licenses/yab-NOTICE.txt`. The SwiftUI implementation was adapted to Rune, with Echo-specific persistence, layout and navigation contrast.

The native window coating in `vendor/rune/crates/rune/src/system_material/macos/backdrop.rs` adapts `Fast/WindowBackdrop.swift`, `Fast/WindowGlass.swift`, and `BackdropVeil` in `Fast/Design.swift`: AppKit sibling views host the HUD material, neutral veil, glass, and a Core Animation gradient. Echo retains its procedural grain instead of Yab’s texture assets.

## Shared application icons

`assets/notebooks/ink/` contains seven original Echo colored illustrations with flat pastel fills and ink-drawn contours. They do not contain Notion logos or copied Notion assets.

Application glyphs in `assets/lucide/` use [Lucide 0.468.0](https://github.com/lucide-icons/lucide/tree/f12b0de177fbc2a6795e99be065887e72b237123), revision `f12b0de177fbc2a6795e99be065887e72b237123`. See `licenses/lucide-LICENSE.txt` (ISC and inherited Feather MIT notices). Echo adjusts the stroke width from 2 to 1.8 on the original 24-unit grid and applies theme colours. All navigation, editor, canvas, insertion and Look action glyphs share this family. Paper-backed category tiles are Echo UI styling inspired by Yab's consistent visual hierarchy; no Central Icon System assets or Yab PenArt drawings are included.

- `assets/sidebar-toggle.svg` is an Echo-drawn sidebar glyph, using a rounded split panel and two short sidebar lines inspired by Yab. It does not include Central Icon System artwork.

- Rune 模态焦点修复：应用明确调用 `Tx::blur()` 时清除当前弹窗的返回焦点，使新建文档后键盘直接进入正文编辑器；正常取消仍返回弹窗触发控件。修改位于 `runtime/frame.rs` 与 `runtime/input/modal_focus.rs`，由新建页面与搜索回归测试覆盖。

- Echo 的窗口底色与遮罩已迁移到 AppKit / CAGradientLayer，颜色不再进入 Rune 的 Metal drawable。普通材质使用 NSVisualEffectView HUD；玻璃使用 NSGlassEffectView，明暗样式及可读性遮罩参照 Yab。撤回强制 Rgba16Float 主窗口，保留 Rune #902 的 sRGB 色彩标记修复。现有控件和可调程序颗粒继续由 Rune 绘制；不引入纹理或替代渲染路径。

Echo 的暗色多色转换使用整组配色共享的连续亮度权重，替代 Yab 按单个色点在 chroma=0.02 处分档的规则，避免同一组浅色在暗色中被额外拉开明暗。色相、色度缩减、用户保存的原色及原生 CAGradientLayer 保持原有路径。

Echo 的玻璃覆盖率同样使用整组配色的连续权重，保留 Yab 的灰色/彩色覆盖率端点，取消拖动跨越灰彩阈值时的硬切换。

## Application updates

Update package verification uses [minisign-verify 0.3.0](https://crates.io/crates/minisign-verify), licensed under MIT (see `licenses/minisign-verify-LICENSE.txt`). Private staging directories use [tempfile 3.27.0](https://crates.io/crates/tempfile), licensed under MIT OR Apache-2.0 (see `licenses/tempfile-MIT.txt` and `licenses/tempfile-Apache-2.0.txt`). The application uses its own update protocol and installer.

## Mermaid and mathematics

Mermaid previews use the vendored Rune `rune-mermaid` crate. Rune implements a documented Mermaid subset; unsupported diagram types show a diagnostic in Echo. Its permissive parser can omit unsupported statements, so the preview explicitly identifies that limitation.

Formula parsing, layout and standalone SVG use [RaTeX 0.1.16](https://github.com/erweixin/RaTeX) (MIT). KaTeX math fonts are embedded through `ratex-katex-fonts` under the SIL Open Font License; see `licenses/KaTeX-fonts-OFL.txt`. The formula worker has no network or WebView dependency.

Echo extends Rune static rich text with baseline-aligned `InlineImage` objects and matching `doc_text::hit_test_images` geometry. These share the existing Parley inline-box, layout cache and image paint path; normal inline icons retain their existing geometry.
