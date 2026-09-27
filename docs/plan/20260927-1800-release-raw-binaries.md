# 20260927-1800-release-raw-binaries Publish release binaries without archives

- **status**: completed
- **createdAt**: 2026-09-27 18:00
- **approvedAt**: 2026-09-27 18:00
- **relatedTask**: 20260927-1800-release-raw-binaries

## Context

- `.github/workflows/release.yml` packages each static binary with `LICENSE` and `README.md`
  into `tk-<version>-<target>.tar.gz` and publishes the archives with `SHA256SUMS`.
- Release `v0.1.0` carries `tk-0.1.0-x86_64-unknown-linux-musl.tar.gz`,
  `tk-0.1.0-aarch64-unknown-linux-musl.tar.gz` and `SHA256SUMS`.
- The README install section downloads and extracts an archive.

## Proposal

- The build job uploads the binary itself as `tk-<target>`; the publish job writes
  `SHA256SUMS` over the binaries and publishes them.
- Versionless asset names keep `releases/latest/download/tk-<target>` stable; the release tag
  carries the version.
- The README downloads the binary, verifies it and installs it with mode 0755.
- `v0.1.0`: extract the binaries from its archives (the same bytes the workflow built), upload
  them with a new `SHA256SUMS`, and delete the archives, without rebuilding.

## Risks

- A downloaded binary has no executable bit; the README installs it with `install -m 0755`.
- Links to the `v0.1.0` archive URLs stop working.

## Scope

`.github/workflows/release.yml`, `README.md`, `docs/architecture.md`, `docs/changelog.md`,
task and plan records; the assets of release `v0.1.0`.

## Alternatives

- Versioned names (`tk-<version>-<target>`): the version is visible in the file name, but
  every install command has to name the version.

## Annotations

- 2026-09-27: requested and approved in one message: publish the binaries directly, no archives.
