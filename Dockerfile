# =============================================================================
# VISITA+ — Dockerfile multi-stage
# -----------------------------------------------------------------------------
# Stage 1 (builder): compila o binario em modo release
# Stage 2 (runtime): imagem minima apenas com o binario e dependencias
# =============================================================================

# -----------------------------------------------------------------------------
# STAGE 1 — BUILDER
# -----------------------------------------------------------------------------
FROM rust:1.96-slim-bookworm AS builder

# Dependencias de sistema para compilar com sqlx (libpq) e openssl
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libpq-dev libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Cache de dependencias: copia apenas o manifest primeiro
COPY Cargo.toml Cargo.lock* ./
RUN mkdir -p src && echo "fn main() {}" > src/main.rs \
    && cargo fetch \
    && cargo build --release || true

# Copia o codigo-fonte de fato e recompila
COPY src ./src
COPY migrations ./migrations
RUN cargo build --release

# -----------------------------------------------------------------------------
# STAGE 2 — RUNTIME
# -----------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends libpq5 ca-certificates tzdata \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --shell /usr/sbin/nologin app

WORKDIR /app
COPY --from=builder /app/target/release/visita_plus /app/visita_plus
COPY --from=builder /app/migrations /app/migrations

USER app
EXPOSE 8080

ENV RUST_LOG=info
CMD ["/app/visita_plus"]