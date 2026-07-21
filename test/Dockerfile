# Clean-room integration test for ccd (Linux container).
# Build context = repo root:   docker build -f test/Dockerfile -t ccd-test .
FROM rust:1-slim

RUN apt-get update \
 && apt-get install -y --no-install-recommends zsh ca-certificates \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml ./
COPY src ./src
COPY shell ./shell
RUN cargo build --release \
 && install -m 755 target/release/ccd /usr/local/bin/ccd

COPY test/integration.zsh /integration.zsh

RUN useradd -m -s /usr/bin/zsh tester
USER tester
WORKDIR /home/tester
ENTRYPOINT ["zsh", "/integration.zsh"]
