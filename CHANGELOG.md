# Changelog

All notable changes to this crate are recorded here, newest first. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Feature IDs (`P7a`, `M2`, …) refer to
[FEATURES.md](FEATURES.md), which holds the current status. This file only records what changed.

## [Unreleased]

### Added

- `tools/gen_pac.py`: generates `src/pac` from a pinned, sha256-checked `stm32-metapac` 21.0.0. The FDCAN maps
  are copied verbatim and RCC is trimmed to a keep list. `--check` verifies the generated files and
  cross-checks the hand-written addresses against metapac (STM32H725IG, STM32G0B1CE). (P7, P7a)
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
- RCC register code went from about 19k generated lines (H7 + G0) to about 400.
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
