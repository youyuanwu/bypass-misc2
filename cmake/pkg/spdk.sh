# Add SPDK bin directory to PATH
SPDK_BIN_PATH="/opt/spdk/bin"
case "$PATH" in
    *"$SPDK_BIN_PATH"* ) true ;;
    * ) PATH="$SPDK_BIN_PATH:$PATH" ;;
esac

# Add SPDK pkgconfig directory to PKG_CONFIG_PATH
# If system DPDK is already installed, add SPDK pkgconfig at lower priority
# to avoid overriding system DPDK. Otherwise add at higher priority.
SPDK_PKGCONFIG_PATH="/opt/spdk/lib/pkgconfig"
case "$PKG_CONFIG_PATH" in
    *"$SPDK_PKGCONFIG_PATH"* ) true ;;
    * )
        # Check if system DPDK is already available (without SPDK's bundled version)
        if pkg-config --exists libdpdk 2>/dev/null; then
            # System DPDK found - add SPDK at end so system DPDK takes precedence
            export PKG_CONFIG_PATH="${PKG_CONFIG_PATH:+$PKG_CONFIG_PATH:}$SPDK_PKGCONFIG_PATH"
        else
            # No system DPDK - add SPDK at beginning (includes bundled DPDK)
            export PKG_CONFIG_PATH="$SPDK_PKGCONFIG_PATH${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
        fi
        ;;
esac
