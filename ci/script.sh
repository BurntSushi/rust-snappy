#!/bin/sh

set -ex

RUST_TARGET="x86_64-unknown-linux-gnu"

if [ "${TRAVIS_RUST_ARCHITECTURE}" = "i386" ]; then
  RUST_TARGET="i686-unknown-linux-gnu"
fi

if [ "${RUST_TARGET}" = "i686-unknown-linux-gnu" ]; then
  apt-get update
  apt-get install -y gcc-multilib
fi

rustup target add ${RUST_TARGET}

cargo build --verbose --target "${RUST_TARGET}"
cargo doc --verbose --target "${RUST_TARGET}"

# If we're testing on an older version of Rust, then only check that we
# can build the crate. This is because the dev dependencies might be updated
# more frequently, and therefore might require a newer version of Rust.
#
# This isn't ideal. It's a compromise.
if [ "$TRAVIS_RUST_VERSION" = "1.21.0" ]; then
  exit
fi

cargo test --verbose --target "${RUST_TARGET}"
if [ "$TRAVIS_RUST_VERSION" = "nightly" ]; then
  cargo bench --verbose --no-run --target "${RUST_TARGET}"
fi
