# inklkunn server image. Built by .github/workflows/release.yml from the PREBUILT, attested release binaries
# (staged under ctx/<arch>/inklkunn); this file never compiles Rust. It bundles the runtime prerequisites a
# bare binary needs on the host: a pinned prikk release (the floor, RFC 010 D-3), bubblewrap (the sandbox,
# RFC 001 D-4) and prlimit (util-linux, the memory ceiling). Running it: bubblewrap needs user namespaces
# (see docs/RELEASING.md).
FROM debian:bookworm-slim@sha256:7c7b2c966bc9ee8cedfeef67e0e279108992c77681fa595db4a9d65c06ccc587

ARG TARGETARCH
ARG PRIKK_VERSION=0.54.0

RUN apt-get update \
 && apt-get install -y --no-install-recommends bubblewrap ca-certificates curl \
 && rm -rf /var/lib/apt/lists/* \
 && prlimit --version

# prikk: the exact release asset for this architecture, checksum-verified before extraction.
RUN set -eu; \
    case "$TARGETARCH" in \
      amd64) T=x86_64-unknown-linux-gnu ;; \
      arm64) T=aarch64-unknown-linux-gnu ;; \
      *) echo "unsupported TARGETARCH: $TARGETARCH" >&2; exit 1 ;; \
    esac; \
    cd /tmp; \
    curl -fsSLO "https://github.com/prikk-vcs/prikk/releases/download/${PRIKK_VERSION}/prikk-${T}.tar.gz"; \
    curl -fsSLO "https://github.com/prikk-vcs/prikk/releases/download/${PRIKK_VERSION}/prikk-${T}.tar.gz.sha256"; \
    sha256sum -c "prikk-${T}.tar.gz.sha256"; \
    tar -xzf "prikk-${T}.tar.gz" -C /usr/local/bin prikk; \
    rm -f "prikk-${T}.tar.gz" "prikk-${T}.tar.gz.sha256"; \
    prikk --version

COPY ctx/${TARGETARCH}/inklkunn /usr/local/bin/inklkunn

RUN useradd --system --uid 10001 --home /var/lib/inklkunn --shell /usr/sbin/nologin inklkunn \
 && mkdir -p /var/lib/inklkunn \
 && chown inklkunn:inklkunn /var/lib/inklkunn

USER inklkunn
WORKDIR /var/lib/inklkunn
VOLUME ["/var/lib/inklkunn"]
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/inklkunn"]
