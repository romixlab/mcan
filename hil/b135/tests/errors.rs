//! Error counters, protocol status and bus-off recovery on FDCAN1 (FEATURES.md E1, E2, E3).
//!
//! Errors are provoked in internal loopback, so the bus is never driven: FD frames with bit rate switching
//! and a transceiver delay compensation offset of 127 minimum time quanta (about 8 data bits at 4 Mbit/s).
//! The core then checks every data phase bit against the one sent 8 bits later; with alternating 0x00 /
//! 0xFF bytes that is a bit error (DLEC, PED) in every frame. Internal loopback ignores missing ACKs, so the
//! ACK error exception doesn't cap the transmit error counter: each attempt adds 8, the node goes error
//! warning (96), error passive (128) and bus-off (> 255). Needs only the board and a probe.
#![no_std]
#![no_main]

use embassy_stm32::interrupt;
use mcan::{FdCanInstance, InterruptLine};

#[interrupt]
fn FDCAN1_IT0() {
    mcan::on_interrupt(FdCanInstance::FdCan1, InterruptLine::Line0);
}

#[interrupt]
fn FDCAN1_IT1() {
    mcan::on_interrupt(FdCanInstance::FdCan1, InterruptLine::Line1);
}

#[cfg(test)]
#[embedded_test::tests]
mod tests {
    use cortex_m::peripheral::NVIC;
    use embassy_stm32::pac::Interrupt;
    use embassy_time::{Duration, Instant};
    use hil_b135::{Board, FRAME_TIMEOUT, IDS, block_on_woken, fdcan1_config, layout};
    use mcan::config::FrameTransmissionConfig;
    use mcan::{
        ConfigMode, DataBitTiming, DataFieldSize, ErrorState, FdCan, InternalLoopbackMode,
        Interrupts, LastErrorCode, TransceiverDelayCompensation, TxBufferIdx, TxFrameHeader,
    };

    /// FDCAN1 in internal loopback with a wrong TDC offset, error interrupts enabled on line 0.
    fn setup(
        board: &mut Board,
        configure: impl FnOnce(&mut FdCan<ConfigMode>),
    ) -> (FdCan<InternalLoopbackMode>, TxBufferIdx) {
        let (l, tx) = layout(
            board,
            (2, DataFieldSize::_64Bytes),
            (0, DataFieldSize::_8Bytes),
        );
        let mut can = fdcan1_config(board, l);
        let tdc = defmt::unwrap!(TransceiverDelayCompensation::new(127, 0));
        let data = defmt::unwrap!(DataBitTiming::new(1, 11, 4, 4));
        can.set_data_bit_timing(defmt::unwrap!(data.with_tdc(tdc)));
        can.set_frame_transmit(FrameTransmissionConfig::AllowFdCanAndBRS);
        can.set_interrupts(Interrupts::ERRORS);
        configure(&mut can);
        let mut can = defmt::unwrap!(can.into_internal_loopback());
        // Clean slate: nothing counted before the first frame.
        defmt::assert_eq!(can.error_counters().transmit, 0);
        can.protocol_status();
        can.take_interrupt_flags(Interrupts::ALL);
        (can, tx)
    }

    /// Requests the failing frame.
    fn request(can: &mut FdCan<InternalLoopbackMode>, tx: TxBufferIdx) {
        let data: [u8; 64] = core::array::from_fn(|i| if i % 2 == 0 { 0x00 } else { 0xFF });
        defmt::unwrap!(can.write_tx_buffer_pend(tx, TxFrameHeader::fd_brs(IDS[1]), &data));
    }

    fn poll_until(timeout: Duration, mut f: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if f() {
                return true;
            }
        }
        false
    }

    #[init]
    fn init() -> Board {
        Board::init()
    }

    /// One attempt per request (automatic retransmission off): TEC grows by 8 per error, EW at 96, EP at
    /// 128, CEL counts one error per read, DLEC reports the data phase bit error (E1, E2).
    #[test]
    fn bit_errors_count_up(mut board: Board) {
        let (mut can, tx) = setup(&mut board, |c| c.set_automatic_retransmit(false));
        request(&mut can, tx);
        for attempt in 1..=16u8 {
            if attempt > 1 {
                // Re-requesting right after the failed attempt (TXBRP already clear, error frame still on the
                // bus) drops the request with IR.MRAF instead (B135B, FEATURES.md X8). Let the error frame end.
                embassy_time::block_for(Duration::from_micros(50));
                defmt::unwrap!(can.tx_buffer_pend(tx));
            }
            // Not `is_idle()`: TXBRP shows the request only a few cycles after the TXBAR write.
            defmt::assert!(
                poll_until(FRAME_TIMEOUT, || can
                    .interrupt_flags()
                    .contains(Interrupts::PROTOCOL_ERROR_DATA)
                    && can.is_idle()),
                "attempt {} not finished: flags {} {}",
                attempt,
                can.interrupt_flags(),
                can.error_counters()
            );
            let counters = can.error_counters();
            let status = can.protocol_status();
            defmt::debug!("attempt {}: {} {}", attempt, counters, status);
            defmt::assert_eq!(counters.transmit, 8 * attempt);
            defmt::assert_eq!(counters.receive, 0);
            defmt::assert!(!counters.receive_error_passive);
            defmt::assert_eq!(counters.error_logging, 1, "CEL is reset by every read");
            defmt::assert!(status.data_last_error.is_error());
            defmt::assert!(!status.last_error.is_error(), "arbitration phase is fine");
            defmt::assert_eq!(status.error_warning, attempt >= 12);
            defmt::assert_eq!(status.error_passive, attempt >= 16);
            defmt::assert!(!status.bus_off);
            let flags = can.take_interrupt_flags(Interrupts::ERRORS);
            defmt::assert!(flags.contains(Interrupts::PROTOCOL_ERROR_DATA));
            defmt::assert!(!flags.contains(Interrupts::PROTOCOL_ERROR_ARBITRATION));
            defmt::assert_eq!(flags.contains(Interrupts::WARNING_STATUS), attempt == 12);
            defmt::assert_eq!(flags.contains(Interrupts::ERROR_PASSIVE), attempt == 16);
        }
        defmt::assert_eq!(can.protocol_status().error_state(), ErrorState::Passive);
    }

    /// Retransmission on: the node goes bus-off and stays there until told to recover (E2, E3).
    #[test]
    fn bus_off(mut board: Board) {
        let (mut can, tx) = setup(&mut board, |_| {});
        request(&mut can, tx);
        defmt::assert!(poll_until(Duration::from_millis(50), || can.is_bus_off()));
        let counters = can.error_counters();
        let status = can.protocol_status();
        defmt::info!("bus-off: {} {}", status, counters);
        defmt::assert_eq!(status.error_state(), ErrorState::BusOff);
        defmt::assert!(status.error_passive && status.error_warning);
        defmt::assert!(status.data_last_error.is_error());
        let flags = can.take_interrupt_flags(Interrupts::ERRORS);
        defmt::assert!(flags.contains(
            Interrupts::BUS_OFF
                | Interrupts::ERROR_PASSIVE
                | Interrupts::WARNING_STATUS
                | Interrupts::PROTOCOL_ERROR_DATA
        ));

        // Cancel the frame, or it fails again right after a recovery.
        defmt::unwrap!(can.abort_blocking(tx));
        defmt::assert!(can.is_idle());
        defmt::assert!(can.is_bus_off(), "stays bus-off until recovered");
        embassy_time::block_for(Duration::from_millis(5));
        defmt::assert!(can.is_bus_off(), "no recovery without clearing INIT");
    }

    /// The recovery: started by `recover_from_bus_off`, done after 129 × 11 recessive bits (E3).
    #[test]
    fn bus_off_recovery_resets_counters(mut board: Board) {
        let (mut can, tx) = setup(&mut board, |_| {});
        request(&mut can, tx);
        defmt::assert!(poll_until(Duration::from_millis(50), || can.is_bus_off()));
        defmt::unwrap!(can.abort_blocking(tx));
        can.take_interrupt_flags(Interrupts::ALL);
        can.protocol_status();

        defmt::assert!(
            defmt::unwrap!(can.recover_from_bus_off()),
            "recovery started"
        );
        defmt::assert!(
            !defmt::unwrap!(can.recover_from_bus_off()),
            "already recovering"
        );
        // 129 × 11 bits at 1 Mbit/s = 1.4 ms.
        let start = Instant::now();
        defmt::assert!(poll_until(Duration::from_millis(50), || !can.is_bus_off()));
        let took = start.elapsed();
        let counters = can.error_counters();
        let status = can.protocol_status();
        defmt::info!(
            "recovered after {} us: {} {}",
            took.as_micros(),
            status,
            counters
        );
        defmt::assert!(
            took >= Duration::from_micros(1400),
            "recovery sequence too short"
        );
        defmt::assert_eq!((counters.transmit, counters.receive), (0, 0));
        defmt::assert_eq!(status.error_state(), ErrorState::Active);
        defmt::assert!(!status.error_warning);
        // During the recovery LEC shows Bit0 for every 11 recessive bits (Bosch M_CAN user manual PSR.LEC).
        defmt::assert_eq!(status.last_error, LastErrorCode::Bit0);
        defmt::assert!(
            can.take_interrupt_flags(Interrupts::BUS_OFF)
                .contains(Interrupts::BUS_OFF),
            "Bus_Off fires when leaving bus-off too"
        );
        defmt::assert!(!defmt::unwrap!(can.recover_from_bus_off()), "not bus-off");
    }

    /// Automatic recovery from the interrupt handler, observed with the async waits (E3, I1, I3).
    #[test]
    fn bus_off_automatic_recovery(mut board: Board) {
        let (mut can, tx) = setup(&mut board, |c| c.set_automatic_bus_off_recovery(true));
        defmt::assert!(can.enabled_interrupts().contains(Interrupts::BUS_OFF));
        unsafe {
            NVIC::unmask(Interrupt::FDCAN1_IT0);
            NVIC::unmask(Interrupt::FDCAN1_IT1);
        }
        request(&mut can, tx);
        let timeout = Duration::from_millis(50);
        defmt::unwrap!(block_on_woken(timeout, can.wait_bus_off()), "no bus-off");
        // Nobody calls recover_from_bus_off: the handler does.
        defmt::unwrap!(
            block_on_woken(timeout, can.wait_bus_off_recovered()),
            "no automatic recovery"
        );
        // The frame is still pending and fails again: the cycle repeats.
        defmt::unwrap!(
            block_on_woken(timeout, can.wait_bus_off()),
            "no second bus-off"
        );
        defmt::unwrap!(can.abort_blocking(tx));
    }
}
