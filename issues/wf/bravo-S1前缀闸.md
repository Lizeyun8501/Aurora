# bravo-S1前缀闸 注记+交付（2026-10-17）

## 落实

1. `HOST_RESERVED_PREFIXES` 硬清单（wasm_runtime.rs）——`plugins:`/`settings:`/`links:`；键域 re-scan 复核：attachblob:/trash:/sync:/backup: 无插件 KV 通路（doc 面+write_path 面），闸随通路扩容同步登记
2. `reserved_prefix_of` 纯函数 + storage_guard 拦截（仅 Write 面——v1 拦写不拦读，诚实边界）；**先于 namespace 检查**——`{id}:` 键形相交（id=`plugins` 边界形）也拒；denied reason=reserved-prefix+审计行（doc_id+prefix 明细）
3. 单测 5 例：命中拒+审计行/边界形兜底/doc 键不误伤/纯函数命中判定/read v1 放行口径
4. 设计稿 §2 闸状态注记落档（间接保证→强制闸落地）

## CI 兜底修债（领地例外披露）

b66b1a2（Alpha DK-链接跟随）write_path.rs `use KVStore` cfg 门误移除——follow_links_on_rename 经 WriteContext.core 具体类型间接消费 KV，不引 trait use→default features 面 unused（clippy -D warnings 红实证）。最小机械修：cfg 门复归+注释如实更正。

## 验证矩阵

| 门 | 结果 |
|---|---|
| plugin lib 135（含本卡 5） | 全绿 |
| plugin e2e 22 | 零回归 |
| core 455 | 全绿 |
| clippy --all-targets -D warnings（core+plugin） | 0 |
| fmt 实跑+check | 0 |
