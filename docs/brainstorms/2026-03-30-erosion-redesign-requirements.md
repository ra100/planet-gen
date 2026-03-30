---
date: 2026-03-30
topic: erosion-redesign
---

# Erosion Redesign: Channel Carving + Detail Preservation

## Problem Frame

The current GPU erosion algorithm acts as a diffusion filter — it uniformly smooths terrain instead of creating geologically realistic features. Flow accumulation only propagates ~8 cells (insufficient for channel formation on a 512x512 grid), thermal erosion aggressively removes detail at all steep slopes, and deposition fills valleys. The net effect is loss of the interesting terrain structure created by the plate shader, with no visible rivers, canyons, or lake features.

## Requirements

**Channel Carving**
- R1. Trace steepest-descent paths from high points to create visible river valley channels at planet scale
- R2. Channel depth should increase with convergence — more upstream area = deeper/wider valley
- R3. Channels terminate at ocean level or in flat basins, creating natural lake bed depressions
- R4. Channel paths should follow terrain structure (plate boundaries become natural drainage divides)

**Detail Preservation**
- R5. Erosion must not remove high-frequency terrain detail from plate boundaries and mountain ridges
- R6. Mountain peaks should be softened slightly (rounded, not flattened)
- R7. Add small-scale roughening noise to eroded lowland areas (weathered texture)
- R8. Remove thermal erosion (talus angle collapse) which is the primary detail destroyer

**Integration**
- R9. Works on the existing per-face 512x512 heightmap and scales to export resolution (up to 8192)
- R10. Erosion iterations slider (0-50) continues to control intensity; default 25 should show clear channels without over-erosion
- R11. GPU compute shader, same ping-pong buffer architecture as current implementation

## Success Criteria

- Visible V-shaped valleys radiating from mountain ranges at default settings
- Mountain ridges and plate boundary terrain preserved or enhanced, not smoothed away
- No uniform "blur" effect at any erosion level
- Flat lowland areas show subtle weathered texture rather than being perfectly smooth

## Scope Boundaries

- Not implementing full particle-based hydraulic simulation (too expensive for real-time)
- Not simulating sediment transport or alluvial fans
- Not creating visible river water/color — just the terrain channels
- Lake detection is geometric (flat basins) not hydrological

## Key Decisions

- **Channel stamp over fluid sim**: Tracing steepest-descent paths and carving along them is orders of magnitude cheaper than proper drainage accumulation, and creates more visible results at planet scale
- **Remove thermal erosion**: It destroys detail without adding realism at this scale
- **Two-pass architecture**: Pass 1 carves channels, Pass 2 does gentle weathering. Keeps concerns separate and tunable

## Outstanding Questions

### Deferred to Planning
- [Affects R1][Technical] How many descent-trace steps per pixel are needed for visible channels at 512x512? Likely 32-64.
- [Affects R2][Technical] Channel width/depth function: linear with convergence count, or sqrt? Sqrt is more physically realistic.
- [Affects R9][Technical] At 8192 export resolution, channel tracing needs proportionally more steps. Consider resolution-adaptive step count.

## Next Steps

→ `/ce:plan` for structured implementation planning
