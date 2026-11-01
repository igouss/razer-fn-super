#!/usr/bin/env bash
set -euo pipefail
repo=$(cd "$(dirname "$0")" && pwd)

cargo build --release --locked --manifest-path "$repo/Cargo.toml" -p razer-fn-super
sudo install -Dm755 "$repo/target/release/razer-fn-super" /usr/local/bin/razer-fn-super
sudo install -Dm644 "$repo/system/razer-fn-super.service" /etc/systemd/system/razer-fn-super.service
sudo install -Dm644 "$repo/system/70-razer-fn-super.rules" /etc/udev/rules.d/70-razer-fn-super.rules
sudo systemctl daemon-reload
sudo udevadm control --reload
sudo systemctl try-restart razer-fn-super.service
