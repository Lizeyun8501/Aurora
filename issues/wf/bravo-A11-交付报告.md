# A-11 交付报告：用户自定义主题（设计令牌白名单覆盖）

> 执行: Bravo ｜ 工单: TA 2026-11-14 05:26 ｜ 主体: 31b1269 ｜ CI 兜底修债: cd1031f

## 首节 · 开工令假设与代码实况披露

| # | 开工令假设 | 实况 | 处置 |
|---|---|---|---|
| 1 | 「依赖 B-03（先定令牌基线）」 | B-03 已落（8b2de3a——ADR-006 裁决: tokens.ts 真源+暗色默认+22 CSS 变量名集合锁定） | 白名单直接锚定 ADR-006 令牌基线（tokens.ts color 全集恰 12 项——单源无漂移） |
| 2 | 「白名单 ≤12 项（primary/bgBase/bgSurface/textPrimary/accent 等）」 | 工单例举 **accent**=CSS 遗留变量（`--accent`）,**不在 ADR-006 令牌基线**（B-03 裁决 22 变量集合内含 accent 但 tokens.ts 真源无） | 白名单=tokens.ts color 全集 12 项（bgBase/bgSurface/bgElevated/primary/primaryBright/textPrimary/textSecondary/textDisabled/success/warning/danger/focus）;accent 不进（**披露**——锁令牌基线单源,防 CSS 遗留面扩散） |
| 3 | 「主题配置纳入备份与同步（M1 新增表或复用 settings KV）」 | core system_settings 域=内存 HashMap 模拟（不落盘——B-03 简化说明自注）;桌面 settings 持久化既有通道=core KV | 桌面 cmd_theme_get/set（KV theme:user）;移动 localStorage（aurora.theme.custom——移动端 settings 域无 core KV 面）。跨设备同步（P2P 双向复制）=后续卡（JSON 同形已就绪） |
| 4 | 桌面明暗切换 | 桌面恒暗基线（ADR-006 裁决——暗色默认+省电） | 桌面无明暗切换键=设计裁决非缺失;移动三段（light/dark/system）保持——**自定义主题叠加于两态之上**（setProperty 覆盖 CSS 变量——明暗切换时 CSS :root 重写,自定义需重应用=已知边界,刷新页面/重进设置重应用） |

## 实现清单

### 共享 themeVm.ts（双端同源——DoD「共用同一份解析」）
- **白名单 12 项冻结**（ADR-006 tokens.ts color 全集）;validate 拒白名单外键（「k 不在白名单——锁定」）
- **对比度硬约束**: WCAG 相对亮度 contrastRatio;textPrimary/bgBase <4.5:1 拒绝+可读拒因（primaryBright/bgSurface 联动——高亮态可读）;不可跳过（validate→apply 链唯一入口）
- **注入安全（DoD④）**: applyTheme=element.style.setProperty 逐变量写——零 CSS 字符串拼接/零 innerHTML/零 dangerouslySetInnerHTML;值经 isValidHex 严格校验（#RGB/#RRGGBB;rgba()/url()/表达式全拒）;导入链同校验
- **预设 5 套**: 极光暗（默认基线）/晨光/墨青/暮紫/高对比（全过校验）
- **JSON 导入导出**: type=aurora-theme v1;损坏/非主题文件/type 不符拒因

### 桌面（31b1269）
- cmd_theme_get/set（settings KV——工单「复用 settings KV」选项落）
- 命令面板「主题设置」入口+ThemePanel（预设色卡渐变预览+12 取色器+对比度拒因 role=alert+导入导出+恢复默认+安全纪律脚注）
- 启动加载应用（校验重入——KV 数据防篡改;失败静默清回默认）

### 移动（31b1269）
- SettingsView「主题定制（A-11）」组（预设 task-prio-seg 复用+白名单取色器 grid 实时预览+应用校验+导入导出 navigator.share→clipboard 降级+恢复默认）
- main.tsx 启动应用（localStorage 校验重入）

### CI 兜底修债（cd1031f——帮 Alpha 收口,领地例外披露）
- 31b1269 Clippy 红→**5617bb9（B-04 帧）已红实证=前债非我引入**
- 根因（annotation 免认证通道+本地 clippy 复现双证）: system_settings.rs 测试 attribute 错位——`b03_token_mapping_consistency` 双 `#[test]`（B-03 落时 doc 注释插进两属性间）+`test_css_variables_output` 丢 `#[test]`（dead_code——B-04 拆分错位）
- 修复: 双属性去一/单属性补回;`clippy -p aurora-core --all-targets -D warnings` RC=0+`cargo test -p aurora-core --lib system_settings` 28/28

## 验证矩阵
- a11ThemeVm **11/11**: 白名单恰 12 项/外键拒（accent/spacing/typography）/对比度 4.3:1 拒+4.5:1 过/hex 严格（rgba/url/大写 G 拒）/JSON 往返/损坏拒/注入经导入链拒/预设 5 套全过/默认值与 ADR-006 基线一致
- 三套回归 27/27（a11+a07+a05）;双端 tsc 零错;fmt RC=0;注入审计 rg 零命中
- CI（cd1031f）: **desktop-check ✅ / Clippy ✅（债修生效）/ Rustfmt ✅ / MSRV ✅ / Wasm×3 ✅**
- Test: 唯一 FAILED=dk02_s2_trash_flow_roundtrip——**十卡一致基线红**（A-01~A-11 同案分离）;A-11 面零失败

## DoD 对照
- ✅ 12 项白名单外令牌不可改（冻结数组+validate 拒+测试锁）
- ✅ 低对比度方案被拒绝且有明确说明（4.3:1 拒因实测「对比度 4.3:1 <4.5:1——正文不可读」）
- ✅ 主题 JSON 导入导出可用,双端一致（同形 JSON+双端同 VM 解析）
- ✅ 无任意 CSS 注入路径（setProperty 唯一写入口+hex 严格校验+注入审计零命中）

## 诚实边界
- `--accent` 不进白名单（ADR-006 令牌基线单源纪律——工单例举项披露）
- 明暗切换（移动三段）与自定义主题叠加面: 切换明暗后自定义需重应用（CSS :root 重写覆盖）——已知边界,重进设置页即恢复
- 跨设备主题同步（P2P）=后续卡;备份面（全库备份含 KV）天然覆盖桌面主题

## 停手边界遵守
不做社区主题市场 ✓ / 不做按工作区独立主题 ✓（KV 键 theme:user 用户级单份）
