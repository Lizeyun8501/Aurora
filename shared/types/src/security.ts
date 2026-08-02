/**
 * Security domain types.
 * Mirrors the concepts in `crates/aurora-security/`
 * (e2ee, key hierarchy, biometric, recovery, post-quantum).
 */

/** Encryption status for the vault / workspace. */
export type EncryptionStatus = 'enabled' | 'disabled' | 'partial' | 'migrating';

/** Biometric sensor availability / enrollment status. */
export type BiometricStatus =
  | 'available'
  | 'unavailable'
  | 'enrolled'
  | 'not_enrolled'
  | 'locked';

/** Recovery status for the key-recovery flow. */
export type RecoveryStatus =
  | 'idle'
  | 'in_progress'
  | 'verified'
  | 'failed'
  | 'expired';

/** Key kind in the hierarchical key chain. */
export type KeyKind =
  | 'master'
  | 'workspace'
  | 'document'
  | 'asset'
  | 'session';

/** A node in the key hierarchy. */
export interface KeyNode {
  id: string;
  kind: KeyKind;
  /** Wrapped/encrypted key material (never plaintext at rest). */
  wrapped_key: string;
  /** Algorithm used to wrap this key. */
  algorithm: string;
  parent_id: string | null;
  created_at: string;
  rotated_at: string | null;
}

/** Key hierarchy (mirrors `aurora-security/key_hierarchy.rs`). */
export interface KeyHierarchy {
  master_key_id: string;
  keys: KeyNode[];
  /** Post-quantum algorithm in use (e.g. "kyber768"), if hybrid PQ is enabled. */
  post_quantum_algorithm: string | null;
}

/** Biometric configuration. */
export interface BiometricConfig {
  enabled: boolean;
  /** Allowed biometric modalities. */
  modalities: BiometricModality[];
  /** Require biometric to unlock the vault. */
  require_to_unlock: boolean;
  fallback_to_passcode: boolean;
}

/** Biometric modality. */
export type BiometricModality =
  | 'fingerprint'
  | 'face'
  | 'iris'
  | 'voice';

/** Encryption configuration for the vault. */
export interface EncryptionConfig {
  status: EncryptionStatus;
  /** Symmetric cipher for data at rest. */
  cipher: 'aes-256-gcm' | 'chacha20-poly1305';
  /** KDF used to derive keys from passphrases. */
  kdf: 'argon2id' | 'pbkdf2' | 'scrypt';
  key_hierarchy: KeyHierarchy;
}

/** Recovery information for key recovery. */
export interface RecoveryInfo {
  status: RecoveryStatus;
  /** Recovery code is stored hashed; this is only a presence flag. */
  recovery_code_set: boolean;
  recovery_contacts: string[];
  last_verified_at: string | null;
}
