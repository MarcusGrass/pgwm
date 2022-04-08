FROM debian:bullseye-slim
WORKDIR /pgwm
COPY . /pgwm
RUN apt-get update -y && apt-get install --no-install-recommends -y \
        build-essential \
        ca-certificates \
        curl \
        lld \
        libx11-dev \
        libxft-dev
RUN curl https://sh.rustup.rs -sSf | bash -s -- -y
ENV PATH="/root/.cargo/bin:$PATH"
RUN cargo install --profile=optimized --path pgwm