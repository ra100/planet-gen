# Worldbuilding Pasta: Climate Research for Procedural Planet Generation

**Sources:**
- [Climate Explorations: Pressure](https://worldbuildingpasta.blogspot.com/2025/11/climate-explorations-pressure.html) (ExoPlaSim pressure simulations)
- [Part VIa: Climate: Global Forcings](https://worldbuildingpasta.blogspot.com/2020/03/an-apple-pie-from-scratch-part-via.html) (atmospheric circulation model)
- [Part VIb: Climate: Biomes and Climate Zones](https://worldbuildingpasta.blogspot.com/2020/05/an-apple-pie-from-scratch-part-vib.html) (winds, precipitation, biomes)
- [Part VIIa: Geology and Landforms](https://worldbuildingpasta.blogspot.com/2021/07/an-apple-pie-from-scratch-part-viia.html) (terrain effects on climate)

**Date:** 2026-04-04
**Relevance:** Wind model, cloud distribution, atmospheric circulation for preview shader and cloud advection pipeline.

---

## Pressure Systems

### Pressure Zones (Earth-like rotation)

The atmosphere forms alternating high/low pressure zones at the surface:

| Latitude | Pressure | Feature | Cloud/Precip |
|----------|----------|---------|-------------|
| 0° (equator) | Low | ITCZ (Intertropical Convergence Zone) | Heaviest precipitation; persistent deep convective clouds |
| ~30° | High | Horse latitudes / subtropical highs | Dry; descending air suppresses clouds; desert belt |
| ~60° | Low | Polar front / subpolar low | Wet; cyclonic storms; moderate precipitation |
| 90° (poles) | High | Polar high | Dry; cold; descending air |

**Key rule:** Converging winds (low pressure) = rising air = clouds and rain. Diverging winds (high pressure) = sinking air = clear and dry.

### Subtropical Highs (Detailed Behavior)

Rather than a uniform belt, the subtropical high at ~30° manifests as **distinct anticyclonic cells** centered over cold ocean areas (cold equatorward legs of ocean gyres, along western coasts of continents).

- **Seasonal shift:** Centered ~25° latitude in winter, ~35° latitude in summer.
- **Anticyclones spiral:** Clockwise in northern hemisphere, counterclockwise in southern hemisphere (same direction as underlying ocean gyres).
- **Continental highs:** In winter, high-pressure cells can form over cold continents, particularly near poles.

### ITCZ Seasonal Migration

- The ITCZ roughly follows the **thermal equator** (warmest latitude band), not the geographic equator.
- It swoops north and south through the hottest regions, shifting **much farther over land** than over ocean due to land's lower thermal inertia.
- On Earth, the ITCZ can reach ~20°N over Asia in summer and ~15°S over South America/Australia in southern summer.
- **Monsoon mechanism:** As the ITCZ shifts, tropical easterlies reverse direction in the regions it crosses. Onshore winds in summer = wet; offshore winds in winter = dry.

### Pressure vs. Atmospheric Density (ExoPlaSim Results)

From the Pressure Explorations article, varying total atmospheric pressure while keeping ~15°C average:

| Pressure | Stellar Flux | Avg Wind Speed | Wind Force (rel.) | Precip (rel.) | Bond Albedo | Notes |
|----------|-------------|----------------|-------------------|---------------|-------------|-------|
| 0.1 bar | 1.356x Earth | 26 km/h | 15% | 200% | ~0.22 | Fewer, wider cells; ITCZ swings far; rapid water cycling |
| 0.25 bar | 1.185x | ~22 km/h | 25% | 145% | ~0.25 | Very wet; 70°C desert summers; -120°C polar winters |
| 0.5 bar | 1.069x | 23 km/h | 57% | 117% | ~0.27 | Wider seasonal swings; more direct sunlight |
| **1 bar** | **1.0x** | **22 km/h** | **100%** | **100%** | **0.30** | **Earth baseline** |
| 2 bar | 0.943x | 19 km/h | 154% | 81% | ~0.33 | More moderate seasons; slower winds but stronger force |
| 4 bar | 0.912x | ~16 km/h | ~200% | 61% | 0.39 | Surface gets only 67% direct sunlight; glaciers appear |
| 10 bar | 0.911x | 13 km/h | >300% | 42% | 0.47 | Max temp 26°C; barely any daily variation; very dry |

**Key relationships for procedural generation:**
- Wind force scales with `air_density * wind_speed²` (drag equation). Density scales linearly with pressure.
- Higher pressure = less temperature variation (daily, seasonal, latitudinal).
- Higher pressure = less precipitation (evaporation suppressed; less direct sunlight reaches surface).
- Higher pressure = more atmospheric scattering/reflection (higher Bond albedo).
- At 0.1 bar, circulation resembles Mars: fewer, wider cells; ITCZ swings to near-poles at solstices.
- At 10 bar, Hadley cell narrows; extra circulation cells may form (similar to faster rotation).

---

## Wind Patterns

### Three-Cell Model (Earth-like Rotation)

For an eastward-rotating planet with Earth-like day length:

1. **Hadley Cell** (0°-30°): Warm air rises at equator, moves poleward at altitude, Coriolis deflects it east. By 30° it's moving nearly due east. Air descends, returns equatorward at surface as **easterly trade winds** (deflected west).

2. **Ferrel Cell** (30°-60°): Warm air from 30° moves poleward at low altitude; Coriolis deflects it east, creating **westerlies**. Air rises at ~60°, returns equatorward at altitude.

3. **Polar Cell** (60°-90°): Warmed air at 60° rises, flows to pole at altitude, descends. Surface flow back to 60° creates **polar easterlies**.

### Coriolis Effect Rules

- Poleward currents curve east; equatorward currents curve west (on eastward-rotating planet).
- Stronger with faster rotation; weaker with slower rotation.
- Effect increases with latitude (zero at equator, maximum at poles).
- Anticyclones (high pressure): air spirals **clockwise** in N hemisphere, **counterclockwise** in S hemisphere.
- Cyclones (low pressure): opposite rotation.

### Trade Winds (Detailed)

- Blow equatorward and curve west, becoming more purely westward near the equator.
- Very consistent in tropics; dominate between ~30° and the ITCZ.
- Direction follows the ITCZ, not the equator; when ITCZ shifts, trade wind direction shifts (monsoon trigger).

### Westerlies and Polar Front

- Mid-latitude westerlies (30°-60°) are less regular than trade winds.
- Where westerlies meet polar easterlies (~60°), the **polar front** forms.
- Polar front is highly variable/mobile; forms traveling wave patterns (Rossby waves) of cyclonic lobes moving eastward.
- Polar front reaches ~40° latitude in summer, ~30° in winter (seasonal shift).
- Brings moderate but widespread precipitation to high latitudes from all coasts, extending ~2,000 km inland even from offshore-wind coasts.

### Convection Cell Boundaries vs. Rotation Rate

From Kaspi and Showman 2015 (cited in Part VIa), cell boundaries per hemisphere:

| Day Length (Earth days) | Cell Boundaries (° lat) | Notes |
|------------------------|------------------------|-------|
| 16 | 3, 70 | Single huge Hadley cell; superrotation at equator |
| 8 | 0, 65 | Two cells; polar cell gone |
| 4 | 0, 55 | Two cells; Hadley extends to 55° |
| 2 | 0, 40, 70 | Three cells; Hadley to 40° |
| **1 (Earth)** | **0, 30, 60** | **Three cells: Hadley, Ferrel, Polar** |
| 1/2 | 0, 25, 40, 55, 70 | Five cells |
| 1/4 | 0, 18, 21, 26, 33, 41, 49, 56, 64 | Nine cells; boundaries blurry |

**Pressure at boundaries alternates:** Low (rising) at equator, High at first boundary, Low, High, ... ending Low at pole (always rising air at equator, sinking at pole).

### Effect of Temperature on Hadley Cell Width

- Hadley cell widens ~1° latitude per 4°C increase in average temperature (up to ~21°C global mean).
- Above ~21°C, trend reverses: melting ice caps reduce pole-equator temperature difference, Hadley cell shrinks back.
- At peak hothouse (~35° width), then down to ~20° width in super-greenhouse.

---

## Cloud Formation

### Fundamental Rules

Clouds and precipitation require two conditions:
1. **Horizontal transport:** Winds carry moisture from oceans over land.
2. **Vertical uplift:** Rising air expands, cools, moisture condenses.

### Sources of Uplift (Cloud-Forming Mechanisms)

1. **Convergence zones (ITCZ, polar front):** Where winds converge, air is forced upward. Strongest cloud formation at ITCZ; moderate at polar front.

2. **Orographic lift:** Moist air hitting mountains is forced upward.
   - Becomes significant when mountain **relief exceeds 1 km**.
   - Rain peaks at 1-1.5 km relief on windward side.
   - Creates **rain shadow** on leeward side when relief exceeds 2 km.
   - Equatorial mountains facing onshore winds may need >3 km to cause rain shadow; inland high-latitude ranges may only need 1 km.
   - Above 4 km, generally dry regardless.

3. **Warm currents / sea surface temperature:** Warm ocean surfaces generate high evaporation. Eastern coastlines receiving warm poleward currents are wetter.

4. **Lee cyclogenesis:** When prevailing winds pass directly over high mountains (>2,000 m relief), a low-pressure zone can form on the lee side, drawing in moisture from nearby oceans. Creates rain on the "wrong" side of mountains.

5. **Fronts:** Where air masses of different temperatures meet (common at subtropical high boundaries), the warm air rides up over the cold air, creating clouds and precipitation. More common where winds approach coastlines at oblique angles.

### Cloud Distribution by Latitude Zone

| Zone | Latitude | Cloud Character |
|------|----------|----------------|
| ITCZ | ~0° (follows thermal equator) | Deep convective towers; towering cumulonimbus; heaviest rain |
| Trade wind belt | 5°-30° | Generally clear/fair weather cumulus; clouds mainly on eastern coasts with onshore trades |
| Horse latitudes | ~25°-35° | Clear skies; descending air suppresses cloud formation; subtropical highs |
| Westerly belt | 35°-60° | Variable; frontal cloud systems; stratiform layers; cyclonic storms |
| Polar front | ~55°-65° | Traveling cyclones; layered cloud bands; moderate persistent precipitation |
| Polar regions | >70° | Low clouds/fog; little precipitation; very cold air holds little moisture |

### Moisture Transport Distances

- Onshore winds carry moisture **2,000-3,000 km** inland before air is depleted.
- Offshore winds: only ~1,000 km of moisture penetration.
- Mountain ranges truncate moisture transport sharply (rain shadow).
- In the polar front zone: moisture reaches ~2,000 km from all coasts, even with offshore winds (fronts are dynamic/mobile).

### Cloud Cover and Albedo

Typical surface albedos:
- Open ocean: 0.06
- Dense forest: 0.08-0.15
- Grassland/shrub: 0.1-0.25
- Desert: 0.3-0.4
- Ice/snow: 0.5-0.8
- Clouds: similar to ice (~0.5-0.8)

Cloud cover raises planetary albedo significantly. Wetter areas have more clouds and thus higher effective albedo. Plant vegetation releases aerosols (monoterpenes) that enhance cloud nucleation.

---

## Atmospheric Circulation

### The Complete Circulation Model

1. Sunlight heats equator most strongly; warm air rises to 10-15 km altitude.
2. Rising air creates low pressure at surface (ITCZ); converging surface winds bring moisture.
3. High-altitude air flows poleward; Coriolis deflects it east.
4. By ~30° latitude, air is moving nearly due east; unable to progress poleward, it descends.
5. Descending air creates high pressure (horse latitudes); dry conditions.
6. Surface air flows equatorward (trade winds) and poleward (westerlies).
7. At ~60° the warm westerly air meets cold polar air; warm air rises (polar front low).
8. Air flows poleward at altitude to poles, descends (polar high), returns to 60° as polar easterlies.

### Obliquity Effects on Circulation

- Earth's 23.5° tilt causes ITCZ to migrate seasonally, creating monsoons.
- At 0° obliquity: permanent polar icecaps to ~50° lat; ITCZ fixed at equator; no monsoons.
- At >50° obliquity: circulation may reverse (hot air rises at poles in summer, sinks at equator).
- At >54° obliquity: poles receive more average annual insolation than equator.
- Higher obliquity = larger seasonal temperature swings, smaller mean equator-pole difference.
- Obliquity increases average global temperature by ~9°C from 30° to 90° due to cloud cover shifts.

### Monsoon Rules

The monsoon is fundamentally the **seasonal reversal of the trade winds** as the ITCZ migrates:

1. Large landmass in the horse latitudes (15°-35°) directly north or south of equatorial ocean.
2. In summer: land heats faster than ocean, ITCZ shifts over land, trade winds reverse to blow onshore (wet monsoon).
3. In winter: land cools, ITCZ retreats toward equator, winds blow offshore (dry monsoon).
4. Effect strongest with high axial tilt (larger ITCZ migration) and large continents.
5. Monsoon coasts: very wet summers (>10 mm/day), dry winters.
6. Transition sequence moving poleward from equator: Rainforest -> Monsoon -> Savanna -> Desert.

### Superrotation

At very long day lengths (>16 Earth days), complex momentum transfer creates **superrotation**: atmospheric circulation faster than the planet's rotation at the equator. Creates westerly winds at the equator (opposite normal easterlies). Produces characteristic Y-shaped cloud patterns (as on Venus).

---

## Key Parameters for Procedural Generation

### Quantitative Reference Table

| Parameter | Value | Source/Context |
|-----------|-------|---------------|
| Hadley cell boundaries (Earth) | 0°, 30°, 60° | Standard 3-cell model |
| Hadley width per 4°C warming | +1° latitude | Up to ~21°C global mean |
| Desert belt latitude | 15°-30° (centered closer to equator) | Horse latitudes |
| Onshore moisture penetration | 1,000-1,500 km (trade wind belt); 2,000-3,000 km (other) | From nearest major ocean coast |
| Offshore moisture penetration | ~1,000 km | Much less than onshore |
| Orographic lift significant | >1 km mountain relief | Creates enhanced windward rain |
| Rain shadow desert threshold | >2 km mountain relief | Equatorial onshore: may need >3 km |
| Above 4 km | Generally dry | Insufficient moisture reaches this altitude |
| Altitude-latitude equivalence | 1 km altitude ~ 8° poleward | For ice/snow line calculations |
| Lapse rate | ~6°C per 1 km altitude | Standard environmental lapse rate |
| Subtropical high seasonal shift | 25° lat (winter) to 35° lat (summer) | Centers of anticyclonic cells |
| ITCZ follows thermal equator | Smoothed path through warmest surface regions | Lags ~1 month behind peak insolation |
| Polar front zone of influence | 30° lat (winter) to 40° lat (summer) | Poleward edge of subtropical highs |
| Ocean current temp effect (60° lat) | +15°C on western coasts, -10°C on eastern coasts | Poleward warm current vs equatorward cold current |
| Steppe boundary width | 100-300 km (tropics); 200-600 km (higher lat) | Transition between desert and other biomes |
| Rainforest belt | Within 10° of equator, near coasts | Extends further on eastern coasts with onshore winds |
| Wind speed at 1 bar (baseline) | ~22 km/h average | From ExoPlaSim |
| Earth Bond albedo | 0.30 | Including cloud effects |
| Ice cap instability threshold | ~40° latitude | Beyond this, rapid snowball transition risk |

### Pressure-Dependent Parameters

For procedural scaling with atmospheric pressure `P` (in bar):

- **Wind speed** roughly scales as: `v ~ 22 * (1/P)^0.15` km/h (approximate from ExoPlaSim data)
- **Wind force** scales as: `F ~ P * v²` (drag equation; density proportional to pressure)
- **Precipitation** roughly scales as: `precip ~ P^(-0.5)` relative to 1 bar baseline
- **Temperature variation** (seasonal/diurnal) decreases with higher pressure
- **Bond albedo** increases roughly as: `albedo ~ 0.30 + 0.06 * log2(P)` (approximate)
- **Direct surface sunlight** decreases with higher pressure due to scattering

### Rotation-Rate Scaling

For procedural generation of different rotation rates `Omega` (relative to Earth = 1.0):

- Number of circulation cells per hemisphere: approximately `round(3 * Omega^0.5)` for Omega >= 0.25
- Hadley cell top latitude: approximately `30° / Omega^0.3` (capped at ~70°)
- At Omega < 0.125 (day > 8 Earth days): only 1-2 cells; superrotation possible
- At Omega > 2 (day < 12h): 5+ cells; narrow pressure bands; weaker individual cells

### Implementation Notes for Planet-Gen

1. **Cloud density map:** Highest at ITCZ (follows thermal equator); clear at horse latitudes; moderate/variable at polar front. Modulate by distance from coast and mountain rain shadows.

2. **Wind field construction:** Build from pressure gradient between cell boundaries. Apply Coriolis deflection proportional to `sin(latitude) * rotation_rate`. Trade winds strongest/most consistent; westerlies more variable.

3. **Precipitation proxy for biome assignment:** Sum of convergence-zone wetness + warm-current wetness + orographic wetness + frontal wetness, minus rain shadow drying and continental interior drying.

4. **Seasonal variation:** Shift ITCZ toward summer hemisphere by `~obliquity * 0.5` degrees latitude (smoothed over continent/ocean). Shift subtropical highs and polar front proportionally.

5. **Pressure system placement:** Subtropical highs centered on cold ocean regions (western continental coasts). Low-pressure zones form over continents in summer and high-latitude oceans in winter.

6. **Cloud advection direction:** Clouds move with prevailing winds at their latitude. Trade wind clouds drift westward; mid-latitude clouds drift eastward. Speed proportional to wind speed at that latitude.

---

## Geology and Terrain Effects on Climate

### Orographic Effects (from Part VIIa)

- Mountain ranges perpendicular to prevailing winds create the strongest orographic effects.
- **Windward side:** Enhanced precipitation, peaking at 1-1.5 km relief. Can be wetter than nearby coastal lowlands.
- **Leeward side:** Rain shadow. Dry foehn/chinook winds descend, compress, and warm. Sharp transition from lush to arid.
- **"Stepped" slopes** (plateaus between steep sections) allow moisture to penetrate further into mountains.
- Continental-scale effects require >1 km relief; rain shadow deserts >2 km relief.

### Continental Position Effects

- Equatorial continents: wet on coasts; interior deserts if large enough (>3,000 km across).
- Polar continents: prone to glaciation; isolated polar continents (like Antarctica) especially so.
- Mid-latitude continents: western coasts warmed by poleward ocean currents; eastern coasts cooled by equatorward currents (at 50-70° latitude). Temperature difference can be up to 25°C between west and east coasts at 60° lat.
- Supercontinents: extreme continental interiors (>50°C summers, very cold winters); large interior deserts.

### Elevation and Temperature

- Standard lapse rate: ~6-6.5°C per 1,000 m altitude.
- Rule of thumb: 1 km altitude ~ 8° latitude poleward (for vegetation/ice line).
- Plateaus above 4 km: interiors should be below 10°C average temperature.
- High plateaus (Tibet-like): create their own weather systems; can block/redirect atmospheric circulation at continental scale.

### Volcanic/Tectonic Climate Coupling

- Active volcanism increases CO2 outgassing, raising global temperatures.
- Young mountain ranges increase weathering, drawing down CO2 and cooling climate.
- Typical pattern: volcanic episode warms planet by a few °C, followed by cooling as fresh rock weathers.
- Faster plate tectonics = more stable climate; episodic tectonics = wider climate swings.
- General trend: volcanic activity declines with planet age as interior cools.
