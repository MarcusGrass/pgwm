cargo +nightly stack-sizes \
      --bin pgwm \
      --release \
      -v \
      -- -C link-arg=-Wl,-Tkeep-stack-sizes.x -C link-arg=-N -C link-arg=-lX11 -C lto=no > stack.txt
# RUSTC=stack-sizes-rustc "cargo" "rustc" "--bin" "hello" "--release" "--" "-C" "link-arg=-Wl,-Tkeep-stack-sizes.x" "-C" "link-arg=-N"