#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

cargo build --release --locked
sudo install -Dm755 target/release/razer-fn-super /usr/local/bin/razer-fn-super
sudo install -Dm644 system/razer-fn-super.service /etc/systemd/system/razer-fn-super.service
sudo install -Dm644 system/70-razer-fn-super.rules /etc/udev/rules.d/70-razer-fn-super.rules
sudo systemctl daemon-reload
sudo udevadm control --reload
sudo systemctl try-restart razer-fn-super.service
