#!/bin/sh
# Starts the server without root. Under Docker (root in the container is root on the host) it
# hands the data directory to the user WF_UID (10001 by default; volumes of versions before
# 0.1.9 belong to root) and drops to it. Under rootless Podman, where root in the container is
# the host user already, nothing changes unless WF_UID is set.
set -eu

if [ "$(id -u)" = 0 ]; then
    # The identity map: this container's root is the host's root.
    map=$(head -n 1 /proc/self/uid_map | tr -s ' \t' ' ' | sed 's/^ //')
    if [ -n "${WF_UID:-}" ] || [ "$map" = "0 0 4294967295" ]; then
        uid=${WF_UID:-10001}
        gid=${WF_GID:-$uid}
        if [ "$uid" != 0 ]; then
            dir=${WF_DATA_DIR:-/data}
            mkdir -p "$dir"
            if [ -n "$(find "$dir" -xdev \( ! -user "$uid" -o ! -group "$gid" \) -print -quit)" ]; then
                echo "windfenster: handing $dir to uid $uid" >&2
                chown -R "$uid:$gid" "$dir"
            fi
            exec setpriv --reuid="$uid" --regid="$gid" --clear-groups --no-new-privs -- "$@"
        fi
    fi
fi
exec "$@"
