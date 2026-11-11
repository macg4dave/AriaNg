#!/bin/sh
set -eu
cp /fixture-key.pub /root/.ssh/authorized_keys
chmod 600 /root/.ssh/authorized_keys
ssh-keygen -A >/dev/null 2>&1
nginx
exec /usr/sbin/sshd -D -e
