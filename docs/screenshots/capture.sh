#!/usr/bin/env bash
# Capture the one screenshot still missing from the README.
#
#   bash docs/screenshots/capture.sh
#
# qualification.png is already in place. This captures live-search.png:
# a search actually running against public sources.

set -euo pipefail
cd "$(dirname "$0")"

cat <<'INTRO'
══════════════════════════════════════════════════════════
  live-search.png — a search in progress
══════════════════════════════════════════════════════════
  1. Discover tab. Type anything real, for example:

     I sell a voice intake system to businesses that depend on
     inbound calls.

  2. Build search plan → Start search.
  3. While it is still running, capture. The frame wants the
     progress bar, the stage line ("Searching Mapped local
     businesses"), and the elapsed timer.

  Company names and domains are fine to show. Avoid your own
  sender identity and any personal or mobile number.
INTRO

read -r -p "  Search running? Press Enter, then click the app window… "
screencapture -w live-search.png

if [ -f live-search.png ]; then
  sips -Z 1600 live-search.png >/dev/null 2>&1 || true
  echo "  ✓ saved live-search.png ($(du -h live-search.png | cut -f1))"
  echo
  ls -lh ./*.png | awk '{print "  •", $9, $5}'
  echo
  echo "  Check the frame before committing."
else
  echo "  ✗ cancelled — re-run to retry"
fi
