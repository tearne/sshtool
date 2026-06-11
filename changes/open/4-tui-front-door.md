# TUI front door

**Mode:** _(to be set after Intent approval)_

## Intent

Add a TUI launched when `sshtool` is invoked with no arguments. It surfaces saved connections (with their port forwards), lets the user launch and monitor them, and provides access to the key-install wizard. The CLI path (`sshtool <args>`) is unchanged. The TUI is the riskiest new surface — Windows console VT/raw-mode/resize difficulty is a known concern — so it comes after the CLI engine (phases 1 and 2) is proven.
