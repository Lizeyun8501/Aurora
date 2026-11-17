# 发现回执 R-β1：dk20_vault T5「锁定态增量保存泄漏明文」CI-only flaky

> 复核: Bravo ｜ 2026-11-18 ｜ 域: aurora-core vault/write_path（Alpha 领地——回执不硬改）
> 触发链: eec422f（Bravo CI 帮收口帧,Test 构建面修复后 T5 首次完整跑通显形）

## 现象
- **CI 红**: `create_then_lock_pipeline ... FAILED` at `dk20_vault.rs:246` —— assert `rec.content.starts_with("enc1:") && !rec.content.contains("def")`（「锁定态增量保存泄漏明文」）
- **CI 重试同红**（test-retry.log 二连 FAILED——非单次抖动）
- **本地 5/5 绿**（同 HEAD eec422f,定向 `cargo test --test dk20_vault create_then_lock_pipeline` 五连 1.51s 稳定过）

## 历史基线
| 帧 | T5 | 备注 |
|---|---|---|
| 04da8aa（DK-20 出生帧） | ✅ 5/5 | 作者自证 |
| ab3dea7（B-09,11-16 晨） | ✅（Test 仅 dk02_s2 红） | T5 完整跑过且绿 |
| d2a856b→8ea386c（C-05~C-07） | 未跑（构建面红掩蔽） | 三连帧 build 失败 |
| **eec422f（构建修复后）** | ❌ CI/✅ 本地 | **窗口内零 vault 域 commit** |

## 审计（write_path/note.rs save_note_content 全读）
完成语义完好：`seal_content → persist_doc.await → put_note_meta.await → events → Ok(WriteReceipt)` —— 测试侧 `save.await.unwrap()` 后串行 load,无悬空任务。

## 嫌疑（供 Alpha 排查,未定谳）
`record.encryption` 陈读竞态：T5 序列 `set_note_encryption(aes) → save_note_content` —— save 入口 229 行 `load_note_meta` 若在 CI I/O 时序下读到**陈旧 encryption=None** 的 record,则 243 行 `seal_content(ctx, note_id, content, &record.encryption)` 明文透传 → 断言炸且库中真实落明文（**非纯测试问题——若坐实为安全洞**）。本地快盘 read-your-writes 恒成立掩蔽;CI runner I/O 调度差显形。建议方向:①set_note_encryption 与 save 间加写屏障/同事务性;②load_note_meta 读路径缓存陈读面审计;③测试加 `encryption=aes` 前置 assert 以便失败时区分「陈读」vs「漏 seal」。

## 处置建议
Alpha 域安全面——按对向审核纪律回执不硬改。CI Test 当前面=dk02_s2（十七卡基线）+本项;下次 vault 域卡可顺带定性（本地循环 100 次+CI 复跑对照）。
