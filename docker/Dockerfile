# syntax=docker/dockerfile:1.7
# Multi-stage: cargo-chef caches dependency builds; the runtime is distroless (no shell).
# Build from the repository root:  docker build -f docker/Dockerfile -t intake .
# The console is served from /app/console/dist when present; build it in its own stage
# (added by the console PR) or mount it in. The server runs fine without it.

FROM rust:1.85-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY Cargo.toml Cargo.lock rustfmt.toml ./
COPY crates ./crates
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Dependencies only; cached until Cargo.toml/Cargo.lock change.
RUN cargo chef cook --release --recipe-path recipe.json -p intake-server
COPY Cargo.toml Cargo.lock rustfmt.toml ./
COPY crates ./crates
COPY migrations ./migrations
COPY .sqlx ./.sqlx
# Offline: the checked-in .sqlx data stands in for a live database at compile time.
ENV SQLX_OFFLINE=true
RUN cargo build --release -p intake-server

FROM gcr.io/distroless/cc-debian12:nonroot AS runtime
WORKDIR /app
COPY --from=builder /app/target/release/intake-server /app/intake-server
COPY prompts ./prompts
ENV PORT=8788 PROMPTS_DIR=/app/prompts CONSOLE_DIST=/app/console/dist WIDGET_DIST=/app/widget/dist TRUST_PROXY=true
EXPOSE 8788
USER nonroot
ENTRYPOINT ["/app/intake-server"]
CMD ["serve"]
