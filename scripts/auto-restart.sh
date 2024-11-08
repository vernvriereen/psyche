#!/usr/bin/env bash
if [ $# -eq 0 ]; then
    echo "Error: No command specified" >&2
    echo "Usage: $0 command [args...]" >&2
    exit 1
fi

trap "trap - SIGTERM && kill -- -$$" SIGINT SIGTERM EXIT

until "$@" & wait $!; do
    echo "Command $* crashed with exit code $?. Respawning.." >&2
    sleep 1
done
