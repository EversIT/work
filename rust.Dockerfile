# syntax=docker/dockerfile:1
# Runtime-only image for Rust binaries. Nothing is compiled here: `nx run <app>:build` (amd64) and
# `build-arm64` produce the release binaries beforehand (container targets depend on them), so the
# dependency tree is compiled once, not a second time inside Docker.
#
# Base is ubuntu:24.04 to match the glibc the GitHub Actions runner (ubuntu-latest) links against:
# a mismatch fails at container start ("GLIBC_x.xx not found"), not at build time. Local builds on
# macOS/Windows cannot use `build` (it produces a native binary, not a Linux ELF): use the
# `container:local` target, which builds the Linux binary inside a container first.
FROM docker.io/ubuntu:24.04 AS runner

ARG APP_NAME
ARG GIT_HASH=unknown
ENV GIT_HASH=${GIT_HASH}
# buildx fills TARGETARCH (amd64/arm64) once per platform in a multi-arch build.
ARG TARGETARCH
# Where the binaries live. CI uses the defaults; `container:local` overrides them.
ARG BIN_AMD64=dist/target/release/${APP_NAME}
ARG BIN_ARM64=dist/target/aarch64-unknown-linux-gnu/release/${APP_NAME}

# ca-certificates: outbound TLS (SMTP, Slack, database with sslmode). Non-root user without a shell login.
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --no-create-home --shell /usr/sbin/nologin --uid 10001 appuser \
    && mkdir -p /data/uploads && chown appuser /data/uploads

COPY ${BIN_AMD64} /tmp/bin-amd64
COPY ${BIN_ARM64} /tmp/bin-arm64
RUN set -eu; \
    case "${TARGETARCH}" in \
      amd64) cp /tmp/bin-amd64 /usr/local/bin/app ;; \
      arm64) cp /tmp/bin-arm64 /usr/local/bin/app ;; \
      *) echo "rust.Dockerfile: unsupported TARGETARCH '${TARGETARCH}'" >&2; exit 1 ;; \
    esac; \
    chmod 0755 /usr/local/bin/app; \
    rm -f /tmp/bin-amd64 /tmp/bin-arm64

ENV WORK_UPLOAD_DIR=/data/uploads
VOLUME /data/uploads
USER appuser
CMD ["/usr/local/bin/app"]
