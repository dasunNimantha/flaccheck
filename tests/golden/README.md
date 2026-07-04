# Golden / integration tests

Place labeled FLAC fixtures under `datasets/output/` (see `datasets/generate.sh`).

`manifest.example.json` shows the format expected by `--benchmark`.

Run:

```bash
cargo test -p flaccheck-detectors
cargo test -p flaccheck-core
```

Synthetic unit tests run without external audio files.
