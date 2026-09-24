# ADR-005: 前端收敛执行决策（DK-0F 落地方向修正）

- 状态: Accepted
- 日期: 2026-09-24
- 决策人: Alpha（装配切片）
- 关联卡: DK-0F（M0 · Phase 0 止血与收敛）
- 关联: ADR-001（React 18 + Zustand 统一）、DK-05M-V 验证报告（GO）、R-04 排期重排

---

## 背景

DK-0F 实测三前端两编辑器栈，核心断言在今天（2026-09-24）复核**全部仍然成立**：

| 项目 | 编辑器 | 现状 |
|---|---|---|
| `apps/web` | TipTap（无 CRDT） | 零迭代遗留，无 CI/workspace 特殊引用 |
| `apps/desktop/src` | 无编辑器内核 | **Tauri 实际加载**（`frontendDist: "../dist"`），M2 全部桌面 UI 交付落点 |
| `apps/mobile` | ProseMirror + loro-prosemirror 0.4.4 | 唯一接 Loro 的编辑器端 |
| `shared/ui-components` | TipTap 系 editors | **零真实 import**——mobile 的 `@aurora/ln` alias 指向不存在的 `shared/ln/src`（死路径），desktop 无共享层依赖声明 |

### 与 DK-0F 卡的偏差

DK-0F 卡定 **方案 A：保留 `apps/web` 为唯一产物，删除 `apps/desktop/src`**。
该文本写于三前端混沌期（当时 `frontendDist` 尚指向 `../web/dist`）。

M2 执行期间实际走向了相反路线：

1. `frontendDist` 已切换为 desktop 自身构建产物（`../dist`），桌面端事实上以 `apps/desktop` 为唯一前端；
2. M2 全部桌面 UI 交付（CommandPalette、tags/deadline、ImportWizard、wifi_only 命令面板开关）均落在 desktop shell，工作流与团队投入已固化；
3. `apps/web` 自 V19 后零功能迭代，TipTap 栈无 CRDT，沉没成本最低的一半是 web 而非 desktop。

**本 ADR 不改变 DK-0F 的目标（收敛为单前端单编辑器栈、共享层真实引用、桌面编辑器接 Loro），只修正路径选择，并为修正补正式记录。**

---

## 决策

### D1 — 唯一桌面产物：`apps/desktop`（方向修正为「保 desktop、删 web」）

- `apps/desktop` 是唯一桌面端前端运行时产物，`frontendDist: "../dist"` 为唯一合法指向；
- **删除 `apps/web`**（git 历史可溯，TipTap 依赖随之一并移除）；
- 「方案 A」作废。删除代价对比：web 侧为 TipTap 死代码 + 配置；desktop 侧为 M2 全部已验收桌面交付——不可比。

### D2 — 编辑器统一：ProseMirror + loro-prosemirror

- 编辑器内核唯一选型 **ProseMirror + loro-prosemirror + loro-crdt**（与 mobile 现栈一致，DK-05M-V 已出 GO）;
- mobile 的 `apps/mobile/src/editor/`（auroraEditor.ts / schema.ts / RichEditor.tsx）**上移**至 `shared/ui-components/src/editors/`，替换 TipTap 系 `DocumentEditor` / `CanvasEditor`（重写，块渲染器不受影响）;
- TipTap 全系依赖从所有 package.json 移除（可选依赖亦不保留，避免双栈复活）;
- `loro-crdt` / `loro-prosemirror` / `prosemirror-*` 以 peerDependencies 声明于共享层，由宿主（desktop / mobile）提供。

### D3 — 共享层唯一化与三名分裂修复

现状三名分裂：目录 `shared/ui-components` ≠ package name `ln-components` ≠ 引用名 `@aurora/ln`（alias 死路径）。收敛为：

- package name 固定 **`@aurora/ln`**，目录保留 `shared/ui-components`（避免大移动），vite alias 修正为指向 `../../shared/ui-components/src/index.ts`;
- desktop / mobile 双端 package.json 显式声明并真实 import（blocks 渲染器、stores、hooks 优先——editors 待 D2 上移后接入）;
- `shared/core-api`、`shared/types` 不在本 ADR 范围（契约层归 DK-00/R-02）。

### D4 — 无障碍

按 R-04 决议，A1–A9 拆入各 UI 卡 DoD；本 ADR 涉及的共享组件改造在每一卡验收时随带对应验收项，不另立卡。

---

## 任务分解

1. **删除 `apps/web`** + 清理 TipTap 依赖与 vite 残留配置（桌面侧独立可做，先行）
2. **共享层别名修复**：`@aurora/ln` alias 改指真实路径，desktop/mobile 声明依赖，blocks/stores 至少各 1 处真实 import（达成 DK-0F DoD 的非零引用项）
3. **编辑器上移**：mobile editor 三件套迁入共享层 editors/，TipTap 系 editors 删除
4. **桌面编辑器接入**：desktop shell 正文区从预览组件升级为共享层 DocumentEditor（ProseMirror + Loro 绑定）——即 DK-05 的起点，边界划归 DK-05 卡执行
5. 全程每步跑双端编译门禁

## 退出条件（= DK-0F DoD 复核版）

- [ ] `frontendDist` 唯一指向 desktop 产物，`apps/web` 不存在
- [ ] `@aurora/ln` 被 desktop 与 mobile 至少各 1 处真实 import（非 alias 死路径）
- [ ] 桌面端编辑器与 Loro CRDT 建立绑定（D2 完成后）
- [ ] 双端编译门禁绿（desktop tsc+vite build / mobile tsc+vite build）

## 已知代价

- 共享层 editors/ 的 TipTap 部分重写为 ProseMirror（DK-0F 卡已预认；范围 = DocumentEditor/CanvasEditor 两个文件）
- desktop 正文区当前预览组件将被编辑器替换，涉及 DesktopShell 布局调整（DK-05 卡内消化）
- mobile editor 路径迁移后需同步 vite alias 与测试文件路径

## 风险

- loro-prosemirror 桌面端行为需按 DK-05M-V 报告「真机补验清单」思路在桌面侧复验输入时序（滚轮/IME 组合输入）——列入 DK-05 开工首项
- 三名分裂修复可能触及 CI 路径断言（R-00 门禁脚本），改动时同步核对 workflow 文件
