#!/usr/bin/env bash
# One-time setup: use the versioned hooks in .githooks/
cd "$(git rev-parse --show-toplevel)" && git config core.hooksPath .githooks && echo "Git hooks installed (.githooks)"
