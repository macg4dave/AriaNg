#!/bin/sh
set -eu
umask 077
secret=$(cat /run/secrets/aria2_rpc_secret)
case "$secret" in
    ''|*[!a-f0-9]*) echo 'Invalid test RPC secret; run tests/run_docker.py --setup-only' >&2; exit 1 ;;
esac
[ "${#secret}" -eq 64 ] || exit 1
cat /etc/aria2/aria2.conf > /tmp/aria2.conf
printf '\nrpc-secret=%s\n' "$secret" >> /tmp/aria2.conf
unset secret
exec aria2c --conf-path=/tmp/aria2.conf
