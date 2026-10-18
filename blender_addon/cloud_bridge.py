"""Planet Gen cloud bridge: solver clouds, procedural billow detail in Blender.

Planet Gen's weather solver decides where clouds are. Its ``cloud_density.exr``
(optical depth, equirectangular) is the cloud; a full-size export is too soft at
close range, so Blender adds detail at render time by *edge erosion*: inverted
Worley billows (Schneider / Horizon Zero Dawn style) eat into thin areas and
edges, while thick cores stay smooth. No pattern is laid over the cloud, so the
detail always follows the solver's shapes.

Inputs, next to each other in the export folder:

    cloud_density.exr    Y optical depth (the cloud)
    wind_drivers.exr     RGB tangent wind in planet space (Y up), A wind speed

The detail domain is compressed along the pole axis (zonal bands) and warped
along the wind, so billows stretch with the flow. Optional extra maps
(``cloud_drivers.exr``, ``climate_drivers.exr``, opt-in in the exporter) are
exposed by the driver sampler for custom node work when present.

Usage inside Blender: the Planet Gen add-on panel calls :func:`apply`.
Headless:

    blender scene.blend --background --python cloud_bridge.py -- \
        --drivers /path/to/export/planet --object Clouds --save out.blend
"""

import math
import os

import bpy

DENSITY_FILE = "cloud_density.exr"
DRIVER_FILES = {
    "wind": "wind_drivers.exr",
    "cloud": "cloud_drivers.exr",
    "climate": "climate_drivers.exr",
}
REQUIRED_DRIVERS = ("wind",)
IMAGE_PREFIX = "PlanetGen "
EQUIRECT_GROUP = "PlanetGen Equirect UV"
SAMPLER_GROUP = "PlanetGen Driver Sampler"
CLOUD_GROUP = "PlanetGen Billow Clouds"
CLOUD_MATERIAL = "PlanetGen Billow Clouds"

CLOUD_DEFAULTS = {
    # Contrast on the solver's optical depth: below Low is clear, above High is a
    # full-thickness column that billows no longer erode.
    "Density Low": 0.05,
    "Density High": 0.35,
    # Base billow cell frequency in the 0-1 shell domain (250 ~ 50 km cells on an
    # Earth-size planet).
    "Scale": 250.0,
    # How far billows eat into the cloud; varies 0.5-1.5x by region.
    "Erosion": 0.35,
    # Low-frequency noise scale that varies the erosion between regions.
    "Variety Scale": 6.0,
    # Fine Perlin FBM added to the billows (wisps).
    "Wisps": 0.15,
    # Pole-axis compression of the detail domain; >1 elongates billows east-west.
    "Zonal Stretch": 2.5,
    # Displacement of the detail domain along the solver wind.
    "Wind Streak": 0.04,
    # Physical optical depth per unit of solver depth above Low: a 0.35 column
    # becomes tau ~11, a typical stratocumulus deck (same as the app preview).
    "Depth Gain": 36.0,
    # Thin-column contrast: depth scales by thickness^Contrast, so the haze between
    # the solver's ripples stays faint while their cores turn bright.
    "Contrast": 0.6,
    # Two-stream reflectance: opacity = a tau / (1 + a tau). Brightness follows
    # thickness instead of clipping to flat white, so the solver's internal
    # structure stays visible. a = (1 - g) / 2 = 0.075 for droplets (g = 0.85), as
    # in the app preview; a flat shell lacks the preview's multiple scattering, so
    # the default is brighter to match it.
    "Diffusion": 0.3,
    # Bump from the cloud's own thickness: sunlit and shaded sides on every puff.
    "Relief": 2.0,
    "Longitude Offset": 0.0,
}
# Inverted Worley F1 octaves: (frequency multiplier, weight).
BILLOW_OCTAVES = ((1.0, 0.5), (2.0, 0.24), (4.0, 0.13), (8.0, 0.07), (16.0, 0.04), (32.0, 0.02))
# Fine FBM frequency multiplier relative to Scale.
WISP_FREQUENCY = 12.0
# p5-p95 of the summed billows, mapped to 0-1.
BILLOW_RANGE = (0.31, 0.65)


# ============ Images ============

def _load_image(path, name):
    image = bpy.data.images.get(name)
    if image is None:
        image = bpy.data.images.load(path)
        image.name = name
    else:
        if image.packed_file is not None:
            image.unpack(method="REMOVE")
        image.filepath = path
        image.reload()
    image.colorspace_settings.name = "Non-Color"
    return image


def load_images(directory):
    """Load (or re-point) the density and driver maps. Fixed datablock names let a
    re-import swap a new export into every material that already uses them."""
    density_path = os.path.join(directory, DENSITY_FILE)
    missing = [DENSITY_FILE] if not os.path.exists(density_path) else []
    drivers = {}
    for key, filename in DRIVER_FILES.items():
        path = os.path.join(directory, filename)
        if os.path.exists(path):
            drivers[key] = _load_image(path, IMAGE_PREFIX + filename)
        elif key in REQUIRED_DRIVERS:
            missing.append(filename)
    if missing:
        raise FileNotFoundError(
            "Missing cloud maps in {}: {}".format(directory, ", ".join(missing))
        )
    density = _load_image(density_path, IMAGE_PREFIX + DENSITY_FILE)
    return density, drivers


# ============ Node helpers ============

def _new_group(name):
    group = bpy.data.node_groups.get(name)
    if group is not None:
        group.nodes.clear()
        group.interface.clear()
    else:
        group = bpy.data.node_groups.new(name, "ShaderNodeTree")
    return group


def _socket(group, name, in_out, socket_type, default=None, min_value=None, max_value=None):
    socket = group.interface.new_socket(name=name, in_out=in_out, socket_type=socket_type)
    if default is not None:
        socket.default_value = default
    if min_value is not None:
        socket.min_value = min_value
    if max_value is not None:
        socket.max_value = max_value
    return socket


def _math(tree, operation, a=None, b=None, location=(0, 0), clamp=False):
    node = tree.nodes.new("ShaderNodeMath")
    node.operation = operation
    node.use_clamp = clamp
    node.location = location
    for index, value in enumerate((a, b)):
        if value is None:
            continue
        if isinstance(value, bpy.types.NodeSocket):
            tree.links.new(value, node.inputs[index])
        else:
            node.inputs[index].default_value = value
    return node.outputs[0]


def _vmath(tree, operation, a=None, b=None, scale=None, location=(0, 0)):
    node = tree.nodes.new("ShaderNodeVectorMath")
    node.operation = operation
    node.location = location
    for index, value in enumerate((a, b)):
        if value is None:
            continue
        if isinstance(value, bpy.types.NodeSocket):
            tree.links.new(value, node.inputs[index])
        else:
            node.inputs[index].default_value = value
    if scale is not None:
        if isinstance(scale, bpy.types.NodeSocket):
            tree.links.new(scale, node.inputs["Scale"])
        else:
            node.inputs["Scale"].default_value = scale
    return node.outputs["Vector"] if operation != "DOT_PRODUCT" else node.outputs["Value"]


# ============ Sampling ============

def build_equirect_group():
    """Object-space position -> UV in planet-gen's equirectangular frame.

    Blender is Z-up, planet-gen is Y-up: planet (x, y, z) = Blender (x, z, -y).
    Longitude 0 faces Blender -Y, matching planet-gen's equirect centre column,
    so the maps and the exported surface line up on any sphere mesh.
    """
    group = _new_group(EQUIRECT_GROUP)
    tree = group
    _socket(group, "Vector", "INPUT", "NodeSocketVector")
    _socket(group, "Longitude Offset", "INPUT", "NodeSocketFloat", 0.0)
    _socket(group, "UV", "OUTPUT", "NodeSocketVector")
    group_in = tree.nodes.new("NodeGroupInput")
    group_in.location = (-1200, 0)
    group_out = tree.nodes.new("NodeGroupOutput")
    group_out.location = (100, 0)
    direction = _vmath(tree, "NORMALIZE", group_in.outputs["Vector"], location=(-1000, 0))
    split = tree.nodes.new("ShaderNodeSeparateXYZ")
    split.location = (-850, 0)
    tree.links.new(direction, split.inputs[0])
    minus_y = _math(tree, "MULTIPLY", split.outputs["Y"], -1.0, (-700, 100))
    lon = _math(tree, "ARCTAN2", split.outputs["X"], minus_y, (-550, 100))
    u = _math(tree, "MULTIPLY_ADD", lon, 1.0 / (2.0 * math.pi), (-400, 100))
    u.node.inputs[2].default_value = 0.5
    u = _math(tree, "ADD", u, group_in.outputs["Longitude Offset"], (-250, 100))
    lat = _math(tree, "ARCSINE", split.outputs["Z"], None, (-550, -100))
    v = _math(tree, "MULTIPLY_ADD", lat, 1.0 / math.pi, (-400, -100))
    v.node.inputs[2].default_value = 0.5
    uv = tree.nodes.new("ShaderNodeCombineXYZ")
    uv.location = (-100, 0)
    tree.links.new(u, uv.inputs["X"])
    tree.links.new(v, uv.inputs["Y"])
    tree.links.new(uv.outputs[0], group_out.inputs["UV"])
    return group


def build_sampler_group(density, drivers):
    """Object-space position -> density and every available driver channel.

    Channels whose map was not exported read as 0.
    """
    group = _new_group(SAMPLER_GROUP)
    tree = group
    _socket(group, "Vector", "INPUT", "NodeSocketVector")
    _socket(group, "Longitude Offset", "INPUT", "NodeSocketFloat", 0.0)
    outputs = [
        ("Density", "NodeSocketFloat"),
        ("Wind", "NodeSocketVector"),
        ("Wind Speed", "NodeSocketFloat"),
        ("Coverage", "NodeSocketFloat"),
        ("Low", "NodeSocketFloat"),
        ("Deep", "NodeSocketFloat"),
        ("High", "NodeSocketFloat"),
        ("Land", "NodeSocketFloat"),
        ("Thermal", "NodeSocketFloat"),
        ("Cloud Top", "NodeSocketFloat"),
        ("Continentality", "NodeSocketFloat"),
    ]
    for name, socket_type in outputs:
        _socket(group, name, "OUTPUT", socket_type)

    group_in = tree.nodes.new("NodeGroupInput")
    group_in.location = (-1200, 0)
    group_out = tree.nodes.new("NodeGroupOutput")
    group_out.location = (700, 0)
    links = tree.links

    uv = tree.nodes.new("ShaderNodeGroup")
    uv.node_tree = build_equirect_group()
    uv.location = (-300, 0)
    links.new(group_in.outputs["Vector"], uv.inputs["Vector"])
    links.new(group_in.outputs["Longitude Offset"], uv.inputs["Longitude Offset"])

    def sample(image, row, interpolation):
        tex = tree.nodes.new("ShaderNodeTexImage")
        tex.name = tex.label = image.name
        tex.image = image
        tex.interpolation = interpolation
        tex.extension = "REPEAT"
        tex.location = (50, 400 - row * 300)
        links.new(uv.outputs["UV"], tex.inputs["Vector"])
        rgb = tree.nodes.new("ShaderNodeSeparateColor")
        rgb.location = (350, 400 - row * 300)
        links.new(tex.outputs["Color"], rgb.inputs[0])
        return rgb.outputs, tex.outputs["Alpha"]

    # The density map is full export size; driver maps are low resolution, where
    # cubic keeps texel diamonds out of the result.
    density_rgb, _ = sample(density, 0, "Linear")
    links.new(density_rgb[0], group_out.inputs["Density"])

    wind, wind_a = sample(drivers["wind"], 1, "Cubic")
    links.new(wind_a, group_out.inputs["Wind Speed"])
    # Planet wind (x, y, z) -> Blender (x, -z, y).
    wind_vec = tree.nodes.new("ShaderNodeCombineXYZ")
    wind_vec.location = (550, 100)
    links.new(wind[0], wind_vec.inputs["X"])
    links.new(_math(tree, "MULTIPLY", wind[2], -1.0, (450, -50)), wind_vec.inputs["Y"])
    links.new(wind[1], wind_vec.inputs["Z"])
    links.new(wind_vec.outputs[0], group_out.inputs["Wind"])

    if "cloud" in drivers:
        cloud, cloud_a = sample(drivers["cloud"], 2, "Cubic")
        links.new(cloud_a, group_out.inputs["Coverage"])
        for index, name in enumerate(("Low", "Deep", "High")):
            links.new(cloud[index], group_out.inputs[name])
    if "climate" in drivers:
        climate, climate_a = sample(drivers["climate"], 3, "Cubic")
        for index, name in enumerate(("Land", "Thermal", "Cloud Top")):
            links.new(climate[index], group_out.inputs[name])
        links.new(climate_a, group_out.inputs["Continentality"])

    # Alpha is data here; keep Blender from premultiplying it into RGB.
    for image in drivers.values():
        image.alpha_mode = "CHANNEL_PACKED"
    return group


# ============ Billow clouds ============

def build_cloud_group(sampler):
    """Solver density eroded at its edges by multi-octave Worley billows.

        m       = clamp((density - Low) / (High - Low))
        billow  = remap(sum(inverted Worley F1 octaves) + wisps, BILLOW_RANGE)
        bn      = 1 - exp(-3 m)                          thickness, 0-1
        lo      = Erosion * variety * (1 - billow)       variety in 0.5-1.5
        kept    = clamp((bn - lo) / (1 - lo)) / bn
        cloud   = m * kept                              Density output
        tau     = Depth Gain * max(density - Low, 0) * kept * m^Contrast
        opacity = Diffusion tau / (1 + Diffusion tau)    two-stream reflectance

    Thin areas and edges lose density where billows are low, thick cores keep
    theirs, so the solver's shapes survive and gain puffy, multi-scale edges.
    The app preview uses the same mapping: opacity keeps rising with thickness
    instead of saturating, so ripples inside decks stay visible, and a bump
    from the opacity shades the sunward and lee sides of each puff.
    """
    group = _new_group(CLOUD_GROUP)
    tree = group
    _socket(group, "Shader", "OUTPUT", "NodeSocketShader")
    _socket(group, "Density", "OUTPUT", "NodeSocketFloat")
    for name, value in CLOUD_DEFAULTS.items():
        _socket(group, name, "INPUT", "NodeSocketFloat", value)

    gi = tree.nodes.new("NodeGroupInput")
    gi.location = (-2600, 0)
    go = tree.nodes.new("NodeGroupOutput")
    go.location = (1600, 0)
    links = tree.links
    inp = gi.outputs

    coords = tree.nodes.new("ShaderNodeTexCoord")
    coords.location = (-2600, 600)
    drivers = tree.nodes.new("ShaderNodeGroup")
    drivers.node_tree = sampler
    drivers.location = (-2300, 700)
    links.new(coords.outputs["Object"], drivers.inputs["Vector"])
    links.new(inp["Longitude Offset"], drivers.inputs["Longitude Offset"])

    # Solver density -> 0-1 cloud amount.
    span = _math(tree, "SUBTRACT", inp["Density High"], inp["Density Low"], (-2050, 900))
    m = _math(tree, "SUBTRACT", drivers.outputs["Density"], inp["Density Low"], (-2050, 750))
    m = _math(tree, "DIVIDE", m, span, (-1900, 800), clamp=True)

    # Detail domain: unit direction in 0-1 space, compressed along the pole (Z),
    # displaced along the wind by a slow signed noise.
    unit = _vmath(tree, "NORMALIZE", coords.outputs["Object"], location=(-2300, 300))
    zonal = tree.nodes.new("ShaderNodeCombineXYZ")
    zonal.location = (-2300, 100)
    zonal.inputs["X"].default_value = zonal.inputs["Y"].default_value = 0.5
    links.new(_math(tree, "MULTIPLY", inp["Zonal Stretch"], 0.5, (-2450, 50)), zonal.inputs["Z"])
    domain = tree.nodes.new("ShaderNodeVectorMath")
    domain.operation = "MULTIPLY_ADD"
    domain.location = (-2100, 250)
    links.new(unit, domain.inputs[0])
    links.new(zonal.outputs[0], domain.inputs[1])
    domain.inputs[2].default_value = (0.5, 0.5, 0.5)
    domain = domain.outputs["Vector"]
    streak = tree.nodes.new("ShaderNodeTexNoise")
    streak.location = (-1950, 400)
    streak.inputs["Scale"].default_value = 6.0
    streak.inputs["Detail"].default_value = 2.0
    links.new(domain, streak.inputs["Vector"])
    signed = _math(tree, "MULTIPLY_ADD", streak.outputs["Fac"], 2.0, (-1800, 400))
    signed.node.inputs[2].default_value = -1.0
    amount = _math(tree, "MULTIPLY", signed, inp["Wind Streak"], (-1650, 400))
    offset = _vmath(tree, "SCALE", drivers.outputs["Wind"], scale=amount, location=(-1500, 400))
    domain = _vmath(tree, "ADD", domain, offset, location=(-1350, 300))

    # Billows: inverted Worley F1 octaves plus zero-mean fine FBM wisps.
    total = None
    for index, (multiplier, weight) in enumerate(BILLOW_OCTAVES):
        y = 900 - index * 220
        cell = tree.nodes.new("ShaderNodeTexVoronoi")
        cell.voronoi_dimensions = "3D"
        cell.feature = "F1"
        cell.location = (-1100, y)
        cell.inputs["Randomness"].default_value = 1.0
        links.new(domain, cell.inputs["Vector"])
        links.new(_math(tree, "MULTIPLY", inp["Scale"], multiplier, (-1250, y)), cell.inputs["Scale"])
        inverted = _math(tree, "SUBTRACT", 1.0, cell.outputs["Distance"], (-900, y))
        weighted = _math(tree, "MULTIPLY", inverted, weight, (-750, y))
        total = weighted if total is None else _math(tree, "ADD", total, weighted, (-600, y))
    wisp = tree.nodes.new("ShaderNodeTexNoise")
    wisp.location = (-1100, -500)
    wisp.inputs["Detail"].default_value = 4.0
    wisp.inputs["Roughness"].default_value = 0.55
    links.new(domain, wisp.inputs["Vector"])
    links.new(_math(tree, "MULTIPLY", inp["Scale"], WISP_FREQUENCY, (-1250, -500)), wisp.inputs["Scale"])
    centred = _math(tree, "SUBTRACT", wisp.outputs["Fac"], 0.5, (-900, -500))
    total = _math(tree, "ADD", total, _math(tree, "MULTIPLY", centred, inp["Wisps"], (-750, -500)), (-600, -500))
    low, high = BILLOW_RANGE
    billow = _math(tree, "SUBTRACT", total, low, (-450, 200))
    billow = _math(tree, "DIVIDE", billow, high - low, (-300, 200), clamp=True)

    # Regional variety: erosion x (0.5-1.5) from a planet-scale noise.
    variety = tree.nodes.new("ShaderNodeTexNoise")
    variety.location = (-450, 700)
    variety.inputs["Detail"].default_value = 2.0
    links.new(domain, variety.inputs["Vector"])
    links.new(inp["Variety Scale"], variety.inputs["Scale"])
    erosion = _math(tree, "MULTIPLY", variety.outputs["Fac"], 2.0, (-300, 700))
    erosion = _math(tree, "MULTIPLY", erosion, inp["Erosion"], (-150, 700))

    # Edge erosion: thin parts give way where billows are low, cores hold.
    bn = _math(tree, "MULTIPLY", m, -3.0, (-150, 900))
    bn = _math(tree, "EXPONENT", bn, None, (0, 900))
    bn = _math(tree, "SUBTRACT", 1.0, bn, (150, 900))
    lo = _math(tree, "SUBTRACT", 1.0, billow, (-150, 300))
    lo = _math(tree, "MULTIPLY", lo, erosion, (0, 400))
    kept = _math(tree, "SUBTRACT", bn, lo, (300, 700))
    kept = _math(tree, "DIVIDE", kept, _math(tree, "SUBTRACT", 1.0, lo, (300, 450)), (450, 600), clamp=True)
    ratio = _math(tree, "DIVIDE", kept, _math(tree, "MAXIMUM", bn, 1.0e-3, (450, 900)), (600, 700))
    cloud = _math(tree, "MULTIPLY", m, ratio, (750, 700))
    links.new(cloud, go.inputs["Density"])

    # Physical optical depth with the thin-column contrast curve, then two-stream
    # reflectance as opacity.
    excess = _math(tree, "SUBTRACT", drivers.outputs["Density"], inp["Density Low"], (600, 500))
    excess = _math(tree, "MAXIMUM", excess, 0.0, (750, 500))
    contrast = _math(tree, "POWER", _math(tree, "MAXIMUM", m, 1.0e-4, (600, 350)), inp["Contrast"], (750, 350))
    tau = _math(tree, "MULTIPLY", excess, ratio, (900, 500))
    tau = _math(tree, "MULTIPLY", tau, contrast, (1050, 500))
    tau = _math(tree, "MULTIPLY", tau, inp["Depth Gain"], (1200, 500))
    scattered = _math(tree, "MULTIPLY", tau, inp["Diffusion"], (1350, 500))
    opacity = _math(tree, "DIVIDE", scattered, _math(tree, "ADD", scattered, 1.0, (1350, 350)), (1500, 450))

    bump = tree.nodes.new("ShaderNodeBump")
    bump.location = (900, 0)
    bump.inputs["Distance"].default_value = 0.002
    links.new(inp["Relief"], bump.inputs["Strength"])
    # Height is the visible cloud itself, so the solver's ripples and cells get
    # sunlit and shaded sides. Billow octaves are left out: unfiltered, they are
    # only a few pixels wide at most distances and shade as sandpaper grain.
    links.new(opacity, bump.inputs["Height"])

    bsdf = tree.nodes.new("ShaderNodeBsdfPrincipled")
    bsdf.location = (1100, 0)
    bsdf.inputs["Base Color"].default_value = (1.0, 1.0, 1.0, 1.0)
    bsdf.inputs["Roughness"].default_value = 1.0
    links.new(bump.outputs["Normal"], bsdf.inputs["Normal"])
    transparent = tree.nodes.new("ShaderNodeBsdfTransparent")
    transparent.location = (1100, 250)
    shader = tree.nodes.new("ShaderNodeMixShader")
    shader.location = (1400, 100)
    links.new(opacity, shader.inputs["Fac"])
    links.new(transparent.outputs[0], shader.inputs[1])
    links.new(bsdf.outputs[0], shader.inputs[2])
    links.new(shader.outputs[0], go.inputs["Shader"])
    return group


# ============ Material ============

def build_cloud_material(group):
    material = bpy.data.materials.get(CLOUD_MATERIAL)
    if material is None:
        material = bpy.data.materials.new(CLOUD_MATERIAL)
    material.use_nodes = True
    tree = material.node_tree
    tree.nodes.clear()
    node = tree.nodes.new("ShaderNodeGroup")
    node.node_tree = group
    node.label = "Clouds (Planet Gen, billow detail)"
    node.location = (0, 0)
    output = tree.nodes.new("ShaderNodeOutputMaterial")
    output.location = (300, 0)
    tree.links.new(node.outputs["Shader"], output.inputs["Surface"])
    if hasattr(material, "surface_render_method"):
        material.surface_render_method = "DITHERED"
    return material


def apply(directory, obj=None):
    """Load the maps from ``directory``, (re)build the node groups and return the
    billow cloud material, assigned to ``obj`` when given."""
    density, drivers = load_images(directory)
    sampler = build_sampler_group(density, drivers)
    group = build_cloud_group(sampler)
    material = build_cloud_material(group)
    if obj is not None:
        if obj.data.materials:
            obj.data.materials[0] = material
        else:
            obj.data.materials.append(material)
    return material


def apply_surface(directory, obj):
    """Planet Gen albedo (and roughness when exported) on ``obj`` in the same
    equirect frame as the cloud maps, so clouds sit over the right land."""
    def load(stem, colorspace):
        for ext in (".exr", ".png"):
            path = os.path.join(directory, stem + ext)
            if os.path.exists(path):
                image = bpy.data.images.load(path, check_existing=True)
                image.colorspace_settings.name = colorspace
                return image
        return None

    albedo = load("albedo", "sRGB")
    if albedo is None:
        raise FileNotFoundError("No albedo.png or albedo.exr in {}".format(directory))
    roughness = load("roughness", "Non-Color")
    material = bpy.data.materials.get("PlanetGen Surface") or bpy.data.materials.new("PlanetGen Surface")
    material.use_nodes = True
    tree = material.node_tree
    tree.nodes.clear()
    coords = tree.nodes.new("ShaderNodeTexCoord")
    coords.location = (-900, 0)
    uv = tree.nodes.new("ShaderNodeGroup")
    uv.node_tree = bpy.data.node_groups.get(EQUIRECT_GROUP) or build_equirect_group()
    uv.location = (-700, 0)
    tree.links.new(coords.outputs["Object"], uv.inputs["Vector"])
    bsdf = tree.nodes.new("ShaderNodeBsdfPrincipled")
    bsdf.location = (0, 0)
    output = tree.nodes.new("ShaderNodeOutputMaterial")
    output.location = (300, 0)
    tree.links.new(bsdf.outputs[0], output.inputs["Surface"])
    for row, (image, socket) in enumerate(((albedo, "Base Color"), (roughness, "Roughness"))):
        if image is None:
            continue
        tex = tree.nodes.new("ShaderNodeTexImage")
        tex.image = image
        tex.interpolation = "Cubic"
        tex.location = (-400, 200 - row * 300)
        tree.links.new(uv.outputs["UV"], tex.inputs["Vector"])
        tree.links.new(tex.outputs["Color"], bsdf.inputs[socket])
    if obj.data.materials:
        obj.data.materials[0] = material
    else:
        obj.data.materials.append(material)
    return material


def _main(argv):
    import argparse

    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--drivers", required=True, help="Planet Gen export directory")
    parser.add_argument("--object", default="Clouds", help="Cloud shell object")
    parser.add_argument("--surface", help="Also put the export's albedo on this planet object")
    parser.add_argument("--save", help="Write the result to this .blend")
    args = parser.parse_args(argv)
    obj = bpy.data.objects.get(args.object)
    if obj is None:
        raise SystemExit("No object named {!r} in this file".format(args.object))
    apply(os.path.abspath(args.drivers), obj)
    if args.surface:
        planet = bpy.data.objects.get(args.surface)
        if planet is None:
            raise SystemExit("No object named {!r} in this file".format(args.surface))
        apply_surface(os.path.abspath(args.drivers), planet)
    if args.save:
        bpy.ops.wm.save_as_mainfile(filepath=os.path.abspath(args.save), copy=True)


if __name__ == "__main__":
    import sys

    _main(sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else [])
