#!/usr/bin/env bash
set -euo pipefail

source shell/lib.sh

deploy_all() {
  build_artifact
  log_line "deployed"
}

deploy_all
