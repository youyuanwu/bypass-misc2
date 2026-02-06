# Add SPDK bin directory to PATH
SPDK_BIN_PATH="/opt/spdk/bin"
case "$PATH" in
    *"$SPDK_BIN_PATH"* ) true ;;
    * ) PATH="$SPDK_BIN_PATH:$PATH" ;;
esac

# Add SPDK pkgconfig directory to PKG_CONFIG_PATH
SPDK_PKGCONFIG_PATH="/opt/spdk/lib/pkgconfig"
case "$PKG_CONFIG_PATH" in
    *"$SPDK_PKGCONFIG_PATH"* ) true ;;
    * ) export PKG_CONFIG_PATH="$SPDK_PKGCONFIG_PATH${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}" ;;
esac
