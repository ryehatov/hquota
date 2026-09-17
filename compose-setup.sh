#!/bin/sh
set -eu

usage() {
    echo "usage: sh ./compose-setup.sh CODEX_BUSINESS_HOME CODEX_PERSONAL_HOME COMMAND_CODE_KEY_FILE [HERMES_DATA_DIR]" >&2
    exit 2
}

[ "$#" -ge 3 ] && [ "$#" -le 4 ] || usage

uid=$(id -u)
gid=$(id -g)
[ "$uid" -ne 0 ] || { echo "compose-setup: do not run as root" >&2; exit 1; }

business=$(realpath "$1")
personal=$(realpath "$2")
command_key=$(realpath "$3")
hermes_data=${4:-"$HOME/.hermes"}
mkdir -p "$hermes_data"
hermes_data=$(realpath "$hermes_data")

[ -d "$business" ] || { echo "compose-setup: not a directory: $business" >&2; exit 1; }
[ -d "$personal" ] || { echo "compose-setup: not a directory: $personal" >&2; exit 1; }
[ -f "$command_key" ] || { echo "compose-setup: not a file: $command_key" >&2; exit 1; }
[ -r "$command_key" ] || { echo "compose-setup: key is not readable: $command_key" >&2; exit 1; }

runtime_dir=$(realpath -m "$PWD/run/hquota")
install -d -m 0700 "$runtime_dir"
[ "$(stat -c %u "$runtime_dir")" -eq "$uid" ] || {
    echo "compose-setup: runtime directory must be owned by uid $uid: $runtime_dir" >&2
    exit 1
}

if [ ! -f config.json ]; then
    cp config.example.json config.json
fi

umask 077
cat > .env <<EOF_ENV
HERMES_UID=$uid
HERMES_GID=$gid
HQUOTA_RUNTIME_DIR=$runtime_dir
HERMES_DATA_DIR=$hermes_data
CODEX_BUSINESS_HOME=$business
CODEX_PERSONAL_HOME=$personal
COMMAND_CODE_KEY_FILE=$command_key
HQUOTA_BROKER_IMAGE=hquota-broker:local
HERMES_HQUOTA_IMAGE=hermes-hquota:local
HQUOTA_CONTAINER_NAME=hermes-hquota
HERMES_BASE_IMAGE=nousresearch/hermes-agent:latest
EOF_ENV
chmod 0600 .env

echo "compose-setup: wrote .env and prepared $runtime_dir"
echo "compose-setup: edit config.json if account names or providers differ"
