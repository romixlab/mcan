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
- Host tests for the message RAM builder and layout. (Q1, R1, R6)
- RX FIFO receive (H7): `receive_fifo(RxFifo, &mut buf)` returns an `RxFrameHeader` (ID, RTR, FD/BRS/ESI, length,
  timestamp, filter index) and the copied length, and acknowledges the element. Also `rx_fifo_fill_level` and
  `take_rx_fifo_message_lost`. (Y1, Y6)
- `examples/h7_embassy` `loopback` bin: internal loopback TX → RX smoke test. (D1)
- Transceiver delay compensation: `TransceiverDelayCompensation` (offset, filter window, `at_sample_point`)
  enabled with `DataBitTiming::with_tdc`, written to DBTP.TDC and TDCR. (T2)
- Host tests for bit timing validation and NBTP / DBTP / TDCR encoding. (Q1, T1, T2)
- `MessageRamBuilderError::TriggerMemoryNotSupported`: trigger memory can only be allocated for FDCAN1. (R1)

### Changed

- **Breaking:** `NominalBitTiming` and `DataBitTiming` are built with `const fn new(…) -> Result<_, BitTimingError>`,
  taking plain integers instead of public `NonZero` fields, and are validated against the RM ranges. The
  `transceiver_delay_compensation: bool` field is replaced by `with_tdc`. (T1, T2)
- **Breaking:** `set_layout` and `apply_config` return `Result` and reject a layout built for another instance
  (`Error::WrongInstance`). (R2)
- **Breaking:** `MessageRamLayout::relayout()` and `MessageRamBuilder::recombine()` removed (they were `todo!()`). (R3)
- Message RAM is no longer zeroed as a whole on entering Config mode: H7 zeroes a layout's own region when
  `set_layout` applies a new layout, lite cores zero the instance's fixed block. (R4)
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

- Message RAM bitfield decoders no longer contain `unreachable!()`. (P8)
- Bit timing values at the top of their range (e.g. nominal prescaler 512, seg1 256) were masked to 0 and
  underflowed on `- 1` (panic in debug, garbage in release). Out-of-range values are now rejected. (T1)
- DBTP.TDC / TDCR were never written, so transceiver delay compensation could not be enabled. (T2)
- Message RAM builder used byte offsets for word-addressed start fields, so every region after the first
  was placed 4× too far. (R1)
- Dedicated TX buffer `idx` ignored the element size, so buffers > 0 overlapped buffer 0. (R6, X1)
- The builder carried the previous instance's dedicated TX buffer count into the next instance, refused
  the last 4 words of RAM, and could panic on `u8` overflow or `expect`. (R1, P8)
- Configuring one H7 instance zeroed the whole shared message RAM, wiping running neighbours. (R4)
- G0 used the H7 register map, so most accesses from offset 0x80 on and most IR/IE bits were wrong. G0 now
  uses the lite map: global filter in RXGFC, `CCCR.BRSE`, lite IR/IE/TXBTIE/TXBCIE masks, typed `TSCC.TSS`.
  (P2)
- Interrupt enable and clear masks no longer set reserved bits on lite cores.
- Wrong doc comment on `crel()` ("Endian Register"), from a hand edit to the old generated file.

### Removed

- **Breaking:** unused `FIFONr`, replaced by the public `RxFifo`. (Y1)
- `paste` dependency (unmaintained, RUSTSEC-2024-0436). (P11)
- Unused `src/pac/rcc_g4.rs`. The generator recreates it when G4 support starts (P3).
