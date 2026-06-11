# Sustained tunnel (CLI)

**Mode:** _(to be set after Intent approval)_

## Intent

Add an auto-reconnecting port-forward mode to the CLI: a foreground, process-lifetime tunnel that survives transient network loss by detecting drops and re-establishing the connection transparently. This is the resilience engine at the heart of the pivot — the highest-value, lowest-new-surface phase. Arguments stay close to `ssh -L` convention. No daemon or background service; the tunnel lives as long as the process runs.
