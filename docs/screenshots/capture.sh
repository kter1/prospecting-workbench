#!/usr/bin/env bash
# Capture the three README screenshots.
#
#   bash docs/screenshots/capture.sh
#
# Each step tells you exactly what must be on screen before you capture.
# If what you see does not match, fix it first — a mismatched frame is worse
# than no frame, because the README caption will describe something else.

set -euo pipefail
cd "$(dirname "$0")"

shot() {
  local file="$1" title="$2"; shift 2
  echo
  echo "══════════════════════════════════════════════════════════"
  echo "  $title"
  echo "  → $file"
  echo "══════════════════════════════════════════════════════════"
  for line in "$@"; do echo "     $line"; done
  echo
  read -r -p "  Matches what you see? Enter to capture, then click the window… "
  screencapture -w "$file"
  if [ -f "$file" ]; then
    sips -Z 1600 "$file" >/dev/null 2>&1 || true
    echo "  ✓ saved ($(du -h "$file" | cut -f1))"
  else
    echo "  ✗ cancelled — re-run the script to retry this shot"
  fi
}

cat <<'INTRO'
══════════════════════════════════════════════════════════
  Prepare first — this matters
══════════════════════════════════════════════════════════
  1. Setup → "Clear all prospects" → confirm.
     Removes real company data so none of it is published.

  2. Discover. Paste this, geography United States, limit 10:

     I sell a voice intake system to businesses that depend on
     inbound calls. Find companies showing evidence of after-hours,
     appointment, or dispatch workflows.

  3. Press "Build search plan" — but do NOT start the search yet.
     Shot 1 is the plan. Then start it and let it finish for shots 2 and 3.
INTRO

shot "01-discover.png" "SHOT 1 — the compiled plan, before the run" \
  "ON SCREEN: Discover tab. A panel headed 'Review this plan, then start'." \
  "MUST SEE:  the segments/signals it inferred, the discovery lanes with" \
  "           automatic/guided labels, and the 'Assumptions to review' list." \
  "NOT THIS:  the empty form with grey placeholder text (plan not built)." \
  "NOT THIS:  a running search with a progress bar."

shot "02-results.png" "SHOT 2 — finished results" \
  "ON SCREEN: Discover tab, the run panel says complete (not a % in progress)." \
  "MUST SEE:  the counts line (discovered / researched / requested) and at" \
  "           least one of 'Strong Matches' or 'Near Matches' with fit scores." \
  "NOT THIS:  a progress bar mid-run, or '0/0 researched'."

shot "03-evidence.png" "SHOT 3 — evidence ledger" \
  "ON SCREEN: Prospects tab, one account selected, scrolled to 'Evidence ledger'." \
  "MUST SEE:  an expanded excerpt with its source link and freshness label." \
  "PICK WELL: choose an account whose 'People and business contacts' panel" \
  "           reads 'Not publicly found'. That frame proves the honesty claim" \
  "           AND avoids publishing a real company's phone number." \
  "NOT THIS:  any frame showing a real phone number or email address."

echo
echo "══════════════════════════════════════════════════════════"
echo "  Check before committing"
echo "══════════════════════════════════════════════════════════"
echo "  • no phone number, email, or sender identity visible"
echo "  • no unrelated windows or menu bar captured"
ls -lh ./*.png 2>/dev/null | awk '{print "  •", $9, $5}'
echo
echo "  Then verify the images before committing them."
