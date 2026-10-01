# Changelog

All notable changes to this crate are recorded here, newest first. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Feature IDs (`P7a`, `M2`, …) refer to
[FEATURES.md](FEATURES.md), which holds the current status. This file only records what changed.

## [Unreleased]

### Added

- `tools/gen-pac`: a Rust (syn/quote) generator for `src/pac` that works from `stm32-metapac` 21.0.0, pinned
  and checksum-verified through its `Cargo.lock`. It copies the FDCAN maps verbatim and trims RCC on the
  syntax tree, keeping Debug/defmt for the remaining fields. It also generates per-chip addresses and the
  FDCAN RCC enable/reset/clock-mux bits from metapac's chip metadata (`mapping_<chip>.rs`). `--check` reports
  out-of-date and stale files. It replaces the short-lived `tools/gen_pac.py`. (P7, P7a)
- FDCAN lite register map `pac::fdcan_v1` for G0/G4/L5/H5, next to the full M_CAN map `pac::fdcan_h7`.
  `pac::fdcan` selects the right one for the chip feature. (P7a, P2)
- `rcc` cargo feature (default on). Without it the driver never touches RCC, and the HAL (e.g.
  `embassy_stm32::rcc::enable_and_reset`) owns the clocks. (M2)
- `FdCanInstances::take()`: takes an instance without touching RCC. (M2)
- Host tests for the register map (`src/pac/tests.rs`): offsets and bit positions from RM0468 / RM0444 for the
  FDCAN and RCC registers in use. (Q1, P7a)

### Changed

- **Breaking:** `pac::registers` is now `pac::fdcan`, and `pac_traits` is now `pac::common`.
- **Breaking:** `FdCanConfig::interrupt_line_config` and both `select_interrupt_line_1` functions take the
  raw `Ils` register value instead of `Ir`. ILS has one bit per interrupt on H7 and one bit per interrupt group on the lite cores.
- **Breaking:** `take_enabled()` and `disable()` exist only with the `rcc` feature.
- RCC register code went from about 19k generated lines (H7 + G0) to about 500.
- Clock enable / reset in `FdCanInstances` goes through the generated `rcc_fdcan` helpers. Only the
  clock-source check is still per chip. (M2)
- AGENTS.md / README: the macro guideline now targets HAL-style "whole driver in one `macro_rules!`" code.
  Macros that remove real repetition are fine. (P10)

### Fixed

- G0 used the H7 register map, so most accesses from offset 0x80 on and most IR/IE bits were wrong. G0 now
  uses the lite map: global filter in RXGFC, `CCCR.BRSE`, lite IR/IE/TXBTIE/TXBCIE masks, typed `TSCC.TSS`.
  (P2)
- Interrupt enable and clear masks no longer set reserved bits on lite cores.
- Wrong doc comment on `crel()` ("Endian Register"), from a hand edit to the old generated file.

### Removed

- Unused `src/pac/rcc_g4.rs`. The generator recreates it when G4 support starts (P3).
