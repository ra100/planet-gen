# Shallow Volumetric Cloud Validation

## Frozen Append-Only Protocol

This file is the append-only evidence ledger for the active shallow-volumetric-cloud plan. Add a new row for every required scenario; never replace or delete a recorded row. The schema is frozen:

| Scenario ID | Owning U-ID | Fixture | Seeds | Mask / domain | Metric | Threshold | Measured value | Artifact path | Result | Reviewer / date |
|---|---|---|---|---|---|---|---|---|---|---|

Recorded evidence must state `PASS` or `FAIL` in `Result`. Before measurement, leave `Measured value`, `Artifact path`, `Result`, and `Reviewer / date` unrecorded. Do not record pending work as a measured `FAIL`; missing required evidence blocks completion of its owning unit.

Frozen common inputs: seeds `7, 19, 37, 73, 101, 211, 509, 997`; named masks `flat_cool_ocean`, `flat_inland`, `mountain_windward`, `mountain_lee`, `coast_band`, `eligible_convective_core`, and `dry_stable_control`; significant component area >=0.25% of one cubemap face; occupied deep core is `deep_mass >=0.15`, significant-component area >=0.25% face, and total deep mass >=0.02. When a percentile is inapplicable, record `not applicable` as its measured value with the reason in the artifact, not a fabricated value.

## Required Evidence

| Scenario ID | Owning U-ID | Fixture | Seeds | Mask / domain | Metric | Threshold | Measured value | Artifact path | Result | Reviewer / date |
|---|---|---|---|---|---|---|---|---|---|---|
| U14-R1-cool-ocean | U14 | matched cool flat ocean/inland | all frozen | `flat_cool_ocean`, `flat_inland` | low-mass ratio | ocean/inland >=1.5 |  |  |  |  |
| U14-R1-cool-geometry | U14 | cool marine | all frozen | `flat_cool_ocean` | low/deep ratio and thickness | ratio >=4; 0.3-1.2 km |  |  |  |  |
| U14-R2-warm-trades | U14 | warm marine | all frozen | `flat_cool_ocean` | low-cloud top and clear-gap fraction | top 1-3 km; gaps >=0.15 |  |  |  |  |
| U14-R2-coast | U14 | coast continuity | all frozen | `coast_band` | coast-gradient correlation and local gradient | correlation <0.3; local <=1.25x surrounding band |  |  |  |  |
| U15-R3-wind | U15 | calm and scaled wind | all frozen | named fixture masks | centroid displacement from calm; substep displacement | >=0.5/1.0/2.0 texels at 0.5/1/2; each substep <0.75 texel |  |  |  |  |
| U15-R4-count | U15 | uniformly eligible convection | all frozen | `eligible_convective_core` | Count 0->8 significant localized deep-core delta; occupied-core deep-mass p95; global mean integrated condensate/column-mass delta | cores >=2; p95 >=25%; condensate/column mass <=20% |  |  |  |  |
| U15-R4-size | U15 | uniformly eligible convection | all frozen | `eligible_convective_core` | Size 0.3->3 median significant-core area; deep-top p95 | area >=50%; top >=2 km |  |  |  |  |
| U15-R4-anvil | U15 | uniformly eligible convection | all frozen | significant deep cores and anvil region | high-mass extension; centroid displacement; major-axis alignment | extension >=10%; shift >=0.5 weather texel along diagnostic anvil-advection direction; alignment <=20 degrees |  |  |  |  |
| U15-R4-dry-stable | U15 | dry/stable candidates | all frozen | `dry_stable_control` | deep/high mass; catalyst-boundary visibility/correlation | mass exactly zero; no circular boundary visible or correlated |  |  |  |  |
| U3-R1-R4-render | U3 | marine, wind, storm, and tower/anvil references | all frozen | rendered cloud density domains | rendered R1-R4 outcomes | explicit PASS/FAIL against the plan's frozen contour, tower/anvil, and density-image gates |  |  |  |  |
| U3-R4-rendered-optical-depth | U3 | Count 0->8 uniformly eligible convection, after U15-R4-count field evidence | all frozen | rendered global cloud domain | global mean optical-depth delta | <=20% |  |  |  |  |
| U6-R4-rendered-optical-depth | U6 | Count 0->8 uniformly eligible convection, after U15-R4-count field evidence | all frozen | rendered global cloud domain | global mean optical-depth delta | <=20% |  |  |  |  |

## Later Evidence

Append U4 lighting, U5 export, U6 rendered contour/parity/seam/image, U11 stress/coalescing, and U7 cleanup-regression rows using the same schema. Missing evidence blocks the owning unit; only recorded measurements receive `PASS` or `FAIL`.
