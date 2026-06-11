# Vision and pivot

**Mode:** Wander

## Intent

sshtool's queued work all pushes toward being a faithful drop-in `ssh`. Before continuing down that path, step back and examine the actual problems the tool should solve, sketch what a solution to them might look like, and weigh that potential direction against what sshtool is today — small, focused, self-contained — to decide whether to pivot, refine the current goal, or hold course. This is deliberately open-ended: the destination isn't known going in, and the exploration may well conclude that no pivot is warranted.

## Log

- Change opened to explore the rationale for the application. Working in three movements: problems to solve → solution sketch → step-back assessment against what exists today and the over-reach risk.

### Movement one — problems (from the customer base)

Raw observations:

- Audience is mixed Linux/Windows, **more often Windows**.
- They **won't set up agent/key auth** even though it's available on Windows — key generation and installing the public key on remotes is too hard. Idea floated: a TUI that guides them through it.
- Heavy users of **SSH tunnels / port forwarding**; a brief network blip drops the tunnel and frustrates them. Idea: a tool that **auto-reconnects a dropped tunnel**.
- TUI for **editing `~/.ssh/config`** and possibly **managing `known_hosts`**.
- **Logging of dropped connections** with best-possible cause attribution (e.g. local WiFi dropped vs remote-side disconnect) so a **support team** can diagnose recurring failures.

Emergent clustering (three themes + an audience signal):

- **Audience signal (load-bearing):** the user is *not* a terminal power user wanting a cleaner `ssh`. They are a **non-expert, Windows-first user** who needs SSH to be *easy to set up* and *stay up*, with a **support team standing behind them**. This reframes the whole product away from the queued "faithful drop-in `ssh`" direction.
- **Theme A — Guided setup / config management.** Key generation + remote install, `~/.ssh/config` editing, `known_hosts` management. Form is a TUI assistant for the fiddly parts of SSH.
- **Theme B — Resilient tunnelling.** Long-running, auto-reconnecting port forwards that survive transient network loss. A persistent background lifecycle, not a one-shot command.
- **Theme C — Connection diagnostics.** Drop logging with failure-cause classification, consumed by both the end-user and a central support function.

Tension noted for movement three: current sshtool is a **one-shot** SSH client (run-a-command / interactive PTY). Themes B and C need a **persistent, monitored, reconnecting** lifecycle the current model doesn't have; Theme A needs a **TUI + local/remote config mutation** the current model doesn't have. Of the four queued changes, only `ssh-config-support` clearly survives this reframe; login-shell, stdin-forwarding, and escape-sequences are `ssh`-fidelity features that this audience may not care about.

Sharpening answers:

- **A + B travel together, no single spearhead.** Setup (A) gets the non-expert connected at all; resilience (B) keeps them connected. Jointly: "make SSH tunnels actually usable for a non-expert." Treat as a pair, not ranked.
- **Diagnostics (C) is a bounded nice-to-have.** Goal is a **local log file the user uploads to support** — not telemetry or a fleet/observability story. Scope fork avoided: "good exportable local logging," not a backend.
- **Form is dual-mode (decisive).** Bare invocation (`sshtool`, no args) → **TUI**; any arguments → **CLI as today**. The pivot is therefore *additive*: the existing CLI is preserved as the args path, the TUI is layered on as the no-args path. Big de-risking of the over-reach concern.

### Movement two — solution sketch

**One-liner:** an SSH companion for Windows-first non-experts — *get connected, stay connected, prove what went wrong* — presenting a TUI when launched bare and the existing CLI when given arguments.

**Get connected (A) — multi-hop key install.** Users typically reach the destination *through a jumphost*, so a key must be installed on **both** the jumphost and the destination. The wizard does both effortlessly (generate keypair → install on each hop via the existing password-auth + jumphost path → write a tidy `~/.ssh/config` block). `known_hosts` management rides alongside.

**Stay connected (B) — the "SSH VPN".** The product's plain-language pitch to users. A **saved connection** bundles endpoint + jumphost + a set of port forwards behind one launchable thing. In the TUI, forwards are configured with one-click presets (RDP, NoMachine, …); the connection is **saved, established, and auto-sustained** across network blips. Persistence model decided: **process-lifetime only** — sustained while the app/command runs, no daemon or service (users lack admin rights). The sustained-tunnel capability is **also available in CLI mode**, not TUI-only.

**Prove what happened (C).** Per-connection drop log with best-effort cause attribution (local network loss vs remote-side close), exportable to a single file the user uploads to support. Bounded nice-to-have.

**Preserved from today:** the entire CLI, plus `connection` / `authentication` / `endpoint` / jumphost logic. The TUI, the saved-connection store, and the tunnel supervisor are *new surfaces on top*, not replacements.

Deferred design details (not needed for the assessment): where saved connections live (own config vs `~/.ssh/config`), and the mechanics of drop-cause attribution.

### Movement three — step-back assessment

**Verdict: pivot, but phased — engine-first, TUI-last.** The vision is sound because it is anchored to one audience's reality (Windows-first non-expert who can't provision keys across a jumphost and whose tunnels die on blips, with a support team behind them). The dual-mode decision makes it *additive*, not a teardown. It is **not over-reaching as a direction**; it would only over-reach if built as one monolithic effort.

Risk flags named:

- **TUI on Windows is the biggest unknown.** A full TUI inherits all the Windows console VT/raw-mode/resize difficulty the just-completed interactive-terminal change fought — at greater surface, on the exact platform the audience uses. Defer it until the engine beneath is proven.
- **Drop-cause attribution (C) is fuzzy.** "Local WiFi vs remote disconnect" is hard to call reliably from the SSH/TCP layer. Keep best-effort (log error kind, timing, which side closed); resist gold-plating since it is a nice-to-have.

**Phasing (each independently shippable; 1–2 are pure CLI so value is never blocked on the hard TUI work):**

1. **Sustained tunnel (CLI)** — auto-reconnecting `-L`-style forward, foreground, process-lifetime. Highest value, lowest new surface; proves the resilience engine.
2. **Multi-hop key install (CLI)** — a setup / copy-id subcommand that provisions jumphost *and* destination. Reuses auth + jumphost.
3. **TUI front door** — saved connections, preset forwards, live status, wrapping the proven engine. Riskiest surface, comes after the engine works.
4. **Diagnostics** — exportable drop log. Lowest priority; threads through the above.

### Conclusions (for resume / replanning)

- **Direction: pivot to an "SSH companion for Windows-first non-experts" — get connected, stay connected, prove what went wrong.** Dual front door: bare `sshtool` → TUI; `sshtool <args>` → CLI as today. Additive, not a teardown.

- **Possible architecture (speculative):** a **core library** with **separate applications for the CLI and the fatter TUI** — a workspace with a shared core crate consumed by a thin CLI binary and a richer TUI binary. To be decided when planning, not now.

- **Queued changes are restructured under this vision, not dropped.** The "faithful SSH replacement" angle (login-shell, stdin-forwarding, escape-sequences) is still wanted — it just comes *later*, sequenced behind the pivot's core. `ssh-config-support` survives and grows into the saved-connection / config story.

- **Design principle — lean on existing SSH conventions (decided, applies when config persistence is planned).** Save connections in `~/.ssh/config` where possible so stock SSH clients can use them too. Rationale: **graceful fallback** — if the app ever fails the user, they can fall back on normal `ssh` and are not stranded.

- **Design principle — keep CLI args/flags as close to normal `ssh` as possible.** Same fallback rationale: familiarity means users can swap in stock `ssh` when needed.

- **Next time — entry point:** begin re/planning the queued changes restructured under this vision. Natural first concrete change is **Phase 1 (sustained auto-reconnecting tunnel, CLI)**. This vision-and-pivot Wander stays **open and active** as the reference frame for that replanning; revisit whether to conclude/archive it once the follow-on changes exist.

## Conclusion

Explored the product direction in three movements: problems from the customer base, a solution sketch, and a step-back assessment. Outcome: pivot to an "SSH companion for Windows-first non-experts" — get connected, stay connected, prove what went wrong — with a dual front door (bare invocation → TUI, args → CLI as today).

Queued changes restructured under four sequenced phases: (1) sustained auto-reconnecting tunnel CLI, (2) multi-hop key install CLI, (3) `~/.ssh/config` support, (4) TUI front door, (5) diagnostics, with the original SSH-fidelity changes sequenced last (#6–8). New stubs opened; existing changes renumbered.

No changelog entry — this was a planning exploration, no code changed.
