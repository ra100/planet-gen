# Performance Analysis: Terrain Generation Pipeline

Date: 2026-04-01
GPU: System GPU (release build)
Erosion iterations: 25

## Benchmark Results

| Resolution | Plates (ms) | Compute (ms) | Erosion (ms) | Upload (ms) | Total (ms) | Erosion/iter (ms) |
| ---------- | ----------- | ------------ | ------------ | ----------- | ---------- | ----------------- |
| 256×256    | 0.0         | 2.5          | 112.3        | 0.4         | 115.3      | 4.5               |
| 512×512    | 0.0         | 6.3          | 336.9        | 1.7         | 344.8      | 13.5              |
| 768×768    | 0.0         | 9.3          | 735.8        | 3.9         | 749.0      | 29.4              |
| 1024×1024  | 0.0         | 11.9         | 1,388.2      | 6.9         | 1,407.0    | 55.5              |
| 2048×2048  | 0.0         | 64.0         | 8,504.3      | 30.2        | 8,598.5    | 340.2             |

## Key Findings

### Erosion dominates (97%+ of total time)

- At every resolution, erosion is the overwhelming bottleneck
- Plates and compute are negligible (< 1% of total)
- Upload (CPU→GPU texture copy) is minimal

### Scaling is super-linear

- 256→512 (4× pixels): 3× time → reasonable
- 512→1024 (4× pixels): 4.1× time → nearly linear per-pixel
- 1024→2048 (4× pixels): 6.1× time → super-linear, possible cache/memory effects
- Erosion per-iteration at 2K: 340ms (6 faces × 2048² = 25M pixels per iteration)

### Optimization Targets (priority order)

1. **Reduce erosion iterations at preview resolution**
   - 25 iterations is the full-quality setting
   - For preview, 10-15 iterations would cut time by 40-60%
   - Could scale iterations with resolution: low res = fewer iterations
   - Impact: 2K goes from 8.5s → ~3.5s

2. **Move erosion to background thread**
   - Already using Arc<GpuContext> pattern from export pipeline
   - Show un-eroded preview immediately, apply erosion progressively
   - User sees terrain instantly, erosion refines over time

3. **GPU compute erosion instead of CPU dispatch**
   - Current erosion dispatches individual compute shader passes
   - Each pass has CPU→GPU sync overhead
   - Batching multiple erosion steps in a single dispatch would reduce sync cost
   - Impact: significant at high resolutions

4. **Resolution-adaptive erosion count**
   - 256: 5 iterations (enough for preview)
   - 512: 10 iterations
   - 768: 15 iterations (default)
   - 1024+: 25 iterations (full quality)
   - Auto-scale with slider, user can override

5. **Progressive rendering**
   - Render un-eroded terrain immediately
   - Run erosion iterations in batches (5 at a time)
   - Re-render after each batch
   - User sees quality improve progressively
