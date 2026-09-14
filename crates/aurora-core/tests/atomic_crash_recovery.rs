//! V26 I3/DK-01 故障注入测试 — 原子事务崩溃恢复（三步事务 + WAL）
//!
//! 模拟三类崩溃现场，断言 recover_on_startup 后数据零丢失：
//! 1. tmp 写入后崩溃（tmp 存在 + pending_writes 登记 → 重做完成）
//! 2. rename 后崩溃（tmp 已消失 + 目标存在 → 清理登记）
//! 3. 未开始的登记（tmp/目标都不存在 → 跳过）
//! 4. 路径穿越攻击登记 → 拒绝恢复

use aurora_core::l1_infrastructure::atomic_transaction::{AtomicTransaction, PendingWrite};

fn pw(id: i64, file_path: &str, tmp_path: &str) -> PendingWrite {
    PendingWrite {
        id,
        file_path: file_path.to_string(),
        tmp_path: tmp_path.to_string(),
        loro_op_id: format!("op-{id}"),
    }
}

#[test]
fn crash_recovery_tmp_exists_redoes_write() {
    let dir = tempfile::tempdir().unwrap();
    let tx = AtomicTransaction::new(dir.path());

    // 崩溃现场 1: tmp 已写入但 rename 未执行
    let tmp_name = "notes_a_1.tmp".to_string();
    std::fs::create_dir_all(dir.path().join(".tmp")).unwrap();
    std::fs::write(
        dir.path().join(".tmp").join(&tmp_name),
        b"crashed content v1",
    )
    .unwrap();
    // 崩溃前登记了 WAL
    let pending = vec![pw(1, "notes/a", &tmp_name)];

    let recovered = tx.recover_on_startup(&pending).unwrap();
    assert_eq!(recovered, vec!["notes/a".to_string()], "tmp 恢复重做");
    let restored = std::fs::read(dir.path().join("notes/a")).unwrap();
    assert_eq!(restored, b"crashed content v1", "零丢失: 崩溃前内容保留");
    assert!(
        !dir.path().join(".tmp").join(&tmp_name).exists(),
        "tmp 清理"
    );
}

#[test]
fn crash_recovery_target_exists_is_cleaned() {
    let dir = tempfile::tempdir().unwrap();
    let tx = AtomicTransaction::new(dir.path());

    // 崩溃现场 2: rename 已完成但 WAL 未清（tmp 消失, 目标存在）
    std::fs::create_dir_all(dir.path().join("notes")).unwrap();
    std::fs::write(dir.path().join("notes/b"), b"final v2").unwrap();
    let pending = vec![pw(2, "notes/b", "notes_b_2.tmp")];

    let recovered = tx.recover_on_startup(&pending).unwrap();
    assert_eq!(recovered, vec!["notes/b".to_string()], "已完成写入被确认");
    let restored = std::fs::read(dir.path().join("notes/b")).unwrap();
    assert_eq!(restored, b"final v2");
}

#[test]
fn crash_recovery_never_started_is_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let tx = AtomicTransaction::new(dir.path());

    // 崩溃现场 3: 登记了但从未写 tmp（启动即崩）
    let pending = vec![pw(3, "notes/c", "notes_c_3.tmp")];

    let recovered = tx.recover_on_startup(&pending).unwrap();
    assert!(recovered.is_empty(), "未开始 → 跳过");
    assert!(!dir.path().join("notes/c").exists());
}

#[test]
fn crash_recovery_blocks_path_traversal() {
    let dir = tempfile::tempdir().unwrap();
    let tx = AtomicTransaction::new(dir.path());

    // 崩溃现场 4: 恶意 WAL 登记路径穿越 — 必须拒绝而非恢复到任意路径
    let tmp_name = "evil.tmp".to_string();
    std::fs::create_dir_all(dir.path().join(".tmp")).unwrap();
    std::fs::write(dir.path().join(".tmp").join(&tmp_name), b"malicious").unwrap();
    let pending = vec![pw(4, "../../etc/passwd_aurora", &tmp_name)];

    let recovered = tx.recover_on_startup(&pending).unwrap();
    assert!(recovered.is_empty(), "路径穿越被拒绝");
}

#[test]
fn atomic_write_roundtrip_and_checksum() {
    let dir = tempfile::tempdir().unwrap();
    let tx = AtomicTransaction::new(dir.path());

    let result = tx
        .atomic_write("docs/hello.md", b"# Aurora", "op-1")
        .unwrap();
    assert_eq!(result.bytes_written, 8);
    // 写入后校验和一致
    tx.check_checksum("docs/hello.md", &result.checksum)
        .unwrap();
    // 内容一致
    let content = std::fs::read(dir.path().join("docs/hello.md")).unwrap();
    assert_eq!(content, b"# Aurora");
    // 无残留 tmp
    assert!(!dir
        .path()
        .join(".tmp")
        .read_dir()
        .map(|mut d| d.next().is_some())
        .unwrap_or(false));
}
