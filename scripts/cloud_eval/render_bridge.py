"""Headless Cycles render of a Planet Gen export through the cloud bridge.

Needs `pip install bpy`. The export directory holds albedo.png and
cloud_density.exr (`planet_capture OUT --file PLANET.json --export FACE_RES`
writes them to OUT/NAME/).

python3 -I scripts/cloud_eval/render_bridge.py --bridge blender_addon --export DIR --out out.png \
    --view LAT,LON,ALT_KM,TILT_DEG,HEADING_DEG,FOV_DEG [--view ...] [--sun-elev 35] [--sun-az 135] \
    [--samples 32] [--size 720x480] [--set Erosion=0.5]

Views used for the Blender close-up pass on planet_risa_06: close-up
12,-5,600,25,0,40; mid-range 15,-5,2500,25,0,40; globe 10,-20,12742,0,0,42.
"""
import argparse
import importlib.util
import math
import os
import sys

import bpy
from mathutils import Vector, Matrix

RADIUS_KM = 6371.0


def load_bridge(path):
    spec = importlib.util.spec_from_file_location("cloud_bridge", os.path.join(path, "cloud_bridge.py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def direction(lat, lon):
    lat, lon = math.radians(lat), math.radians(lon)
    # Planet (x, y, z) -> Blender (x, z, -y); longitude 0 faces Blender -Y.
    return Vector((math.cos(lat) * math.sin(lon), -math.cos(lat) * math.cos(lon), math.sin(lat)))


def build_scene(bridge, export_dir, shell, sun_elev, sun_az, view, settings):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = settings.samples
    scene.cycles.use_denoising = settings.denoise
    scene.cycles.max_bounces = 4
    scene.cycles.transparent_max_bounces = 16
    scene.view_settings.view_transform = "AgX" if "AgX" in [
        v.identifier for v in scene.view_settings.bl_rna.properties["view_transform"].enum_items
    ] else "Filmic"
    world = bpy.data.worlds.new("World")
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs["Color"].default_value = (0.004, 0.006, 0.012, 1.0)
    scene.world = world

    bpy.ops.mesh.primitive_uv_sphere_add(segments=512, ring_count=256, radius=1.0)
    planet = bpy.context.active_object
    planet.name = "Planet"
    bpy.ops.object.shade_smooth()
    bpy.ops.mesh.primitive_uv_sphere_add(segments=512, ring_count=256, radius=1.0)
    clouds = bpy.context.active_object
    clouds.name = "Clouds"
    clouds.scale = (shell, shell, shell)
    bpy.ops.object.shade_smooth()

    bridge.apply(export_dir, clouds)
    bridge.apply_surface(export_dir, planet)
    for key, value in settings.cloud_inputs.items():
        group = bpy.data.materials[bridge.CLOUD_MATERIAL].node_tree.nodes[0]
        group.inputs[key].default_value = value

    lat, lon, alt_km, tilt, heading, fov = view
    target = direction(lat, lon)
    east = Vector((0.0, 0.0, 1.0)).cross(target).normalized()
    north = target.cross(east).normalized()
    forward_ground = (north * math.cos(math.radians(heading)) + east * math.sin(math.radians(heading))).normalized()
    # Camera above a point offset back along the heading so the view centre is the target.
    alt = alt_km / RADIUS_KM
    back = math.tan(math.radians(tilt)) * alt
    position = (target - forward_ground * back).normalized() * (1.0 + alt)
    look = (target * 1.0 - position).normalized()
    cam_data = bpy.data.cameras.new("Camera")
    cam_data.angle = math.radians(fov)
    cam_data.clip_start = 1.0e-5
    cam_data.clip_end = 10.0
    camera = bpy.data.objects.new("Camera", cam_data)
    scene.collection.objects.link(camera)
    up = target
    z_axis = -look
    x_axis = up.cross(z_axis).normalized()
    y_axis = z_axis.cross(x_axis).normalized()
    rot = Matrix((x_axis, y_axis, z_axis)).transposed()
    camera.matrix_world = Matrix.Translation(position) @ rot.to_4x4()
    scene.camera = camera

    sun_data = bpy.data.lights.new("Sun", "SUN")
    sun_data.energy = 6.0
    sun_data.angle = math.radians(0.53)
    sun = bpy.data.objects.new("Sun", sun_data)
    scene.collection.objects.link(sun)
    # Sun direction in the local frame of the target: elevation above the horizon, azimuth from north.
    el, az = math.radians(sun_elev), math.radians(sun_az)
    to_sun = (target * math.sin(el) + (north * math.cos(az) + east * math.sin(az)) * math.cos(el)).normalized()
    sun.rotation_euler = (-to_sun).to_track_quat("-Z", "Y").to_euler()
    return scene


def main(argv):
    parser = argparse.ArgumentParser()
    parser.add_argument("--bridge", required=True)
    parser.add_argument("--export", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--view", action="append", required=True)
    parser.add_argument("--shell", type=float, default=1.005)
    parser.add_argument("--sun-elev", type=float, default=35.0)
    parser.add_argument("--sun-az", type=float, default=135.0)
    parser.add_argument("--samples", type=int, default=48)
    parser.add_argument("--size", default="960x640")
    parser.add_argument("--denoise", action="store_true")
    parser.add_argument("--set", action="append", default=[], help="cloud group input NAME=VALUE")
    args = parser.parse_args(argv)
    args.cloud_inputs = {k: float(v) for k, v in (item.split("=", 1) for item in args.set)}
    bridge = load_bridge(args.bridge)
    width, height = (int(v) for v in args.size.split("x"))
    for index, spec in enumerate(args.view):
        view = [float(v) for v in spec.split(",")]
        scene = build_scene(bridge, os.path.abspath(args.export), args.shell, args.sun_elev, args.sun_az, view, args)
        scene.render.resolution_x = width
        scene.render.resolution_y = height
        scene.render.resolution_percentage = 100
        out = args.out if len(args.view) == 1 else args.out.replace(".png", "-{}.png".format(index))
        scene.render.filepath = os.path.abspath(out)
        bpy.ops.render.render(write_still=True)
        print("rendered", out)


if __name__ == "__main__":
    main(sys.argv[1:])
