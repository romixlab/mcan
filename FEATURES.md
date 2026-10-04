# Feature list — source of truth

This file is the **single source of truth** for what this driver supports, what is planned, and what
is broken. Update it in the same change that adds, fixes, breaks or tests a feature (see
[AGENTS.md](AGENTS.md)).

Last full audit: 2026-10-01 (against commit `3d49144` + dependency bump). PAC section updated 2026-10-02.

## Legend

| Status | Meaning |
|---|---|
| ✅ Done | Implemented, believed correct. Still needs tests unless the Tests column says otherwise. |
| 🟡 Partial | Some of it is implemented; the Notes column says what is missing. |
| 🔴 Broken | Code exists but is known to be wrong or does not compile. |
| ⬜ Not started | Wanted, no code yet (commented-out code counts as not started). |
| 💭 Idea | Might be done, not committed to yet. |

**Use case columns:** *An* = needed for bus analyzers (receive everything, under any load, with accurate
timestamps). *No* = needed for regular nodes. ● required, ○ nice to have, – not relevant.

**Tests column:** `H` = host unit tests, `HIL` = hardware-in-the-loop tests, `–` = none. Goal: 100 %
coverage, with every hardware-facing feature also covered by HIL.

**Intended targets:** STM32 H7, H5, G0, G4, L5 (others with M_CAN should be possible).

**Chips:** H7 = full M_CAN with configurable message RAM (3 instances). G0/G4/L5/H5 = ST "FDCAN lite"
(fixed message RAM layout, RXGFC register, no dedicated TX/RX buffers, 1–3 instances depending on the part;
confirm per reference manual).

## Test boards

Custom boards are named `BnnnR` (number + revision letter). Pinouts and hardware notes: [boards/](boards/README.md).

| Board | MCU | CAN channels | Chip feature | HIL crate | Notes |
|---|---|---|---|---|---|
| [B135A](boards/b135.md) | STM32H725IGKx | 1: FDCAN1 (TX PB9, RX PB8), isolated ADM3050E | `h7` | `hil/b135` | USB HS ↔ FDCAN adapter. Latching termination relay: >10 ms pulse on PC13 enables it, on PE4 disables it. Two RGB LEDs. |
| [B135B](boards/b135.md) | STM32H725IGKx | 1: same as B135A, plus FDCAN3 on header J203 (no transceiver) | `h7` | `hil/b135` | Same as A, plus a button (PE1), the J203 GPIO header with **FDCAN3 (TX PG9 / RX PD12, no transceiver)** and fixture pogo contacts. One RGB LED (B on PH13). `examples/h7_embassy` runs on both revisions. |
| [B125A](boards/b125.md) | STM32H725IGKx | 2: Ch1 = FDCAN1 (TX **PD1**, RX PB8), Ch2 = **FDCAN3** (TX PG9, RX PG10). Separately isolated ADM3050E. | `h7` | `hil/b125` | Ethernet (PoE) + USB HS bridge. Latching terminator per channel (Ch1 PC13/PE4, Ch2 PH14/PA10). OLED, button PE11. Main board for two-node tests on one MCU. |
| [B125B](boards/b125.md) | STM32H725IGKx | 2: same as B125A | `h7` | `hil/b125` | Same pinout and CAN hardware as A. Only SD card detect differs (wrong on A, fixed on B). |
| [B129A](boards/b129.md) (CANnify Micro HV) | STM32G0B1CETxN | 1: FDCAN1 (TX PD1, RX PB8), TCAN1044V, non-isolated, **no terminator**, standby on PC13 (drive low) | `g0` | `hil/b129` | HIL crate exists (bring-up, idle bus; 11 pass, 2026-10-04). The only lite-core board with a transceiver. Pico-Lock connectors. 12 MHz crystal HSE. FDCAN2 pins on the headers. |
| [NUCLEO-G0B1RE](boards/nucleo.md) | STM32G0B1RET6 | none (no transceivers): FDCAN1 + FDCAN2 | `g0` | `hil/nucleo_g0b1re` | Software and loopback tests, single-wire CAN (FDCAN1 ↔ FDCAN2 or to other boards). Proposed pins: FDCAN1 PB9/PB8, FDCAN2 PB13/PB12, 1 kΩ pull-up (not wired yet). |
| [NUCLEO-H533RE](boards/nucleo.md) | STM32H533RET6 | none (no transceivers): FDCAN1 + FDCAN2 | `h5` (not implemented yet) | `hil/nucleo_h533re` | Same as above. Proposed pins: FDCAN1 PB7/PB8, FDCAN2 PB13/PB12, 1 kΩ pull-up (not wired yet). |

---

## 1. Build, platform and crate hygiene

| ID | Feature | Status | An | No | Tests | Notes |
|---|---|---|---|---|---|---|
| P1 | H7 support (`h7`) | 🟡 Partial | ● | ● | – | Builds. See the per-feature status below. |
| P2 | G0 support (`g0`) | 🔴 Broken | ○ | ● | H (regs) | Builds on thumbv7em only. On the real target (thumbv6m) `static_cell` needs CAS, so it only works if the user enables `portable-atomic/critical-section`. `g0,embassy` builds (fixed with I1). Now uses the lite register map (`pac::fdcan_v1`), so the register accesses that exist are correct (global filter goes to RXGFC, BRSE, IR/IE masks). Still missing for lite: RXGFC LSS/LSE/F0OM/F1OM, the fixed RAM layout (no `MessageRamLayout`), ILS group semantics in the API. |
| P3 | G4 support (`g4`) | ⬜ Not started | ○ | ● | – | Feature flag exists, no `mapping`/RCC → does not compile. |
| P4 | L5 support (`l5`) | ⬜ Not started | ○ | ○ | – | Feature flag exists, does not compile. |
| P4a | H5 support (`h5`) | ⬜ Not started | ● | ● | – | Intended target (FDCAN lite). No feature flag, mapping or RCC yet. H533: FDCAN1 @ 0x4000_A400, FDCAN2 @ 0x4000_A800, RAM blocks @ 0x4000_AC00 / 0x4000_AF50 (stm32-data). Test board: NUCLEO-H533RE. |
| P5 | `compile_error!` when no / multiple chip features are selected | ⬜ Not started | ● | ● | – | Today you get cryptic `unresolved import mapping` errors. |
| P6 | `defmt` feature wiring | 🔴 Broken | ● | ● | – | `defmt = []` does not enable `dep:defmt` (`--no-default-features --features h7,defmt` → 485 errors). `default = ["dep:defmt"]` pulls the dependency but `cfg(feature = "defmt")` stays off. |
| P7 | Stand-alone, no external PAC/HAL dependency | ✅ Done | ● | ● | H | Own register layer in `src/pac`, generated by `tools/gen-pac` from a pinned `stm32-metapac` (P7a). RCC trimmed to the registers the driver uses, and optional (`rcc` feature, M2). |
| P7a | Regenerable PAC, full/lite register maps | 🟡 Partial | ● | ● | H | Done: generator with pinned source, full (`fdcan_h7`) + lite (`fdcan_v1`) maps, trimmed RCC, address cross-check, host tests of offsets/bits against the RMs. Open: CI check, full audit of every used field, H5/G4/L5 entries. See *PAC* below. |
| P8 | No panics / asserts, everything returns `Result` | ✅ Done | ● | ● | – | No `unwrap`/`expect`/`todo!`/`unreachable!` left in library code (2026-10-01, incl. `pac/message_ram.rs` bitfield decoders). RAM builder and bit timing validate input and return errors. Keep it that way in new code. |
| P9 | No blocking waits without timeout | ✅ Done | ● | ● | – | `util::checked_wait` with iteration-count timeouts. Not time-based. |
| P10 | Few generics, no giant driver macros, `FdCan` not generic over instance | ✅ Done | ● | ● | – | |
| P11 | Dependencies up to date | ✅ Done | ● | ● | – | Bumped 2026-10-01: bitfield-struct 0.13, embassy-sync 0.8; example on embassy-stm32 0.6 / executor 0.10 / time 0.5, stable toolchain. PAC source: stm32-metapac 21.0.0 (pinned in `tools/gen-pac/Cargo.toml` + `Cargo.lock`, latest as of 2026-10-02). `paste` (unmaintained, RUSTSEC-2024-0436) removed 2026-10-01. |
| P12 | Publishable crate name | ⬜ Not started | – | – | – | `mcan` is taken on crates.io (GrepitAB/mcan). |
| P13 | `embedded-can` traits | 💭 Idea | ○ | ○ | – | |

### PAC (P7a)

`src/pac` is generated by `tools/gen-pac` (Rust, syn/quote/prettyplease) from **stm32-metapac 21.0.0**.
The version is pinned with `=21.0.0` under an impossible `cfg(any())` target in `tools/gen-pac/Cargo.toml`,
so cargo resolves, downloads and checksum-verifies it (`Cargo.lock`, `--locked`) but never compiles it.
Run `cargo run --manifest-path tools/gen-pac/Cargo.toml` to regenerate, and add `-- --check` to verify
(reports out-of-date and stale generated files). Bump the version only on purpose and record it in P11 and
CHANGELOG.md. Per-chip input that isn't in metapac (reference chip, message RAM size, RCC fields for the
clock-source check) lives in `tools/gen-pac/src/chips.rs`.

| File | Source | Notes |
|---|---|---|
| `common.rs` | `src/common.rs` | chiptool `Reg<T, A>` (same as embassy-usb-synopsys-otg), verbatim |
| `fdcan_h7.rs` | `src/peripherals/can_fdcan_h7.rs` | full M_CAN, verbatim |
| `fdcan_v1.rs` | `src/peripherals/can_fdcan_v1.rs` | FDCAN lite (G0/G4/L5/H5), verbatim |
| `rcc_h7.rs` | `src/peripherals/rcc_h7.rs` | trimmed on the syn tree to the FDCAN enable/reset/mux bits plus `rcc_extra`. Debug/defmt are filtered to the kept fields. |
| `rcc_g0.rs` | `src/peripherals/rcc_g0x1.rs` | same |
| `mapping_h7.rs` / `mapping_g0.rs` | `src/chips/stm32h725ig` / `stm32g0b1ce` metadata | instance, message RAM and RCC addresses, `rcc_fdcan` helpers (enable, reset, kernel clock) |
| `message_ram.rs` | hand-written | `bitfield-struct` element layouts. They're values copied in and out of RAM, so they keep this style on purpose. |
| `mod.rs`, `tests.rs` | hand-written | variant selection (`pac::fdcan`, `pac::rcc`, `pac::mapping`), `variant` masks, register-map tests |

Verbatim files only get a header and `crate::common` → `crate::pac::common`, so they stay diffable against
upstream. The generator checks that all FDCAN instances of a chip share one register version and one RCC
enable/reset/mux, and fails on any keep-list entry that doesn't exist upstream. Generated modules are
`#[rustfmt::skip]` and get metapac's lint allowances in `mod.rs`.

Findings (2026-10-02):

- The old `registers.rs` turned out to be metapac's `can_fdcan_h7.rs` from an older release, with hand edits.
  One of them was a wrong doc on `crel()` ("Endian Register"). The regenerated files fixed it.
- Full vs. lite: from 0x80 on almost every offset moves (RXGFC, XIDAM 0x84, HPMS 0x88, RXFS/RXFA 0x90/0x94,
  TXBRP…TXBCIE 0xC8…0xE0, TXEFS/TXEFA 0xE4/0xE8, CKDIV 0x100; no SIDFC/XIDFC/NDATx/RXFC/RXBC/RXESC/TXESC/
  TXEFC/TT*). IR/IE bit positions differ from bit 6 up (TC 7 vs 9, BO 19 vs 25), ILS is completely different
  (lite: 7 group bits), CCCR.BSE is BRSE, TXBC only has TFQM (bit 24), and FIFO index fields are narrower.
  G0 used the H7 map before, so it accessed wrong registers/bits; it now uses `fdcan_v1`.
- Upstream doc nit: G0 `APBENR1.FDCANEN` is documented as "USBEN" in stm32-data (bit 12 is correct).

To do:

1. Run `cargo run --manifest-path tools/gen-pac/Cargo.toml -- --check` in CI (no CI yet).
2. Audit every field the driver uses against RM0468 / RM0444 / the Bosch M_CAN manual and extend
   `src/pac/tests.rs`; add H5/G4/L5 entries to `tools/gen-pac/src/chips.rs` when P3/P4/P4a start.
3. If we ever need to patch upstream data (beyond trimming), consider chiptool's IR transforms instead of
   syn edits (needs chiptool as a pinned git dependency).

## 2. Instances, clocks and operating modes

| ID | Feature | Status | An | No | Tests | Notes |
|---|---|---|---|---|---|---|
| M1 | `FdCanInstances` singleton, take/put back instances | ✅ Done | ● | ● | – | `StaticCell` guard. |
| M2 | Clock enable + reset, clock-source sanity check | ✅ Done | ● | ● | H (regs) | H7 and G0. One clock feeds every instance, so enable/disable only works when all instances are present. Optional `rcc` feature (default on): without it the HAL owns RCC (e.g. `embassy_stm32::rcc::enable_and_reset`) and `FdCanInstances::take` is used instead of `take_enabled`. The no-`rcc` path isn't used by the example yet. |
| M3 | Clock disable (all instances in PoweredDown) | ✅ Done | ● | ● | HIL (B129A) | |
| M4 | Core check (ENDN, CREL) | ✅ Done | ● | ● | – | Accepts only `CREL.REL == 3`. Verify against G0/G4 lite cores. |
| M5 | Typestate modes: PoweredDown → Config → Normal / Restricted / BusMonitoring / Internal loopback / External loopback / Test | 🟡 Partial | ● | ● | HIL (B135B, test mode) | Transitions out of Config exist. `into_powered_down` keeps INIT set while CSR/CSA are set (clearing it timed out; fixed and HIL-tested on B129A, 2026-10-04; not yet on H7). Test mode: normal operation plus `set_tx_pin(TxPinControl)` / `rx_pin()` (TEST.TX / TEST.RX). HIL `hil/b135` `test_mode` (B135B): RX reads the idle bus; with TX held recessive a requested frame never fails, the core stays "transmitter" at SOF with no errors counted, so this can't be used to provoke errors. LBCK is only reachable through the loopback modes. |
| M6 | Return from a running mode to Config mode | ⬜ Not started | ● | ○ | – | Needed for dynamic reconfiguration and bus-off recovery flows. |
| M7 | Shared H7 resources: CKDIV (FDCAN1 only), CCU | ⬜ Not started | ○ | ○ | – | `clock_divider` is stored in the config but never applied. |
| M8 | Sleep / wake-up | ⬜ Not started | ○ | ○ | – | |
| M9 | Time-triggered CAN (TTCAN, FDCAN1 on H7) | 💭 Idea | ○ | ○ | – | The builder already reserves trigger memory. |

## 3. Message RAM

| ID | Feature | Status | An | No | Tests | Notes |
|---|---|---|---|---|---|---|
| R1 | Const RAM layout builder (typestate, per instance) | ✅ Done | ● | ● | H, HIL (B135B) | Works in 32-bit words throughout (start address fields are word offsets), the whole RAM (2560 words) is usable, every instance starts from an empty layout (dedicated TX buffer count no longer leaks into the next instance), the layout records its instance and RAM region, trigger memory only on FDCAN1. Errors instead of panics on overflow. HIL: every `hil/b135` `loopback` test uses a different layout. |
| R2 | Apply layout to registers (`set_layout`) | 🟡 Partial | ● | ● | – | Writes start addresses / sizes / element sizes; rejects a layout built for another instance (`WrongInstance`); TTTMC only on FDCAN1. Missing: FIFO watermark (X4), FIFO mode (X5), TX FIFO vs queue mode (TFQM), TX event FIFO watermark. |
| R3 | Re-layout one instance / recombine layouts | ⬜ Not started | ○ | ○ | – | The `todo!()` stubs `relayout()` / `recombine()` were removed. Layouts now carry their region (`start_addr..end_addr`), which a re-layout can build on. |
| R4 | Zero message RAM (ECC init), only the instance's own region | ✅ Done | ● | ● | H (G0 region) | H7: `set_layout` zeroes the layout's region when the layout changes (re-applying the same layout on mode transitions keeps filters / TX buffers written in Config mode). Lite: entering Config zeroes the instance's fixed 212-word block (FDCAN2 at +0x350, matches stm32-data). Needs HIL (two instances, Q5). Open: `FDCAN_MSGRAM_LEN_WORDS = 512` on G0 vs. 2 × 212 used words, check against RM0444. |
| R5 | G0/G4 fixed layout support | 🟡 Partial | ○ | ● | HIL (B129A) | Fixed 212-word block per instance (`lite` offsets in `message_ram_layout.rs`): RX FIFO0/1 (3 elements each) and the 3-element TX FIFO are accessed; no filters yet (everything is accepted into FIFO0 as non-matching), no TX event FIFO. 64-byte elements always. |
| R6 | Bounds-checked element access helpers | 🟡 Partial | ● | ● | H, HIL (B135B) | Dedicated TX buffers and RX FIFO elements: offset = start + idx × element size, index (and instance for TX) checked. Filter / RX buffer / TX event accessors missing (come with F2, Y2, X5). |

## 4. Transmit

| ID | Feature | Status | An | No | Tests | Notes |
|---|---|---|---|---|---|---|
| X1 | Dedicated TX buffers (allocate via builder, write + pend, re-pend) | 🟡 Partial | ○ | ● | HIL (B135B) | Addresses fixed (R1/R6). HIL: `hil/b135` `loopback` sends every frame through a dedicated buffer. Doesn't check TXBRP before overwriting a pending buffer. H7 only. `is_idle()` right after a request can still return `true`: TXBRP shows the request a few kernel clock cycles after the TXBAR write (B135B). Completion tracking (X4) must not rely on TXBRP alone. |
| X2 | TX FIFO / priority queue transmit | ⬜ Not started | ● | ● | – | Only commented-out code ported from `fdcan`. |
| X3 | Abort transmission (blocking) | 🟡 Partial | ○ | ● | – | `abort_blocking` exists. Async abort and multi-buffer abort are missing. |
| X4 | TX completion tracking / backpressure (per-buffer waker, TXBTO/TXBCF) | ⬜ Not started | ● | ○ | – | Needed for host notifications on analyzers. Model to consider: hansihe/mcan `TxRef` (generation counters). |
| X5 | TX event FIFO + message markers | ⬜ Not started | ● | ○ | – | Elements are written with `EFC=DontStoreTxEvents`; marker high byte is always 0. |
| X6 | Time spent in TX queue (request → on-bus timestamp) | ⬜ Not started | ● | ○ | – | Needs X5 + TSCV read at request time + timestamps (S1). |
| X7 | Transmit pause (TXP) | ✅ Done | ● | ● | – | |
| X8 | Disable automatic retransmission (DAR) | ✅ Done | ● | ● | HIL (B135B) | `hil/b135` `errors` `bit_errors_count_up`: one attempt per request. Finding: re-requesting a dedicated buffer immediately after a failed attempt (TXBRP already clear, error frame still running) drops the request and sets IR.MRAF; with a 50 µs gap it works. Not handled by the driver yet. |
| X9 | Remote frames (RTR) | ⬜ Not started | ○ | ○ | – | Always sends data frames. |
| X10 | FD frames, BRS, ESI | 🟡 Partial | ● | ● | HIL (B135B) | FD frames of every length (0–64 bytes) with and without BRS at 1 / 4 Mbit/s pass in internal loopback (`hil/b135` `loopback` `fd_lengths_with_and_without_brs`). ESI not tested (needs an error-passive node). Not yet across real transceivers (Q5). |

## 5. Receive

| ID | Feature | Status | An | No | Tests | Notes |
|---|---|---|---|---|---|---|
| Y1 | RX FIFO 0/1 read + acknowledge | 🟡 Partial | ● | ● | H, HIL (B135B) | H7: `receive_fifo(RxFifo, &mut buf)` reads the element at FxGI (header: ID, RTR, FDF, BRS, ESI, DLC length, timestamp, filter index / non-matching), copies the data and acknowledges via FxAI. `BufferTooSmall` leaves the frame in the FIFO; truncation by a too-small element is reported. `rx_fifo_fill_level`. Host tests: element layout, DLC mapping, element addressing, header decoding. HIL (`hil/b135` `loopback`): classic IDs × 0–8 bytes, FD lengths, FIFO1 via global filter, short buffer, truncation. Lite cores: same `receive_fifo` on G0 (HIL B129A, `hil/b129` `loopback`). Missing: B135A / B125 runs. |
| Y2 | Dedicated RX buffers (NDAT1/2) | ⬜ Not started | ○ | ● | – | Builder allocates them, nothing reads them. H7 only. |
| Y3 | Async receive | ⬜ Not started | ● | ● | – | Building block exists: `wait_interrupts(Interrupts::RX_FIFO0_NEW_MESSAGE)` (I3). |
| Y4 | RX FIFO watermark (FWM, RFxW interrupt) | ⬜ Not started | ● | ● | – | |
| Y5 | RX FIFO blocking vs overwrite mode (FOM, Bosch MCAN p. 80) | ⬜ Not started | ○ | ● | – | Analyzers must keep up with any load; nodes may prefer overwrite. |
| Y6 | Overrun / message-lost detection (RFxL) | 🟡 Partial | ● | ● | HIL (B135B) | `take_rx_fifo_message_lost` reads and clears IR.RFxL (H7). HIL: a full 4-element FIFO in blocking mode keeps the first 4 frames and flags the 5th as lost (`hil/b135` `loopback` `full_fifo_loses_frame`, interrupts not unmasked). Goes through `take_interrupt_flags`, so it also sees the flag after the interrupt handler cleared it (`hil/b135` `interrupts` `lost_frame_seen_after_handler`). No counter yet (E6). |
| Y7 | All IDs are received (every standard ID, extended ID sweep) | ⬜ Not started | ● | ● | – | The embassy fork misses ID 0x125. Needs Y1 + filters + a HIL sweep test. |

## 6. Filters

| ID | Feature | Status | An | No | Tests | Notes |
|---|---|---|---|---|---|---|
| F1 | Global filter (non-matching std/ext, reject remote) | ✅ Done | ● | ● | HIL (B135B) | H7 GFC, lite RXGFC. Fixed 2026-10-01: `set_global_filter` didn't store the filter in the config, so the next mode transition re-applied the old one (found by `hil/b135` `loopback` `global_filter_routes_to_fifo1`). Reject-remote not tested. |
| F2 | Standard ID filter elements | ⬜ Not started | ○ | ● | – | Bitfield types exist in `pac/message_ram.rs`, unused. |
| F3 | Extended ID filter elements + XIDAM mask | ⬜ Not started | ○ | ● | – | |
| F4 | High-priority message handling (HPM, HPMS) | 💭 Idea | ○ | ○ | – | |

## 7. Interrupts and async

| ID | Feature | Status | An | No | Tests | Notes |
|---|---|---|---|---|---|---|
| I1 | `on_interrupt(instance, line)` entry point | ✅ Done | ● | ● | H, HIL (B135B) | `mcan::on_interrupt`, no feature needed. Handles only flags that are set, enabled (IE) and routed to its line (ILS, lite groups expanded), clears exactly those (W1C) and latches them per instance (`portable-atomic`), so `take_interrupt_flags` / `take_rx_fifo_message_lost` still report each event once. On Bus_Off it reads PSR (keeping LEC/DLEC for E2) and starts the recovery if automatic recovery is on (E3). Wakes the instance's waker. Host tests: ILS mapping, line masks, latching. HIL: `hil/b135` `interrupts`, 6 tests pass on B135B (2026-10-01): NVIC pending bits per line, the handler releases its line and leaves the other line and disabled flags alone, lost frames still reported, real handler wakes `wait_interrupts`; `errors` `bus_off_automatic_recovery`. |
| I2 | Interrupt enable / line selection | ✅ Done | ● | ● | H, HIL (B135B) | `Interrupts` set with per-core bit positions (host-tested against the PAC accessors). `FdCanConfig::interrupts` (default none), `interrupt_line_1`; `enable_interrupts` / `disable_interrupts` at runtime. ILE enables a line only if an enabled source is routed to it; TXBTIE/TXBCIE are set for all buffers. Lite cores route per ILS group (RXFIFO0/1, SMSG, TFERR, MISC, BERR, PERR); a source moves its whole group, and the config then shows the widened set. BEC/BEU (H7 IR 20/21) have no constant yet (E5). |
| I3 | `asynchronous` feature (embassy-sync wakers) | 🟡 Partial | ● | ● | HIL (B135B) | `wait_interrupts(Interrupts)`, `wait_bus_off`, `wait_bus_off_recovered`. One waker per instance (fine while every async op takes `&mut self`; split TX/RX halves will need more). Missing: async RX/TX (Y3, X4). |
| I4 | `embassy` feature (`configure_pins!`) | 🟡 Partial | ○ | ○ | – | Updated for the embassy-stm32 0.6 `Peri` API. TX is hard-coded push-pull; single-wire CAN (Q4a) needs open-drain. |
| I5 | RTIC / sync channel mode | 💭 Idea | ○ | ○ | – | |
| I6 | Async power-down / init-mode waits | ⬜ Not started | ○ | ○ | – | |

## 8. Errors and bus-off

| ID | Feature | Status | An | No | Tests | Notes |
|---|---|---|---|---|---|---|
| E1 | Error counters (ECR: TEC/REC/RP/CEL, Bosch MCAN p. 26) | ✅ Done | ● | ● | H, HIL (B135B) | `error_counters() -> ErrorCounters`. Reading resets CEL, hence `&mut self`. HIL: `hil/b135` `errors` `bit_errors_count_up` passes on B135B (2026-10-01): TEC +8 per attempt, EW at 96, EP at 128, CEL 1 per read. In bus-off ECR read TEC 248 and CEL 31 (observed). |
| E2 | Protocol status (PSR: LEC/DLEC/EP/EW/BO/activity) | ✅ Done | ● | ● | H, HIL (B135B) | `protocol_status() -> ProtocolStatus` (LEC, DLEC, ACT, EW, EP, BO, RESI, RBRS, REDL, PXE, TDCV, `error_state()`). PSR reads reset LEC/DLEC/RESI/RBRS/REDL/PXE: reads by the driver itself (`is_bus_off`, the interrupt handler) latch those fields per instance, and `protocol_status` merges them in, so no error code is lost to the driver. HIL (B135B): DLEC Bit1 and PED for data phase errors, PEA not set, EW/EP/BO, LEC Bit0 during the bus-off recovery. Not tested: RESI/RBRS/REDL/PXE (need received FD frames / another node), LEC Ack and other arbitration-phase codes (need a node driving the bus, Q5). |
| E3 | Bus-off management / recovery task (Bosch MCAN p. 28) | ✅ Done | ● | ● | HIL (B135B) | `is_bus_off`, `recover_from_bus_off()` (clears INIT in the running mode, no typestate change, returns whether a recovery started), `FdCanConfig::automatic_bus_off_recovery` (default off; enables Bus_Off, the handler clears INIT), async `wait_bus_off` / `wait_bus_off_recovered`. HIL: `hil/b135` `errors`, 4 tests pass on B135B (2026-10-01). Bus-off is provoked without driving the bus: internal loopback, FD + BRS, TDC offset 127 mtq → data-phase bit errors (no ACK exception), bus-off after ~1.3 ms. Manual recovery took 1434 µs (129 × 11 bits = 1419 µs at 1 Mbit/s), counters reset to 0; no recovery without clearing INIT; `abort_blocking` works while bus-off; automatic recovery by the handler. Not yet on a real bus (Q5). |
| E4 | Protocol exception handling (PXHD) | ✅ Done | ○ | ○ | – | |
| E5 | Message RAM ECC/parity errors (BEC/BEU) | ⬜ Not started | ○ | ○ | – | |
| E6 | Driver statistics via the `cnt` crate (per-instance `cnt::Counters<E>`: RX/TX frames, FIFO overruns / lost messages, error / bus-off events, …) | ⬜ Not started | ● | ○ | – | Use cnt *Instance counters* (`#[derive(cnt::Count)]` event enum, `&'static Counters<E>` passed in by the firmware), no hand-rolled counters. ISR-safe, lock-free. Users can compile them out with cnt's `disabled` feature. Instance counters rather than `cnt!`: `on_interrupt` is one call site for all instances, per-instance call sites would reserve words for unused instances, and only `Counters` can be read back on the target (Q9). Needs cnt ≥ 0.4.1 (`get` / `clear`, host builds). Checked 2026-10-03: instance counters link next to embedded-test on thumbv7em, build on thumbv6m, and parallel host tests with per-test statics pass. On B135B a throwaway embedded-test suite passed: `count` / `add` / `get`, u64 carry into the high word, `clear`, and a shared static reading 0 at the start of every test (target reset); `loopback`, `interrupts` and `errors` still pass with `cnt.x` linked. The firmware will likely need a larger `CNT_RAM_BUFFER_SIZE_WORDS` than the 64-word default (one block per instance). |

## 9. Bit timing

| ID | Feature | Status | An | No | Tests | Notes |
|---|---|---|---|---|---|---|
| T1 | Manual nominal / data bit timing (NBTP/DBTP) | ✅ Done | ● | ● | H | `NominalBitTiming::new` / `DataBitTiming::new` (const) validate the RM0468 / RM0444 ranges (nominal: prescaler 1..=512, seg1 2..=256, seg2 1..=128, SJW 1..=128; data: 1..=32, 1..=32, 1..=16, 1..=16) and SJW <= seg2, returning `BitTimingError`. Register encoding (value - 1) is a pure function, host-tested against hand-computed NBTP/DBTP values. Needs HIL (frames at a known bit rate, Q4/Q5). |
| T2 | Transceiver delay compensation (TDCR, DBTP.TDC) | 🟡 Partial | ○ | ● | H | `TransceiverDelayCompensation` (offset TDCO, filter window TDCF, 0..=127 mtq; `at_sample_point` helper) via `DataBitTiming::with_tdc`, rejected for data prescaler > 2. Writes DBTP.TDC and TDCR. Not verified on hardware: needs FD + BRS at ≥ 2 Mbit/s through real transceivers (Q5/Q6); check TDCV readback. |
| T3 | Predefined timing sets (consts per kernel clock / bitrate) | ⬜ Not started | ○ | ○ | – | |
| T4 | Bitrate → timing calculator | 💭 Idea | ○ | ○ | – | |
| T5 | Dynamic reconfiguration (e.g. auto-baud on analyzers) | 🟡 Partial | ● | ○ | – | Setters exist in ConfigMode and `apply_config` now applies TDC. Blocked by M6 and `apply_config` not applying timestamp / clock divider. |
| T6 | Non-ISO mode, edge filtering, FD enable / BRS enable | ✅ Done | ○ | ○ | – | |

## 10. Timestamping

| ID | Feature | Status | An | No | Tests | Notes |
|---|---|---|---|---|---|---|
| S1 | Timestamp counter source + prescaler (TSCC) | 🔴 Broken | ● | ○ | – | `set_timestamp_counter_source` is never called from `apply_config`. Prescaler writes the divisor instead of divisor − 1 (embassy fixed the same bugs in #7125). |
| S2 | External timer timestamping (required for CAN FD) | ⬜ Not started | ● | ○ | – | The embassy fork only does peripheral-clock timestamps. Check per chip which timer (TIM3 / TTCAN TSU). |
| S3 | Raw RX / TX-event timestamps exposed to users | ⬜ Not started | ● | ○ | – | Depends on Y1 / X5. TSCE is always disabled in TX elements. |
| S4 | Timestamp counter read (TSCV) / wrap handling | ⬜ Not started | ● | ○ | – | |

## 11. Examples

| ID | Feature | Status | An | No | Tests | Notes |
|---|---|---|---|---|---|---|
| D1 | H7 + embassy (`examples/h7_embassy`, STM32H725IG, B135 pinout) | 🟡 Partial | – | – | – | Builds on stable with up-to-date embassy. Runs on B135A/B. B125 would need FDCAN1 TX on PD1 instead of PB9. `simple`: sends via a dedicated buffer (addresses fixed with R1/R6, not re-tested on hardware). `loopback`: internal loopback TX → RX FIFO0 over 6 IDs × 4 lengths, logs PASS/FAIL: **passed on B135B** (2026-10-01). |
| D2 | G0 example (NUCLEO-G0B1RE) | ⬜ Not started | – | – | – | Blocked by P2. |
| D2a | H5 example (NUCLEO-H533RE) | ⬜ Not started | – | – | – | Blocked by P4a. |
| D3 | H7 + RTIC, H7 + stm32h7xx-hal, G4, L5, advanced (analyzer) examples | 💭 Idea | – | – | – | |

## 12. Test infrastructure

| ID | Feature | Status | Notes |
|---|---|---|---|
| Q1 | Host unit tests (`cargo test` on the host target) | 🟡 Partial | Interrupt sets / ILS mapping / PSR latching (`interrupt.rs`), ECR/PSR decoding (`status.rs`). Register map tests (`src/pac/tests.rs`: offsets and bit positions from RM0468 / RM0444, for `h7`, `g0`, with and without `rcc`), RAM builder and layout (`message_ram_builder.rs`, `message_ram_layout.rs`), bit timing validation and NBTP/DBTP/TDCR encoding (`config.rs`), RX/TX element bitfields and DLC mapping (`pac/message_ram.rs`), RX header decoding (`tx_rx.rs`). Next: `Dlc::from_len`, IDs, filter elements. |
| Q2 | Host coverage measurement (`cargo llvm-cov`) | ⬜ Not started | `cargo-llvm-cov` is not installed yet. |
| Q3 | HIL test harness (probe-rs + `embedded-test`) | 🟡 Partial | `hil/b135` exists: embedded-test 0.7.2 with probe-rs 0.32 (pin the pair; embedded-test must match the probe-rs protocol), sync tests, target reset before every test. Run with `cd hil/b135 && cargo test`. `hil/b129` exists too (2026-10-04, builds for thumbv6m, **11/11 pass on B129A, 2026-10-04**): 12 MHz HSE as FDCAN kernel clock, PC13 low, `bring_up` (7: config mode, both instances, modes) and `idle_bus` (4: RX recessive, normal/monitoring idle without errors, standby quiet). `loopback` (4, 2026-10-04) runs classic and FD+BRS frames through the lite TX FIFO / RX FIFO0 on B129A. Still to create, one crate per board, named after the board number (all revisions of one board share a crate; revision selected by a cargo feature; board-ID resistors are not trusted for now): `hil/b125`, `hil/nucleo_g0b1re`, `hil/nucleo_h533re`. embedded-test covers rigs on one MCU (Q4, Q4a, Q5). Board ↔ board (Q5a) and the external reference node (Q6) need a host-orchestrated runner (host test binary driving probe-rs + SocketCAN, firmware as responder). |
| Q4 | HIL rig: internal / external loopback | 🟡 Partial | Internal loopback on B135B (2026-10-01), all pass: `hil/b135` `loopback` (6), `interrupts` (6), `errors` (4, errors provoked by a wrong TDC offset), plus `test_mode` (1, pins connected, TX held recessive, needs an idle bus). Missing: external loopback (drives the bus, needs an isolated bus), B135A, B125, other families. |
| Q5 | HIL rig: two instances on one board wired through transceivers | ⬜ Not started | B125 Ch1 (FDCAN1) ↔ Ch2 (FDCAN3) over a DE-9 cable carrying CAN_H, CAN_L **and GND** (the channels are isolated from each other). This also covers two instances sharing the message RAM (R4). Alternatively two B135s on one bus. Terminators are switchable on both boards. Possible on the Nucleos too, with two external transceivers. Real bus: arbitration, errors, bus-off (short CANH/CANL), DAR, TXP. |
| Q4a | HIL rig: single-wire CAN without transceivers | ⬜ Not started | Wired-AND bus with a pull-up and open-drain TX (or diodes), see [boards/nucleo.md](boards/nucleo.md). FDCAN1 ↔ FDCAN2 on one Nucleo (G0B1, H533), or across boards: B135B FDCAN3 on J203, B129 FDCAN2 on its headers. Needs open-drain TX pin config. Limited bit rates. |
| Q5a | HIL rig: board ↔ board over transceivers | 🟡 Partial | **Plan for B129A (R5 lite TX/RX exists since 2026-10-04; **runner passes on B129A ↔ B135B, 2026-10-04** (run 3 times, all four scenarios): `hil/run-bus.sh` resets both boards, starts the receiving board, waits for its `RX READY` line, then starts the sender; STLink on B129A, J-Link on B135B; shared frame set in `hil/common`; scenarios `frames_*` (16 frames: classic, FD, BRS, std and ext IDs, 0–64 bytes) and `soak_*` (1000 frames) in both directions, data checked and error counters asserted 0 (items 1 and 4). Lessons: seg1 excludes the sync quantum (total tq = 1 + seg1 + seg2; B129A was running 12/13 of the rate until fixed to `(1, 9, 2, 2)`); a board keeps running its last test after probe-rs exits, so reset before each run; B129A needs a fast CPU (PLL 64 MHz) or the 3-element RX FIFO overruns on back-to-back short frames. Without a second node `hil/b129` `send_frames` fails with the TX FIFO stuck full at frame 3 and ACK error counters at 128 (no ACK). Items 2 (ACK with a listener only half done) and 3 (STB high) still to write:** (1) B129 sends classic and FD+BRS frames, B135/B125 receives and checks ID/DLC/data, then the reverse; (2) ACK: B129 alone on a terminated bus gets ACK errors, with a B135 listening it doesn't; (3) STB high: B129 transmits nothing and the B135 sees an idle bus; (4) error counters on both sides stay 0 over 1000 frames. Needs a host-orchestrated runner. Mixed families on one bus: B129 (G0, lite) ↔ B125/B135 (H7) with a Pico-Lock ↔ DE-9 adapter. B129 has no terminator, so enable a B125/B135 relay. Later H5 ↔ H7 via single wire. |
| Q6 | HIL rig: external reference node (USB-CAN / SocketCAN on the host) | ⬜ Not started | Analyzer load tests, ID sweep (Y7), FD + BRS interop, timestamps against a known source. |
| Q7 | On-target coverage (e.g. `minicov`) | 💭 Idea | |
| Q8 | CI: build matrix of all chip / feature combinations + clippy | ⬜ Not started | |
| Q9 | Use `cnt` counters in tests / HIL | 🟡 Partial | `hil/b135` depends on `cnt` 0.4.1 (links `cnt.x`) with a compile-time guard against cnt's `disabled` feature, which would make `get` return 0 and every "no errors" assert pass trivially. Still to do, once E6 exists: HIL tests read the driver's counters on the target with `Counters::get` / `clear` (one `counters!` static per test, as the cnt README recommends; embedded-test holds the probe, so `cnt read` can't be used inside it) and assert no unexpected errors / lost frames / overruns. Test-only counters also use `cnt`. |

---

## References and prior art (checked 2026-10-01)

- Bosch M_CAN user manual (page numbers above refer to it), ST RM0468 (H72x/73x), RM0444 (G0).
- [embassy-stm32 `can/fd`](https://github.com/embassy-rs/embassy/tree/main/embassy-stm32/src/can): active. Fixed
  RAM layout on H7, no dedicated buffers or FIFO watermark / overwrite config. Useful fixes: TDC (#5936),
  timestamp source + TX event FIFO (#7125), bus-off recovery control, RX drain races.
- [hansihe/mcan](https://github.com/hansihe/mcan): standalone extraction of the `fdcan-rewrite` embassy fork.
  Configurable RAM, `TxRef`, bus-off wait, watermark. Dormant since Aug 2025, no license file seen.
- [GrepitAB/mcan](https://github.com/GrepitAB/mcan): mature, ATSAM-oriented (message RAM in system RAM).
- [stm32-rs/fdcan](https://github.com/stm32-rs/fdcan): origin of much of the config code, stagnant.
