---
title: "feat: Blender cloud bridge"
type: feat
status: completed
date: 2026-10-12
origin: "User request: combine the procedural cloud detail of planet3-Planet-Shader-v10.blend with Planet Gen's land/heat/sea-driven cloud placement, without a 16K export"
---

# Blender cloud bridge — implementation plan

Planet Gen's weather solver owns cloud placement; its detail only reaches close-range
sharpness through a full-size `clouds.exr`. The v10 Blender shader has
resolution-independent procedural detail but places clouds with a noise hole mask.
The bridge splits the work: Planet Gen exports low-frequency driver maps, Blender
keeps the procedural detail and takes placement from the drivers.

## U1: Driver map export

- `cloud_drivers.wgsl`: equirect compute pass over the export weather textures,
  writing cloud mass (low, deep, high, coverage), climate (land, thermal, cloud top,
  continentality) and tangent wind per pixel centre.
- `cloud_export.wgsl` split so the column pass (`cloud_export_columns.wgsl`) and the
  driver pass share bindings and pipeline construction.
- `export_cloud_drivers` writes `cloud_drivers.exr`, `climate_drivers.exr`,
  `wind_drivers.exr` (RGBA) whenever the clouds layer is exported, at
  `cloud_driver_dimensions(weather_resolution)`.

## U2: Blender bridge

- `blender_addon/cloud_bridge.py`: equirect UV group, driver sampler group, the v10
  cloud recipe rebuilt as `PlanetGen Driven Clouds` with Drive Strength (0 = v10
  exactly), coverage-driven mask and fill, wind streaks, top-height bump.
- Add-on operators **Drive Clouds** and **Aligned Surface**; headless CLI.

## Verification

- `streamed_layers_release_previous_face_buffers` checks the driver files exist,
  have the expected size and are finite.
- Cycles render at Drive Strength 0 matches the original v10 material.
