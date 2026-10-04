//! Clock, reset, core check and mode transitions of FDCAN1 on the lite core (FEATURES.md M2, M4, M5, R4). Needs
//! only the board and a probe; the bus is never driven.
#![no_std]
#![no_main]

#[cfg(test)]
#[embedded_test::tests]
mod tests {
    use hil_b129::Board;
    use mcan::{Error, FdCanInstance};

    #[init]
    fn init() -> Board {
        Board::init()
    }

    /// RCC enable and reset, CREL check, Config mode and back to powered down (M2, M3, M4).
    #[test]
    fn config_mode_and_back(mut board: Board) {
        board.connect_fdcan1_pins();
        let can = defmt::unwrap!(board.instances.take_enabled(FdCanInstance::FdCan1));
        let can = defmt::unwrap!(can.into_config_mode());
        defmt::assert!(!can.is_bus_off());
        defmt::unwrap!(can.into_powered_down().map_err(|(e, _)| e));
    }

    /// Both lite instances enter Config mode, each zeroing its own half of the message RAM (R4).
    #[test]
    fn both_instances_enter_config(mut board: Board) {
        let c1 = defmt::unwrap!(board.instances.take_enabled(FdCanInstance::FdCan1));
        let c2 = defmt::unwrap!(board.instances.take(FdCanInstance::FdCan2));
        let _c1 = defmt::unwrap!(c1.into_config_mode());
        let _c2 = defmt::unwrap!(c2.into_config_mode());
    }

    /// An instance can be taken only once.
    #[test]
    fn instance_taken_twice(mut board: Board) {
        let _c1 = defmt::unwrap!(board.instances.take_enabled(FdCanInstance::FdCan1));
        defmt::assert!(matches!(
            board.instances.take(FdCanInstance::FdCan1),
            Err(Error::PeripheralTaken)
        ));
    }

    /// Normal mode is entered with the pins connected and the transceiver awake (M5).
    #[test]
    fn enter_normal(mut board: Board) {
        board.connect_fdcan1_pins();
        let can = board.fdcan1_config();
        let _can = defmt::unwrap!(can.into_normal().map_err(|(e, _)| e));
    }

    /// Restricted operation (receives and ACKs nothing, never transmits) (M5).
    #[test]
    fn enter_restricted(mut board: Board) {
        board.connect_fdcan1_pins();
        let can = board.fdcan1_config();
        let _can = defmt::unwrap!(can.into_restricted().map_err(|(e, _)| e));
    }

    /// Bus monitoring (M5).
    #[test]
    fn enter_bus_monitoring(mut board: Board) {
        board.connect_fdcan1_pins();
        let can = board.fdcan1_config();
        let _can = defmt::unwrap!(can.into_bus_monitoring().map_err(|(e, _)| e));
    }

    /// Internal loopback needs no pins (M5).
    #[test]
    fn enter_internal_loopback(mut board: Board) {
        let can = board.fdcan1_config();
        let _can = defmt::unwrap!(can.into_internal_loopback().map_err(|(e, _)| e));
    }
}
