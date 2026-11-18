## 验收总闸（2026-11-18 TA 10:57 定——所有卡共用关账门槛）

1. **CI 五门全绿** + `cargo +1.91 clippy --workspace --exclude aurora-desktop -- -D warnings` 零告警
2. **零静默占位**: 新增代码无 Null/空集合/{"ok":true}/伪文本/静态兜底冒充结果;未实现必须 NotImplemented（C-04 dataBase64:'' 假数据撤即本条实例）
3. **WritePath 不旁路**: 新增命令一律 apply_note_change 链路,集成测试可检出旁路
4. **双端对齐**: 新能力双端有消费者,或标「桌面/移动专属」+理由
5. **无障碍内建**: UI 卡 DoD 五项——语义标签/键盘可达/对比度/触摸目标/读屏遍历
6. **交付报告五项**: commit hash/挂接清单自查/停手边界/CI 结果/双端影响面——缺一不关账

# AGENTS.md — Aurora 工作区协作规范

> 本文件服务所有接入本仓的执行 Agent（多 agent 并行工作流）;README「文档索引」锚点。
> 首要原则: **领地纪律**——只改工单声明域;接口不够用先写 request 文档，不硬改冻结面。

## 五条铁律

### 1. 派卡前重新拉 HEAD
开工必 `git pull`（fetch 不够——本地链缺父 commit 时 push 会被拒）;开工前
`git log --all --grep <工单号>` 查在途同名卡（命中即让位——重复实现=双套 API）;
「用户裁决」类工单先核 git log 裁决链（卡面文字可能已被 V2 修订覆盖）。

### 2. 单卡单发 commit
一张工单一个（或一组语义完整）commit;commit message 含工单号+验证矩阵;
commit 前必 `cargo fmt --all` **实跑**（--check 假绿有过三犯实录）;收束后
`git status` 复核零脏再 push。

### 3. 禁止静默占位
- 未验证的结论不写「已完成」;环境受限（无法本地编译/测试的面）在交付报告
  显式披露,由 CI 兜底终证
- 「受阻披露≠获准变更」——受阻结论须实验证伪,形态裁决类停手报告先于自行改形
- 复核发现全收不辩解;错因学习入 daily/台账

### 4. CI 五门
交付前本地最小集（改动面 crate 真实重编）+ push 后 CI 终证:
**Test (stable) / Clippy (-D warnings) / Rustfmt / MSRV (1.91) / desktop-check**
（+Wasm 三 guest 构建;E2E/coverage/backup-drill 周跑通道另立）。CI 红先读
check-runs annotation raw-tail 定位（免认证诊断通道）——annotation 碎片是
诊断素材不是结论,判因前须排他验证。

### 5. 挂接清单自查
交付前逐项核对工单任务的挂接面（例: 新命令→invoke_handler 注册+桥+DTO+测试
四点同步;新测试→CI 可跑;新文档→README 索引+索引自检脚本通过）;**测试名单
grep 核验**（防 feature 门控静默吞测试）;`rg` 验证一律无管道直跑（`| head`
吞 rc 全假绿）。

## 补充约定
- 长命令（cargo/git 大操作）本地一律 `setsid` 落盘+轮询——会话 504 闪断不丢工作
- 本地全量验证不可行（2核4G）: 分批跑改动面 crate + 快门（clippy CI 同款）+ CI 兜底
- 帮他方收口 CI 红时,commit 注明「帮 X 收口」+ 领地例外披露
- 诚实边界优先: 交付报告首节做「工单假设 vs 代码实况」对账——过时假设如实披露
