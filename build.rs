/// OpenEXR headers reference Imath headers unqualified (e.g.
/// `#include "ImathBox.h"`), so the Imath include directory must be on the
/// search path in addition to the prefix or OpenEXR directory that was given.
/// Covers both layouts: a prefix containing `OpenEXR/` + `Imath/` (vcpkg,
/// Homebrew) and a directory pointing straight at the OpenEXR headers with
/// Imath as a sibling (system installs).
fn add_imath_include(build: &mut cc::Build, include_dir: &str) {
    let root = std::path::Path::new(include_dir);
    let candidates = [
        Some(root.join("Imath")),
        root.parent().map(|parent| parent.join("Imath")),
    ];
    for candidate in candidates.into_iter().flatten() {
        if candidate.join("ImathBox.h").exists() {
            build.include(candidate);
        }
    }
}

/// Resolves the linker name for a library base (e.g. "OpenEXR") by scanning
/// `lib_dir`. OpenEXR's CMake appends a version suffix to output names by
/// default (`OPENEXR_LIB_SUFFIX`), so vcpkg installs e.g. `OpenEXR-3_4.lib`
/// instead of `OpenEXR.lib`; plain `-lOpenEXR` would not find it. An exact
/// name wins over a suffixed one when both are present.
fn link_name_for(base: &str, lib_dir: &str) -> String {
    let mut suffixed: Option<String> = None;
    if let Ok(entries) = std::fs::read_dir(lib_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            // Reduce to the linker-visible stem: OpenEXR-3_4.lib -> OpenEXR-3_4,
            // libOpenEXR.so.34 -> libOpenEXR, libImath.a -> Imath.
            let stem = name
                .strip_suffix(".lib")
                .or_else(|| name.strip_suffix(".a"))
                .map(str::to_string)
                .or_else(|| name.split_once(".so").map(|(stem, _)| stem.to_string()))
                .unwrap_or(name);
            let stem = stem.strip_prefix("lib").unwrap_or(&stem).to_string();
            if stem.eq_ignore_ascii_case(base) {
                return base.to_string();
            }
            if suffixed.is_none()
                && let Some(rest) = stem.strip_prefix(base)
                && rest.starts_with('-')
                && !rest[1..].is_empty()
            {
                suffixed = Some(stem);
            }
        }
    }
    suffixed.unwrap_or_else(|| base.to_string())
}

fn main() {
    println!("cargo:rerun-if-changed=src/openexr_writer.cpp");
    println!("cargo:rerun-if-env-changed=OPENEXR_INCLUDE_DIR");
    println!("cargo:rerun-if-env-changed=OPENEXR_LIB_DIR");

    let mut build = cc::Build::new();
    build.cpp(true).file("src/openexr_writer.cpp");
    if let Ok(library) = pkg_config::Config::new().probe("OpenEXR") {
        for path in library.include_paths {
            build.include(path);
        }
    } else if let (Ok(include), Ok(lib)) = (
        std::env::var("OPENEXR_INCLUDE_DIR"),
        std::env::var("OPENEXR_LIB_DIR"),
    ) {
        build.include(&include);
        add_imath_include(&mut build, &include);
        println!("cargo:rustc-link-search=native={lib}");
        for base in ["OpenEXR", "Imath"] {
            println!("cargo:rustc-link-lib={}", link_name_for(base, &lib));
        }
    } else if std::path::Path::new("/usr/include/OpenEXR/ImfOutputFile.h").exists() {
        println!(
            "cargo:warning=OpenEXR pkg-config metadata missing; using system include fallback. Set OPENEXR_INCLUDE_DIR and OPENEXR_LIB_DIR for a nonstandard installation."
        );
        build
            .include("/usr/include/OpenEXR")
            .include("/usr/include/Imath");
        println!("cargo:rustc-link-lib=OpenEXR");
        println!("cargo:rustc-link-lib=Imath");
    } else {
        panic!(
            "OpenEXR development files not found. Install OpenEXR with pkg-config support, or set OPENEXR_INCLUDE_DIR and OPENEXR_LIB_DIR."
        );
    }
    build.compile("planet_gen_openexr");
}
