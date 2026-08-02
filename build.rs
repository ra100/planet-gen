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
        build.include(include);
        println!("cargo:rustc-link-search=native={lib}");
        println!("cargo:rustc-link-lib=OpenEXR");
        println!("cargo:rustc-link-lib=Imath");
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
