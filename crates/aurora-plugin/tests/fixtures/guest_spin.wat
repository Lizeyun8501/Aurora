;; DK-14d.2 验证载荷 · 死循环 component guest（epoch 打断测试专用）
;;
;; 导出 handle-event（同 guest_minimal 签名）但 core 侧为无限循环——
;; 验证 Store 级 epoch deadline 对 guest 生效（invoke → 打断 → 明确错误）。
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
    (func (export "handle-event")
      (param $c_ptr i32) (param $c_len i32)
      (param $e_ptr i32) (param $e_len i32)
      (param $p_ptr i32) (param $p_len i32)
      (result i32)
      (loop $forever (br $forever))
      (i32.const 0))
  )
  (core instance $i (instantiate $m))
  (alias core export $i "memory" (core memory $mem))
  (alias core export $i "canonical_abi_realloc" (core func $realloc))
  (alias core export $i "handle-event" (core func $he_core))

  (type $he_sig (func (param "capability" string) (param "event" string) (param "payload" string) (result (result))))
  (func $he (type $he_sig)
    (canon lift (core func $he_core) (memory $mem) (realloc (core func $realloc)) string-encoding=utf8))

  (export "handle-event" (func $he))
)
