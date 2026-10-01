//! Test mode TX / RX pin access on FDCAN1 (FEATURES.md M5). Connects the pins, but holds TX recessive, so the
//! bus is never driven. Needs an idle bus (nothing transmitting) and a powered transceiver.
#![no_std]
#![no_main]

#[cfg(test)]
#[embedded_test::tests]
mod tests {
    use embassy_time::{Duration, block_for};
    use hil_b135::{Board, IDS, classic, fdcan1_config, layout};
    use mcan::{Activity, DataFieldSize, TxPinControl};

    #[init]
    fn init() -> Board {
        Board::init()
    }

    /// RX reads the idle (recessive) bus. A frame requested while TX is held recessive never gets a bit
    /// error: the core stays "transmitter" at the start of frame, without counting errors (observed on
    /// B135B, 2026-10-01). So this override can't be used to provoke errors.
    #[test]
    fn tx_held_recessive(mut board: Board) {
        let (l, tx) = layout(
            &mut board,
            (2, DataFieldSize::_8Bytes),
            (0, DataFieldSize::_8Bytes),
        );
        let can = fdcan1_config(&mut board, l);
        board.connect_fdcan1_pins();
        let mut can = defmt::unwrap!(can.into_test_mode());
        can.set_tx_pin(TxPinControl::Recessive);
        defmt::assert!(
            can.rx_pin(),
            "RX dominant: bus busy or transceiver not powered"
        );

        defmt::unwrap!(can.write_tx_buffer_pend(tx, classic(IDS[1]), &[0x55; 8]));
        block_for(Duration::from_millis(5));
        let status = can.protocol_status();
        let counters = can.error_counters();
        defmt::info!("{} {}", status, counters);
        defmt::assert_eq!(status.activity, Activity::Transmitter);
        defmt::assert!(!status.last_error.is_error());
        defmt::assert_eq!((counters.transmit, counters.error_logging), (0, 0));
        defmt::assert!(!can.is_idle());
        defmt::assert!(can.rx_pin());
        defmt::unwrap!(can.abort_blocking(tx));
    }
}
