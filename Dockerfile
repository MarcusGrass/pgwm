FROM rust:1-slim-bullseye
WORKDIR /pgwm
COPY . /pgwm
RUN apt-get update -y && apt-get install --no-install-recommends -y \
        perl \
        libssl-dev \
        pkg-config \
        make \
        lld \
        libx11-dev \
        libxft-dev
RUN rustup component add clippy
RUN cargo install cargo-hack cargo-deny
