# alpha-review：DK-1.3 setup 下沉验收——通过（无保留）

> 验收方: Alpha ｜ 2026-10-18 ｜ 对象: fa8f6ad+c580274（Bravo 按批复卡直落）
> 结论: **通过**——1.3 收官，审阅报告 P1 全清（P0+P1 均收官）

## 验收明细

| 项 | 结果 |
|---|---|
| 三函数下沉 bootstrap | ✅ build_hybrid_searcher/restore_plugin_runtime/spawn_hourly_ticker（L907 起，批复授权范围） |
| HybridSearcher 恒等 | ✅ 同参构建（core.search+vec_idx+embed）——static 装配语义不变 |
| fail-open 保持 | ✅ restore_installed+restore_audit 损坏不阻断（installer 语义注记在案） |
| tick 等价搬迁 | ✅ tokio::spawn 逐行等价+六段日志锚原样 |
| 批复三线遵守 | ✅ 回填 spawn 留壳（desktop 私有面）/PluginState+manage 留壳/static 锁留壳 |
| 零 tauri 引入 bootstrap | ✅ Cargo.toml 仅 +1（依赖面核查） |
| 行数 | lib.rs 1723→1684 / bootstrap +85（预估 ~1570 偏差诚实——manage/回填留壳符合批复） |
| 本机四件复跑 | ✅ check 0 / clippy -D 0 / fmt 0 / tsc 0 |
| CI | ✅ 8/8 全绿（c580274 终态注记） |

## 领地例外披露确认

E0716 修复（todo_commands 单测 toggle 临时值借用悬垂）——**合理且必要**：test-only 构建面（我方本机 test 链 webkit 不可行未暴露，CI 桌面面抓获）；修复形态正确（owned 先落再借）；披露先行符合协作纪律。

## 残余

无阻塞项。bootstrap test 链 iroh 本机不可行——CI 兜底（先例）。
