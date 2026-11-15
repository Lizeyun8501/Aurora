# B-05 交付报告：编辑器往返一致性 property 测试 + E2E 补位

> 执行: Bravo ｜ 工单: TA 2026-11-14 09:01 ｜ 主体: 097c2d5 ｜ 续: 本帧

## 首节 · 测试金字塔现状核实（工单背景对账）

| 维度 | 工单断言 | 实况核实 |
|---|---|---|
| core 454+ vs 编辑器 26 文件 vs E2E 0 | 偏科 | 属实: TS 面 26 测试文件 124 用例（vitest——**不在 CI**）;E2E=55 个 repro 临时脚本零正式化 |
| proptest 已在依赖树 | 可用 | 属实（vitest 无内置 property——**种子 PRNG 自建** mulberry32,失败报 seed 可复现） |
| repro-* 非正式回归 | 需固化 | 固化 e2eTopPaths.mjs（选择器承袭已验证面）+ CI 挂接 |

## 交付物

### ① 往返 property 测试（DoD 首项 ✅）
`shared/ui-components/src/__tests__/b05RoundtripProperty.test.ts`:
- 随机块树生成器——**工单六类全覆盖**: 表格（首行 header）/公式（行内+块级）/mermaid（code_block 语言面）/任务/嵌套列表/块引用+heading1-6/blockquote/hr
- **白名单 W1-W9 显式声明**（每项带理由）: W1 嵌套列表拍平（序列化无缩进语法）/W2 task_id 丢弃（投影域键）/W3 plaintext↔空语言归一/W4 容器三系不入域（序列化-only,E24 面覆盖）/W5 空段落=块界语义/W6 表格 colspan 不生成（GFM v1 无语义）等
- **1200 用例零未声明差异;5000 压力同绿**（DoD ≥1000 达标）;md→doc→md 幂等+种子复现两测
- **A-03 源码模式复用**: 同组 docToMd/mdToNodes API（源码 apply 链同面——工单依赖语义达成）

### ② 真 bug 修复三枚（测试体系首跑即抓——「E2E 0」断档期隐性回归实锤）
| # | bug | 根因 | 修 |
|---|---|---|---|
| 1 | 空 code_block 保存后重载即崩 | `schema.text('')` 抛 Empty text nodes（fence body=[''] join 后空判缺失） | auroraMarkdown 连接后空判 |
| 2 | **移动端生产构建启动即崩**（P0） | React #310: `if(!ready)` 早退门后 A-04/A-06/A-10 三卡加 hooks——ready 翻转即 hook 数变（E2E 断档 3 天无人发现） | 门移至 hooks 全毕,SplashScreen 语义零变 |
| 3 | moveBlock [列表,引用] 换位尾部多空 P | **482ec2a 的 R19-⑤b 原子 replaceWith 修复被 E 系卡回退**成 delete+insert（PM 对空 doc 自动补空段） | 恢复原子 replaceWith（moveBlock 3/3 复绿——B-05 旧测回归实抓） |

### ③ E2E 正式套件（DoD 次项 ✅）
`scripts/tests/e2eTopPaths.mjs`——**10/10 PASS**（本机实跑三连绿）:
P1 新建（库+1）/P2 编辑保存（快照落库+重开回显双证）/P8 markdown 输入成块/P3 搜索（title 面）/P4 建任务（捕获→今日）/P5 今日勾选（态落库）/P6 回收站还原（原 id 回库）/P7 导出视图/P9 零页面错误（双监听）
- 形态: PW chromium+file:// 零服务依赖;BASE 仓相对解析（CI 可移植）
- **诚实边界**: PW 断言过≠真机可用（E25/E26 教训——原生面 cargo/javac 验证另链）

### ④ CI 挂接（「纳入 CI 或周跑」✅）
- `.github/workflows/e2e.yml`: 相关路径 push+每周一 03:00+手动——dist 构建→PW E2E→**TS 覆盖率基线入 job summary**→5000 用例压力
- `.github/workflows/coverage.yml`: 每周日 03:00+手动——cargo llvm-cov workspace（排除集同 CI Test）→**Rust 基线周追踪**+lcov 产物

### ⑤ 覆盖率基线（DoD 三项——**数字先行,门禁待评审**）

| 面 | 基线（本机实测 2026-11-15） | 门禁（提案——待评审确认） |
|---|---|---|
| core (Rust) | **CI 周跑首跑出数**（本机 2核4G 插桩全量不可行——coverage.yml 通道） | ≥90%（提案） |
| **编辑器 src/editors** | **26.96% lines**（RichEditor 16.47/auroraEditor 11.61/codeHighlight 93.02/themeVm 89.34——分层明细） | ≥80%（提案——**未达,缺口披露**） |
| 参照系 | blocks 95.28/stores 95.95/schema 80.34/theme 89.34/views 60.93 | — |

**缺口诚实披露**: editors 26.96% 距 80% 门禁缺口 53pp——主因 RichEditor.tsx（1330 行 Loro/插件生命周期重面）+auroraEditor.ts（318 行编辑器创建链）需 jsdom 重装夹具;非本卡可补（估 3-4 人日独立卡）。**门禁值不在本卡启用 enforce**——基线数字+周追踪通道已立,达线后评审启用。

### ⑥ TOP20 用户路径 manifest（每条≥1 断言锚）

| # | 路径 | 锚（自动化） | 形态 |
|---|---|---|---|
| 1 | 新建笔记 | e2eTopPaths P1 | PW |
| 2 | 编辑保存 | P2（快照+回显双证） | PW |
| 3 | 搜索 | P3（title 面）+core FTS cargo 测试 | PW+cargo |
| 4 | 建任务 | P4 | PW |
| 5 | 今日勾选 | P5 | PW |
| 6 | 回收站还原 | P6+mock-test.mjs node 直测 | PW+node |
| 7 | 导入 | core import 测试（ImportWizard 面）+桌面 tsc | cargo |
| 8 | 导出 | P7（视图面）+E25 SAF javac 验证链 | PW+javac |
| 9 | 同步冲突 | core conflict 测试族（R 系列回执链） | cargo |
| 10 | 加密解锁 | core crypto vault 测试族 | cargo |
| 11 | markdown 输入成块 | P8 | PW |
| 12 | 块引用 | a05BlockRefBreadcrumb 16 用例 | vitest |
| 13 | 集合视图 | a07CollectionVm 9 用例 | vitest |
| 14 | 主题定制 | a11ThemeVm 11 用例 | vitest |
| 15 | 源码模式 | b05 往返同组+markdownSourceRoundtrip | vitest |
| 16 | 移动块换位 | moveBlock 3 用例（本卡复绿） | vitest |
| 17 | 表格编辑 | auroraMarkdownTable 5+tableOps | vitest |
| 18 | 每日笔记 | dk19_daily 5 用例（A-12） | cargo |
| 19 | 数学公式 | mathRoundTrip+mathInputRules | vitest |
| 20 | 容器块 | e24ContainerRoundTrip+BlockRenderer | vitest |

## 验证矩阵
- **124/124 全绿**（vitest 全套——含本卡新增 3）;E2E 10/10 三连跑;moveBlock 3/3
- 双端 tsc 零错;fmt RC=0;CI（主体+续帧）: desktop-check/Clippy/Rustfmt/MSRV/Wasm×3/Test 分离终证见 push 后
- 停手边界: 不做视觉回归截图比对（v2）✓/不做跨浏览器矩阵（恒 chromium 单通道）✓

## 附 · B-06 断档事故链案录（帮 Alpha 收口两笔——CI 三帧红分离纪实）

desktop-check 三帧连红（ce52696 绿→ec5c26f 红起）诊断链——「annotation
碎片是诊断素材不是结论」纪律实证（oom 注释=dmesg 零命中空噪声）:

| 帧 | 表象 | 真凶 | 修 |
|---|---|---|---|
| ec5c26f(B-06)→2ad5de4 | exit 3+『OOM』注 | 3315499 wip 原子落把**整份 lib.rs(87KB)覆写进 SyncStatusBadge.tsx**→desktop 前端构建零产物（本机 vite build RC=1 复现） | 恢复 461c0c6 DK-48 组件（0c7128c）;**T6 Badge 实时化 UI 层丢失待 Alpha 重落**（Rust 发布点在） |
| 0c7128c | exit 101 两编译错 | B-06 桥码: E0308 failed_docs=Vec 误按整数比较+E0277 bump_attempts Error 域错（前帧被前端红掩蔽浮面） | 91fa687 双修（本机 fakepc 树 check RC=0 实证） |

附: swap 4G→8G（1fcb8af）保留——workspace 增长趋势下余量无害;E2E workflow
CI 全绿（2ad5de4 帧实证）。
