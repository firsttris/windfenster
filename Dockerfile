# syntax=docker/dockerfile:1

# ---- 1. SPA ------------------------------------------------------------------------------
FROM node:22-trixie-slim AS web
WORKDIR /src
RUN corepack enable
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY web/package.json web/
RUN pnpm install --frozen-lockfile --filter @windfenster/web
COPY web web
RUN pnpm --filter @windfenster/web build

# ---- 2. Server binary, SPA embedded via rust-embed ---------------------------------------
FROM rust:1-trixie AS server
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY .cargo .cargo
COPY crates crates
COPY migrations migrations
# Station pages take their meta texts from the SPA's message files.
COPY web/messages web/messages
COPY --from=web /src/web/dist web/dist
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p windfenster && cp target/release/windfenster /windfenster

# ---- 3. Runtime --------------------------------------------------------------------------
FROM debian:trixie-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates tini \
    && rm -rf /var/lib/apt/lists/*
COPY --from=server /windfenster /usr/local/bin/windfenster
# Drops root before the server starts (setpriv from util-linux, in every Debian image)
COPY docker/entrypoint.sh /usr/local/bin/entrypoint.sh

ENV WF_DATA_DIR=/data \
    WF_LISTEN=0.0.0.0:8080

# SQLite database (and from Phase 3 the map textures)
VOLUME ["/data"]
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s CMD ["/bin/bash", "-c", "exec 3<>/dev/tcp/127.0.0.1/8080 && printf 'GET /api/health HTTP/1.0\\r\\n\\r\\n' >&3 && grep -q schemaVersion <&3"]
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/entrypoint.sh", "windfenster"]
CMD ["serve"]
