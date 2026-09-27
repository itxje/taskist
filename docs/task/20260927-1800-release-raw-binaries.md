# 20260927-1800-release-raw-binaries Publish release binaries without archives

- **status**: completed
- **priority**: P1
- **owner**: session/release-binaries
- **createdAt**: 2026-09-27 18:00

## Description

The release workflow publishes `tk-<version>-<target>.tar.gz` archives. Publish the static
binaries themselves instead, named `tk-<target>` so that
`releases/latest/download/tk-<target>` always names the newest build, with `SHA256SUMS` over
the binaries. Replace the assets of the existing `v0.1.0` release the same way.

Acceptance:

- A `v*` tag publishes `tk-x86_64-unknown-linux-musl`, `tk-aarch64-unknown-linux-musl` and
  `SHA256SUMS`, and no archive.
- The `v0.1.0` release carries the same three assets; the downloaded binaries match
  `SHA256SUMS`, are statically linked, and the aarch64 one runs.
- The README install section downloads a binary directly.

## ActiveForm

Publishing release binaries without archives

## Dependencies

- **blocked by**: (none)
- **blocks**: (none)

## Notes

Plan: `docs/plan/20260927-1800-release-raw-binaries.md`. Approved in the request: publish the
binaries directly instead of archives.

- complete: Release workflow publishes tk-<target> binaries with SHA256SUMS (actionlint clean); v0.1.0 assets replaced with the same binaries, verified through latest/download: checksums match, static, aarch64 runs.
