#!/bin/bash
# Install Loka CLI.
# Requires Rust toolchain (cargo) to be installed.

set -e

echo "Building Loka (release)..."
cargo build --release -p loka-cli

INSTALL_DIR="${HOME}/.loka/bin"
mkdir -p "$INSTALL_DIR"

echo "Installing to $INSTALL_DIR/loka ..."
cp target/release/loka "$INSTALL_DIR/loka"
chmod +x "$INSTALL_DIR/loka"

echo ""
echo "Done! Add $INSTALL_DIR to your PATH if not already there:"
echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
echo ""
echo "Usage:"
echo "  loka serve                    Start the HTTP server"
echo "  loka query \"SELECT ...\"       Run a SPARQL query"
echo "  loka import data.nt           Import N-Triples file"
echo "  loka export -o dump.nt        Export all triples"
echo "  loka info                     Show database statistics"
echo ""
