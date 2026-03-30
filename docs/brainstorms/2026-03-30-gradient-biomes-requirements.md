---
date: 2026-03-30
topic: gradient-biomes
---

# Continuous Gradient Biome Coloring

## Problem Frame

The current biome system uses a discrete Whittaker lookup (13 biome IDs) with fixed colors per biome. Even with multi-sample noise blending, boundaries between biomes remain visible as color-coded regions rather than natural terrain. A VFX artist needs photorealistic output where visual contrast comes from height and roughness, not abrupt color jumps.

## Requirements

**Land Coloring**
- R1. Replace discrete biome ID → color lookup with a continuous 2D gradient: temperature × moisture → color via smooth interpolation between anchor points
- R2. No visible biome boundaries — color transitions should be as gradual as the underlying temperature/moisture fields
- R3. Anchor colors: cold/dry → gray-white, cold/wet → blue-white, temperate/dry → warm tan, temperate/wet → rich green, hot/dry → orange-tan, hot/wet → deep saturated green
- R4. Altitude zonation (snow caps, rock, alpine) continues to work on top of the gradient base
- R5. Per-pixel noise variation preserved for natural texture

**Ocean Coloring**
- R6. Smooth depth-based ocean color gradient (shallow turquoise → deep navy) without hard transitions
- R7. Smooth ice-to-water transition at polar regions — no hard -2°C cutoff line

**Integration**
- R8. Debug biome view (view_mode 4) should show the gradient directly (no lighting)
- R9. Export albedo shader (albedo_map.wgsl) updated to match the new gradient coloring

## Success Criteria

- Planet surface looks like a satellite photo — color varies smoothly with climate
- Visual distinction between terrain types comes from height/shadow and roughness, not color jumps
- No rectangular or banded artifacts at any resolution

## Scope Boundaries

- Not changing the temperature or moisture computation — only how they map to color
- Not adding new rendering techniques (atmosphere, specular) — just the color mapping
- Roughness map generation unchanged

## Key Decisions

- **Continuous interpolation over lookup table**: Bilinear interpolation between anchor colors at defined (temp, moisture) coordinates. Simpler, faster, and eliminates all boundary artifacts by construction.
- **Remove whittaker_lookup and biome_color**: These functions are replaced entirely, not wrapped. The multi-sample blending hack also becomes unnecessary.

## Next Steps

→ Proceed directly to work — scope is clear and self-contained
