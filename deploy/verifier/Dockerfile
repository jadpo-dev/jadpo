# The verifier's image (docs/language/11-integrations.md, "The verifier";
# chunk R6): the `jadpo` of one release, and what its sandbox needs. Nothing
# is compiled here: `jadpo` is the release's own binary, checked against its
# attestation before it reaches this build (.github/workflows/verifier-image.yml).
#
# bubblewrap and prlimit are the sandbox; git fetches commits; ca-certificates
# checks the git host and the registry. It runs as a user with no privileges
# of its own, and `jadpo package verify --serve` refuses to start if the
# sandbox doesn't work where it is run.
#
# In a plain Docker container the sandbox needs `--security-opt
# seccomp=unconfined --security-opt apparmor=unconfined --security-opt
# systempaths=unconfined`, for its namespaces. The registry runs it on
# Cloudflare Containers, where it starts as it is.
FROM debian:trixie-slim@sha256:f6b08b09a05725f9431502d3b5714f5c4576e82c1a84fa1debe0c24a7e0c3ec4

RUN apt-get update \
    && apt-get install --yes --no-install-recommends \
        bubblewrap ca-certificates git util-linux \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 10001 --create-home --home-dir /var/lib/verifier verifier

COPY jadpo /usr/local/bin/jadpo

USER 10001
WORKDIR /var/lib/verifier
# It runs itself, whatever a package says. Earlier releases it checks a
# package with are kept in its cache.
ENV JADPO_TOOLCHAIN=local XDG_CACHE_HOME=/var/lib/verifier/cache
EXPOSE 8080

ENTRYPOINT ["/usr/local/bin/jadpo", "package", "verify", "--serve", "--listen", "0.0.0.0:8080"]
