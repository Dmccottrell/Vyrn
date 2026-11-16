#!/bin/sh
# Builds the embedding library and runs the C tests against it.
set -e
cd "$(dirname "$0")/.."
cargo build --release
mkdir -p target/ctest
for t in embed_test unreal_test; do
    cc -Wall -Wextra -Iinclude tests/$t.c -Ltarget/release -lvyrn -lm -Wl,-rpath,"$PWD/target/release" -o target/ctest/$t
done
cd target/ctest
LD_LIBRARY_PATH=../release DYLD_LIBRARY_PATH=../release ./embed_test
LD_LIBRARY_PATH=../release DYLD_LIBRARY_PATH=../release ./unreal_test ../../../../unreal/VyrnScripting/Scripts
