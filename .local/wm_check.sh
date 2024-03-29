#!/bin/bash
set -ex
# Deny all warnings here, becomes a pain to scroll back otherwise
cargo hack clippy -p pgwm-app --feature-powerset -- -D warnings
# Running all modules like this causes a lot of rebuilds which take a lot of time
cargo hack test -p pgwm-core --feature-powerset
cargo test -p pgwm-app --all-features
# Make sure dependencies don't have any advisories or weird licensing
cargo deny --all-features --frozen --locked check
