FROM rust:1-slim-bullseye
WORKDIR /pgwm
COPY . /pgwm
RUN cargo install --profile=lto --path pgwm
