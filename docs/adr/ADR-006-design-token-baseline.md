# ADR-006: 设计令牌基线裁决（代码为准）

- 状态：已接受（2026-11-13）
- 决策人：Alpha（B-03 卡裁决）/ 用户令确认
- 影响域：`apps/desktop/src/design/tokens.ts`、`apps/mobile/src/styles/mobile.css`、
  `crates/aurora-core/src/l3_domain/system_settings.rs`（DesignTokens）、V25/V26 界面规范（外部）
- 停手边界：不做主题重设计、不引入新色板、不动 A-11 白名单

## 1. 背景（规范与代码漂移实测）

`tokens.ts` 头注释声称「双端共享单一事实源、Rust 侧主题结构体与本文件字段一一对应」，
但实测三方漂移：

| 项 | tokens.ts（代码） | V25 界面规范 | Rust DesignTokens |
|---|---|---|---|
| 默认主题 | 暗色（bgBase `#10151C`） | 浅色内容区+深色固定侧栏 | `Theme::default()=Light` |
| 主色 | `#1B6B7A`/`#37DCF2`（青） | `#4F46E5`（靛蓝） | accent `#0066cc`/`#4d9fff` |
| 侧栏 | 随主题变 | 明暗下均固定 `#111827` | — |
| 字段数 | color 14 键+spacing/radius/typography/motion/a11y | — | 7 字段 |

规范与代码同时存在、互不引用——后续每张 UI 卡都会各选一套（B-02 拆分中
已实际出现 mobile.css 39 处、DesktopShell 20 处硬编码 hex 与令牌并行）。

## 2. 裁决：选 (a)——以代码为准，修订规范

**理由**：

1. **暗色优先在 OLED 移动端更省电**（V23-P1 设计原则承继，B-03 卡面推荐项）；
2. **代码已有真实运行资产**：双端 UI 实际渲染走 tokens.ts / mobile.css 变量，
   用户已按暗色基线真机验收多轮（V25 M14 平板双栏落地即此色系）；
3. **改文档成本远低于改代码**：101KB CSS 全量回归（39+20+ 处硬编码清理与
   亮暗两套对比度重验）风险高于修订规范条目；
4. **规范漂移项已实际失效**：`#4F46E5` 靛蓝与固定 `#111827` 侧栏从未上线。

**裁决结果**：

- **单一事实源 = `apps/desktop/src/design/tokens.ts`**（暗色默认 + 青色主色系）。
- **V25/V26 界面规范按本 ADR 修订**：默认主题改为暗色（bgBase #10151C）、
  主色改 #1B6B7A/#37DCF2、侧栏条目删除「明暗固定深色」改为「随主题」。
  （外部规范文档维护方按此三条执行；repo 内以本 ADR 为修订记录锚点。）
- Rust `DesignTokens` 定位降级为**插件 iframe 主题投影**（`css_variables()`
  唯一消费方为 plugin iframe_runtime）——**独立语义域，色板值不强行对齐**，
  但**字段名映射与本 ADR 映射表锁定**（§4）。

## 3. 三层字段映射表（单一事实源锁定）

真源方向：`tokens.ts`（真源）→ CSS 自定义属性（mobile.css `:root`）→
Rust DesignTokens（插件投影，字段名对齐、值自由）。

### 3.1 tokens.ts color 键 → CSS 自定义属性（mobile.css）

| tokens.ts 键 | CSS 变量 | 值（暗色基线） |
|---|---|---|
| color.bgBase | `--bg-base` | #10151C |
| color.bgSurface | `--bg-surface` | #162235 |
| color.bgElevated | `--bg-elevated` | #1D2633 |
| color.primary | `--primary` | #1B6B7A |
| color.primaryBright | `--primary-bright` | #37DCF2 |
| color.textPrimary | `--text-primary` | #E8EDF2 |
| color.textSecondary | `--text-secondary` | #9AA7B4 |
| color.textDisabled | `--text-disabled` | #5A6672 |
| color.success | `--success` | #3DDB8E |
| color.warning | `--warning` | #E8B33D |
| color.danger | `--danger` | #E85D5D |
| color.focus | `--focus` | #37DCF2 |
| spacing.* | `--space-xs/sm/md/lg/xl` | 4/8/16/24/32 |
| radius.* | `--radius-sm/md/lg` | 4/8/12 |

### 3.2 tokens.ts color 键 → Rust DesignTokens 字段（插件投影）

| Rust 字段 | tokens.ts 键（语义映射） | 说明 |
|---|---|---|
| bg_primary | color.bgBase | 值域独立（插件 iframe 色板自由） |
| bg_secondary | color.bgSurface | 同上 |
| text_primary | color.textPrimary | 同上 |
| text_secondary | color.textSecondary | 同上 |
| accent | color.primaryBright | 同上 |
| border | —（css rgba 线族） | Rust 投影独有，无对应 |
| font_size_base | typography.body.size | "16px"（Rust 侧历史值） |

## 4. 一致性保障（进 CI）

1. **字段集合比对单测**（`crates/aurora-core/src/l3_domain/system_settings.rs`
   `b03_token_mapping_consistency`）：`include_str!` 解析 tokens.ts 的 color
   键集合，与本 ADR 映射表在 Rust 侧的常量声明比对——**集合不匹配即测试红**。
2. **对比度守卫单测**（同文件 `b03_contrast_guard`）：暗色基线
   textPrimary/bgBase、textSecondary/bgBase 两组 WCAG 对比度 ≥ 4.5:1；
   亮色基线同验（`light()` 组）。不达标即红。
3. 硬编码清理计数基线见 §5（每轮卡以 `rg '#[0-9A-Fa-f]{6}' apps/` 复查，
   趋势必须下降）。

## 5. 硬编码清理基线（B-03 时点）

- 清理前：**259 处 / 34 文件**（apps/ 下 hex 六位命中，排除 node_modules/dist/assets）。
- B-03 本轮清理：mobile.css 39 处 → `:root` 变量；DesktopShell 20 处 →
  `tokens.color.*` 引用。清理后计数见交付报告（DoD 要求给出前后数字）。
- **纪律**：此后新增 UI 代码禁止裸 hex——评审按本 ADR 锁定变量名集合。

## 6. 互引（规范↔代码）

- `tokens.ts` 头部注释引用本 ADR（见该文件头 `ADR-006` 链接行）。
- 本 ADR §2 即 V25/V26 规范修订指令——外部规范文档头部应引用本 ADR 编号
  （规范维护方执行，repo 侧以 ADR 为锚）。

## 7. 后果

- 正面：每张 UI 卡不再各选色板；对比度进 CI 守卫；硬编码趋势可度量。
- 负面/接受：外部规范需人工同步一次（三条修订）；Rust 插件投影色板与
  主 UI 不一致（接受——独立域，iframe 插件自持主题语义）。
- 撤销路径：若未来裁决翻为 (b)（规范为准），需全量回归 101KB CSS 与
  双端真机验收——成本记录在案，翻案需新 ADR。
