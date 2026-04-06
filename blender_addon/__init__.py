bl_info = {
    "name": "Planet Gen Importer",
    "author": "Planet Gen",
    "version": (1, 0, 0),
    "blender": (4, 0, 0),
    "location": "View3D > Sidebar > Planet Gen",
    "description": "Import procedurally generated planet textures and set up materials",
    "category": "Import-Export",
}

import bpy
import os
from bpy.props import StringProperty, EnumProperty, FloatProperty
from bpy_extras.io_utils import ImportHelper


# ============ Texture loading ============

TEXTURE_FILES = {
    "albedo": "albedo.exr",
    "normal": "normal.exr",
    "roughness": "roughness.exr",
    "height": "height.exr",
    "ao": "ao.exr",
    "clouds": "clouds.exr",
    "emission": "emission.exr",
    "water_mask": "water_mask.exr",
}


def load_textures(directory):
    """Load all available planet texture files from a directory as Image datablocks."""
    loaded = {}
    for key, filename in TEXTURE_FILES.items():
        filepath = os.path.join(directory, filename)
        if os.path.exists(filepath):
            img = bpy.data.images.load(filepath, check_existing=True)
            img.colorspace_settings.name = "Non-Color" if key != "albedo" else "sRGB"
            loaded[key] = img
    return loaded


# ============ Material builder ============

def build_planet_material(name, textures, engine):
    """Create a Principled BSDF material wired with planet textures."""
    mat = bpy.data.materials.new(name=name)
    mat.use_nodes = True
    tree = mat.node_tree
    tree.nodes.clear()

    # Output
    output = tree.nodes.new("ShaderNodeOutputMaterial")
    output.location = (600, 0)

    # Principled BSDF
    bsdf = tree.nodes.new("ShaderNodeBsdfPrincipled")
    bsdf.location = (200, 0)
    tree.links.new(bsdf.outputs["BSDF"], output.inputs["Surface"])

    x_offset = -400
    y = 400

    # Albedo -> Base Color
    if "albedo" in textures:
        tex = tree.nodes.new("ShaderNodeTexImage")
        tex.image = textures["albedo"]
        tex.location = (x_offset, y)
        tree.links.new(tex.outputs["Color"], bsdf.inputs["Base Color"])

        # AO multiply on albedo
        if "ao" in textures:
            ao_tex = tree.nodes.new("ShaderNodeTexImage")
            ao_tex.image = textures["ao"]
            ao_tex.location = (x_offset - 300, y)

            mix = tree.nodes.new("ShaderNodeMix")
            mix.data_type = "RGBA"
            mix.blend_type = "MULTIPLY"
            mix.location = (x_offset + 200, y)
            mix.inputs[0].default_value = 1.0  # Factor = 1 = full multiply

            tree.links.new(tex.outputs["Color"], mix.inputs[6])  # A
            tree.links.new(ao_tex.outputs["Color"], mix.inputs[7])  # B
            tree.links.new(mix.outputs[2], bsdf.inputs["Base Color"])  # Result
        y -= 300

    # Normal map
    if "normal" in textures:
        tex = tree.nodes.new("ShaderNodeTexImage")
        tex.image = textures["normal"]
        tex.location = (x_offset, y)

        normal_map = tree.nodes.new("ShaderNodeNormalMap")
        normal_map.location = (x_offset + 300, y)
        tree.links.new(tex.outputs["Color"], normal_map.inputs["Color"])
        tree.links.new(normal_map.outputs["Normal"], bsdf.inputs["Normal"])
        y -= 300

    # Roughness
    if "roughness" in textures:
        tex = tree.nodes.new("ShaderNodeTexImage")
        tex.image = textures["roughness"]
        tex.location = (x_offset, y)
        tree.links.new(tex.outputs["Color"], bsdf.inputs["Roughness"])
        y -= 300

    # Height -> Displacement
    if "height" in textures:
        tex = tree.nodes.new("ShaderNodeTexImage")
        tex.image = textures["height"]
        tex.location = (x_offset, y)

        if engine == "CYCLES":
            disp = tree.nodes.new("ShaderNodeDisplacement")
            disp.location = (x_offset + 300, y)
            disp.inputs["Scale"].default_value = 0.1
            disp.inputs["Midlevel"].default_value = 0.5
            tree.links.new(tex.outputs["Color"], disp.inputs["Height"])
            tree.links.new(disp.outputs["Displacement"], output.inputs["Displacement"])
            mat.cycles.displacement_method = "BOTH"
        else:
            # EEVEE: use bump node instead
            bump = tree.nodes.new("ShaderNodeBump")
            bump.location = (x_offset + 300, y)
            bump.inputs["Strength"].default_value = 0.3
            tree.links.new(tex.outputs["Color"], bump.inputs["Height"])
            # Chain with normal map if present
            for link in tree.links:
                if link.to_socket == bsdf.inputs["Normal"]:
                    tree.links.new(link.from_socket, bump.inputs["Normal"])
                    break
            tree.links.new(bump.outputs["Normal"], bsdf.inputs["Normal"])
        y -= 300

    # Emission (city lights)
    if "emission" in textures:
        tex = tree.nodes.new("ShaderNodeTexImage")
        tex.image = textures["emission"]
        tex.location = (x_offset, y)
        tree.links.new(tex.outputs["Color"], bsdf.inputs["Emission Color"])
        bsdf.inputs["Emission Strength"].default_value = 2.0

    return mat


# ============ Operators ============

class PLANETGEN_OT_import(bpy.types.Operator, ImportHelper):
    """Import planet textures from an output directory"""
    bl_idname = "planetgen.import_textures"
    bl_label = "Import Planet"
    bl_options = {"REGISTER", "UNDO"}

    directory: StringProperty(subtype="DIR_PATH")
    filter_folder: bpy.props.BoolProperty(default=True, options={"HIDDEN"})

    def execute(self, context):
        textures = load_textures(self.directory)
        if not textures:
            self.report({"ERROR"}, f"No planet textures found in {self.directory}")
            return {"CANCELLED"}

        context.scene.planetgen_dir = self.directory
        self.report({"INFO"}, f"Loaded {len(textures)} texture(s) from {self.directory}")
        return {"FINISHED"}

    def invoke(self, context, event):
        context.window_manager.fileselect_add(self)
        return {"RUNNING_MODAL"}


class PLANETGEN_OT_create_planet(bpy.types.Operator):
    """Create a new UV sphere with planet material applied"""
    bl_idname = "planetgen.create_planet"
    bl_label = "Create Planet"
    bl_options = {"REGISTER", "UNDO"}

    def execute(self, context):
        directory = context.scene.planetgen_dir
        if not directory or not os.path.isdir(directory):
            self.report({"ERROR"}, "Import planet textures first")
            return {"CANCELLED"}

        textures = load_textures(directory)
        if not textures:
            self.report({"ERROR"}, "No textures found")
            return {"CANCELLED"}

        # Create icosphere
        bpy.ops.mesh.primitive_ico_sphere_add(
            subdivisions=6,
            radius=1.0,
            location=(0, 0, 0),
        )
        obj = context.active_object
        obj.name = "Planet"

        # Add subdivision modifier for smooth surface
        subsurf = obj.modifiers.new(name="Subdivision", type="SUBSURF")
        subsurf.levels = 2
        subsurf.render_levels = 3

        # Set up UVs: use cube projection for equirectangular textures
        bpy.ops.object.mode_set(mode="EDIT")
        bpy.ops.mesh.select_all(action="SELECT")
        bpy.ops.uv.sphere_project()
        bpy.ops.object.mode_set(mode="OBJECT")

        # Build and apply material
        engine = context.scene.render.engine
        mat = build_planet_material("Planet Material", textures, engine)
        obj.data.materials.append(mat)

        # Smooth shading
        bpy.ops.object.shade_smooth()

        self.report({"INFO"}, f"Created planet with {len(textures)} texture layers")
        return {"FINISHED"}


class PLANETGEN_OT_apply_to_selected(bpy.types.Operator):
    """Apply planet material to the selected mesh object"""
    bl_idname = "planetgen.apply_to_selected"
    bl_label = "Apply to Selected"
    bl_options = {"REGISTER", "UNDO"}

    def execute(self, context):
        directory = context.scene.planetgen_dir
        if not directory or not os.path.isdir(directory):
            self.report({"ERROR"}, "Import planet textures first")
            return {"CANCELLED"}

        obj = context.active_object
        if not obj or obj.type != "MESH":
            self.report({"ERROR"}, "Select a mesh object first")
            return {"CANCELLED"}

        textures = load_textures(directory)
        if not textures:
            self.report({"ERROR"}, "No textures found")
            return {"CANCELLED"}

        engine = context.scene.render.engine
        mat = build_planet_material(f"{obj.name} Planet", textures, engine)

        # Replace or append material
        if obj.data.materials:
            obj.data.materials[0] = mat
        else:
            obj.data.materials.append(mat)

        self.report({"INFO"}, f"Applied planet material to {obj.name}")
        return {"FINISHED"}


# ============ Panel ============

class PLANETGEN_PT_panel(bpy.types.Panel):
    bl_label = "Planet Gen"
    bl_idname = "PLANETGEN_PT_panel"
    bl_space_type = "VIEW_3D"
    bl_region_type = "UI"
    bl_category = "Planet Gen"

    def draw(self, context):
        layout = self.layout
        scene = context.scene

        # Import section
        layout.label(text="Import", icon="IMPORT")
        layout.operator("planetgen.import_textures", icon="FILE_FOLDER")
        if scene.planetgen_dir:
            box = layout.box()
            box.label(text=os.path.basename(scene.planetgen_dir.rstrip("/\\")), icon="CHECKMARK")
            # Show which textures are available
            textures = load_textures(scene.planetgen_dir)
            for key in sorted(textures.keys()):
                box.label(text=f"  {key}", icon="IMAGE_DATA")

        layout.separator()

        # Create / Apply section
        layout.label(text="Setup", icon="WORLD")
        row = layout.row(align=True)
        row.operator("planetgen.create_planet", icon="MESH_UVSPHERE")
        row = layout.row(align=True)
        row.operator("planetgen.apply_to_selected", icon="MATERIAL")
        row.enabled = context.active_object is not None and context.active_object.type == "MESH"


# ============ Registration ============

classes = (
    PLANETGEN_OT_import,
    PLANETGEN_OT_create_planet,
    PLANETGEN_OT_apply_to_selected,
    PLANETGEN_PT_panel,
)


def register():
    for cls in classes:
        bpy.utils.register_class(cls)
    bpy.types.Scene.planetgen_dir = StringProperty(
        name="Planet Directory",
        description="Path to the imported planet texture directory",
        default="",
        subtype="DIR_PATH",
    )


def unregister():
    for cls in reversed(classes):
        bpy.utils.unregister_class(cls)
    del bpy.types.Scene.planetgen_dir


if __name__ == "__main__":
    register()
