
---

## Alpha 验收记录（2026-09-21 06:4x · 合入 main）

**M3 其余格式（HTML / Notion / OPML）+ S3 铺路验收通过。**

- 本地独立复核：test + clippy + fmt && 链 X=0；30 测试全绿（4 单元 + 25 集成 + 1 doc），dk09_m3_* 8 项含 fail-closed（畸形 HTML/XML fatal）与 fail-open（空 HTML → stem）语义双验
- 有损映射原则延续 S2（未映射透传 + warning），img 保留 + warning 符合内容保真底线
- 接线 request 三问已裁决（同窗口集成/不授权跨领地/sidecar 约定采纳）
- DK-09 进度：S1/S2/M3 格式层完成；余 = 附件 API 落地后 ENEX 改造 + 迁移向导 UI（M3 里程碑内）
