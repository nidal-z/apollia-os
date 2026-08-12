# Toolchain image for `just linux-check`, built by scripts/linux-check.sh.
# Never built by hand: the script owns the tag, the platform and the cache
# volumes that go with it.
#
# One tag per architecture. The base image is multi-arch, but the compiled
# dependencies that land in the cache volumes are not portable across
# architectures, so the tag carries the architecture and so do the volumes.
FROM rust:1.95-bookworm

# The system dependency list of the "Install system deps" step of the blocking
# Clippy job, .github/workflows/ci.yml:76-82, plus python3-dev.
#
# python3-dev is not in that list because the GitHub runner image already ships
# the CPython headers and .so that PyO3 0.24 needs: apollia-aip enables the
# `auto-initialize` feature (Cargo.toml:214), so the crate links against
# libpython at check time. A bare rust:1.95-bookworm has python3 and no
# headers, so the workspace fails to link without this line.
#
# This is the ninth hand-written copy of that list in this repository and
# nothing confronts it with the eight in ci.yml, three of which already
# diverge. Captured as CAP-115, out of the scope of the lot that wrote it.
RUN apt-get update -qq \
    && apt-get install -y --no-install-recommends \
        libwebkit2gtk-4.1-dev libgtk-3-dev \
        libayatana-appindicator3-dev librsvg2-dev \
        libssl-dev libxdo-dev \
        libasound2-dev libpulse-dev libjack-jackd2-dev \
        cmake clang pkg-config \
        python3-dev \
    && rm -rf /var/lib/apt/lists/*

# rust-toolchain.toml pins 1.95.0 with four components, and rustup honours that
# file over the image default. Installing the components here means a run pays
# for a channel sync once, when the image is built, instead of on every pass.
RUN rustup toolchain install 1.95.0 --profile minimal \
        --component rustfmt --component clippy \
        --component rust-src --component rust-analyzer

# The source tree is mounted read-only, so nothing cargo writes may land inside
# it. The target directory is a volume, and the registry cache lives under the
# default CARGO_HOME, which is a volume too.
ENV CARGO_TARGET_DIR=/target
ENV PYO3_PYTHON=/usr/bin/python3
