#!/usr/bin/env python3
import argparse
import hashlib
import json
import math
import os
import shutil
import subprocess
import sys
import time
import uuid
from pathlib import Path


REPOSITORY = Path(__file__).resolve().parents[3]
NATIVE = REPOSITORY / "target/terrain-diffusion-native"
REFERENCE = NATIVE / "pytorch-real-reference"
OUTPUT_ROOT = REPOSITORY / "target/terrain-diffusion-libtorch"
EXPECTED_FIXTURE_MANIFEST = (
    "c10d628d4bbd8ac524ad4b9c3a00a2abe4493f7252b5451bc52dcc138081973c"
)
EXPECTED_ELEVATION = "990f8e0ded02f2cde99727a9ce4a22fa88168c221cbe74849bae1dbeb91532e0"
EXPECTED_PINS = {
    "source_commit": "6d770c943cf18f6732a7b15bf95f667cacf1ca17",
    "source_tree": "12a2f7add9211b985b1db34808c081b8269f9536",
    "source_exporter_blob": "affe13d4e6db6d461230ff232caecf3119286cbc",
    "source_requirements_blob": "35d7711000cddc7375d7a3ca0e81d696c4dd110d",
    "stats": "0d2578c765a3cc4d21b58994fcd40d0dc0f657b41ad67ab6a5fe45cf6db535e4",
    "tools/requirements-hashed.txt": "83fac52a39d3b5543bde55b9fdd481ceb776e9d7c329c9e5a7d0f54f00d0db32",
    "tools/uv.lock": "2bfe78f234bdd7e55c81a44e71a283043b89d6e84b0a303d5d4c67d2b9d424c1",
    "weights/config.json": "6c12c857e6d2cea349b5dba116da94cb17680c04361948b167d583e0347443cd",
    "weights/coarse_model/config.json": "00761cab3f2156547694d9cd7c062b51face2c4c4add4fd0b27586775d836937",
    "weights/coarse_model/diffusion_pytorch_model.safetensors": "13c21db4581d2072db56fd76fe3b92fa2d5efa8805be2aae0b63448b5d28ac5c",
    "weights/base_model/config.json": "fc0fe1f77e3cc41849a20ac7b8a35853b2ab9523a9db36f45fd06afa348923b1",
    "weights/base_model/diffusion_pytorch_model.safetensors": "e426277db86517335d4b0bc02b3d456bb812b2a8726ea010635686f77373be36",
    "weights/decoder_model/config.json": "3aad7a2c6903002120fac168bf4c8102c9c62bc314993e50fc09422d3f23f92d",
    "weights/decoder_model/diffusion_pytorch_model.safetensors": "b6c7fa99f836ad75c514236c9529e18a68ea207ed59dd39fd1341fc9a8a03bcc",
}
EXPECTED_PINSET = "e909cdaa0379a58732ed33850a4a018acddf5da2987fd40b512fe4c18482d6d7"
EXPECTED_ENV = {
    "python": "3.11.15",
    "torch": "2.4.1+cu121",
    "cuda": "12.1",
    "cudnn": 90100,
    "cxx11_abi": False,
}
EXPECTED_COUNTS = {"coarse_model": 80, "base_model": 74, "decoder_model": 4}
SCHEMAS = {
    "coarse_model": (
        [
            "x",
            "noise_labels",
            "conditional_inputs.0",
            "conditional_inputs.1",
            "conditional_inputs.2",
            "conditional_inputs.3",
            "conditional_inputs.4",
        ],
        [[1, 11, 64, 64], [1], [1], [1], [1], [1], [1]],
        ["output"],
        [[1, 6, 64, 64]],
    ),
    "base_model": (
        ["x", "noise_labels", "conditional_inputs.0"],
        [[1, 5, 64, 64], [1], [1, 58]],
        ["output"],
        [[1, 5, 64, 64]],
    ),
    "decoder_model": (
        ["x", "noise_labels"],
        [[1, 5, 512, 512], [1]],
        ["output"],
        [[1, 1, 512, 512]],
    ),
}
GATES = {
    "normalized_max": 5e-5,
    "nmae": 1e-5,
    "nrmse": 1.5e-5,
    "normalized_p99": 4e-5,
    "normalized_bias": 1e-5,
    "cosine": 0.99999999,
    "masked_relative_p99": 2.5e-4,
    "absolute_max": 5e-4,
}


def sha256(path_or_bytes):
    digest = hashlib.sha256()
    if isinstance(path_or_bytes, Path):
        with path_or_bytes.open("rb") as handle:
            for block in iter(lambda: handle.read(1024 * 1024), b""):
                digest.update(block)
    else:
        digest.update(path_or_bytes)
    return digest.hexdigest()


def command(*args, cwd):
    return subprocess.check_output(args, cwd=cwd, text=True).strip()


def safe_child(path, parent):
    if path.is_symlink() or parent.is_symlink():
        raise RuntimeError(f"unsafe symlink: {path}")
    resolved = path.resolve(strict=True)
    if resolved.parent != parent.resolve():
        raise RuntimeError(f"unsafe path: {path}")
    return resolved


def file_hash(path, expected):
    if not path.is_file() or path.is_symlink() or sha256(path) != expected:
        raise RuntimeError(f"pin mismatch: {path}")


def env_record():
    interpreter = NATIVE / "env/bin/python"
    if Path(sys.executable).resolve() != interpreter.resolve():
        raise RuntimeError(f"managed interpreter required: {interpreter}")
    result = subprocess.check_output(
        [
            str(interpreter),
            "-c",
            "import json,os,platform; assert os.environ.get('CUBLAS_WORKSPACE_CONFIG') == ':4096:8'; import torch; torch.use_deterministic_algorithms(True); torch.backends.cudnn.benchmark=False; torch.backends.cudnn.deterministic=True; torch.backends.cuda.matmul.allow_tf32=False; torch.backends.cudnn.allow_tf32=False; torch.set_float32_matmul_precision('highest'); print(json.dumps({'python':platform.python_version(),'torch':torch.__version__,'cuda':torch.version.cuda,'cudnn':torch.backends.cudnn.version(),'cxx11_abi':torch.compiled_with_cxx11_abi(),'controls':[os.environ['CUBLAS_WORKSPACE_CONFIG'],torch.are_deterministic_algorithms_enabled(),torch.backends.cudnn.benchmark,torch.backends.cudnn.deterministic,torch.backends.cuda.matmul.allow_tf32,torch.backends.cudnn.allow_tf32,torch.get_float32_matmul_precision()]}))",
        ],
        text=True,
    )
    record = json.loads(result)
    controls = record.pop("controls")
    if record != EXPECTED_ENV or controls != [
        ":4096:8",
        True,
        False,
        True,
        False,
        False,
        "highest",
    ]:
        raise RuntimeError(f"environment mismatch: {record}")
    return {**record, "controls": controls}


def validate_fixture():
    manifest_path = safe_child(REFERENCE / "manifest.json", REFERENCE)
    if sha256(manifest_path) != EXPECTED_FIXTURE_MANIFEST:
        raise RuntimeError("fixture manifest hash mismatch")
    manifest = json.loads(manifest_path.read_text())
    first = manifest.get("first", {})
    if manifest.get("status") != "PYTORCH_REAL_REFERENCE_PASS" or not manifest.get(
        "deterministic"
    ):
        raise RuntimeError("fixture is not accepted")
    counts = {
        graph: sum(call.get("graph") == graph for call in first.get("calls", []))
        for graph in EXPECTED_COUNTS
    }
    if counts != EXPECTED_COUNTS or first.get("call_counts") != counts:
        raise RuntimeError("fixture call-count mismatch")
    elevation = safe_child(REFERENCE / "elevation.f32", REFERENCE)
    if sha256(elevation) != EXPECTED_ELEVATION:
        raise RuntimeError("fixture elevation hash mismatch")
    tensors = safe_child(REFERENCE / "tensors", REFERENCE)
    expected = set()
    calls = first.get("calls", [])
    if [call.get("ordinal") for call in calls] != list(range(len(calls))):
        raise RuntimeError("fixture ordinal mismatch")
    for call in calls:
        feeds, outputs = call.get("feeds", []), call.get("outputs", [])
        names, shapes, output_names, output_shapes = SCHEMAS.get(
            call.get("graph"), (None, None, None, None)
        )
        if (
            [item.get("name") for item in feeds],
            [item.get("shape") for item in feeds],
            [item.get("name") for item in outputs],
            [item.get("shape") for item in outputs],
        ) != (names, shapes, output_names, output_shapes):
            raise RuntimeError("frozen graph schema mismatch")
        if len(feeds) < 2 or [item["name"] for item in feeds[:2]] != [
            "x",
            "noise_labels",
        ]:
            raise RuntimeError("fixture feed schema mismatch")
        identity = call.get("semantic_identity")
        noise = safe_child(tensors / f"{identity}.feeds.1.f32", tensors).read_bytes()
        payload = {
            "graph": call["graph"],
            "stage": call["stage"],
            "feeds": feeds,
            "noise_label_f32_bits": [
                noise[offset : offset + 4].hex() for offset in range(0, len(noise), 4)
            ],
        }
        if (
            sha256(json.dumps(payload, sort_keys=True, separators=(",", ":")).encode())
            != identity
        ):
            raise RuntimeError("fixture semantic identity mismatch")
        for group, records in (("feeds", feeds), ("outputs", outputs)):
            for index, item in enumerate(records):
                name = f"{identity}.{group}.{index}.f32"
                path = safe_child(tensors / name, tensors)
                if (
                    path.stat().st_size != 4 * math.prod(item["shape"])
                    or sha256(path) != item["sha256"]
                ):
                    raise RuntimeError(f"fixture tensor mismatch: {name}")
                expected.add(name)
    actual = {item.name for item in tensors.iterdir()}
    if actual != expected or any(item.is_symlink() for item in tensors.iterdir()):
        raise RuntimeError("fixture tensor set mismatch")
    if (
        sum(item.stat().st_size for item in tensors.iterdir())
        + elevation.stat().st_size
        != 59853768
    ):
        raise RuntimeError("fixture byte total mismatch")
    return manifest


def validate_before_imports():
    if os.environ.get("CUBLAS_WORKSPACE_CONFIG") not in (None, ":4096:8"):
        raise RuntimeError("CUBLAS_WORKSPACE_CONFIG conflicts with fixed control")
    os.environ["CUBLAS_WORKSPACE_CONFIG"] = ":4096:8"
    if shutil.disk_usage(OUTPUT_ROOT.parent).free < 5 * 1024**3:
        raise RuntimeError("at least 5 GiB free space is required")
    source = safe_child(NATIVE / "source", NATIVE)
    if (
        command("git", "rev-parse", "HEAD", cwd=source)
        != EXPECTED_PINS["source_commit"]
    ):
        raise RuntimeError("source commit mismatch")
    if (
        command("git", "rev-parse", "HEAD^{tree}", cwd=source)
        != EXPECTED_PINS["source_tree"]
    ):
        raise RuntimeError("source tree mismatch")
    if command("git", "status", "--porcelain", cwd=source):
        raise RuntimeError("pinned source is dirty")
    for relative, expected in (
        ("terrain_diffusion/onnx/export.py", EXPECTED_PINS["source_exporter_blob"]),
        ("requirements.txt", EXPECTED_PINS["source_requirements_blob"]),
    ):
        if command("git", "hash-object", relative, cwd=source) != expected:
            raise RuntimeError(f"source blob mismatch: {relative}")
    for key, expected in EXPECTED_PINS.items():
        if key.startswith("weights/"):
            file_hash(NATIVE / key, expected)
        if key.startswith("tools/"):
            file_hash(REPOSITORY / "spikes/terrain-diffusion-native" / key, expected)
    file_hash(source / "data/global/synthetic_map_stats.json", EXPECTED_PINS["stats"])
    if sha256(json.dumps(EXPECTED_PINS, sort_keys=True).encode()) != EXPECTED_PINSET:
        raise RuntimeError("pin-set constant mismatch")
    return {
        "environment": env_record(),
        "fixture": validate_fixture(),
        "source": source,
    }


def load_array(path, shape, np):
    value = np.frombuffer(safe_child(path, path.parent).read_bytes(), dtype="<f4")
    if value.size != math.prod(shape):
        raise RuntimeError(f"tensor shape mismatch: {path}")
    return value.reshape(shape).copy()


def inputs_for(call, np, torch):
    tensors = REFERENCE / "tensors"
    values = [
        torch.from_numpy(
            load_array(
                tensors / f"{call['semantic_identity']}.feeds.{index}.f32",
                item["shape"],
                np,
            )
        ).cuda()
        for index, item in enumerate(call["feeds"])
    ]
    return tuple(values)


def enforce_torch_controls(torch):
    torch.use_deterministic_algorithms(True)
    torch.backends.cudnn.benchmark = False
    torch.backends.cudnn.deterministic = True
    torch.backends.cuda.matmul.allow_tf32 = False
    torch.backends.cudnn.allow_tf32 = False
    torch.set_float32_matmul_precision("highest")
    controls = [
        os.environ.get("CUBLAS_WORKSPACE_CONFIG"),
        torch.are_deterministic_algorithms_enabled(),
        torch.backends.cudnn.benchmark,
        torch.backends.cudnn.deterministic,
        torch.backends.cuda.matmul.allow_tf32,
        torch.backends.cudnn.allow_tf32,
        torch.get_float32_matmul_precision(),
    ]
    if controls != [":4096:8", True, False, True, False, False, "highest"]:
        raise RuntimeError(f"unable to enforce deterministic controls: {controls}")
    return controls


def canonical_cpu(tensor, torch, np):
    torch.cuda.synchronize()
    value = tensor.detach().to(device="cpu", dtype=torch.float32).contiguous().clone()
    raw = value.numpy().astype("<f4", copy=False).tobytes()
    return np.frombuffer(raw, dtype="<f4").reshape(value.shape).copy(), raw


def wrappers(torch, models):
    class Coarse(torch.nn.Module):
        def __init__(self, model):
            super().__init__()
            self.model = model

        def forward(self, x, noise, c0, c1, c2, c3, c4):
            return self.model(
                x, noise_labels=noise, conditional_inputs=[c0, c1, c2, c3, c4]
            )

    class Base(torch.nn.Module):
        def __init__(self, model):
            super().__init__()
            self.model = model

        def forward(self, x, noise, cond58):
            return self.model(x, noise_labels=noise, conditional_inputs=[cond58])

    class Decoder(torch.nn.Module):
        def __init__(self, model):
            super().__init__()
            self.model = model

        def forward(self, x, noise):
            return self.model(x, noise_labels=noise, conditional_inputs=[])

    return {
        "coarse_model": Coarse(models["coarse_model"]),
        "base_model": Base(models["base_model"]),
        "decoder_model": Decoder(models["decoder_model"]),
    }


def metric(reference, actual, np):
    reference, actual = (
        reference.astype("<f4", copy=False),
        actual.astype("<f4", copy=False),
    )
    if reference.shape != actual.shape or not np.isfinite(actual).all():
        raise RuntimeError("non-finite or schema mismatch")
    error = actual.astype(np.float64) - reference.astype(np.float64)
    difference = np.abs(error)
    scale = float(np.sqrt(np.mean(np.square(reference))))
    mask = np.abs(reference) >= max(1e-2, 1e-3 * scale)
    relative = difference[mask] / np.abs(reference[mask])
    dot = float(
        np.dot(reference.ravel().astype(np.float64), actual.ravel().astype(np.float64))
    )
    denom = float(np.linalg.norm(reference.ravel()) * np.linalg.norm(actual.ravel()))
    return {
        "scale": scale,
        "normalized_max": float(difference.max() / scale),
        "nmae": float(difference.mean() / scale),
        "nrmse": float(np.sqrt(np.mean(np.square(difference))) / scale),
        "normalized_p99": float(np.quantile(difference, 0.99) / scale),
        "normalized_bias": float(abs(error.mean()) / scale),
        "cosine": min(1.0, dot / denom) if denom else 1.0,
        "masked_relative_p99": float(np.quantile(relative, 0.99))
        if relative.size
        else 0.0,
        "mask_coverage": int(mask.sum()),
        "mask_required": max(256, int(math.ceil(mask.size * 0.01))),
        "absolute_max": float(difference.max()),
        "allclose_1e-5": bool(np.allclose(reference, actual, rtol=1e-5, atol=1e-5)),
    }


def passes(value):
    return (
        value["scale"] >= 1e-6
        and value["normalized_max"] <= GATES["normalized_max"]
        and value["nmae"] <= GATES["nmae"]
        and value["nrmse"] <= GATES["nrmse"]
        and value["normalized_p99"] <= GATES["normalized_p99"]
        and value["normalized_bias"] <= GATES["normalized_bias"]
        and value["cosine"] >= GATES["cosine"]
        and value["masked_relative_p99"] <= GATES["masked_relative_p99"]
        and value["mask_coverage"] >= value["mask_required"]
        and value["absolute_max"] <= GATES["absolute_max"]
    )


def worker(stage, module_paths, label):
    record = validate_before_imports()
    import numpy as np
    import torch

    if not torch.cuda.is_available():
        raise RuntimeError("CUDA is required")
    controls = enforce_torch_controls(torch)
    started = time.perf_counter()
    torch.cuda.reset_peak_memory_stats()
    modules = {
        name: torch.jit.load(str(path), map_location="cuda").eval()
        for name, path in module_paths.items()
    }
    metrics_by_graph, raw_by_graph, failures, call_checks, exact = {}, {}, [], [], True
    for call in record["fixture"]["first"]["calls"]:
        module = modules[call["graph"]]
        args = inputs_for(call, np, torch)
        if [tuple(value.shape) for value in args] != [
            tuple(shape) for shape in SCHEMAS[call["graph"]][1]
        ] or any(value.dtype != torch.float32 for value in args):
            raise RuntimeError("frozen input dtype/schema mismatch")
        with torch.inference_mode():
            first = module(*args)
            first_np, first_raw = canonical_cpu(first, torch, np)
            second = module(*args)
            second_np, second_raw = canonical_cpu(second, torch, np)
        repeat_max_diff = float(
            np.max(np.abs(first_np.astype(np.float64) - second_np.astype(np.float64)))
        )
        repeat_exact = first_raw == second_raw
        exact = exact and repeat_exact
        expected = call["outputs"]
        if (
            len(expected) != 1
            or [item["name"] for item in expected] != SCHEMAS[call["graph"]][2]
            or [item["shape"] for item in expected] != SCHEMAS[call["graph"]][3]
            or tuple(first_np.shape) != tuple(expected[0]["shape"])
            or first_np.dtype != np.dtype("<f4")
        ):
            failures.append(
                {
                    "identity": call["semantic_identity"],
                    "error": "output schema mismatch",
                }
            )
            continue
        reference = load_array(
            REFERENCE / "tensors" / f"{call['semantic_identity']}.outputs.0.f32",
            expected[0]["shape"],
            np,
        )
        channels = [
            metric(reference[:, channel], first_np[:, channel], np)
            for channel in range(reference.shape[1])
        ]
        metrics_by_graph.setdefault(call["graph"], []).append(channels)
        raw_by_graph.setdefault(call["graph"], [[], []])
        raw_by_graph[call["graph"]][0].append(reference.ravel())
        raw_by_graph[call["graph"]][1].append(first_np.ravel())
        call_checks.append(
            {
                "identity": call["semantic_identity"],
                "graph": call["graph"],
                "first_sha256": sha256(first_raw),
                "second_sha256": sha256(second_raw),
                "repeat_exact": repeat_exact,
                "repeat_max_diff": repeat_max_diff,
                "channel_pass": all(passes(value) for value in channels),
            }
        )
        if not all(passes(value) for value in channels):
            failures.append(
                {"identity": call["semantic_identity"], "channels": channels}
            )
    pooled = {}
    for graph, values in metrics_by_graph.items():
        reference, actual = (np.concatenate(parts) for parts in raw_by_graph[graph])
        pooled[graph] = metric(reference, actual, np)
        pooled[graph]["calls"] = len(values)
        pooled[graph]["allclose_1e-5_channels"] = sum(
            item["allclose_1e-5"] for call in values for item in call
        )
        if not passes(pooled[graph]):
            failures.append({"graph": graph, "pooled_metrics": pooled[graph]})
    torch.cuda.synchronize()
    output_hash = hashlib.sha256()
    for graph in sorted(raw_by_graph):
        output_hash.update(graph.encode())
        output_hash.update(
            np.concatenate(raw_by_graph[graph][1]).astype("<f4", copy=False).tobytes()
        )
    return {
        "label": label,
        "exact_repeated_hashes": exact,
        "first_pass_aggregate_sha256": output_hash.hexdigest(),
        "call_counts": {
            graph: len(values) for graph, values in metrics_by_graph.items()
        },
        "metrics": pooled,
        "failures": failures,
        "per_call": call_checks,
        "wall_seconds": time.perf_counter() - started,
        "rss_bytes": int(open("/proc/self/statm").read().split()[1])
        * os.sysconf("SC_PAGE_SIZE"),
        "gpu_memory_bytes": torch.cuda.max_memory_allocated(),
        "environment": record["environment"],
        "controls": controls,
    }


def child_worker(stage, module_paths, label):
    output = stage / f"{label}.json"
    completed = subprocess.run(
        [
            sys.executable,
            str(Path(__file__).resolve()),
            "--worker",
            "--stage",
            str(stage),
            "--modules",
            json.dumps({key: str(value) for key, value in module_paths.items()}),
            "--label",
            label,
        ],
        capture_output=True,
        text=True,
    )
    if completed.returncode:
        raise RuntimeError(
            f"worker {label} failed: {completed.stderr or completed.stdout}"
        )
    return json.loads(output.read_text())


def export(stage):
    record = validate_before_imports()
    import torch

    controls = enforce_torch_controls(torch)

    sys.path.insert(0, str(NATIVE / "source"))
    from terrain_diffusion.inference.world_pipeline import WorldPipeline

    if not torch.cuda.is_available():
        raise RuntimeError("CUDA is required")
    source_before = command("git", "status", "--porcelain", cwd=NATIVE / "source")
    world = WorldPipeline.from_pretrained(
        str(NATIVE / "weights"),
        seed=1,
        latents_batch_size=1,
        torch_compile=False,
        dtype=None,
        caching_strategy="direct",
    )
    world.to("cuda")
    models = {
        "coarse_model": world.coarse_model.eval(),
        "base_model": world.base_model.eval(),
        "decoder_model": world.decoder_model.eval(),
    }
    wrapped = wrappers(torch, models)
    module_paths = {}
    for graph, module in wrapped.items():
        calls = [
            call
            for call in record["fixture"]["first"]["calls"]
            if call["graph"] == graph
        ]
        primary, alternate = (
            inputs_for(calls[0], __import__("numpy"), torch),
            inputs_for(calls[1], __import__("numpy"), torch),
        )
        with torch.inference_mode():
            traced = torch.jit.trace(
                module.eval(),
                primary,
                strict=True,
                check_trace=True,
                check_inputs=[alternate],
            )
        incomplete = stage / f"{graph}.pt.{uuid.uuid4().hex}.incomplete"
        torch.jit.save(traced, str(incomplete))
        final = stage / f"{graph}.pt"
        os.replace(incomplete, final)
        module_paths[graph] = final
    if command("git", "status", "--porcelain", cwd=NATIVE / "source") != source_before:
        raise RuntimeError("pinned source mutated by export")
    operators = {
        name: torch.jit.export_opnames(torch.jit.load(str(path), map_location="cuda"))
        for name, path in module_paths.items()
    }
    if any(
        any("PythonOp" in operator for operator in values)
        for values in operators.values()
    ):
        raise RuntimeError("custom Python operator in traced graph")
    return (
        module_paths,
        operators,
        {"environment": record["environment"], "controls": controls},
    )


def run(stage):
    module_paths, operators, export_controls = export(stage)
    first = child_worker(stage, module_paths, "first")
    second = child_worker(stage, module_paths, "second")
    post_replay = validate_before_imports()
    hashes = {
        name: {"sha256": sha256(path), "bytes": path.stat().st_size}
        for name, path in module_paths.items()
    }
    result = {
        "status": "TORCHSCRIPT_PYTHON_REPLAY_PASS",
        "modules": hashes,
        "operators": operators,
        "first": first,
        "second": second,
        "first_pass_cross_worker_exact": first["first_pass_aggregate_sha256"]
        == second["first_pass_aggregate_sha256"],
        "export_controls": export_controls,
        "post_replay_validation": {
            "environment": post_replay["environment"],
            "script_sha256": sha256(Path(__file__).resolve()),
            "command": sys.argv,
            "cublas_workspace_config": os.environ["CUBLAS_WORKSPACE_CONFIG"],
        },
    }
    if (
        not first["exact_repeated_hashes"]
        or not second["exact_repeated_hashes"]
        or first["failures"]
        or second["failures"]
        or not result["first_pass_cross_worker_exact"]
    ):
        result["status"] = "TORCHSCRIPT_PYTHON_REPLAY_NO_GO"
    (stage / "report.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n"
    )
    return result


def write_index(stage):
    files = [
        {
            "path": str(path.relative_to(stage)),
            "bytes": path.stat().st_size,
            "sha256": sha256(path),
        }
        for path in sorted(stage.rglob("*"))
        if path.is_file() and not path.is_symlink() and path.name != "index.json"
    ]
    (stage / "index.json").write_text(
        json.dumps({"files": files}, indent=2, sort_keys=True) + "\n"
    )
    return sha256(stage / "index.json")


def self_check():
    assert (
        sha256(b"abc")
        == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    )
    assert EXPECTED_COUNTS == {"coarse_model": 80, "base_model": 74, "decoder_model": 4}
    assert sha256(json.dumps(EXPECTED_PINS, sort_keys=True).encode()) == EXPECTED_PINSET
    assert {graph: len(values[0]) for graph, values in SCHEMAS.items()} == {
        "coarse_model": 7,
        "base_model": 3,
        "decoder_model": 2,
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-check", action="store_true")
    parser.add_argument("--worker", action="store_true")
    parser.add_argument("--stage")
    parser.add_argument("--modules")
    parser.add_argument("--label")
    args = parser.parse_args()
    if args.self_check:
        self_check()
        return
    if args.worker:
        stage = Path(args.stage).resolve(strict=True)
        report = worker(
            stage,
            {
                key: Path(value).resolve(strict=True)
                for key, value in json.loads(args.modules).items()
            },
            args.label,
        )
        (stage / f"{args.label}.json").write_text(
            json.dumps(report, indent=2, sort_keys=True) + "\n"
        )
        return
    OUTPUT_ROOT.mkdir(parents=True, exist_ok=True)
    stage = OUTPUT_ROOT / f"stage-a.{uuid.uuid4().hex}.incomplete"
    stage.mkdir()
    try:
        result = run(stage)
        if result["status"] != "TORCHSCRIPT_PYTHON_REPLAY_PASS":
            raise RuntimeError(result["status"])
        write_index(stage)
        final = OUTPUT_ROOT / "stage-a"
        if final.exists():
            raise RuntimeError("published stage-a already exists")
        os.replace(stage, final)
        print(json.dumps({"status": result["status"], "evidence": str(final)}))
    except Exception as error:
        (stage / "failure.txt").write_text(f"{type(error).__name__}: {error}\n")
        print(
            json.dumps(
                {
                    "status": "TORCHSCRIPT_PYTHON_REPLAY_NO_GO",
                    "evidence": str(stage),
                    "index_sha256": write_index(stage),
                    "error": str(error),
                }
            )
        )
        raise SystemExit(3)


if __name__ == "__main__":
    main()
