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
  Two shots are needed. qualification.png is already captured.

  1. Discover. Paste this, geography United States, limit 10:

     I sell a voice intake system to businesses that depend on
     inbound calls. Find companies showing evidence of after-hours,
     appointment, or dispatch workflows.

  2. Press "Build search plan" — but do NOT start the search yet.
     Shot 1 is the plan. Then start it, let it finish, and open a
     prospect for shot 2.
INTRO

shot "search-plan.png" "SHOT 1 of 2 — the compiled plan, before the run" \
  "ON SCREEN: Discover tab. A panel headed 'Review this plan, then start'." \
  "MUST SEE:  the segments/signals it inferred, the discovery lanes with" \
  "           automatic/guided labels, and the 'Assumptions to review' list." \
  "NOT THIS:  the empty form with grey placeholder text (plan not built)." \
  "NOT THIS:  a running search with a progress bar."

shot "evidence-ledger.png" "SHOT 2 of 2 — evidence ledger" \
  "ON SCREEN: Prospects tab, one account selected, scrolled to 'Evidence ledger'." \
  "MUST SEE:  an expanded excerpt with its source link and freshness label." \
  "PICK WELL: an account whose contacts read 'Not publicly found' makes the" \
  "           strongest frame — it demonstrates the central claim directly." \
  "FINE:      company names, domains and a business main line are public." \
  "NOT THIS:  a personal or mobile number, or your own sender identity."

echo
echo "══════════════════════════════════════════════════════════"
echo "  Check before committing"
echo "══════════════════════════════════════════════════════════"
echo "  • no personal/mobile number or your own sender identity visible"
echo "  • no unrelated windows or menu bar captured"
ls -lh ./*.png 2>/dev/null | awk '{print "  •", $9, $5}'
echo
echo "  Then verify the images before committing them."
