# Screenshot capture guide

Three images, referenced by the README. Capture them with a clean database so no
real company's contact details end up in a public repository.

## Before capturing

1. Open the app, go to **Setup → Clear all prospects**. This removes accounts,
   evidence and search history while keeping your offer profile and settings.
2. Run one search worth showing. A vertical with good OpenStreetMap coverage
   demonstrates the full pipeline:

   > I sell a voice intake system to businesses that depend on inbound calls.
   > Find companies showing evidence of after-hours, appointment, or dispatch workflows.

   Geography `Ohio`, limit `Up to 10 strong matches`.
3. Let the run finish so the results panel shows real tiers and counts.

## Capturing

`screencapture -w` waits for you to click a window and writes a clean PNG with
the macOS window shadow — no cropping needed. Run each line, then click the app
window.

```sh
screencapture -w ~/Projects/prospecting-workbench/docs/screenshots/01-discover.png
screencapture -w ~/Projects/prospecting-workbench/docs/screenshots/02-results.png
screencapture -w ~/Projects/prospecting-workbench/docs/screenshots/03-evidence.png
```

## What each frame should show

| File | View | What must be visible |
| --- | --- | --- |
| `01-discover.png` | Discover, plan compiled | The description you typed, the compiled plan with segments and signals, the discovery lanes with their automatic/guided labels, and the assumptions list. This frame proves the app shows its reasoning before acting. |
| `02-results.png` | Discover, run finished | The progress bar at 100%, the counts line (discovered / researched / requested), and both Strong Matches and Near Matches sections with fit scores. This frame proves it ranks and separates rather than dumping a list. |
| `03-evidence.png` | Prospects, one account open | The evidence ledger expanded on a real excerpt with its source and freshness label, plus the qualification panel showing hard gates and cited reasons. This frame proves the citation claim. |

## Checks before committing

- No personal email address, phone number, or sender identity is visible.
- No unrelated desktop windows or menu-bar content were captured.
- Files are PNG and each under about 1 MB. Shrink if needed:
  `sips -Z 1600 docs/screenshots/*.png`
