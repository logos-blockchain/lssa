#!/usr/bin/env bash
set -e

# Base directory for deployment
LSSA_DIR="/home/arjentix/test_deploy/lssa"

# Expect GITHUB_ACTOR to be passed as first argument or environment variable
GITHUB_ACTOR="${1:-${GITHUB_ACTOR:-unknown}}"

# Function to log messages with timestamp
log_deploy() {
  echo "[$(date '+%Y-%m-%d %H:%M:%S %Z')] $1" >> "${LSSA_DIR}/deploy.log"
}

# Error handler
handle_error() {
  echo "✗ Deployment failed by: ${GITHUB_ACTOR}"
  log_deploy "Deployment failed by: ${GITHUB_ACTOR}"
  exit 1
}

find_sequencer_runner_pids() {
  pgrep -f "sequencer_runner" | grep -v $$
}

# Set trap to catch any errors
trap 'handle_error' ERR

# Log deployment info
log_deploy "Deployment initiated by: ${GITHUB_ACTOR}"

# Navigate to code directory
if [ ! -d "${LSSA_DIR}/code" ]; then
  mkdir -p "${LSSA_DIR}/code"
fi
cd "${LSSA_DIR}/code"

# Stop current sequencer if running
if find_sequencer_runner_pids > /dev/null; then
  echo "Stopping current sequencer..."
  find_sequencer_runner_pids | xargs -r kill -SIGINT || true
  sleep 2
  # Force kill if still running
  find_sequencer_runner_pids | grep -v $$ | xargs -r kill -9 || true
fi

# Clone or update repository
if [ -d ".git" ]; then
  echo "Updating existing repository..."
  git fetch origin
  git checkout main
  git reset --hard origin/main
else
  echo "Cloning repository..."
  git clone https://github.com/logos-blockchain/lssa.git .
  git checkout main
fi

# Install Nomos circuits for compatibility
chmod 777 /ci_scripts/setup-nomos-circuits.sh
/ci_scripts/setup-nomos-circuits.sh

# Build sequencer_runner and wallet in release mode
echo "Building sequencer_runner"
# That could be just `cargo build --release --bin sequencer_runner --bin wallet`
# but we have `no_docker` feature bug, see issue #179
cd sequencer_runner
cargo build --release
cd ../wallet
cargo build --release
cd ..

# Run sequencer_runner with config
echo "Starting sequencer_runner..."
export RUST_LOG=info
nohup ./target/release/sequencer_runner "${LSSA_DIR}/configs/sequencer" > "${LSSA_DIR}/sequencer.log" 2>&1 &

# Wait 5 seconds and check health using wallet
sleep 5
if ./target/release/wallet check-health; then
  echo "✓ Sequencer started successfully and is healthy"
  log_deploy "Deployment completed successfully by: ${GITHUB_ACTOR}"
  exit 0
else
  echo "✗ Sequencer failed health check"
  tail -n 50 "${LSSA_DIR}/sequencer.log"
  handle_error
fi
