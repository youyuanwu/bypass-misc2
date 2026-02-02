# SPDK v26.01 Patches

This patch file (`spdk-v26.01.patch`) contains fixes required to build and run SPDK v26.01 in our CI/test environment.

## Patches Included

### 1. pip-tools Compatibility Fix (`scripts/pkgdep/debian.sh`)

**Problem:** SPDK's `pkgdep.sh` script installs `pip-tools` without version pinning. Starting with pip 26.0, the `allow_all_prereleases` attribute was removed from `PackageFinder`, causing pip-tools 7.5.2+ to fail with:
```
AttributeError: 'PackageFinder' object has no attribute 'allow_all_prereleases'
```

**Fix:** Pin pip to version <25 and pip-tools to 7.4.1 which are compatible with each other.

### 2. Duplicate `--iova-mode=pa` Argument Fix (`lib/env_dpdk/init.c`)

**Problem:** When running SPDK in a VM that uses vfio-pci with noiommu mode (no hardware IOMMU), two conditions in `build_eal_cmdline()` both evaluate to true:
1. `rte_vfio_noiommu_is_enabled()` returns true (line 533)
2. `!x86_cpu_support_iommu()` returns true (line 546)

Both conditions add `--iova-mode=pa` to the DPDK EAL arguments, causing DPDK to fail with:
```
ARGPARSE: argument --iova-mode should not occur multiple times!
```

**Fix:** Change the second `if` to `else if` so only one of the conditions can add the argument.

### 3. Missing `version.py` (`python/spdk/version.py`)

**Problem:** The SPDK Python package is missing `version.py` which is required by hatchling during `pip-compile`. This causes the dependency resolution to fail during `pkgdep.sh` execution.

**Fix:** Create the missing `version.py` with the appropriate version string.

## IOVA Modes Explained

**IOVA** = I/O Virtual Address - how DPDK maps memory for Direct Memory Access (DMA):

- **PA (Physical Address)**: Uses physical memory addresses directly. Works without IOMMU, simpler but requires root privileges.
- **VA (Virtual Address)**: Uses virtual addresses with IOMMU translation. More flexible but requires hardware IOMMU support.

In VMs without hardware IOMMU emulation (like QEMU without `-device intel-iommu`), PA mode must be used.

## Applying the Patch

The patch is automatically applied by CMake during configure:
```bash
cmake -S . -B build
```

The patching is idempotent - a marker file (`.spdk-patched`) is created after successful patching to prevent re-application on subsequent cmake runs.

## Manual Application

If needed, apply manually:
```bash
cd build/spdk-src
patch -p1 < ../../patches/spdk-v26.01.patch
```
