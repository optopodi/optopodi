#!/bin/bash
# update sub-module rust-playground
git submodule update --remote rust-playground
# change into proper top-crates directory — exit if fail
cd rust-playground/top-crates || exit
# run top-crates binary
cargo run
# copy the generated `crate-information.json` to a
# newly-created `crate-information.json` in the base directory
cp ../compiler/base/crate-information.json ../../crate-information.json
