# Partial bit-buffer refill benchmarks

Sources: [Xeon measurements](benches/results/xeon-w11855m.csv), [Mac mini M2 measurements](benches/results/m2-mac-mini.csv).

## Results

Values below are Criterion's relative mean **time changes**. Bold entries are regressions. 

### Current vs byte loop

| Case | Xeon LTR | Xeon RTL | M2 LTR | M2 RTL |
|---|---:|---:|---:|---:|
| bulk/aligned | -62.31% | -60.09% | -57.84% | -65.88% |
| bulk/unaligned | -60.34% | -62.13% | -53.90% | -57.12% |
| eight-byte boundary | -61.65% | -62.20% | -58.36% | -59.90% |
| short tails | **+11.23%** | **+9.43%** | -28.01% | -30.70% |
| empty input | -26.65% | -16.72% | -0.60% (n.s.) | -15.68% |
| no whole-byte capacity | -22.62% | **+24.22%** | -6.38% | -45.02% |

### Current vs padded copy

| Case | Xeon LTR | Xeon RTL | M2 LTR | M2 RTL |
|---|---:|---:|---:|---:|
| bulk/aligned | -66.92% | -66.76% | -75.64% | -76.04% |
| bulk/unaligned | -74.84% | -73.08% | -74.57% | -76.30% |
| eight-byte boundary | -71.18% | -69.80% | -74.84% | -75.97% |
| short tails | -44.07% | -40.07% | -60.60% | -61.99% |
| empty input | -23.90% | **+4.65%** | -48.54% | -25.93% |
| no whole-byte capacity | -21.20% | -14.33% | -59.07% | -57.74% |

## Measurement provenance

Rust:  1.99.0 (`b940084d7`, 2026-09-28)
Build: Optimized bench profile, `-C target-cpu=native`
Samples per benchmark:  100
Warmup / measurement: 1 second / 3 seconds

## Reproduce

From the repository root:

```sh
bash symphonia-core/benches/compare_partial_refill.sh --warm-up-time 1 --measurement-time 3 --sample-size 100
```

The [runner](benches/compare_partial_refill.sh) sets `RUSTFLAGS='-C target-cpu=native'` for all implementations.

- `baseline.log` contains the previous implementation's measurements.
- `comparison.log` contains the current-versus-previous results.
- `progress.log` contains build, warmup, and measurement progress.
- `report/index.html` opens the HTML report.
