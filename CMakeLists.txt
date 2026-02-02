cmake_minimum_required(VERSION 3.28)
project(bypass-misc2 LANGUAGES)

add_subdirectory(tests)


include(FetchContent)

message(STATUS "Fetching SPDK source code...")
FetchContent_Populate(
  spdk
  GIT_REPOSITORY https://github.com/spdk/spdk.git
  GIT_TAG        2ef883ef96e79c3cc16da02f667a7a58c2453f2f # v26.01
  GIT_SHALLOW    TRUE
)

message(STATUS "SPDK source code fetched to: ${spdk_SOURCE_DIR}")
message(STATUS "SPDK build directory: ${spdk_BINARY_DIR}")

# Shallow update of submodules
message(STATUS "Updating SPDK submodules (shallow)...")
execute_process(
  COMMAND git submodule update --init --recursive --depth 1
  WORKING_DIRECTORY ${spdk_SOURCE_DIR}
  RESULT_VARIABLE GIT_SUBMODULE_RESULT
)

if(NOT GIT_SUBMODULE_RESULT EQUAL "0")
  message(FATAL_ERROR "git submodule update failed: ${GIT_SUBMODULE_RESULT}")
endif()

# Apply SPDK patches (idempotent - checks if already applied)
set(SPDK_PATCH_FILE ${CMAKE_CURRENT_SOURCE_DIR}/patches/spdk-v26.01.patch)
set(SPDK_PATCH_MARKER ${spdk_SOURCE_DIR}/.spdk-patched)

if(EXISTS ${SPDK_PATCH_FILE} AND NOT EXISTS ${SPDK_PATCH_MARKER})
  message(STATUS "Applying SPDK patches...")
  # Remove version.py if it exists (patch creates it, but it might exist from a previous partial apply)
  file(REMOVE ${spdk_SOURCE_DIR}/python/spdk/version.py)
  execute_process(
    COMMAND patch -p1 --forward -i ${SPDK_PATCH_FILE}
    WORKING_DIRECTORY ${spdk_SOURCE_DIR}
    RESULT_VARIABLE PATCH_RESULT
  )
  if(PATCH_RESULT EQUAL "0")
    file(WRITE ${SPDK_PATCH_MARKER} "Patched with ${SPDK_PATCH_FILE}\n")
    message(STATUS "SPDK patches applied successfully")
  else()
    message(WARNING "Patch may have already been applied or failed: ${PATCH_RESULT}")
  endif()
elseif(EXISTS ${SPDK_PATCH_MARKER})
  message(STATUS "SPDK patches already applied, skipping")
endif()

# Run manually:
add_custom_target(spdk_pkgdep
    COMMAND ./scripts/pkgdep.sh
    WORKING_DIRECTORY ${spdk_SOURCE_DIR}
    COMMENT "Installing SPDK package dependencies"
)

# Let SPDK build its bundled DPDK (system DPDK 22.11 is too old for SPDK v26.01)
# Use --target-arch=nehalem for broad compatibility (SSE4.2). QEMU TCG can emulate this.
# ISA-L is only used by crypto module (disabled via --without-crypto), so no libisal dependency
add_custom_target(configure_spdk
    COMMAND ./configure --with-shared --without-rdma --without-fio --without-rbd --without-dpdk --without-ocf --without-crypto --disable-tests --disable-unit-tests --target-arch=nehalem
    WORKING_DIRECTORY ${spdk_SOURCE_DIR}
    COMMENT "Configuring SPDK"
)

add_custom_target(build_spdk
    COMMAND $(MAKE)
    WORKING_DIRECTORY ${spdk_SOURCE_DIR}
    COMMENT "Building SPDK"
)

# Add custom target to install SPDK to system
add_custom_target(spdk_install
    COMMAND $(MAKE) install
    COMMAND ldconfig
    WORKING_DIRECTORY ${spdk_SOURCE_DIR}
    COMMENT "Installing SPDK artifacts"
    DEPENDS build_spdk
)

# Add custom target to create .deb package without installing to system
# Uses DESTDIR staging with dpkg-deb - no extra tools required
set(SPDK_VERSION "26.01")
set(SPDK_PKG_DIR ${CMAKE_CURRENT_BINARY_DIR}/spdk-pkg)

# Create the DEBIAN control file content
file(WRITE ${CMAKE_CURRENT_BINARY_DIR}/spdk-control.txt
"Package: spdk
Version: ${SPDK_VERSION}
Section: libs
Priority: optional
Architecture: amd64
Depends: libc6, libnuma1, libssl3t64 | libssl3, libaio1t64 | libaio1, libfuse3-3, libuuid1, libiscsi7
Maintainer: maintainer@example.com
Description: Storage Performance Development Kit (SPDK) - custom build
")

add_custom_target(spdk_deb
    # Clean previous package directory
    COMMAND ${CMAKE_COMMAND} -E remove_directory ${SPDK_PKG_DIR}
    COMMAND ${CMAKE_COMMAND} -E make_directory ${SPDK_PKG_DIR}/DEBIAN
    # Install SPDK to staging directory (skip Python which doesn't respect DESTDIR)
    COMMAND ${CMAKE_COMMAND} -E env DESTDIR=${SPDK_PKG_DIR} $(MAKE) -C ${spdk_SOURCE_DIR}/lib install
    COMMAND ${CMAKE_COMMAND} -E env DESTDIR=${SPDK_PKG_DIR} $(MAKE) -C ${spdk_SOURCE_DIR}/module install
    COMMAND ${CMAKE_COMMAND} -E env DESTDIR=${SPDK_PKG_DIR} $(MAKE) -C ${spdk_SOURCE_DIR}/shared_lib install
    COMMAND ${CMAKE_COMMAND} -E env DESTDIR=${SPDK_PKG_DIR} $(MAKE) -C ${spdk_SOURCE_DIR}/include install
    COMMAND ${CMAKE_COMMAND} -E env DESTDIR=${SPDK_PKG_DIR} $(MAKE) -C ${spdk_SOURCE_DIR}/app install
    # Install DPDK to staging directory
    COMMAND ${CMAKE_COMMAND} -E env DESTDIR=${SPDK_PKG_DIR} ninja -C ${spdk_SOURCE_DIR}/dpdk/build-tmp install
    # Move DPDK files from build prefix to /usr/local (DPDK uses absolute prefix path)
    COMMAND ${CMAKE_COMMAND} -E copy_directory ${SPDK_PKG_DIR}${spdk_SOURCE_DIR}/dpdk/build ${SPDK_PKG_DIR}/usr/local
    COMMAND ${CMAKE_COMMAND} -E remove_directory ${SPDK_PKG_DIR}/home
    # Install bundled ISA-L library if it was built (depends on nasm being present during configure)
    COMMAND ${CMAKE_COMMAND} -DSRC=${spdk_SOURCE_DIR}/isa-l/.libs/libisal.so.2 -DDST=${SPDK_PKG_DIR}/usr/local/lib -P ${CMAKE_CURRENT_SOURCE_DIR}/cmake/copy_if_exists.cmake
    # Copy control file
    COMMAND ${CMAKE_COMMAND} -E copy ${CMAKE_CURRENT_BINARY_DIR}/spdk-control.txt ${SPDK_PKG_DIR}/DEBIAN/control
    # Build the .deb package
    COMMAND dpkg-deb --build ${SPDK_PKG_DIR} ${CMAKE_CURRENT_BINARY_DIR}/spdk_${SPDK_VERSION}_amd64.deb
    WORKING_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}
    COMMENT "Creating SPDK .deb package (staged, no system install)"
    DEPENDS build_spdk
)