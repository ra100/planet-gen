"""Planet Gen cloud bridge: physics-driven placement, procedural Blender detail.

Planet Gen exports three small equirectangular RGBA driver maps next to
``clouds.exr``. They hold the weather solver's low-frequency fields:

    cloud_drivers.exr    R low mass, G deep (convective) mass, B high (cirrus) mass, A coverage
    climate_drivers.exr  R land fraction, G thermal (0 at -25 C, 1 at 30 C),
                         B cloud top / 20 km, A continentality
    wind_drivers.exr     RGB tangent wind in planet space (Y up), A wind speed

The maps decide *where* clouds are, how much they fill in, and which way they
streak. The multi-octave 4D noise network (the same recipe as the "Clouds"
group in planet3-Planet-Shader-v10.blend) supplies the fine detail at render
time, so the cloud shell stays sharp at any resolution without a 16K export.

Usage inside Blender: the Planet Gen add-on panel calls :func:`apply`.
Headless:

    blender scene.blend --background --python cloud_bridge.py -- \
        --drivers /path/to/export/planet --object Clouds --save out.blend
"""

import math
import os

import bpy

DRIVER_FILES = {
    "cloud": "cloud_drivers.exr",
    "climate": "climate_drivers.exr",
    "wind": "wind_drivers.exr",
}
IMAGE_PREFIX = "PlanetGen "
EQUIRECT_GROUP = "PlanetGen Equirect UV"
SAMPLER_GROUP = "PlanetGen Driver Sampler"
CLOUD_GROUP = "PlanetGen Driven Clouds"
CLOUD_MATERIAL = "PlanetGen Driven Clouds"

# Detail defaults copied from the v10 "Clouds" material.
DETAIL_DEFAULTS = {
    "W": 0.4,
    "Roughness": 0.5179,
    "Distortion": 0.43,
    "Detail": 16.0,
    "Scale": 2.68,
}
DRIVE_DEFAULTS = {
    "Drive Strength": 1.0,
    "Coverage Gain": 2.5,
    "Fill Boost": 0.1,
    "Background Cover": 0.1,
    "Wind Streak": 0.04,
    "Longitude Offset": 0.0,
}


# ============ Images ============

def load_driver_images(directory):
    """Load (or re-point) the driver maps. Fixed datablock names let a re-import
    swap a new export into every material that already uses them."""
    images = {}
    missing = []
    for key, filename in DRIVER_FILES.items():
        path = os.path.join(directory, filename)
        if not os.path.exists(path):
            missing.append(filename)
            continue
        name = IMAGE_PREFIX + filename
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
        images[key] = image
    if missing:
        raise FileNotFoundError(
            "Missing cloud driver maps in {}: {}".format(directory, ", ".join(missing))
        )
    return images


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


# ============ Driver sampler ============

def build_equirect_group():
    """Object-space position -> UV in planet-gen's equirectangular frame.

    Blender is Z-up, planet-gen is Y-up: planet (x, y, z) = Blender (x, z, -y).
    Longitude 0 faces Blender -Y, matching planet-gen's equirect centre column,
    so driver maps and the exported surface maps line up on any sphere mesh.
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


def build_sampler_group(images):
    """Object-space position -> every driver channel at that point."""
    group = _new_group(SAMPLER_GROUP)
    tree = group
    _socket(group, "Vector", "INPUT", "NodeSocketVector")
    _socket(group, "Longitude Offset", "INPUT", "NodeSocketFloat", 0.0)
    outputs = [
        ("Coverage", "NodeSocketFloat"),
        ("Low", "NodeSocketFloat"),
        ("Deep", "NodeSocketFloat"),
        ("High", "NodeSocketFloat"),
        ("Land", "NodeSocketFloat"),
        ("Thermal", "NodeSocketFloat"),
        ("Cloud Top", "NodeSocketFloat"),
        ("Continentality", "NodeSocketFloat"),
        ("Wind", "NodeSocketVector"),
        ("Wind Speed", "NodeSocketFloat"),
    ]
    for name, socket_type in outputs:
        _socket(group, name, "OUTPUT", socket_type)

    group_in = tree.nodes.new("NodeGroupInput")
    group_in.location = (-1200, 0)
    group_out = tree.nodes.new("NodeGroupOutput")
    group_out.location = (600, 0)

    uv = tree.nodes.new("ShaderNodeGroup")
    uv.node_tree = build_equirect_group()
    uv.location = (-300, 0)
    tree.links.new(group_in.outputs["Vector"], uv.inputs["Vector"])
    tree.links.new(group_in.outputs["Longitude Offset"], uv.inputs["Longitude Offset"])

    samples = {}
    for row, key in enumerate(("cloud", "climate", "wind")):
        tex = tree.nodes.new("ShaderNodeTexImage")
        tex.name = tex.label = "PlanetGen " + key
        tex.image = images[key]
        # Driver maps are low resolution; cubic keeps texel diamonds out of the detail.
        tex.interpolation = "Cubic"
        tex.extension = "REPEAT"
        tex.location = (50, 300 - row * 300)
        tree.links.new(uv.outputs["UV"], tex.inputs["Vector"])
        rgb = tree.nodes.new("ShaderNodeSeparateColor")
        rgb.location = (350, 300 - row * 300)
        tree.links.new(tex.outputs["Color"], rgb.inputs[0])
        samples[key] = (rgb.outputs, tex.outputs["Alpha"])

    cloud, cloud_a = samples["cloud"]
    climate, climate_a = samples["climate"]
    wind, wind_a = samples["wind"]
    links = tree.links
    links.new(cloud_a, group_out.inputs["Coverage"])
    links.new(cloud[0], group_out.inputs["Low"])
    links.new(cloud[1], group_out.inputs["Deep"])
    links.new(cloud[2], group_out.inputs["High"])
    links.new(climate[0], group_out.inputs["Land"])
    links.new(climate[1], group_out.inputs["Thermal"])
    links.new(climate[2], group_out.inputs["Cloud Top"])
    links.new(climate_a, group_out.inputs["Continentality"])
    links.new(wind_a, group_out.inputs["Wind Speed"])
    # Planet wind (x, y, z) -> Blender (x, -z, y).
    wind_vec = tree.nodes.new("ShaderNodeCombineXYZ")
    wind_vec.location = (450, -500)
    links.new(wind[0], wind_vec.inputs["X"])
    links.new(_math(tree, "MULTIPLY", wind[2], -1.0, (350, -650)), wind_vec.inputs["Y"])
    links.new(wind[1], wind_vec.inputs["Z"])
    links.new(wind_vec.outputs[0], group_out.inputs["Wind"])
    # Alpha is data here; keep Blender from premultiplying it into RGB.
    for image in images.values():
        image.alpha_mode = "CHANNEL_PACKED"
    return group


# ============ Driven cloud group ============

def _noise(tree, location):
    node = tree.nodes.new("ShaderNodeTexNoise")
    node.noise_dimensions = "4D"
    if hasattr(node, "noise_type"):
        node.noise_type = "FBM"
    if hasattr(node, "normalize"):
        node.normalize = True
    node.inputs["Lacunarity"].default_value = 2.0
    node.location = location
    return node


def _ramp(tree, stops, location):
    node = tree.nodes.new("ShaderNodeValToRGB")
    node.location = location
    elements = node.color_ramp.elements
    (p0, c0), (p1, c1) = stops
    elements[0].position, elements[0].color = p0, (c0, c0, c0, 1.0)
    elements[1].position, elements[1].color = p1, (c1, c1, c1, 1.0)
    return node


def build_cloud_group(sampler):
    """The v10 four-stack cloud recipe, with planet-gen coverage replacing its
    procedural hole mask and wind/coverage shaping the detail stacks.

    Drive Strength 0 reproduces the original v10 look exactly.
    """
    group = _new_group(CLOUD_GROUP)
    tree = group
    _socket(group, "Shader", "OUTPUT", "NodeSocketShader")
    _socket(group, "Coverage", "OUTPUT", "NodeSocketFloat")
    for name, value in DETAIL_DEFAULTS.items():
        _socket(group, name, "INPUT", "NodeSocketFloat", value)
    _socket(group, "Drive Strength", "INPUT", "NodeSocketFloat", 1.0, 0.0, 1.0)
    _socket(group, "Coverage Gain", "INPUT", "NodeSocketFloat", DRIVE_DEFAULTS["Coverage Gain"], 0.0, 20.0)
    _socket(group, "Fill Boost", "INPUT", "NodeSocketFloat", DRIVE_DEFAULTS["Fill Boost"], 0.0, 1.0)
    _socket(group, "Background Cover", "INPUT", "NodeSocketFloat", DRIVE_DEFAULTS["Background Cover"], 0.0, 1.0)
    _socket(group, "Wind Streak", "INPUT", "NodeSocketFloat", DRIVE_DEFAULTS["Wind Streak"], 0.0, 0.5)
    _socket(group, "Longitude Offset", "INPUT", "NodeSocketFloat", 0.0)

    gi = tree.nodes.new("NodeGroupInput")
    gi.location = (-2600, 0)
    go = tree.nodes.new("NodeGroupOutput")
    go.location = (1400, 0)
    links = tree.links
    inp = gi.outputs

    # Drivers sampled at the shading point.
    coords = tree.nodes.new("ShaderNodeTexCoord")
    coords.location = (-2600, 600)
    drivers = tree.nodes.new("ShaderNodeGroup")
    drivers.node_tree = sampler
    drivers.location = (-2300, 700)
    links.new(coords.outputs["Object"], drivers.inputs["Vector"])
    links.new(inp["Longitude Offset"], drivers.inputs["Longitude Offset"])
    drive = inp["Drive Strength"]
    coverage = _math(tree, "MULTIPLY", drivers.outputs["Coverage"], inp["Coverage Gain"], (-2050, 800), clamp=True)
    # Smooth the solver's soft edges into a cloud-bank silhouette.
    coverage_node = tree.nodes.new("ShaderNodeMapRange")
    coverage_node.interpolation_type = "SMOOTHSTEP"
    coverage_node.location = (-1900, 800)
    links.new(coverage, coverage_node.inputs["Value"])
    coverage = coverage_node.outputs["Result"]
    links.new(coverage, go.inputs["Coverage"])

    # Wind streaks: displace the detail domain along the wind by a slow noise,
    # which stretches features along the flow without moving the cloud banks.
    streak_noise = _noise(tree, (-2300, 300))
    streak_noise.noise_dimensions = "3D"
    streak_noise.inputs["Scale"].default_value = 6.0
    streak_noise.inputs["Detail"].default_value = 2.0
    links.new(coords.outputs["Generated"], streak_noise.inputs["Vector"])
    centred = _math(tree, "SUBTRACT", streak_noise.outputs["Fac"], 0.5, (-2100, 300))
    amount = _math(tree, "MULTIPLY", centred, inp["Wind Streak"], (-1950, 300))
    amount = _math(tree, "MULTIPLY", amount, drive, (-1800, 300))
    offset = _vmath(tree, "SCALE", drivers.outputs["Wind"], scale=amount, location=(-1650, 300))
    domain = _vmath(tree, "ADD", coords.outputs["Generated"], offset, location=(-1500, 300))

    # Fill: dense planet-gen coverage lowers the detail thresholds, sparse raises them.
    fill = _math(tree, "SUBTRACT", coverage, 0.5, (-1650, 600))
    fill = _math(tree, "MULTIPLY", fill, inp["Fill Boost"], (-1500, 600))
    fill = _math(tree, "MULTIPLY", fill, drive, (-1350, 600))

    roughness = _math(tree, "MINIMUM", inp["Roughness"], 1.0, (-2300, -200))
    roughness = _math(tree, "MAXIMUM", roughness, 0.0, (-2150, -200))
    scale = inp["Scale"]

    def stack(multipliers, y):
        """Three domain-warped 4D fBm noises; returns the last noise's Fac."""
        previous = domain
        last = None
        for column, multiplier in enumerate(multipliers):
            node = _noise(tree, (-1200 + column * 220, y))
            links.new(inp["W"], node.inputs["W"])
            links.new(inp["Detail"], node.inputs["Detail"])
            links.new(inp["Distortion"], node.inputs["Distortion"])
            links.new(roughness, node.inputs["Roughness"])
            links.new(_math(tree, "MULTIPLY", scale, multiplier, (-1300 + column * 220, y - 250)), node.inputs["Scale"])
            links.new(previous, node.inputs["Vector"])
            previous = node.outputs["Color"]
            last = node
        return last.outputs["Fac"]

    cumulus_stops = ((0.558, 0.0), (0.707, 1.0))
    detail_a = stack((2.0, 4.0, 8.0), 900)
    detail_b = stack((1.25, 2.5, 5.0), 400)
    detail_c = stack((1.0, 2.0, 4.0), -100)
    holes = stack((0.75, 1.5, 3.0), -600)

    def shaped(fac, y):
        ramp = _ramp(tree, cumulus_stops, (-300, y))
        links.new(_math(tree, "ADD", fac, fill, (-450, y)), ramp.inputs["Fac"])
        return ramp.outputs["Color"]

    ramp_a = shaped(detail_a, 900)
    ramp_b = shaped(detail_b, 400)
    ramp_c = shaped(detail_c, -100)
    hole_ramp = _ramp(tree, ((0.409, 1.0), (0.61, 0.0)), (-300, -600))
    links.new(holes, hole_ramp.inputs["Fac"])

    def mix(blend, a, b, factor, location):
        node = tree.nodes.new("ShaderNodeMix")
        node.data_type = "RGBA"
        node.blend_type = blend
        node.location = location
        if isinstance(factor, bpy.types.NodeSocket):
            links.new(factor, node.inputs["Factor"])
        else:
            node.inputs["Factor"].default_value = factor
        links.new(a, node.inputs[6])
        links.new(b, node.inputs[7])
        return node.outputs[2]

    combined = mix("MIX", ramp_a, ramp_b, 0.5, (0, 700))
    combined = mix("MIX", combined, ramp_c, 0.5, (200, 500))
    # Planet-gen coverage takes over the large-scale placement from the hole mask.
    # A little of the procedural mask survives as background cover so clear
    # regions keep scattered fair-weather cloud instead of going empty.
    background = _math(tree, "MULTIPLY", hole_ramp.outputs["Color"], inp["Background Cover"], (0, -500))
    driven_mask = _math(tree, "MAXIMUM", coverage, background, (100, -400))
    mask = mix("MIX", hole_ramp.outputs["Color"], driven_mask, drive, (250, -400))
    density = mix("MULTIPLY", combined, mask, 1.0, (400, 300))

    opacity = _math(tree, "MULTIPLY", density, 8.0, (600, 300))
    # Taller columns read as rougher cloud tops.
    top_relief = _math(tree, "MULTIPLY_ADD", drivers.outputs["Cloud Top"], 2.5, (400, -100))
    top_relief.node.inputs[2].default_value = 0.5
    relief_node = tree.nodes.new("ShaderNodeMix")
    relief_node.data_type = "FLOAT"
    relief_node.location = (550, -100)
    links.new(drive, relief_node.inputs["Factor"])
    relief_node.inputs[2].default_value = 1.0
    links.new(top_relief, relief_node.inputs[3])
    bump_strength = _math(tree, "MULTIPLY", relief_node.outputs[0], 0.05, (700, -100), clamp=True)

    bump = tree.nodes.new("ShaderNodeBump")
    bump.location = (850, -100)
    bump.inputs["Distance"].default_value = 0.1
    links.new(bump_strength, bump.inputs["Strength"])
    links.new(density, bump.inputs["Height"])
    bsdf = tree.nodes.new("ShaderNodeBsdfPrincipled")
    bsdf.location = (1000, -100)
    bsdf.inputs["Base Color"].default_value = (1.0, 1.0, 1.0, 1.0)
    bsdf.inputs["Roughness"].default_value = 1.0
    links.new(bump.outputs["Normal"], bsdf.inputs["Normal"])
    transparent = tree.nodes.new("ShaderNodeBsdfTransparent")
    transparent.location = (1000, 200)
    shader = tree.nodes.new("ShaderNodeMixShader")
    shader.location = (1200, 0)
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
    node.label = "Clouds (Planet Gen driven)"
    node.location = (0, 0)
    output = tree.nodes.new("ShaderNodeOutputMaterial")
    output.location = (300, 0)
    tree.links.new(node.outputs["Shader"], output.inputs["Surface"])
    if hasattr(material, "surface_render_method"):
        material.surface_render_method = "DITHERED"
    return material


def suggested_coverage_gain(image):
    """Gain that maps the 98th-percentile solver coverage to full cover.

    Solver coverage is a physical column fraction and often peaks well below 1;
    the procedural mask needs most of its range to read as cloud banks.
    """
    import numpy

    pixels = numpy.empty(len(image.pixels), dtype=numpy.float32)
    image.pixels.foreach_get(pixels)
    coverage = pixels[3::4]
    cloudy = coverage[coverage > 1.0e-3]
    if cloudy.size == 0:
        return DRIVE_DEFAULTS["Coverage Gain"]
    return float(min(max(1.0 / float(numpy.percentile(cloudy, 98.0)), 1.0), 20.0))


def apply(directory, obj=None, auto_gain=True):
    """Load the driver maps from ``directory``, (re)build the node groups and
    return the driven cloud material, assigned to ``obj`` when given."""
    images = load_driver_images(directory)
    sampler = build_sampler_group(images)
    group = build_cloud_group(sampler)
    material = build_cloud_material(group)
    if auto_gain:
        node = material.node_tree.nodes[0]
        node.inputs["Coverage Gain"].default_value = suggested_coverage_gain(images["cloud"])
    if obj is not None:
        if obj.data.materials:
            obj.data.materials[0] = material
        else:
            obj.data.materials.append(material)
    return material


def apply_surface(directory, obj):
    """Planet Gen albedo (and roughness when exported) on ``obj`` in the same
    equirect frame as the cloud drivers, so clouds sit over the right land."""
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
    parser.add_argument("--object", default="Clouds", help="Cloud shell object to drive")
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
