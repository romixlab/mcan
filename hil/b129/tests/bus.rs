//! B129A on a real CAN bus with a second node (FEATURES.md Q5a). Not run by plain `cargo test`: the tests need
//! B135B on the other end of the cable, run both sides with `hil/run-bus.sh`. Whichever side receives is
//! started first and waits up to `FIRST_FRAME_TIMEOUT_MS` for the sender.
#![no_std]
#![no_main]

#[cfg(test)]
#[embedded_test::tests]
mod tests {
    use embassy_time::{Duration, Instant};
    use hil_b129::Board;
    use hil_common::{
        FIRST_FRAME_TIMEOUT_MS, FRAME_GAP_TIMEOUT_MS, FRAMES, Received, SOAK_FRAMES, check, frame,
        payload,
    };
    use mcan::config::FrameTransmissionConfig;
    use mcan::fdcan::{Receive, Transmit};
    use mcan::pac::message_ram::{Esi, FrameFormat};
    use mcan::{Error, ExtendedId, FdCan, Id, RxFifo, StandardId, TxFrameHeader};

    #[init]
    fn init() -> Board {
        Board::init()
    }

    /// FDCAN1 in normal mode on the bus, FD with BRS allowed.
    fn bus_up(board: &mut Board) -> FdCan<mcan::fdcan::NormalOperationMode> {
        board.connect_fdcan1_pins();
        let mut can = board.fdcan1_config();
        can.set_frame_transmit(FrameTransmissionConfig::AllowFdCanAndBRS);
        defmt::unwrap!(can.into_normal().map_err(|(e, _)| e))
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

    fn send_run<M: Transmit>(can: &mut FdCan<M>, count: usize) {
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
            let deadline = Instant::now() + Duration::from_millis(FRAME_GAP_TIMEOUT_MS);
            loop {
                match can.transmit(header, &data[..fr.len as usize]) {
                    Ok(_) => break,
                    Err(Error::TxQueueFull) => {
                        if Instant::now() >= deadline {
                            let (c, st) = (can.error_counters(), can.protocol_status());
                            defmt::panic!("TX FIFO stuck full at frame {}: {} {}", n, st, c);
                        }
                    }
                    Err(e) => defmt::panic!("transmit frame {}: {}", n, e),
                }
            }
        }
        let deadline = Instant::now() + Duration::from_millis(FRAME_GAP_TIMEOUT_MS);
        while !can.is_idle() {
            defmt::assert!(Instant::now() < deadline, "frames still pending: no ACK from the other node?");
        }
    }

    fn receive_run<M: Receive>(can: &mut FdCan<M>, count: usize) {
        // run-bus.sh starts the sender when it sees this line.
        defmt::info!("RX READY");
        let mut buf = [0u8; 64];
        let mut deadline = Instant::now() + Duration::from_millis(FIRST_FRAME_TIMEOUT_MS);
        for n in 0..count {
            let (h, copied) = loop {
                if let Some(r) = defmt::unwrap!(can.receive_fifo(RxFifo::Fifo0, &mut buf)) {
                    break r;
                }
                defmt::assert!(Instant::now() < deadline, "no frame {} received", n);
            };
            deadline = Instant::now() + Duration::from_millis(FRAME_GAP_TIMEOUT_MS);
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
                let lost = can.take_rx_fifo_message_lost(RxFifo::Fifo0);
                defmt::panic!(
                    "frame {} differs: {} (got id {:#x} ext {} fd {} len {}, FIFO overrun {}, fill {})",
                    n, what, id, extended, rx.fd, rx.len, lost, can.rx_fifo_fill_level(RxFifo::Fifo0)
                );
            }
        }
        defmt::assert!(!can.take_rx_fifo_message_lost(RxFifo::Fifo0), "RX FIFO overrun");
    }

    /// Sends [FRAMES] once.
    #[test]
    fn send_frames(mut board: Board) {
        let mut can = bus_up(&mut board);
        send_run(&mut can, FRAMES.len());
        assert_clean(&mut can);
    }

    /// Receives [FRAMES] once.
    #[test]
    #[timeout(60)]
    fn receive_frames(mut board: Board) {
        let mut can = bus_up(&mut board);
        receive_run(&mut can, FRAMES.len());
        assert_clean(&mut can);
    }

    /// Sends `SOAK_FRAMES` frames.
    #[test]
    fn send_soak(mut board: Board) {
        let mut can = bus_up(&mut board);
        send_run(&mut can, SOAK_FRAMES);
        assert_clean(&mut can);
    }

    /// Receives `SOAK_FRAMES` frames.
    #[test]
    #[timeout(60)]
    fn receive_soak(mut board: Board) {
        let mut can = bus_up(&mut board);
        receive_run(&mut can, SOAK_FRAMES);
        assert_clean(&mut can);
    }

    /// Diagnosis: sends frame 0 and logs the status once a second, never fails.
    #[test]
    fn diag_send(mut board: Board) {
        let mut can = bus_up(&mut board);
        let fr = frame(1);
        let h = TxFrameHeader {
            frame_format: FrameFormat::Classic,
            id: Id::Standard(defmt::unwrap!(StandardId::new(fr.id as u16))),
            bit_rate_switching: false,
            error_state: Esi::EsiDependsOnErrorPassive,
            marker: None,
        };
        _ = can.transmit(h, &[1]);
        for _ in 0..8 {
            embassy_time::block_for(Duration::from_millis(1000));
            let c = can.error_counters();
            let s = can.protocol_status();
            defmt::info!("B129 {} {}", s, c);
        }
    }
}
