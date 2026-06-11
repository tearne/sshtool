# Diagnostics

**Mode:** _(to be set after Intent approval)_

## Intent

Add per-connection drop logging with best-effort cause attribution — distinguishing local network loss from remote-side disconnects where possible — written to a local log file the user can export and send to a support team. Bounded scope: good exportable local logging, not telemetry or a backend. Lowest-priority phase; threads through the tunnel and connection machinery added in earlier phases.
