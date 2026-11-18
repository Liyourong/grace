#!/usr/bin/env bash
set -euo pipefail
# One release number per first-parent dev commit after the last pre-pipeline commit.
baseline=bcffa6bb4bb0eaae2827e45d48b82b2bec4eef46
git merge-base --is-ancestor "$baseline" "$1"
count=$(git rev-list --first-parent --count "$baseline..$1")
(( count > 0 ))
echo "v$((5 + count))"
