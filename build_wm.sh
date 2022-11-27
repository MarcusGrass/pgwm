#!/bin/sh
set -e

RUSTFLAGS='-C panic=abort -C link-arg=-nostartfiles -C link-arg=-fuse-ld=mold' cargo b -p pgwm --target x86_64-unknown-linux-gnu "$@"