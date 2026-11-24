# ADR-003: WritePath 唯一写入入口

- 状态：已接受（V26 I2 / DK-01W）
- 日期：2026-09-14
- 关联：ADR-002（契约治理）、ADR-004（权威源三阶段）

## 背景

V25 之前桌面（Tauri cmd）与移动（JNI impl）各自实现笔记写入：
两条互不相通的路（卡 DK-01W 原话），行为漂移、加密语义不一、
blocks 派生与事件发布各自维护——同构对拍测试揭示桌面 seal 加密
与移动明文路径在快照产出上曾不一致。

## 决策

`aurora-core/src/write_path.rs` 成为**唯一写入入口**：

```
load → apply → 原子保存（WAL: 快照先落, 元数据后落为权威指针）
     → blocks 派生（失败不阻断） → 发事件（High 实时 + Medium 持久化）
```

- 桌面 cmd 与移动 JNI 全部委托同一实现（I2, commit 0516dc4）
- WriteContext 注入差异：seal（桌面加密）/ None（移动明文）
- blocks 派生失败仅告警（ADR-004 派生侧语义）

## 后果

- 正面：双端行为同源可对拍（isomorphic_write_path.rs 3.5 场景锁死）；
  新增写入操作只改一处；事件序号/快照/派生顺序有单一权威定义
- 代价：desktop 与 mobile-ffi 不再各自持有写逻辑（迁移期删除代码 ~70 行）
- 回滚：不设回滚路径（单入口即正确性来源）

## 验证

- 同构对拍：桌面 seal vs 移动明文同序列操作，key 集/元数据/blocks/删除状态全一致
- `cargo test -p aurora-bootstrap --test isomorphic_write_path`

## 增补：KV 命名空间注册表（O-02 / 2026-11-24）

WritePath 管住了笔记域（A 类）写入，但 B 类（配置/缓存/派生态）直写 KV
长期无门禁：前缀随处新增、无「含用户内容须 seal」约束。本次增补
`kv_namespace_registry.rs` 为**写入面分类的单一来源**：

- **A 类**（note/notesnap/trash/attachment）——必须经 WritePath；豁免
  白名单（write_path 自身/projection.rs/bootstrap）之外直写即 CI 阻断
- **B 类**（settings/palette/fsrs/liq/plugins 等 19 前缀）——允许直写
  但必须登记；**含用户内容且未 seal** 者以 `debt` 字段显式挂 OPEN 债
  （当前 7 处：event/cal/task/dbview/bookmark/theme/liq——seal 改造另卡）
- **门禁双闸**：Rust 单测（5 条：前缀唯一/A 类强制 seal+sync/B 类
  seal-or-debt/三形态 lookup/债面快照锁）+ Node 静态扫描
  （`scripts/check-kv-namespaces.mjs`：全仓 `kv*.set(` 字面量前缀
  对账，未登记/A 类直写即 exit 1；内联 `#[cfg(test)]` 与 tests/ 目录
  为夹具白名单）
- 新增前缀流程：REGISTRY 登记一行（prefix/class/user_content/sync/
  sealed/debt 六元组）→ CI 自动收编

前缀语义歧义处理先例（登记时归类，勿改实现）：
`notevec`/`palette`/`settings`/`shortcut` 判非用户创作内容
（可重建缓存/产品词表/配置面），`bookmark` 判用户内容挂债——
争议项以 purpose 字段留痕，PR 审查可翻案。
