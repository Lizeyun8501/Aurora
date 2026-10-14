// DK-14c.1 台账②：wit 目录变更→guest 重编（wit 不在 cargo fingerprint——
// 14c.1 CI wasm job 0.1.0 假败根因；挂载 rerun-if-changed 根治）
fn main() {
    println!("cargo:rerun-if-changed=../aurora-plugin/wit");
}
