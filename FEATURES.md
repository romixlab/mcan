# Feature list — source of truth

This file is the **single source of truth** for what this driver supports, what is planned, and what
is broken. Update it in the same change that adds, fixes, breaks or tests a feature (see
[AGENTS.md](AGENTS.md)).

Last full audit: 2026-10-01 (against commit `3d49144` + dependency bump).

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
| [B129A](boards/b129.md) (CANnify Micro HV) | STM32G0B1CETxN | 1: FDCAN1 (TX PD1, RX PB8), TCAN1044V, non-isolated, **no terminator**, standby on PC13 (drive low) | `g0` | `hil/b129` | The only lite-core board with a transceiver. Pico-Lock connectors. 12 MHz crystal HSE. FDCAN2 pins on the headers. |
| [NUCLEO-G0B1RE](boards/nucleo.md) | STM32G0B1RET6 | none (no transceivers): FDCAN1 + FDCAN2 | `g0` | `hil/nucleo_g0b1re` | Software and loopback tests, single-wire CAN (FDCAN1 ↔ FDCAN2 or to other boards). Proposed pins: FDCAN1 PB9/PB8, FDCAN2 PB13/PB12, 1 kΩ pull-up (not wired yet). |
| [NUCLEO-H533RE](boards/nucleo.md) | STM32H533RET6 | none (no transceivers): FDCAN1 + FDCAN2 | `h5` (not implemented yet) | `hil/nucleo_h533re` | Same as above. Proposed pins: FDCAN1 PB7/PB8, FDCAN2 PB13/PB12, 1 kΩ pull-up (not wired yet). |

---

## 1. Build, platform and crate hygiene

| ID | Feature | Status | An | No | Tests | Notes |
|---|---|---|---|---|---|---|
| P1 | H7 support (`h7`) | 🟡 Partial | ● | ● | – | Builds. See the per-feature status below. |
| P2 | G0 support (`g0`) | 🔴 Broken | ○ | ● | – | Builds on thumbv7em only. On the real target (thumbv6m) `static_cell` needs CAS, so it only works if the user enables `portable-atomic/critical-section`. `g0,embassy` will also fail: `asynchronous.rs` uses `FdCan3` / `FDCAN3_REGISTER_BLOCK_ADDR` unconditionally. Lite-core filter config (RXGFC) not handled. **Uses the full M_CAN (H7) register map, which is wrong for the lite core** (P7a). |
| P3 | G4 support (`g4`) | ⬜ Not started | ○ | ● | – | Feature flag exists, no `mapping`/RCC → does not compile. |
| P4 | L5 support (`l5`) | ⬜ Not started | ○ | ○ | – | Feature flag exists, does not compile. |
| P4a | H5 support (`h5`) | ⬜ Not started | ● | ● | – | Intended target (FDCAN lite). No feature flag, mapping or RCC yet. H533: FDCAN1 @ 0x4000_A400, FDCAN2 @ 0x4000_A800, RAM blocks @ 0x4000_AC00 / 0x4000_AF50 (stm32-data). Test board: NUCLEO-H533RE. |
| P5 | `compile_error!` when no / multiple chip features are selected | ⬜ Not started | ● | ● | – | Today you get cryptic `unresolved import mapping` errors. |
| P6 | `defmt` feature wiring | 🔴 Broken | ● | ● | – | `defmt = []` does not enable `dep:defmt` (`--no-default-features --features h7,defmt` → 485 errors). `default = ["dep:defmt"]` pulls the dependency but `cfg(feature = "defmt")` stays off. |
| P7 | Stand-alone, no external PAC/HAL dependency | ✅ Done | ● | ● | – | Own register layer generated from stm32-data (`src/pac`). RCC files are huge and mostly unused (TODO: trim). |
| P7a | **TODO: re-check the PAC registers, make them regenerable, maybe switch register model** | ⬜ Not started | ● | ● | – | See *PAC TODO* below. |
| P8 | No panics / asserts, everything returns `Result` | 🔴 Broken | ● | ● | – | Violated by `todo!()` in `relayout()`/`recombine()`, `.expect()` in the RAM builder, and `- 1` underflow in bit timing (see T1). |
| P9 | No blocking waits without timeout | ✅ Done | ● | ● | – | `util::checked_wait` with iteration-count timeouts. Not time-based. |
| P10 | Few generics / macros, `FdCan` not generic over instance | ✅ Done | ● | ● | – | |
| P11 | Dependencies up to date | ✅ Done | ● | ● | – | Bumped 2026-10-01: bitfield-struct 0.13, embassy-sync 0.8; example on embassy-stm32 0.6 / executor 0.10 / time 0.5, stable toolchain. `paste` is unmaintained (RUSTSEC-2024-0436) → replace with `pastey` or remove. |
| P12 | Publishable crate name | ⬜ Not started | – | – | – | `mcan` is taken on crates.io (GrepitAB/mcan). |
| P13 | `embedded-can` traits | 💭 Idea | ○ | ○ | – | |

### PAC TODO (P7a)

Current state of `src/pac`:

- `registers.rs`: one FDCAN register block (about 6.5 k lines, 375 accessors) in chiptool style, using our own
  `pac_traits::Reg`. It was generated from stm32-data once, but **how to regenerate it is not recorded**
  (generator, version, source YAML, post-processing).
- **It only has the full M_CAN (H7) layout.** The `g0` feature uses it too, but the FDCAN-lite cores (G0, G4,
  L5, H5) have a different map: RXGFC at 0x80 instead of GFC/SIDFC/XIDFC, no dedicated-buffer and TT registers,
  different RXF/TXB config fields. So **G0 register access is wrong in places today** (see P2). The lite
  variants need their own block (per-chip `cfg` or a separate module).
- `rcc_g0.rs` / `rcc_h7.rs`: full RCC blocks (5 k / 14 k lines), of which only a few registers are used.
- `message_ram.rs`: hand-written `bitfield-struct` element layouts, a different style from `registers.rs`.

To do:

1. **Audit** every register and field we use against the reference manuals (RM0468 for H72x/73x, RM0444 for
   G0, and the H5/G4/L5 RMs) and the Bosch M_CAN manual. Record mismatches here as 🔴 rows.
2. **Make the PAC regenerable**: a script (e.g. `tools/gen-pac`) with a pinned generator and source (stm32-data
   YAML via chiptool, as embassy does). Commit the output and add a CI check that regenerating gives no diff.
   Include only the RCC registers we need.
3. **Add host tests for the register map**: offsets, field positions and reset values, possibly cross-checked
   against `stm32-metapac` as a dev-dependency only (so the crate stays stand-alone).
4. **Consider switching the register model**, ideally one model for both MMIO registers and message RAM
   elements. Candidates:
   - Keep chiptool output (current style, same as embassy / stm32-metapac).
   - Own YAML + chiptool from a build script, with separate full and lite variants. hansihe/mcan's `mcan_pac`
     does this and is worth a look.
   - `bitfield-struct` for registers too (already used for message RAM).
   - svd2rust, tock-registers, ral-registers, derive-mmio.

   Compare them on host testability (fake register blocks), const-friendliness, codegen size on thumbv6m, and
   how easily a full/lite variant split can be expressed.

## 2. Instances, clocks and operating modes

| ID | Feature | Status | An | No | Tests | Notes |
|---|---|---|---|---|---|---|
| M1 | `FdCanInstances` singleton, take/put back instances | ✅ Done | ● | ● | – | `StaticCell` guard. |
| M2 | Clock enable + reset, clock-source sanity check | ✅ Done | ● | ● | – | H7 and G0. One clock feeds every instance, so enable/disable only works when all instances are present. |
| M3 | Clock disable (all instances in PoweredDown) | ✅ Done | ● | ● | – | |
| M4 | Core check (ENDN, CREL) | ✅ Done | ● | ● | – | Accepts only `CREL.REL == 3`. Verify against G0/G4 lite cores. |
| M5 | Typestate modes: PoweredDown → Config → Normal / Restricted / BusMonitoring / Internal loopback / External loopback / Test | 🟡 Partial | ● | ● | – | Transitions out of Config exist. `into_powered_down` clears INIT while CSR is set (check against the spec). Test mode is unusable (no TEST register API). |
| M6 | Return from a running mode to Config mode | ⬜ Not started | ● | ○ | – | Needed for dynamic reconfiguration and bus-off recovery flows. |
| M7 | Shared H7 resources: CKDIV (FDCAN1 only), CCU | ⬜ Not started | ○ | ○ | – | `clock_divider` is stored in the config but never applied. |
| M8 | Sleep / wake-up | ⬜ Not started | ○ | ○ | – | |
| M9 | Time-triggered CAN (TTCAN, FDCAN1 on H7) | 💭 Idea | ○ | ○ | – | The builder already reserves trigger memory. |

## 3. Message RAM

| ID | Feature | Status | An | No | Tests | Notes |
|---|---|---|---|---|---|---|
| R1 | Const RAM layout builder (typestate, per instance) | 🔴 Broken | ● | ● | – | **Unit mismatch:** `pos` advances in bytes (`len * words * 4`), `end` is in words, register setters (`set_flssa`, `set_tbsa`, …) take word offsets → every start address is 4× too large. Prime target for host tests. |
| R2 | Apply layout to registers (`set_layout`) | 🟡 Partial | ● | ● | – | Writes start addresses / sizes / element sizes. Missing: FIFO watermark (X4), FIFO mode (X5), TX FIFO vs queue mode (TFQM), TX event FIFO watermark. |
| R3 | `relayout()` / `recombine()` (re-layout one instance or all) | 🔴 Broken | ○ | ○ | – | `todo!()` (panics). |
| R4 | Zero message RAM on entering Config (ECC init) | 🔴 Broken | ● | ● | – | Zeroes the **whole shared RAM**, so configuring one instance destroys a running neighbour on H7. Must zero only this instance's region (and the lite per-instance block on G0). On G0, `FDCAN_MSGRAM_LEN_WORDS = 512` (2 KB), but stm32-data shows two 0x350-byte blocks (0x4000_B400, 0x4000_B750 = 424 words), so the zero loop may write past the FDCAN RAM. Check against the RM0444 memory map. |
| R5 | G0/G4 fixed layout support | ⬜ Not started | ○ | ● | – | `MessageRam` for non-H7 is an empty stub. |
| R6 | Bounds-checked element access helpers | 🔴 Broken | ● | ● | – | `tx_buffer()` computes `tx_buffers_addr + idx` without multiplying `idx` by the element size (2 + data words). RX/filter/event accessors missing. |

## 4. Transmit

| ID | Feature | Status | An | No | Tests | Notes |
|---|---|---|---|---|---|---|
| X1 | Dedicated TX buffers (allocate via builder, write + pend, re-pend) | 🔴 Broken | ○ | ● | – | API exists (`write_tx_buffer_pend`, `tx_buffer_pend`) but sends wrong data because of R1/R6. Doesn't check TXBRP before overwriting a pending buffer. H7 only. |
| X2 | TX FIFO / priority queue transmit | ⬜ Not started | ● | ● | – | Only commented-out code ported from `fdcan`. |
| X3 | Abort transmission (blocking) | 🟡 Partial | ○ | ● | – | `abort_blocking` exists. Async abort and multi-buffer abort are missing. |
| X4 | TX completion tracking / backpressure (per-buffer waker, TXBTO/TXBCF) | ⬜ Not started | ● | ○ | – | Needed for host notifications on analyzers. Model to consider: hansihe/mcan `TxRef` (generation counters). |
| X5 | TX event FIFO + message markers | ⬜ Not started | ● | ○ | – | Elements are written with `EFC=DontStoreTxEvents`; marker high byte is always 0. |
| X6 | Time spent in TX queue (request → on-bus timestamp) | ⬜ Not started | ● | ○ | – | Needs X5 + TSCV read at request time + timestamps (S1). |
| X7 | Transmit pause (TXP) | ✅ Done | ● | ● | – | |
| X8 | Disable automatic retransmission (DAR) | ✅ Done | ● | ● | – | |
| X9 | Remote frames (RTR) | ⬜ Not started | ○ | ○ | – | Always sends data frames. |
| X10 | FD frames, BRS, ESI | 🟡 Partial | ● | ● | – | Header fields exist. Not verified on hardware because of X1. |

## 5. Receive

| ID | Feature | Status | An | No | Tests | Notes |
|---|---|---|---|---|---|---|
| Y1 | RX FIFO 0/1 read + acknowledge | ⬜ Not started | ● | ● | – | **No RX path at all yet.** Only commented-out code. |
| Y2 | Dedicated RX buffers (NDAT1/2) | ⬜ Not started | ○ | ● | – | Builder allocates them, nothing reads them. H7 only. |
| Y3 | Async receive | ⬜ Not started | ● | ● | – | ISR wakes `rx_dedicated_waker` on DRX, but nothing awaits it. |
| Y4 | RX FIFO watermark (FWM, RFxW interrupt) | ⬜ Not started | ● | ● | – | |
| Y5 | RX FIFO blocking vs overwrite mode (FOM, Bosch MCAN p. 80) | ⬜ Not started | ○ | ● | – | Analyzers must keep up with any load; nodes may prefer overwrite. |
| Y6 | Overrun / message-lost detection (RFxL) | ⬜ Not started | ● | ● | – | |
| Y7 | All IDs are received (every standard ID, extended ID sweep) | ⬜ Not started | ● | ● | – | The embassy fork misses ID 0x125. Needs Y1 + filters + a HIL sweep test. |

## 6. Filters

| ID | Feature | Status | An | No | Tests | Notes |
|---|---|---|---|---|---|---|
| F1 | Global filter (non-matching std/ext, reject remote) | ✅ Done | ● | ● | – | H7 GFC. Lite cores use RXGFC: not handled. |
| F2 | Standard ID filter elements | ⬜ Not started | ○ | ● | – | Bitfield types exist in `pac/message_ram.rs`, unused. |
| F3 | Extended ID filter elements + XIDAM mask | ⬜ Not started | ○ | ● | – | |
| F4 | High-priority message handling (HPM, HPMS) | 💭 Idea | ○ | ○ | – | |

## 7. Interrupts and async

| ID | Feature | Status | An | No | Tests | Notes |
|---|---|---|---|---|---|---|
| I1 | `on_interrupt(instance, line)` entry point | 🔴 Broken | ● | ● | – | Clears **all** IR flags blindly (events are lost), ignores `irq`, only wakes on DRX. Doesn't compile without `h7` (FdCan3). |
| I2 | Interrupt enable / line selection | 🔴 Broken | ● | ● | – | Enables every interrupt, but only EINT0 in ILE, so the IT1 line never fires. `interrupt_line_config` (ILS) is applied. |
| I3 | `asynchronous` feature (embassy-sync wakers) | 🟡 Partial | ● | ● | – | Only the State struct and one waker. |
| I4 | `embassy` feature (`configure_pins!`) | 🟡 Partial | ○ | ○ | – | Updated for the embassy-stm32 0.6 `Peri` API. TX is hard-coded push-pull; single-wire CAN (Q4a) needs open-drain. |
| I5 | RTIC / sync channel mode | 💭 Idea | ○ | ○ | – | |
| I6 | Async power-down / init-mode waits | ⬜ Not started | ○ | ○ | – | |

## 8. Errors and bus-off

| ID | Feature | Status | An | No | Tests | Notes |
|---|---|---|---|---|---|---|
| E1 | Error counters (ECR: TEC/REC/RP/CEL, Bosch MCAN p. 26) | ⬜ Not started | ● | ● | – | |
| E2 | Protocol status (PSR: LEC/DLEC/EP/EW/BO/activity) | ⬜ Not started | ● | ● | – | Reading PSR clears LEC/DLEC, so cache it in the ISR. |
| E3 | Bus-off management / recovery task (Bosch MCAN p. 28) | ⬜ Not started | ● | ● | – | Clearing INIT while running; must not require a typestate transition. See embassy `automatic_bus_off_recovery`, hansihe/mcan `wait_bus_off`. |
| E4 | Protocol exception handling (PXHD) | ✅ Done | ○ | ○ | – | |
| E5 | Message RAM ECC/parity errors (BEC/BEU) | ⬜ Not started | ○ | ○ | – | |
| E6 | Driver statistics via the `cnt` crate (per-instance `cnt::Counters<E>`: RX/TX frames, FIFO overruns / lost messages, error / bus-off events, …) | ⬜ Not started | ● | ○ | – | Use cnt *Instance counters* (`#[derive(cnt::Count)]` event enum, `&'static Counters<E>` passed in by the firmware), no hand-rolled counters. ISR-safe, lock-free. Users can compile them out with cnt's `disabled` feature. |

## 9. Bit timing

| ID | Feature | Status | An | No | Tests | Notes |
|---|---|---|---|---|---|---|
| T1 | Manual nominal / data bit timing (NBTP/DBTP) | 🔴 Broken | ● | ● | – | Values are masked and then `- 1` is applied, so e.g. prescaler 32 → 0 → underflow (panic in debug). Ranges not validated. Doc ranges don't match the masks. |
| T2 | Transceiver delay compensation (TDCR, DBTP.TDC) | 🔴 Broken | ○ | ● | – | `transceiver_delay_compensation` field exists but is never written. Required for fast data phases. |
| T3 | Predefined timing sets (consts per kernel clock / bitrate) | ⬜ Not started | ○ | ○ | – | |
| T4 | Bitrate → timing calculator | 💭 Idea | ○ | ○ | – | |
| T5 | Dynamic reconfiguration (e.g. auto-baud on analyzers) | 🟡 Partial | ● | ○ | – | Setters exist in ConfigMode, but blocked by M6, R4 and `apply_config` not applying timestamp / clock divider / TDC. |
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
| D1 | H7 + embassy (`examples/h7_embassy`, STM32H725IG, B135 pinout) | 🟡 Partial | – | – | – | Builds on stable with up-to-date embassy. Runs on B135A/B. B125 would need FDCAN1 TX on PD1 instead of PB9. Sends via a dedicated buffer (wrong data, see X1). No RX. |
| D2 | G0 example (NUCLEO-G0B1RE) | ⬜ Not started | – | – | – | Blocked by P2. |
| D2a | H5 example (NUCLEO-H533RE) | ⬜ Not started | – | – | – | Blocked by P4a. |
| D3 | H7 + RTIC, H7 + stm32h7xx-hal, G4, L5, advanced (analyzer) examples | 💭 Idea | – | – | – | |

## 12. Test infrastructure

| ID | Feature | Status | Notes |
|---|---|---|---|
| Q1 | Host unit tests (`cargo test` on the host target) | ⬜ Not started | No tests exist. The RAM builder, `Dlc`, IDs, bitfield layouts and timing math are pure logic, so test them first. `cargo test --features h7` already builds and runs on the host (0 tests). |
| Q2 | Host coverage measurement (`cargo llvm-cov`) | ⬜ Not started | `cargo-llvm-cov` is not installed yet. |
| Q3 | HIL test harness (probe-rs + `embedded-test`) | ⬜ Not started | One harness crate per test board (see *Test boards*) under `hil/`, named after the board number (all revisions of one board share a crate; revision selected by a cargo feature; board-ID resistors are not trusted for now): `hil/b125`, `hil/b129`, `hil/b135`, `hil/nucleo_g0b1re`, `hil/nucleo_h533re`. |
| Q4 | HIL rig: internal / external loopback | ⬜ Not started | Needs only a board + probe. Runs on every test board. |
| Q5 | HIL rig: two instances on one board wired through transceivers | ⬜ Not started | B125 Ch1 (FDCAN1) ↔ Ch2 (FDCAN3) over a DE-9 cable carrying CAN_H, CAN_L **and GND** (the channels are isolated from each other). This also covers two instances sharing the message RAM (R4). Alternatively two B135s on one bus. Terminators are switchable on both boards. Possible on the Nucleos too, with two external transceivers. Real bus: arbitration, errors, bus-off (short CANH/CANL), DAR, TXP. |
| Q4a | HIL rig: single-wire CAN without transceivers | ⬜ Not started | Wired-AND bus with a pull-up and open-drain TX (or diodes), see [boards/nucleo.md](boards/nucleo.md). FDCAN1 ↔ FDCAN2 on one Nucleo (G0B1, H533), or across boards: B135B FDCAN3 on J203, B129 FDCAN2 on its headers. Needs open-drain TX pin config. Limited bit rates. |
| Q5a | HIL rig: board ↔ board over transceivers | ⬜ Not started | Mixed families on one bus: B129 (G0, lite) ↔ B125/B135 (H7) with a Pico-Lock ↔ DE-9 adapter. B129 has no terminator, so enable a B125/B135 relay. Later H5 ↔ H7 via single wire. |
| Q6 | HIL rig: external reference node (USB-CAN / SocketCAN on the host) | ⬜ Not started | Analyzer load tests, ID sweep (Y7), FD + BRS interop, timestamps against a known source. |
| Q7 | On-target coverage (e.g. `minicov`) | 💭 Idea | |
| Q8 | CI: build matrix of all chip / feature combinations + clippy | ⬜ Not started | |
| Q9 | Use `cnt` counters in tests / HIL | ⬜ Not started | HIL tests read the driver's `cnt` counters (E6) via `counters_ram_buffer()` or `cnt read` and assert no unexpected errors / lost frames / overruns. Test-only counters also use `cnt`. |

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
