# Working on this crate

`mcan` is a stand-alone `no_std` driver for the Bosch M_CAN / ST FDCAN peripheral. Intended targets: STM32 H7,
H5, G0, G4 and L5. Today only H7 and (partially) G0 exist.
It has no external PAC or HAL dependency: registers live in `src/pac` (generated from stm32-data). Optional
async support is built on embassy-sync. Design goals are listed in [README.md](README.md).

> **Temporary:** the crate is under heavy development. Breaking changes are not only acceptable but wanted
> if they bring a better design: don't keep compatibility shims, deprecated aliases or awkward APIs just to
> avoid a break. Still mark them **Breaking:** in CHANGELOG.md.

## FEATURES.md is the source of truth

[FEATURES.md](FEATURES.md) lists every feature with its status (done / partial / broken / not started / idea),
its relevance for analyzers vs. nodes, and its test coverage.

- **Read it before starting** a task. Feature IDs (`R1`, `X4`, …) are the shared vocabulary for issues,
  commits and TODOs.
- **Name IDs with a short slug when talking to the user** (answers, plans, summaries, tables):
  `E6 cnt-driver-stats`, never a bare `E6`. The slug is 2-4 kebab-case words from the item's title. Commit
  messages, CHANGELOG and code `TODO`s keep the bare ID.
- **Update it in the same change** that implements, fixes, breaks, removes or tests something. That includes
  the Tests column and the *Last full audit* line when you re-audit.
- Do not track status anywhere else (README checklists, TODO files, etc.). Code `TODO`s should reference a
  feature ID.
- If you discover a bug, add it to the relevant row (status 🔴 Broken plus a note) even if you don't fix it.

## CHANGELOG.md records every change

[CHANGELOG.md](CHANGELOG.md) is the history (what changed), FEATURES.md is the current status. Keep doing both:

- Every change that a user or contributor would notice gets an entry under `## [Unreleased]`, in the same
  change: added / changed / fixed / removed features, API breaks (mark them **Breaking:**), new cargo
  features, dependency or `stm32-metapac` bumps, and tooling. Pure refactors and doc typos don't need one.
- Reference feature IDs in parentheses, e.g. `(P7a)`. Keep entries short and user-facing; details belong
  in FEATURES.md and commit messages.
- On a release, rename `[Unreleased]` to the version and date and start a new empty `[Unreleased]`.

## Layout

| Path | Contents |
|---|---|
| `src/fdcan.rs` | Instances, clocks/reset, typestate modes, core check |
| `src/config.rs` | `FdCanConfig`, bit timing, mode transitions, register application, `set_layout` |
| `src/message_ram_builder.rs` | Const typestate builder for the H7 message RAM layout |
| `src/message_ram_layout.rs` | `MessageRamLayout`, element accessors, `TxBufferIdx` |
| `src/tx_rx.rs` | Transmit / receive (RX is mostly commented-out code to port) |
| `src/asynchronous.rs`, `src/embassy.rs` | Interrupt handler and wakers, embassy helpers |
| `src/pac/` | Generated registers (full + lite FDCAN, trimmed RCC), message RAM bitfields, address mapping, register-map host tests |
| `boards/` | HIL test board pinouts and hardware notes (one file per `BnnnR` board number) |
| `tools/netlist.py` | KiCad netlist inspector used to extract board pinouts |
| `tools/gen-pac/` | Rust generator for `src/pac` from a pinned stm32-metapac (own `Cargo.lock`, `-- --check` to verify) |
| `hil/<board>/` | HIL test crates (embedded-test + probe-rs), own `Cargo.lock`, toolchain and `.cargo/config.toml`. Run with `cargo test` from the crate directory |
| `examples/h7_embassy/` | Separate crate (own `Cargo.lock`, toolchain and `.cargo/config.toml`) for STM32H725IG |

## Building

Exactly one chip feature must be enabled (`h7`, `g0`; `h5`, `g4` and `l5` are not implemented yet).

```sh
# Library: check every supported combination before committing
cargo build --target thumbv7em-none-eabihf --features h7
cargo build --target thumbv7em-none-eabihf --features h7,embassy
cargo build --target thumbv7em-none-eabihf --no-default-features --features h7   # no rcc
cargo build --target thumbv6m-none-eabi    --features g0          # currently broken, see FEATURES.md P2
cargo clippy --target thumbv7em-none-eabihf --features h7,embassy
cargo test --features h7 && cargo test --features g0
cargo run --manifest-path tools/gen-pac/Cargo.toml -- --check

# Example (run from its directory, it pins its own target and runner)
cd examples/h7_embassy && cargo build

# HIL tests (board + probe attached, see Testing below)
cd hil/b135 && cargo build --tests   # always
cd hil/b135 && cargo test            # with the board connected
```

Keep the library warning-free once the dead code is gone. Don't add `#[allow]` just to silence warnings about
unfinished features.

## Code rules

- No panics in library code: no `unwrap`, `expect`, `todo!`, `unreachable!` on reachable paths, and no
  unchecked arithmetic on user input. Return `Result` with a crate `Error` variant instead.
- Every busy wait goes through `util::checked_wait` (or an async equivalent) with a timeout.
- Prefer `const fn` and typestate. Avoid generics over the peripheral instance.
- Macros are fine where they add value (removing real repetition, small helpers). What to avoid is the style of
  some `stm32xx-hal` crates where whole drivers live inside one giant `macro_rules!`: driver logic must be
  plain Rust that is readable, greppable and debuggable.
- Message RAM is shared between instances on H7: never touch memory outside the current instance's layout.
- `src/pac/common.rs`, `fdcan_*.rs`, `rcc_*.rs` and `mapping_*.rs` are generated by `tools/gen-pac` from a
  pinned `stm32-metapac` (FEATURES.md P7a). Never edit them by hand: change the generator (per-chip data in
  `tools/gen-pac/src/chips.rs`) and re-run it with `cargo run --manifest-path tools/gen-pac/Cargo.toml`, then
  `-- --check`. `fdcan_h7` is the full M_CAN map, `fdcan_v1` the lite map (G0/G4/L5/H5). Driver code uses
  `pac::fdcan`, `pac::rcc` and `pac::mapping` (incl. `rcc_fdcan`). Full/lite differences go behind `cfg` at
  the call site or into `pac::variant`. RCC code must be behind the `rcc` feature.
- Cite the reference manual (Bosch M_CAN user manual page, or ST RM section) in comments for non-obvious
  register behaviour.
- Statistics and event counting use the [`cnt`](https://crates.io/crates/cnt) crate (published version
  from crates.io), both in the driver and in tests/HIL. Don't hand-roll counter structs or atomics.
  - Driver: declare events as a `#[derive(cnt::Count)]` enum and take a `&'static cnt::Counters<E>` per
    instance (cnt *Instance counters*), so the firmware names and places them (e.g. `fdcan1`, `fdcan3`).
    Counting is ISR-safe and lock-free, so it is fine in `on_interrupt`. Users opt out with cnt's `disabled`
    feature. Not plain `cnt!`: `on_interrupt` is one call site for all instances, per-instance call sites
    would reserve buffer words for instances the firmware doesn't use, and `cnt!` counters can't be read
    back on the target. Tracked as FEATURES.md E6.
  - HIL: read the counters on the target with `Counters::get` (embedded-test holds the probe, so `cnt read`
    only works outside it) to assert that no unexpected errors / lost frames / overruns happened during a
    test. Tracked as FEATURES.md Q9.
  - **Never enable cnt's `disabled` feature in a HIL crate.** Features unify across the build, `get` then
    returns 0 and "nothing went wrong" asserts pass without checking anything. Every HIL crate keeps the
    compile-time guard `const _: () = assert!(!cnt::DISABLED, ...)` in its `src/lib.rs`.
- Dependencies: keep them current, record bumps in FEATURES.md (P11), and re-build the example after every
  bump.

## Testing (very important)

**The goal is 100 % coverage, including on real hardware.** A feature only counts as fully done when it has
host tests for its logic and HIL tests for its hardware behaviour, and the Tests column in FEATURES.md shows
it.

### Host tests

- Pure logic (RAM builder arithmetic, layouts, `Dlc`, IDs, bit timing math/validation, bitfield
  encodings, config → register values) gets `#[cfg(test)]` unit tests that run on the host:
  `cargo test --features h7` (and the other chip features as they become available).
- Abstract register access where needed so register-writing code can be tested against a fake register block
  in host tests.
- Measure coverage with `cargo llvm-cov --features h7` (install with `cargo install cargo-llvm-cov` if missing).
- Write a failing test that reproduces a bug before fixing it.

### Hardware-in-the-loop (HIL) tests

- Prepare HIL tests **while you work**: every new or fixed hardware-facing feature gets a HIL test in the same
  change, even if it can't be run right away.
- Harness: a test crate per board under `hil/` (`hil/b135` exists), using
  [`embedded-test`](https://crates.io/crates/embedded-test) with `probe-rs` as the runner, so `cargo test`
  flashes and runs the tests on the target, resetting it before every test. Board bring-up and helpers live in
  the crate's `src/lib.rs`. Keep the embedded-test / probe-rs version pair recorded in FEATURES.md Q3.
  embedded-test only covers rigs on one MCU; board ↔ board and reference-node tests (Q5a, Q6) will need a
  host-orchestrated runner.
- Rigs, from simplest to most capable (FEATURES.md Q4–Q6):
  1. Board + probe only: internal / external loopback.
  1a. Single-wire CAN without transceivers (Nucleos: FDCAN1 ↔ FDCAN2, or across boards).
  2. Two instances on one board through transceivers: B125 Ch1 (FDCAN1) ↔ Ch2 (FDCAN3), on a terminated bus.
  2a. Board ↔ board through transceivers, mixing chip families (B129 G0 ↔ B125/B135 H7).
  3. An external reference node (USB-CAN adapter / SocketCAN on the host) for load tests, full ID sweeps,
     FD/BRS interop and timestamp checks.
- HIL tests run on these boards (details and pinouts in the *Test boards* table in FEATURES.md, keep it updated):
  - **B135A / B135B** (`h7`): `hil/b135`. STM32H725IG, one isolated channel, switchable terminator. Pinout in
    [boards/b135.md](boards/b135.md).
  - **B125A / B125B** (`h7`): `hil/b125`. STM32H725IG, two isolated channels: Ch1 = FDCAN1 (TX PD1), Ch2 = FDCAN3.
    Used for two-node tests on one MCU. Pinout in [boards/b125.md](boards/b125.md).
  - **B129A** CANnify Micro HV (`g0`): `hil/b129`. STM32G0B1CE, FDCAN1 + TCAN1044V, no terminator. PC13 (standby)
    must be driven low. Pinout in [boards/b129.md](boards/b129.md).
  - **NUCLEO-G0B1RE** (`g0`): `hil/nucleo_g0b1re`
  - **NUCLEO-H533RE** (`h5`, once implemented): `hil/nucleo_h533re`

  Custom boards are named `BnnnR` (number + revision). Each board number gets one file in `boards/` covering all
  its revisions and their differences. When the user provides a KiCad netlist, extract the pinout with
  `tools/netlist.py` (see [boards/README.md](boards/README.md)). Record anything the netlist can't tell you
  (assembly-option values, relay polarity, pulse widths) as an open question, not a guess.

  The Nucleos have no transceivers: software tests, loopback and single-wire CAN only (see
  [boards/nucleo.md](boards/nucleo.md); record the chosen pins there). Ask for any pinout or
  clock detail that is missing from FEATURES.md instead of guessing. A feature that is relevant for a chip
  family is only HIL-covered when it passes on every test board of that family.
- **Never assume hardware is attached.** Before running HIL tests, ask the user to connect the required board,
  probe, transceivers or reference node, and say which rig the tests need. Then check with `probe-rs list`.
  If no probe shows up, point the user at the probe-rs udev setup instead of retrying. Report HIL results
  (pass / fail / not run) explicitly and record them in FEATURES.md.
- HIL tests that drive the bus (external loopback, error injection, bus-off) can disturb other nodes. Confirm
  with the user that the bus is isolated first.

## Versions

Every commit with real work bumps the version in the same commit (manifest + CHANGELOG entry), so any build
traces back to a commit:
- Patch for fixes and small changes, minor for features or anything breaking before 1.0, major only when the
  owner says so. In a workspace, only the crates that changed.
- Docs-only, CI-only and no-behaviour-change refactors skip it; a burst of follow-up fixes shares one bump.
- CLIs print version, git SHA and build time in `--version`, e.g. `tool 0.4.2 (a1b2c3d-dirty, built 3 Oct 2026
  18:20)`: a small `build.rs` without extra crates (`git rev-parse --short HEAD`, `-dirty` when
  `git status --porcelain` isn't empty, `rerun-if-changed` on `.git/HEAD` and `.git/index`, `unknown` without
  git). Firmware reports the same through `fw_info`. When touching a CLI that lacks it, add it.
