# Alpha 交付报告 — DK-03 S3 语义召回徽章（S3a core 面 + S3b 全链接线）

> 基线 7a206a5（S2 验收回执）· S3a=158564a（core）· S3b=81d066c（desktop 全链）· issue「语义召回徽章：不含关键词的结果必须解释来源，否则用户不信任搜索」

## 一、交付内容

### S3a core 面（158564a）
- `search_vector` 返回 **VectorHit**（note_id+score+preview）——S2 签名变更列明（(String,f32)→结构体，调用点 core 内 5 处全对齐）；
- **S2 留缺口关闭**：hybrid 纯向量命中 snippet = content_preview（与 content_hash 同步的新鲜快照）；
- title 仍留空——**架构裁决**：改标题不触发重嵌（content_hash 只看内容），title 入向量记录必过期 → 由调用方实时解密回填（注释即契约）；
- T7 纯向量 snippet 断言（只向量化不进 BM25 → source=Vector + snippet=preview）+ T1 补 preview 断言。

### S3b desktop 全链（81d066c）
1. **装配**：setup 组装 HybridSearcher（vector_index_for + EmbedFromAiProvider，nomic-embed-text 768 口径）——诚实标注：维度配置口径冻结，AIProvider 实际维度不符时向量面静默失效、BM25 兜底（不做 probe 惰性组装，模型配置化留配置卡）；
2. **boot 回填 spawn**：预载解密 content_of（backfill 同步闭包的唯一路径）+ 限速 250ms×50 篇防本地推理过载；失败 log warn（Ollama 缺席=诚实降级）；
3. **cmd_search_notes 加 mode**：hybrid 走 RRF；**纯向量 title 实时解密回填**；source 透传（Debug 格式 "Bm25"/"Vector"/"Both"）；**顺手修现网 bug**——后端返回 doc_id 而前端契约 note_id（键名错位，真机搜索跳转应为坏的）→ 双键兼容；
4. **索引管线**：cmd_update_note 内容变更 → vectors.index_note fire-and-forget（hash 去重内建，重复保存不重嵌）；**单篇 purge + 批量 purge_expired 接 remove_note**（trash 软删靠检索面过滤，purge 后残留会命中已删笔记——残留防御）；
5. **前端**：PaletteItem.source 徽章（Vector=「语义」紫 / Both=「混合」蓝，title tooltip 解释来源）+ 语义检索开关（checkbox → mode=hybrid，标注「Ollama 未运行时自动退回全文」）+ RawHit 桥接。

## 二、验证矩阵

| 门 | 结果 |
|---|---|
| core 全量 | ✅ **408 passed / 0 failed**（+T7 纯向量 snippet / +T8 remove_note 联动） |
| clippy / fmt | ✅ 0 warning / fmt 净（core + desktop fmt --check） |
| 前端 | ✅ tsc --noEmit 净 + vite build 通过（3.24s） |
| desktop 全量编译 | ⚠️ 本地 GTK 系统依赖无 root 不可装——**CI desktop-check 兜底**（环境限制如实记录） |
| CI | Rustfmt ✅；其余见回填段 |

## 三、挂起项

1. 模型/维度配置化（现冻结 768 口径——配置卡）；
2. BGE-reranker 精排（S3 可选面）；
3. backfill 预载全库解密内存峰值——按需解密优化留待（个人库量级可控）；
4. BM25 分页语义（hybrid total 无分页——后续卡）。

— Alpha 2026-10-01（DK-03 S3）

## CI 终验（回填）

（待 CI 完成后回填）

— Alpha 2026-10-01
