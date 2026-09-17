#!/bin/sh
set -eu

usage() {
    echo "usage: sh ./compose-setup.sh CODEX_BUSINESS_HOME CODEX_PERSONAL_HOME COMMAND_CODE_KEY_SOURCE" >&2
    exit 2
}

[ "$#" -eq 3 ] || usage

uid=$(id -u)
gid=$(id -g)
[ "$uid" -ne 0 ] || { echo "compose-setup: do not run as root" >&2; exit 1; }

business=$(realpath "$1")
personal=$(realpath "$2")
command_key=$(realpath "$3")

[ -d "$business" ] || { echo "compose-setup: not a directory: $business" >&2; exit 1; }
[ -d "$personal" ] || { echo "compose-setup: not a directory: $personal" >&2; exit 1; }
[ -f "$command_key" ] || { echo "compose-setup: not a file: $command_key" >&2; exit 1; }
[ -r "$command_key" ] || { echo "compose-setup: key is not readable: $command_key" >&2; exit 1; }

install -d -m 0700 run/hquota secrets
[ "$(stat -c %u run/hquota)" -eq "$uid" ] || {
    echo "compose-setup: run/hquota must be owned by uid $uid" >&2
    exit 1
}
install -m 0600 "$command_key" secrets/command-code-goat

[ -f config.json ] || cp config.example.json config.json

umask 077
cat > .env <<EOF
HOST_UID=$uid
HOST_GID=$gid
CODEX_BUSINESS_HOME=$business
CODEX_PERSONAL_HOME=$personal
EOF

echo "compose-setup: ready"
