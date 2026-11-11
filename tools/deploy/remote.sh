#!/bin/sh
# Executed over authenticated SSH. Output is suppressed by the Rust operator CLI.
set -eu
umask 077
action=$1
root=$2
id=$3
health=$4
case "$root" in /*) ;; *) exit 1 ;; esac
case "$id" in r-[0-9]*-[0-9]*) ;; *) exit 1 ;; esac
test -d "$root"
test "$(realpath "$root")" = "$root"
cd "$root"
if test ! -f .ariang-deployment; then
    test "$action" = prepare
    test -z "$(find . -mindepth 1 -maxdepth 1 -print -quit)"
    printf 'ariang-static-v1\n' > .ariang-deployment
fi
test "$(cat .ariang-deployment)" = ariang-static-v1
valid_link() {
    case "$1" in releases/r-*) test -f "$1/index.html" && test -f "$1/LICENSE" && test "$(realpath "$1")" = "$root/$1" ;; *) return 1 ;; esac
}
if test "$action" = prepare || test "$action" = rollback; then
    mkdir .deploy-lock
    printf '%s\n' "$id" > .deploy-lock/owner
else
    test "$(cat .deploy-lock/owner)" = "$id"
fi
unlock() { rm -f .deploy-lock/owner; rmdir .deploy-lock; }
trap unlock EXIT
if test "$action" = prepare; then
    mkdir -p releases
    test ! -L releases
    test "$(realpath releases)" = "$root/releases"
    # Persist the lock across transfer and activation; abort only by matching ID.
    trap - EXIT
    exit 0
fi
if test "$action" = abort; then
    rm -f "$id.tar"
    exit 0
fi
previous=''
if test -e current || test -L current; then
    test -L current
    previous=$(readlink current)
    valid_link "$previous"
fi
activate() {
    test ! -e ".current-$id" && test ! -L ".current-$id"
    ln -s "$1" ".current-$id"
    mv -Tf ".current-$id" current
}
check_health() {
    curl --fail --silent --show-error --max-time 10 --max-filesize 2097152 --output ".health-$id" "$health" && cmp -s ".health-$id" "$1/index.html"
}
if test "$action" = rollback; then
    test -L previous
    target=$(readlink previous)
    valid_link "$target"
elif test "$action" = activate; then
    test -f "$id.tar" && test ! -L "$id.tar"
    # The sender enforces the static allowlist. Also reject traversal, links and
    # special files before extraction at the receiver.
    tar -tf "$id.tar" > ".manifest-$id"
    awk '/(^|\/)\.\.(\/|$)/ || /[^a-zA-Z0-9_.\/-]/ || /^\// { exit 1 }' ".manifest-$id"
    tar -tvf "$id.tar" | awk 'substr($0,1,1) != "-" && substr($0,1,1) != "d" { exit 1 }'
    mkdir "releases/$id"
    tar --no-same-owner --no-same-permissions -xf "$id.tar" -C "releases/$id"
    target="releases/$id"
    valid_link "$target"
    chmod -R a+rX "$target"
    chmod a+x . releases
    rm -f "$id.tar" ".manifest-$id"
else
    exit 1
fi
activate "$target"
if ! check_health "$target"; then
    if test -n "$previous"; then activate "$previous"; else rm -f current; fi
    rm -f ".health-$id"
    exit 1
fi
rm -f ".health-$id"
if test -n "$previous"; then
    ln -s "$previous" ".previous-$id"
    mv -Tf ".previous-$id" previous
fi
