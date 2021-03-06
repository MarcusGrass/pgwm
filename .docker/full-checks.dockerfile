FROM rust:1-slim-bullseye
RUN apt-get update -y && apt-get install --no-install-recommends -y \
        perl \
        libssl-dev \
        pkg-config \
        make
RUN rustup component add clippy
RUN cargo install cargo-hack cargo-deny
