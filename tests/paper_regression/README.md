# Paper regression targets

When real fixtures are available, compare metrics against published results:

| Paper | Target | Notes |
|-------|--------|-------|
| D'Alessandro & Shi 2009 | ~97% bitrate class, ~99% up-transcode detect | Spectral tier |
| Derrien JAES 2019 | 0% FPR AAC at high precision | Quant tier — calibrate thresholds |
| Lacroix AES 2015 | 100% upscale/transcode, 91.3% upsample | Hi-res tier |

```bash
cargo run -p flaccheck -- --benchmark datasets/output/manifest.json --mode max -o tests/paper_regression/results.json .
```
