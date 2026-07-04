# Releasing flaccheck

Releases are **tag-driven**, matching [tablio](https://github.com/dasunNimantha/tablio).

## Cut a release

1. Bump `version` in the workspace root [`Cargo.toml`](Cargo.toml) (and run `cargo check` so `Cargo.lock` updates if needed).
2. Commit the version bump on `master`.
3. Create and push an annotated tag whose name matches the version:

   ```bash
   git tag -a v0.1.0 -m "flaccheck v0.1.0"
   git push origin v0.1.0
   ```

4. The [Release workflow](.github/workflows/release.yml) runs automatically:
   - format, clippy, and full test suite
   - cross-platform release binaries (Linux, macOS, Windows)
   - GitHub Release with downloadable archives

The tag **must** match `Cargo.toml` exactly (`v0.1.0` ↔ `version = "0.1.0"`). A mismatch fails the build.

## Pre-releases

Tags containing a hyphen after the version (e.g. `v0.2.0-beta.1`) are published as GitHub **pre-releases**.

## CI vs release

| Workflow | Trigger | Purpose |
| --- | --- | --- |
| [ci.yml](.github/workflows/ci.yml) | Pull requests, manual | Required checks before merge |
| [release.yml](.github/workflows/release.yml) | `v*` tag push | Tests + binaries + GitHub Release |
