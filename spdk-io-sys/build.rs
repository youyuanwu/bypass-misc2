//! Build script for spdk-io-sys
//!
//! Uses pkg-config to find SPDK installation and generates Rust bindings via bindgen.
//! Links statically against SPDK/DPDK libraries with --whole-archive.
//!
//! Environment variables:
//! - `PKG_CONFIG_PATH`: Must include SPDK's pkg-config directory (e.g., /opt/spdk/lib/pkgconfig)

use std::collections::HashSet;
use std::env;
use std::path::PathBuf;

/// System libraries that should be linked dynamically (not with --whole-archive)
const SYSTEM_LIBS: &[&str] = &[
    "crypto",
    "ssl",
    "numa",
    "uuid",
    "aio",
    "dl",
    "m",
    "rt",
    "pthread",
    "uring",
    "pcap",
    "ibverbs",
    "rdmacm",
    "mlx5",
    "keyutils",
    "isal",
    "isal_crypto",
];

fn is_system_lib(name: &str) -> bool {
    SYSTEM_LIBS.contains(&name)
}

/// Check if this is a colon-prefixed archive name like `:librte_foo.a`
/// These are duplicates of the regular lib names and should be skipped
fn is_archive_name(name: &str) -> bool {
    name.starts_with(':') || name.ends_with(".a")
}

fn main() {
    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");

    // Core SPDK libraries we need
    let spdk_libs = [
        "spdk_env_dpdk",
        "spdk_thread",
        "spdk_bdev",
        "spdk_blob",
        "spdk_blob_bdev",
        "spdk_nvme",
        "spdk_log",
        "spdk_util",
        "spdk_json",
        "spdk_rpc",
        "spdk_jsonrpc",
        "spdk_event",
        "spdk_bdev_malloc",
        "spdk_bdev_null",
        "libdpdk", // Include for Libs.private (numa, dl, m, pthread)
    ];

    // Use pkg-config to get include paths and link flags (static mode)
    let mut include_paths = Vec::new();
    let mut link_paths = HashSet::new();
    let mut spdk_dpdk_libs = HashSet::new();
    let mut system_libs = HashSet::new();

    for lib in &spdk_libs {
        let library = pkg_config::Config::new()
            .statik(true)  // Request static libraries
            .env_metadata(true)
            .probe(lib)
            .unwrap_or_else(|e| panic!("Failed to find {}: {}. Set PKG_CONFIG_PATH to include SPDK's pkg-config directory.", lib, e));

        for path in &library.include_paths {
            if !include_paths.contains(path) {
                include_paths.push(path.clone());
            }
        }
        for path in &library.link_paths {
            link_paths.insert(path.clone());
        }
        for lib_name in &library.libs {
            // Skip colon-prefixed archive names like `:librte_foo.a` - these are duplicates
            if is_archive_name(lib_name) {
                continue;
            }
            if is_system_lib(lib_name) {
                system_libs.insert(lib_name.clone());
            } else {
                spdk_dpdk_libs.insert(lib_name.clone());
            }
        }
    }

    // System dependencies not in SPDK's pkg-config - probe via pkg-config
    let extra_system_pkgs = [
        ("libssl", "ssl"),                 // OpenSSL
        ("libcrypto", "crypto"),           // OpenSSL crypto
        ("libisal", "isal"),               // ISA-L (may be at /opt/spdk/lib/pkgconfig)
        ("libisal_crypto", "isal_crypto"), // ISA-L crypto
        ("uuid", "uuid"),                  // libuuid
    ];
    for (pkg_name, lib_name) in &extra_system_pkgs {
        if pkg_config::Config::new()
            .statik(true)
            .probe(pkg_name)
            .is_ok()
        {
            // pkg-config handled it
        } else {
            // Fallback to direct linking
            system_libs.insert(lib_name.to_string());
        }
    }
    // libaio doesn't have pkg-config
    system_libs.insert("aio".to_string());

    // Emit link search paths
    for path in &link_paths {
        println!("cargo:rustc-link-search=native={}", path.display());
    }

    // Link SPDK/DPDK static libs with --whole-archive to include all symbols
    // This ensures callback tables and other statically-initialized data are included
    println!("cargo:rustc-link-arg=-Wl,--whole-archive");
    for lib in &spdk_dpdk_libs {
        println!("cargo:rustc-link-lib=static={}", lib);
    }
    println!("cargo:rustc-link-arg=-Wl,--no-whole-archive");

    // Link system libraries normally (dynamic)
    for lib in &system_libs {
        println!("cargo:rustc-link-lib={}", lib);
    }

    // Build clang args for bindgen
    let clang_args: Vec<String> = include_paths
        .iter()
        .map(|p| format!("-I{}", p.display()))
        .collect();

    // Generate bindings
    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_args(&clang_args)
        // Allowlist SPDK types and functions
        .allowlist_function("spdk_.*")
        .allowlist_type("spdk_.*")
        .allowlist_var("SPDK_.*")
        // Also allow some DPDK types we need
        .allowlist_type("rte_.*")
        .allowlist_function("rte_.*")
        // Generate Default impls for structs
        .derive_default(true)
        .derive_debug(true)
        .derive_copy(true)
        // Rust 2024 compatibility - wrap extern blocks in unsafe
        .wrap_unsafe_ops(true)
        // Handle opaque types (internal SPDK structs we don't need layout for)
        .opaque_type("spdk_nvme_ctrlr")
        .opaque_type("spdk_nvme_ns")
        .opaque_type("spdk_nvme_qpair")
        .opaque_type("spdk_bdev")
        .opaque_type("spdk_bdev_desc")
        .opaque_type("spdk_io_channel")
        .opaque_type("spdk_thread")
        .opaque_type("spdk_poller")
        .opaque_type("spdk_blob_store")
        .opaque_type("spdk_blob")
        // Make packed structs with aligned fields opaque to avoid E0588
        .opaque_type("spdk_nvme_ctrlr_data")
        .opaque_type("spdk_bdev_ext_io_opts")
        .opaque_type("spdk_nvmf_fabric_connect_rsp")
        .opaque_type("spdk_nvmf_fabric_prop_get_rsp")
        .opaque_type("spdk_nvme_tcp_cmd")
        .opaque_type("spdk_nvme_tcp_rsp")
        // Layout tests can fail on different systems
        .layout_tests(false)
        .generate()
        .expect("Failed to generate SPDK bindings");

    // Write bindings to OUT_DIR
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Failed to write bindings");
}
