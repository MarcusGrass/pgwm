FROM rust:1-slim-bullseye
RUN apt-get update -y && apt-get install --no-install-recommends -y \
        perl \
        libssl-dev \
        git \
        pkg-config \
        make \
        lld \
        libx11-dev \
        libxft-dev
RUN rustup component add clippy
RUN cargo install cargo-hack cargo-deny
RUN git clone https://github.com/MarcusGrass/pgwm.git
WORKDIR /pgwm
CMD ["./check.sh"]