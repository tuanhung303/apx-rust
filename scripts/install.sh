#!/usr/bin/env bash
# apx-rust installer — builds apx + apx-mcp from GitHub and registers the
# apx MCP server so agents (Hermes, Codex) can use `apx` instead of
# apply_patch for file edits.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/tuanhung303/apx-rust/main/scripts/install.sh | bash
#   # or: git clone https://github.com/tuanhung303/apx-rust && cd apx-rust && ./scripts/install.sh
#
# Env overrides:
#   APX_SRC_DIR   clone/build location (default: ~/src/apx-rust)
#   APX_BIN_DIR   symlink destination (default: ~/.local/bin)
#   APX_SKIP_MCP  set to 1 to skip registering the Hermes MCP server
set -euo pipefail

REPO_URL="${APX_REPO_URL:-https://github.com/tuanhung303/apx-rust.git}"
SRC_DIR="${APX_SRC_DIR:-$HOME/src/apx-rust}"
BIN_DIR="${APX_BIN_DIR:-$HOME/.local/bin}"

echo "==> apx-rust installer"
echo "    repo:   $REPO_URL"
echo "    source: $SRC_DIR"
echo "    bin:    $BIN_DIR"

# 1. Clone or update the source checkout.
if [[ -d "$SRC_DIR/.git" ]]; then
  echo "==> updating existing checkout in $SRC_DIR"
  git -C "$SRC_DIR" pull --ff-only
else
  echo "==> cloning $REPO_URL"
  mkdir -p "$(dirname "$SRC_DIR")"
  git clone "$REPO_URL" "$SRC_DIR"
fi

# 2. Build release binaries (requires Rust toolchain, MSRV 1.95+).
echo "==> building release binaries (cargo build --release)"
cargo build --release --manifest-path "$SRC_DIR/Cargo.toml"

# 3. Symlink apx + apx-mcp into BIN_DIR.
echo "==> installing binaries into $BIN_DIR"
mkdir -p "$BIN_DIR"
for bin in apx apx-mcp; do
  ln -sf "$SRC_DIR/target/release/$bin" "$BIN_DIR/$bin"
  echo "    $BIN_DIR/$bin -> $SRC_DIR/target/release/$bin"
done

# 4. Register the MCP server with Hermes (best-effort; skip with APX_SKIP_MCP=1).
if [[ "${APX_SKIP_MCP:-0}" != "1" ]] && command -v hermes >/dev/null 2>&1; then
  echo "==> registering apx MCP server with Hermes"
  if hermes mcp list 2>/dev/null | grep -q "apx"; then
    echo "    apx MCP server already registered (hermes mcp remove apx to re-add)"
  else
    hermes mcp add apx --command "$BIN_DIR/apx-mcp" --connect-timeout 30 || {
      echo "    WARNING: hermes mcp add failed — register manually:"
      echo "      hermes mcp add apx --command $BIN_DIR/apx-mcp"
    }
  fi
else
  echo "==> skipping Hermes MCP registration (hermes not found or APX_SKIP_MCP=1)"
  echo "    register manually with:"
  echo "      hermes mcp add apx --command $BIN_DIR/apx-mcp"
fi

echo ""
echo "==> done."
echo "    apx CLI:     $BIN_DIR/apx --help"
echo "    apx MCP:     $BIN_DIR/apx-mcp (stdio)"
echo "    Hermes MCP:  hermes mcp list && hermes mcp test apx"
echo "    Restart Hermes so the new MCP tools (mcp_apx_apx, mcp_apx_peek) load."
echo "    Steering: see https://github.com/tuanhung303/apx-rust/blob/main/docs/codex-agent-install.md"
