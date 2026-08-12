#!/usr/bin/env bash
# Capture the three README screenshots.
#
# Run this, then follow the prompts. For each shot you set the app up, press
# Enter, and click the app window once. macOS writes a clean PNG with the
# window shadow, so nothing needs cropping.
#
#   bash docs/screenshots/capture.sh

set -euo pipefail
cd "$(dirname "$0")"

shot() {
  local file="$1" title="$2" setup="$3" must_show="$4"
  echo
  echo "──────────────────────────────────────────────────────────"
  echo "  $title  →  $file"
  echo "──────────────────────────────────────────────────────────"
  echo "  Set up:     $setup"
  echo "  Must show:  $must_show"
  echo
  read -r -p "  Ready? Press Enter, then click the app window… "
  screencapture -w "$file"
  if [ -f "$file" ]; then
    sips -Z 1600 "$file" >/dev/null 2>&1 || true
    echo "  ✓ saved $(du -h "$file" | cut -f1)"
  else
    echo "  ✗ not saved (capture cancelled)"
  fi
}

cat <<'INTRO'
Screenshot capture — three images for the README.

First, so no real company's contact details end up in a public repo:
  Setup → "Clear all prospects", then run one search worth showing.

  Query:      I sell a voice intake system to businesses that depend on
              inbound calls. Find companies showing evidence of after-hours,
              appointment, or dispatch workflows.
  Geography:  Ohio
  Limit:      Up to 10 strong matches

Let the run finish before starting.
INTRO

shot "01-discover.png" "Discover — plan compiled" \
  "Discover tab, after 'Build search plan', before starting the run." \
  "Your description, the compiled plan, discovery lanes with automatic/guided labels, and the assumptions list."

shot "02-results.png" "Discover — run finished" \
  "Same tab once the run completes." \
  "100% progress, the discovered/researched/requested counts, and both Strong and Near Match sections with fit scores."

shot "03-evidence.png" "Prospects — evidence ledger" \
  "Prospects tab, one account selected, evidence ledger expanded." \
  "A real excerpt with its source and freshness label, plus the qualification panel with hard gates and cited reasons."

echo
echo "──────────────────────────────────────────────────────────"
echo "Check before committing:"
echo "  • no personal email, phone, or sender identity visible"
echo "  • no unrelated windows or menu-bar content captured"
ls -lh ./*.png 2>/dev/null | awk '{print "  •", $9, $5}'
echo
echo "Then tell Claude they are in place, or commit them yourself."
