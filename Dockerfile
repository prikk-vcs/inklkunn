# inklkunn server image. Built by .github/workflows/release.yml from the PREBUILT, attested release binaries
# (staged under ctx/<arch>/inklkunn); this file never compiles Rust. It bundles the runtime prerequisites a
# bare binary needs on the host: a pinned prikk release (the floor, RFC 010 D-3, checked against the checksum
# inklkunn recorded, not one fetched beside it), bubblewrap (the sandbox, RFC 001 D-4) and prlimit
# (util-linux, the memory ceiling). The fetch happens in a separate stage so the runtime image carries no
# download tool. Running it: bubblewrap needs user namespaces (see docs/RELEASING.md).
FROM debian:bookworm-slim@sha256:7c7b2c966bc9ee8cedfeef67e0e279108992c77681fa595db4a9d65c06ccc587 AS fetch

ARG TARGETARCH
ARG PRIKK_VERSION=0.54.0
ARG PRIKK_SHA256_X86_64=e998909328e4d1216c0e5ccf4b00141b32c6109498f04ba94a65cc61124ab687
ARG PRIKK_SHA256_AARCH64=9a325df0661c767ace382f0d7550a5a28ca9fd0a73bdf6f7878968133535df0d

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/*

# prikk: the exact release asset for this architecture, checked against the recorded checksum before extraction.
RUN set -eu; \
    case "$TARGETARCH" in \
      amd64) T=x86_64-unknown-linux-gnu; SUM="$PRIKK_SHA256_X86_64" ;; \
      arm64) T=aarch64-unknown-linux-gnu; SUM="$PRIKK_SHA256_AARCH64" ;; \
      *) echo "unsupported TARGETARCH: $TARGETARCH" >&2; exit 1 ;; \
    esac; \
    cd /tmp; \
    curl -fsSLO "https://github.com/prikk-vcs/prikk/releases/download/${PRIKK_VERSION}/prikk-${T}.tar.gz"; \
    echo "${SUM}  prikk-${T}.tar.gz" | sha256sum -c -; \
    mkdir -p /out; \
    tar -xzf "prikk-${T}.tar.gz" -C /out prikk; \
    /out/prikk --version

FROM debian:bookworm-slim@sha256:7c7b2c966bc9ee8cedfeef67e0e279108992c77681fa595db4a9d65c06ccc587

ARG TARGETARCH

RUN apt-get update \
 && apt-get install -y --no-install-recommends bubblewrap ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && prlimit --version

COPY --from=fetch /out/prikk /usr/local/bin/prikk
COPY ctx/${TARGETARCH}/inklkunn /usr/local/bin/inklkunn

RUN useradd --system --uid 10001 --home /var/lib/inklkunn --shell /usr/sbin/nologin inklkunn \
 && mkdir -p /var/lib/inklkunn \
 && chown inklkunn:inklkunn /var/lib/inklkunn \
 && prikk --version && inklkunn --version

USER inklkunn
WORKDIR /var/lib/inklkunn
VOLUME ["/var/lib/inklkunn"]
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/inklkunn"]
