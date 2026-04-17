set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

bin_name := "svg2tex"
release_bin := "target/release/svg2tex"
local_bin_dir := env_var_or_default("LOCAL_BIN_DIR", env_var("HOME") + "/.local/bin")

default:
    just --list

build:
    cargo build --release

chmod: build
    chmod +x "{{release_bin}}"

install: chmod
    mkdir -p "{{local_bin_dir}}"
    mv -f "{{release_bin}}" "{{local_bin_dir}}/{{bin_name}}"
