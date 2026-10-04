//! B135B on a real CAN bus with a second node (FEATURES.md Q5a). Not run by plain `cargo test`: the tests need
//! B129A on the other end of the cable, run both sides with `hil/run-bus.sh`. Whichever side receives is
//! started first and waits up to `FIRST_FRAME_TIMEOUT_MS` for the sender.
#![no_std]
#![no_main]

#[cfg(test)]
#[embedded_test::tests]
mod tests {
    use embassy_stm32::gpio::{Level, Output, Speed};
    use embassy_time::{Duration, block_for};
    use hil_b135::{Board, fdcan1_config, layout, receive_before, send};
    use hil_common::{
        FIRST_FRAME_TIMEOUT_MS, FRAME_GAP_TIMEOUT_MS, FRAMES, Received, SOAK_FRAMES, check, frame,
        payload,
    };
    use mcan::config::FrameTransmissionConfig;
    use mcan::fdcan::NormalOperationMode;
    use mcan::pac::message_ram::{Esi, FrameFormat};
    use mcan::{
        DataBitTiming, DataFieldSize, ExtendedId, FdCan, Id, NominalBitTiming, RxFifo, StandardId,
        TxBufferIdx, TxFrameHeader,
    };

    #[init]
    fn init() -> Board {
        Board::init()
    }

    /// Latching termination relay: pulse TERM_EN (PC13) or TERM_DIS (PE4) for > 10 ms (boards/b135.md).
    fn termination(board: &mut Board, on: bool) {
        // SAFETY: the pins are only used here, and dropped (floating, relay keeps its state) afterwards.
        let mut pin = if on {
            Output::new(unsafe { board.peripherals.PC13.clone_unchecked() }, Level::Low, Speed::Low)
        } else {
            Output::new(unsafe { board.peripherals.PE4.clone_unchecked() }, Level::Low, Speed::Low)
        };
        pin.set_high();
        block_for(Duration::from_millis(30));
        pin.set_low();
        block_for(Duration::from_millis(5));
    }

    /// FDCAN1 in normal mode on the bus at 1 Mbit/s / 2 Mbit/s like B129A (64 MHz kernel clock, 84 % sample
    /// point on both), 8 RX FIFO0 elements of 64 bytes.
    fn bus_up(board: &mut Board) -> (FdCan<NormalOperationMode>, TxBufferIdx) {
        board.connect_fdcan1_pins();
        let (l, tx) = layout(board, (8, DataFieldSize::_64Bytes), (0, DataFieldSize::_8Bytes));
        let mut can = fdcan1_config(board, l);
        can.set_nominal_bit_timing(defmt::unwrap!(NominalBitTiming::new(1, 52, 11, 11)));
        can.set_data_bit_timing(defmt::unwrap!(DataBitTiming::new(1, 26, 5, 5)));
        can.set_frame_transmit(FrameTransmissionConfig::AllowFdCanAndBRS);
        (defmt::unwrap!(can.into_normal().map_err(|(e, _)| e)), tx)
    }

    /// Both counters zero, no protocol error recorded, error active.
    fn assert_clean<M>(can: &mut FdCan<M>) {
        let counters = can.error_counters();
        let status = can.protocol_status();
        defmt::info!("{} {}", status, counters);
        defmt::assert_eq!((counters.transmit, counters.receive, counters.error_logging), (0, 0, 0));
        defmt::assert!(!status.last_error.is_error() && !status.data_last_error.is_error());
        defmt::assert!(!status.bus_off && !status.error_warning && !status.error_passive);
    }

    fn send_run(can: &mut FdCan<NormalOperationMode>, tx: TxBufferIdx, count: usize) {
        for n in 0..count {
            let fr = frame(n);
            let data = payload(n, fr.len as usize);
            let id = if fr.extended {
                Id::Extended(defmt::unwrap!(ExtendedId::new(fr.id)))
            } else {
                Id::Standard(defmt::unwrap!(StandardId::new(fr.id as u16)))
            };
            let header = TxFrameHeader {
                frame_format: if fr.fd { FrameFormat::FD } else { FrameFormat::Classic },
                id,
                bit_rate_switching: fr.brs,
                error_state: Esi::EsiDependsOnErrorPassive,
                marker: None,
            };
            // Waits until the frame is on the bus (and ACKed: without an ACK it never completes).
            send(can, tx, header, &data[..fr.len as usize]);
        }
    }

    fn receive_run(can: &mut FdCan<NormalOperationMode>, count: usize) {
        // run-bus.sh starts the sender when it sees this line.
        defmt::info!("RX READY");
        let mut buf = [0u8; 64];
        let mut timeout = Duration::from_millis(FIRST_FRAME_TIMEOUT_MS);
        for n in 0..count {
            let (h, copied) = receive_before(can, RxFifo::Fifo0, &mut buf, timeout);
            timeout = Duration::from_millis(FRAME_GAP_TIMEOUT_MS);
            let (id, extended) = match h.id {
                Id::Standard(s) => (s.as_raw() as u32, false),
                Id::Extended(e) => (e.as_raw(), true),
            };
            let rx = Received {
                id,
                extended,
                fd: matches!(h.frame_format, FrameFormat::FD),
                brs: h.bit_rate_switching,
                rtr: h.rtr,
                len: h.len as usize,
                data: &buf[..copied],
            };
            if let Err(what) = check(n, &rx) {
                defmt::panic!("frame {} differs: {}", n, what);
            }
        }
        defmt::assert!(!can.take_rx_fifo_message_lost(RxFifo::Fifo0), "RX FIFO overrun");
    }

    /// Connects the 120 ohm termination (B129A has none). The relay latches, so this is needed once.
    #[test]
    fn terminate_on(mut board: Board) {
        termination(&mut board, true);
    }

    /// Disconnects the termination.
    #[test]
    fn terminate_off(mut board: Board) {
        termination(&mut board, false);
    }

    /// Sends [FRAMES] once.
    #[test]
    fn send_frames(mut board: Board) {
        let (mut can, tx) = bus_up(&mut board);
        send_run(&mut can, tx, FRAMES.len());
        assert_clean(&mut can);
    }

    /// Receives [FRAMES] once.
    #[test]
    #[timeout(60)]
    fn receive_frames(mut board: Board) {
        let (mut can, _) = bus_up(&mut board);
        receive_run(&mut can, FRAMES.len());
        assert_clean(&mut can);
    }

    /// Sends `SOAK_FRAMES` frames.
    #[test]
    fn send_soak(mut board: Board) {
        let (mut can, tx) = bus_up(&mut board);
        send_run(&mut can, tx, SOAK_FRAMES);
        assert_clean(&mut can);
    }

    /// Receives `SOAK_FRAMES` frames.
    #[test]
    #[timeout(60)]
    fn receive_soak(mut board: Board) {
        let (mut can, _) = bus_up(&mut board);
        receive_run(&mut can, SOAK_FRAMES);
        assert_clean(&mut can);
    }

    /// Diagnosis: listens in normal mode (ACKs) and logs the status once a second, never fails.
    #[test]
    fn diag_listen(mut board: Board) {
        {
            let rcc = unsafe { board.peripherals.RCC.clone_unchecked() };
            let c = embassy_stm32::rcc::clocks(&rcc);
            defmt::info!("pll1_q {} hse {} sys {}", c.pll1_q, c.hse, c.sys);
            let ckdiv = unsafe { core::ptr::read_volatile(0x4000_A100 as *const u32) };
            defmt::info!("FDCAN_CKDIV {=u32:#x}", ckdiv);
        }
        board.connect_fdcan1_pins();
        let (l, _) = layout(&mut board, (8, DataFieldSize::_64Bytes), (0, DataFieldSize::_8Bytes));
        let mut can = fdcan1_config(&mut board, l);
        can.set_nominal_bit_timing(defmt::unwrap!(NominalBitTiming::new(1, 52, 11, 11)));
        can.set_data_bit_timing(defmt::unwrap!(DataBitTiming::new(1, 26, 5, 5)));
        can.set_frame_transmit(FrameTransmissionConfig::AllowFdCanAndBRS);
        let mut can = defmt::unwrap!(can.into_normal().map_err(|(e, _)| e));
        for _ in 0..12 {
            block_for(Duration::from_millis(1000));
            let c = can.error_counters();
            let s = can.protocol_status();
            defmt::info!("B135 {} {} rx fifo {}", s, c, can.rx_fifo_fill_level(RxFifo::Fifo0));
        }
    }
}
