;; DK-14d.2 验证载荷 · 最小 component guest（wat 文本，workspace `wat` crate
;; 编译——零外部工具链）
;;
;; 导出面与本卡 invoke 映射对齐（顶层函数导出）：
;; - init(capability: string) -> result          — 返回 result::ok
;; - handle-event(capability, event, payload: string) -> result — 返回 result::ok
;;
;; canonical ABI 完整实现：memory + canonical_abi_realloc（bump 分配器）+
;; cabi_post_* post-return 函数（调用后解锁实例——二次 invoke 必须，DK-14d.2
;; 实证：缺 post-return 则第二次调用报 "cannot enter component instance"）。
(component
  (core module $m
    (memory (export "memory") 1)
    (global $heap (mut i32) (i32.const 1024))
    (func (export "canonical_abi_realloc")
      (param $orig i32) (param $old i32) (param $align i32) (param $size i32) (result i32)
      (local $ptr i32)
      (local.set $ptr (global.get $heap))
      (global.set $heap (i32.add (global.get $heap) (local.get $size)))
      (local.get $ptr))
    (func (export "init") (param $cap_ptr i32) (param $cap_len i32) (result i32)
      (i32.const 0))
    (func (export "cabi_post_init") (param $r i32))
    (func (export "handle-event")
      (param $c_ptr i32) (param $c_len i32)
      (param $e_ptr i32) (param $e_len i32)
      (param $p_ptr i32) (param $p_len i32)
      (result i32)
      (i32.const 0))
    (func (export "cabi_post_handle-event") (param $r i32))
  )
  (core instance $i (instantiate $m))
  (alias core export $i "memory" (core memory $mem))
  (alias core export $i "canonical_abi_realloc" (core func $realloc))
  (alias core export $i "init" (core func $init_core))
  (alias core export $i "cabi_post_init" (core func $post_init))
  (alias core export $i "handle-event" (core func $he_core))
  (alias core export $i "cabi_post_handle-event" (core func $post_he))

  (type $init_sig (func (param "capability" string) (result (result))))
  (func $init (type $init_sig)
    (canon lift (core func $init_core) (memory $mem) (realloc (core func $realloc)) (post-return (core func $post_init)) string-encoding=utf8))

  (type $he_sig (func (param "capability" string) (param "event" string) (param "payload" string) (result (result))))
  (func $he (type $he_sig)
    (canon lift (core func $he_core) (memory $mem) (realloc (core func $realloc)) (post-return (core func $post_he)) string-encoding=utf8))

  (export "init" (func $init))
  (export "handle-event" (func $he))
)
