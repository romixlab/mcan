# Changelog

All notable changes to this crate are recorded here, newest first. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Feature IDs (`P7a`, `M2`, …) refer to
[FEATURES.md](FEATURES.md), which holds the current status. This file only records what changed.

## [Unreleased]

### Added

- `hil/run-bus.sh`, `hil/common`, `bus` tests in `hil/b129` and `hil/b135`: board <-> board HIL runner for B129A and
  B135B on one bus (frames and 1000-frame soak, both directions, counters must stay 0). Passes on the real
  boards. (Q5a)
- `hil/b129`: CPU on a 64 MHz PLL; nominal bit timing fixed to 12 tq (was 13, so 923 kbit/s). (Q3)
- FDCAN lite (G0) frame TX/RX: `FdCan::transmit` (3-element TX FIFO, `Error::TxQueueFull`), `receive_fifo` and
  `RxFrameHeader` on lite cores too, fixed message RAM offsets. `TxBufferIdx` is exported on every chip.
  `hil/b129` `loopback` (classic, FD + BRS, FIFO order) passes on B129A. (R5, Y1, Q5a)
- `hil/b129`: HIL test crate for B129A (STM32G0B1CE, thumbv6m): 12 MHz HSE as FDCAN kernel clock, CAN_STBY (PC13)
  driven low, `bring_up` (config mode, both instances, every mode) and `idle_bus` (RX recessive, idle without
  errors, standby). 11 tests pass on B129A (Q3, Q4).
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
- `hil/b135` depends on `cnt` 0.4.1 and links `cnt.x`. A compile-time guard refuses builds where cnt's `disabled`
  feature is on, as counter asserts would then pass trivially. (Q9)
- `hil/b135`: first HIL test crate (embedded-test 0.7.2 + probe-rs 0.32). Internal loopback tests for classic and
  FD frames, FIFO1 routing, FIFO overflow, short buffers and truncation; all pass on B135B. (Q3, Q4)
- `examples/h7_embassy` `loopback` bin: internal loopback TX → RX smoke test. (D1)
- Transceiver delay compensation: `TransceiverDelayCompensation` (offset, filter window, `at_sample_point`)
  enabled with `DataBitTiming::with_tdc`, written to DBTP.TDC and TDCR. (T2)
- Host tests for bit timing validation and NBTP / DBTP / TDCR encoding. (Q1, T1, T2)
- `MessageRamBuilderError::TriggerMemoryNotSupported`: trigger memory can only be allocated for FDCAN1. (R1)
- `Interrupts`: typed set of interrupt sources with the right bit positions for the full and lite cores.
  `FdCanConfig::interrupts` / `set_interrupts` enable them, `FdCan::enable_interrupts` /
  `disable_interrupts` change them in any mode, `interrupt_flags` / `take_interrupt_flags` poll them. (I2)
- `mcan::on_interrupt(instance, InterruptLine)` (module `interrupt`), available without the `asynchronous`
  feature. (I1)
- Async `wait_interrupts`, `wait_bus_off` and `wait_bus_off_recovered`. (I3, E3)
- `error_counters()` (ECR: TEC, REC, RP, CEL) and `protocol_status()` (PSR: LEC, DLEC, activity, EW / EP /
  BO, RESI / RBRS / REDL / PXE, TDCV), with `ErrorState`. Error codes that the driver's own PSR reads
  reset are kept for the next `protocol_status()`. (E1, E2)
- Bus-off: `is_bus_off()`, `recover_from_bus_off()` (clears INIT, no mode change) and
  `FdCanConfig::automatic_bus_off_recovery` (recovery started by the interrupt handler). (E3)
- `TestMode` can transmit and receive, `set_tx_pin(TxPinControl)` and `rx_pin()` give access to TEST.TX /
  TEST.RX. (M5)
- `hil/b135`: `interrupts`, `errors` and `test_mode` suites, all passing on B135B. Errors and bus-off are
  provoked in internal loopback through a wrong TDC offset, so the bus is never driven. Shared helpers
  (`layout`, `fdcan1_config`, `classic`, `block_on_woken`, `connect_fdcan1_pins`) moved into the crate. (Q4)
- `portable-atomic` dependency for the state shared with the interrupt handler.

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
- **Breaking:** interrupts are enabled from the config only. The driver used to enable every source when
  `asynchronous` was on; now none are enabled by default, and ILE enables a line only if a source is routed
  to it. `FdCanConfig::interrupt_line_config: Ils` is replaced by `interrupt_line_1: Interrupts`
  (`select_interrupt_line_1` takes `Interrupts`). On the lite cores a source moves its whole ILS group. (I2)
- **Breaking:** `asynchronous::on_interrupt` moved to `mcan::on_interrupt`, and `FdCanInterrupt::{Irq0, Irq1}`
  is now `InterruptLine::{Line0, Line1}`. (I1)
- **Breaking:** `clear_transmission_completed_flag` and `clear_transmission_cancelled_flag` removed, use
  `take_interrupt_flags`. (I1)
- **Breaking:** `take_enabled()` and `disable()` exist only with the `rcc` feature.
- RCC register code went from about 19k generated lines (H7 + G0) to about 500.
- Clock enable / reset in `FdCanInstances` goes through the generated `rcc_fdcan` helpers. Only the
  clock-source check is still per chip. (M2)
- AGENTS.md / README: the macro guideline now targets HAL-style "whole driver in one `macro_rules!`" code.
  Macros that remove real repetition are fine. (P10)

### Fixed

- `into_powered_down` timed out waiting for INIT to clear in clock stop mode; INIT now stays set (M3, M5).
  Found by `hil/b129` `config_mode_and_back`.
- The interrupt handler cleared every IR flag, so events nobody had handled were lost (e.g. RFxL for
  `take_rx_fifo_message_lost`). It now handles only enabled flags routed to its line, and latches them. (I1, Y6)
- Interrupt line 1 never fired: only EINT0 was set in ILE. (I2)
- `g0,embassy` didn't compile (the interrupt code used FDCAN3 unconditionally). (P2, I1)
- `set_global_filter` didn't store the filter in the config, so leaving Config mode re-applied the previous
  one. (F1)
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
