FROM rust:1.99.1-bookworm AS build
WORKDIR /src

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations

RUN cargo build --release --locked -p apex-sync-server

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home apexsync \
    && install -d --owner 10001 --group 10001 /data
COPY --from=build /src/target/release/apex-sync-server /usr/local/bin/apex-sync-server
COPY --from=build /src/migrations /app/migrations
WORKDIR /app
USER apexsync
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/apex-sync-server"]
