#!/bin/sh
set -eu
secret=$(cat /run/secrets/aria2_rpc_secret)
printf '{"jsonrpc":"2.0","id":"health","method":"aria2.getVersion","params":["token:%s"]}' "$secret" |
    curl --fail --silent --max-time 2 -H 'Content-Type: application/json' \
        --data-binary @- http://127.0.0.1:6800/jsonrpc | grep -q '"version"'
