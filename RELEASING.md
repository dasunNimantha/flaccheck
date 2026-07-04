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
   - GitHub Release with per-platform binaries (no archive extract step)

The tag **must** match `Cargo.toml` exactly (`v0.1.0` ↔ `version = "0.1.0"`). A mismatch fails the build.

## Pre-releases

Tags containing a hyphen after the version (e.g. `v0.2.0-beta.1`) are published as GitHub **pre-releases**.

## CI vs release

| Workflow | Trigger | Purpose |
| --- | --- | --- |
| [ci.yml](.github/workflows/ci.yml) | Pull requests, manual | Required checks before merge |
| [release.yml](.github/workflows/release.yml) | `v*` tag push | Tests + binaries + GitHub Release |

## Dependency updates (Renovate)

Same setup as [tablio](https://github.com/dasunNimantha/tablio):

- [`renovate.json`](renovate.json) — `platformAutomerge` + automerge for minor/patch after 7-day release age
- [**Renovate Approve**](https://github.com/apps/renovate-approve) GitHub App — auto-approves Renovate PRs (required for protected-branch automerge)
- **Branch rules** — `CI Required` must pass + 1 approving review before merge

Flow: Renovate opens PR → `renovate-approve` approves → CI passes → Renovate squash-merges automatically.
