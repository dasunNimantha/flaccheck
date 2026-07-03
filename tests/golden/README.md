# Golden / integration tests

Place labeled FLAC fixtures under `datasets/output/` (see `datasets/generate.sh`).

`manifest.example.json` shows the format expected by `--benchmark`.

Run:

```bash
cargo test -p lossless-scan-detectors
cargo test -p lossless-scan-core
```

Synthetic unit tests run without external audio files.
