# third_party — vendored & patched crates

Local source replacements used via `[patch.crates-io]` in the workspace root
`Cargo.toml`. Each subdir documents *why* it deviates from the published crate
and *how* the divergence is kept minimal.

## netwatch/

- **Upstream:** [`netwatch 0.19.1`](https://crates.io/crates/netwatch) (MIT OR Apache-2.0)
- **Why vendored:** On Windows `netwatch 0.19.1` hard-pins both
  `windows = "0.62.2"` *and* `wmi = "0.18"`. `wmi 0.18.4` (the latest published
  `wmi`) relies on `ComObject::into_interface` taking the `Free` trait from
  `windows-core 0.61.x`; under `windows-core 0.62.x` that bound is no longer
  satisfiable, and `wmi` fails to compile with 16 errors (`E0277` missing
  `Free`/`Interface`/`RuntimeName` bounds, `E0308` type mismatches). Upstream
  has not published a fix (as of 2026-08-06: `wmi` latest = 0.18.4, `netwatch`
  latest = 0.19.1, `iroh` latest = 1.0.3). The pinned combination is
  intrinsically broken on Windows until either `wmi` is updated for
  `windows-core 0.62.x` or `netwatch` relaxes its `windows = "0.62.2"` pin.
- **Local change:** Only the Windows-targeted `windows` and `windows-result`
  version requirements are loosened so the resolver unifies on
  `windows 0.61.3` / `windows-core 0.61.2` / `windows-result 0.3.4`, against
  which `wmi 0.18.4` (pulled via `netwatch → portmapper → ...`) compiles
  cleanly:
  - `windows = "0.62.2"` → `">=0.59, <0.62"`  (forces `windows 0.61.3`)
  - `windows-result = "0.4"` → `"0.3"`        (matches `windows 0.61.3`'s
    `windows-core 0.61.2` chain, so `n0-error::stack_error`'s derived
    `From<windows_result::Error>` impl resolves to the same crate instance)
  Plus a **lockfile-side** `cargo update -p windows-core@0.62.2 --precise 0.61.2`
  in `Cargo.lock` so the previously chosen `windows-core 0.62.2` (resolved by
  `wmi 0.18.4`'s own `windows-core = ">=0.59, <0.63"` falling on the newest
  available) is dropped from the lock entirely and the entire tree unifies on
  `0.61.2`.
  **No `src/` file is touched.** The exact Win32 feature list used by
  `src/netmon/windows.rs` and `src/interfaces/windows.rs` is preserved.
- **Tracking:** PR `feat/ai-ollama-provider` on `Lizeyun8501/Aurora`. Drop this
  directory, the matching `[patch.crates-io]` block, and re-run `cargo update`
  when upstream publishes a `netwatch` (or `wmi`) version that compiles against
  `windows-core 0.62`.
- **Syncing:** To pull a future `netwatch 0.19.x` patch:
  1. Replace this directory with the unpacked new version.
  2. Re-apply the windows/windows-result version relaxation below if still
     needed (or remove it if upstream fixed the pin).
  3. Re-run `cargo update -p windows-core@0.62.2 --precise 0.61.2` to drop any
     stale `windows-core 0.62.<x>` from the lock.
  4. `cargo build --workspace --all-targets`.
