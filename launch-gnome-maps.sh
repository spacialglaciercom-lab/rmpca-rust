#!/usr/local/bin/bash
# Launch GNOME Maps with rmpca integration
ROOT="/home/drone/Documents/rmpca-rust"
MAPS_DIR="$ROOT/gnome-maps-main"
BUILD_DIR="$MAPS_DIR/build_v2"

# ============================================================
# Display and X11 Authentication Setup
# ============================================================

# ============================================================
# If running as root, re-exec as the desktop user via su/sudo
# ============================================================
CURRENT_USER=$(whoami)

# Find the user who owns the active GNOME/X11 session
find_desktop_user() {
    # Check who runs gnome-shell or Xorg
    ps aux 2>/dev/null | grep -E 'gnome-shell' | grep -v grep | awk '{print $1}' | head -1
}

DESKTOP_USER=$(find_desktop_user)

if [ "$CURRENT_USER" = "root" ] && [ -n "$DESKTOP_USER" ] && [ "$DESKTOP_USER" != "root" ]; then
    echo "[launch-gnome-maps] Running as root, re-execing as desktop user: $DESKTOP_USER"
    exec sudo -u "$DESKTOP_USER" \
        DISPLAY=:0 \
        XDG_RUNTIME_DIR="/run/user/$(id -u "$DESKTOP_USER")" \
        XDG_SESSION_TYPE=wayland \
        "$0" "$@"
fi

# ============================================================
# Display detection
# ============================================================
if [ -z "$DISPLAY" ]; then
    # Check for X11 sockets
    if [ -d /tmp/.X11-unix ]; then
        ACTIVE_SOCKET=$(ls /tmp/.X11-unix/ 2>/dev/null | head -n 1)
        if [ -n "$ACTIVE_SOCKET" ]; then
            ACTIVE_DISPLAY=${ACTIVE_SOCKET#X}
            export DISPLAY=":$ACTIVE_DISPLAY"
        fi
    fi
    # Fallback
    if [ -z "$DISPLAY" ]; then
        export DISPLAY=:0
    fi
fi

# Set WAYLAND_DISPLAY if on Wayland (GNOME on FreeBSD uses Xwayland)
if [ -z "$WAYLAND_DISPLAY" ] && [ -S "/run/user/$(id -u)/wayland-0" ]; then
    export WAYLAND_DISPLAY=wayland-0
fi

echo "[launch-gnome-maps] Display detected: DISPLAY=$DISPLAY WAYLAND_DISPLAY=$WAYLAND_DISPLAY"
echo "[launch-gnome-maps] Running as user: $(whoami)"

# ============================================================
# X11 Authentication
# ============================================================
if ! xauth list 2>/dev/null | grep -q "$(echo $DISPLAY | sed 's/:/\\:/')" 2>/dev/null; then
    echo "[launch-gnome-maps] No X11 authority found for $DISPLAY, attempting to acquire..."

    # Try SDDM xauth files
    for XAUTH_FILE in /var/run/sddm/xauth_*; do
        if [ -r "$XAUTH_FILE" ] 2>/dev/null; then
            echo "[launch-gnome-maps] Merging X11 auth from: $XAUTH_FILE"
            xauth merge "$XAUTH_FILE" 2>/dev/null
            break
        fi
    done

    # Try logged-in user's .Xauthority
    if [ -z "$(xauth list 2>/dev/null | grep "$DISPLAY")" ]; then
        DESKTOP_UID=$(id -u "$DESKTOP_USER" 2>/dev/null || echo "")
        DESKTOP_HOME=$(getent passwd "$DESKTOP_USER" 2>/dev/null | cut -d: -f6)
        if [ -n "$DESKTOP_HOME" ] && [ -r "$DESKTOP_HOME/.Xauthority" ]; then
            echo "[launch-gnome-maps] Merging X11 auth from: $DESKTOP_HOME/.Xauthority"
            xauth merge "$DESKTOP_HOME/.Xauthority" 2>/dev/null
        fi
    fi
fi

# Ensure XDG_RUNTIME_DIR is set correctly for the current user
export XDG_RUNTIME_DIR="/run/user/$(id -u)"

# Debug output
echo "[launch-gnome-maps] Current xauth entries:"
xauth list 2>/dev/null | while read entry; do
    echo "[launch-gnome-maps]   $entry"
done

# ============================================================
# GNOME Maps Setup
# ============================================================

# Point GSettings to local schema
export GSETTINGS_SCHEMA_DIR="$MAPS_DIR/data"
# Point to local Typelib (GnomeMaps-1.0)
export GI_TYPELIB_PATH="$BUILD_DIR/lib:/usr/local/lib/girepository-1.0"
# Ensure blueprint-compiler is in path for runtime if needed (though usually only for build)
export PATH="$HOME/.local/bin:$PATH"

# Setup environment variables for rmpca-rust
# These are read by the org.gnome.Maps wrapper to set GSettings in-process
export RMPCA_PATH="$ROOT/target/release/rmpca"
export RMPCA_OFFLINE_MAP="$HOME/Downloads/montreal.osm.pbf"

MAPS_BIN="$BUILD_DIR/src/org.gnome.Maps"
echo "[launch-gnome-maps] Launching GNOME Maps: $MAPS_BIN"
echo "[launch-gnome-maps] DISPLAY=$DISPLAY"
echo "[launch-gnome-maps] GSETTINGS_SCHEMA_DIR=$GSETTINGS_SCHEMA_DIR"
echo "[launch-gnome-maps] GI_TYPELIB_PATH=$GI_TYPELIB_PATH"

exec "$MAPS_BIN" "$@"
