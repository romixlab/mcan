//! FDCAN1 on an idle bus through the TCAN1044V (FEATURES.md M5, E1, E2). Needs the board, a probe and **no other
//! node transmitting**. The bus needs no terminator for these tests: nothing is sent.
#![no_std]
#![no_main]

#[cfg(test)]
#[embedded_test::tests]
mod tests {
    use embassy_time::{Duration, block_for};
    use hil_b129::Board;
    use mcan::{Activity, ErrorState};

    #[init]
    fn init() -> Board {
        Board::init()
    }

    /// RX reads recessive through the awake transceiver (STB low).
    #[test]
    fn rx_recessive_when_awake(mut board: Board) {
        board.connect_fdcan1_pins();
        let can = board.fdcan1_config();
        let can = defmt::unwrap!(can.into_test_mode().map_err(|(e, _)| e));
        board.wake();
        defmt::assert!(can.rx_pin(), "RX dominant: bus busy or transceiver not awake");
    }

    /// Normal mode synchronizes after 11 recessive bits, then idles without errors.
    #[test]
    fn normal_goes_idle_without_errors(mut board: Board) {
        board.connect_fdcan1_pins();
        let can = board.fdcan1_config();
        let mut can = defmt::unwrap!(can.into_normal().map_err(|(e, _)| e));
        block_for(Duration::from_millis(5));
        let status = can.protocol_status();
        let counters = can.error_counters();
        defmt::info!("{} {}", status, counters);
        defmt::assert_eq!(status.activity, Activity::Idle);
        defmt::assert!(!status.last_error.is_error());
        defmt::assert!(!status.bus_off && !status.error_warning && !status.error_passive);
        defmt::assert_eq!((counters.transmit, counters.receive, counters.error_logging), (0, 0, 0));
        defmt::assert!(matches!(status.error_state(), ErrorState::Active));
    }

    /// Bus monitoring on an idle bus behaves the same.
    #[test]
    fn bus_monitoring_goes_idle(mut board: Board) {
        board.connect_fdcan1_pins();
        let can = board.fdcan1_config();
        let mut can = defmt::unwrap!(can.into_bus_monitoring().map_err(|(e, _)| e));
        block_for(Duration::from_millis(5));
        let status = can.protocol_status();
        defmt::assert_eq!(status.activity, Activity::Idle);
        defmt::assert!(!status.last_error.is_error());
    }

    /// Transceiver in standby (PC13 high, hardware STB): the core must still sit quietly. The RX level in
    /// standby is logged, not asserted: it depends on the TCAN1044V wake-up pattern behaviour (boards/b129.md).
    #[test]
    fn standby_is_quiet(mut board: Board) {
        board.sleep();
        board.connect_fdcan1_pins();
        let can = board.fdcan1_config();
        let mut can = defmt::unwrap!(can.into_normal().map_err(|(e, _)| e));
        block_for(Duration::from_millis(5));
        let status = can.protocol_status();
        defmt::info!("standby: {} {}", status, can.error_counters());
        defmt::assert!(!status.bus_off);
    }
}
