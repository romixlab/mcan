//! Interrupt enable, line routing and the interrupt handler on FDCAN1 (FEATURES.md I1, I2, I3), in internal
//! loopback. Needs only the board and a probe: internal loopback doesn't drive the TX pin.
//!
//! Most tests keep both FDCAN1 vectors masked in the NVIC: the pending bits show which line an event raised,
//! and the tests call `on_interrupt` themselves. The lines are level-triggered, so a pending bit that comes
//! back after `unpend` means a flag is still set.
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
    use embassy_time::{Duration, block_for};
    use hil_b135::{
        Board, IDS, block_on_woken, classic, fdcan1_config, layout, pattern, receive, send,
    };
    use mcan::config::FdCanConfig;
    use mcan::{
        DataFieldSize, FdCan, FdCanInstance, InternalLoopbackMode, InterruptLine, Interrupts,
        RxFifo, on_interrupt,
    };

    const IT0: Interrupt = Interrupt::FDCAN1_IT0;
    const IT1: Interrupt = Interrupt::FDCAN1_IT1;

    /// FDCAN1 in internal loopback with the given interrupts and line 1 selection, FIFO0 of 4 elements.
    fn setup(
        board: &mut Board,
        interrupts: Interrupts,
        line1: Interrupts,
    ) -> (FdCan<InternalLoopbackMode>, mcan::TxBufferIdx) {
        let (l, tx) = layout(
            board,
            (4, DataFieldSize::_8Bytes),
            (0, DataFieldSize::_8Bytes),
        );
        let mut can = fdcan1_config(board, l);
        can.set_interrupts(interrupts);
        can.select_interrupt_line_1(line1);
        (defmt::unwrap!(can.into_internal_loopback()), tx)
    }

    /// Sends one frame and waits until it has been received too.
    fn send_one(can: &mut FdCan<InternalLoopbackMode>, tx: mcan::TxBufferIdx, n: usize) {
        send(can, tx, classic(IDS[n % IDS.len()]), &pattern(n, 8)[..8]);
        block_for(Duration::from_micros(200));
    }

    /// Whether the line is still asserted: clear the pending bit and see if the level sets it again.
    fn asserted(irq: Interrupt) -> bool {
        NVIC::unpend(irq);
        // The peripheral line needs a few cycles to set the pending bit again.
        block_for(Duration::from_micros(2));
        NVIC::is_pending(irq)
    }

    #[init]
    fn init() -> Board {
        Board::init()
    }

    /// The driver enables no interrupt and no line unless configured (I2). Flags are still set in IR.
    #[test]
    fn nothing_enabled_by_default(mut board: Board) {
        let (l, tx) = layout(
            &mut board,
            (4, DataFieldSize::_8Bytes),
            (0, DataFieldSize::_8Bytes),
        );
        let can = fdcan1_config(&mut board, l);
        defmt::assert_eq!(FdCanConfig::default().interrupts, Interrupts::NONE);
        let mut can = defmt::unwrap!(can.into_internal_loopback());
        defmt::assert_eq!(can.enabled_interrupts(), Interrupts::NONE);
        send_one(&mut can, tx, 0);
        defmt::assert!(!asserted(IT0) && !asserted(IT1));
        let flags = can.interrupt_flags();
        defmt::assert!(flags.contains(Interrupts::RX_FIFO0_NEW_MESSAGE | Interrupts::TX_COMPLETED));
    }

    /// Line 0: the handler clears what it handles, which releases the line, and latches it for
    /// `take_interrupt_flags` (I1).
    #[test]
    fn line0_handler_clears_and_latches(mut board: Board) {
        let enabled = Interrupts::RX_FIFO0_NEW_MESSAGE | Interrupts::TX_COMPLETED;
        let (mut can, tx) = setup(&mut board, enabled, Interrupts::NONE);
        defmt::assert_eq!(can.enabled_interrupts(), enabled);
        send_one(&mut can, tx, 0);
        defmt::assert!(asserted(IT0));
        defmt::assert!(!asserted(IT1));

        on_interrupt(FdCanInstance::FdCan1, InterruptLine::Line0);
        defmt::assert!(!asserted(IT0), "handled flags must be cleared in IR");
        defmt::assert_eq!(can.take_interrupt_flags(enabled), enabled, "latched");
        defmt::assert_eq!(
            can.take_interrupt_flags(enabled),
            Interrupts::NONE,
            "taken once"
        );

        let mut buf = [0u8; 8];
        let (h, _) = receive(&mut can, RxFifo::Fifo0, &mut buf);
        defmt::assert_eq!(h.id, IDS[0]);
    }

    /// Sources selected for line 1 raise FDCAN1_IT1, and each handler only touches its own line (I1, I2).
    #[test]
    fn line1_routing(mut board: Board) {
        let enabled = Interrupts::RX_FIFO0_NEW_MESSAGE | Interrupts::TX_COMPLETED;
        let (mut can, tx) = setup(&mut board, enabled, Interrupts::RX_FIFO0_NEW_MESSAGE);
        send_one(&mut can, tx, 1);
        defmt::assert!(asserted(IT0), "TC on line 0");
        defmt::assert!(asserted(IT1), "RF0N on line 1");

        on_interrupt(FdCanInstance::FdCan1, InterruptLine::Line0);
        defmt::assert!(!asserted(IT0));
        defmt::assert!(
            asserted(IT1),
            "line 0 handler must leave line 1 flags alone"
        );
        defmt::assert_eq!(
            can.take_interrupt_flags(Interrupts::TX_COMPLETED),
            Interrupts::TX_COMPLETED
        );

        on_interrupt(FdCanInstance::FdCan1, InterruptLine::Line1);
        defmt::assert!(!asserted(IT1));
        defmt::assert_eq!(
            can.take_interrupt_flags(enabled),
            Interrupts::RX_FIFO0_NEW_MESSAGE
        );
    }

    /// Flags of sources that aren't enabled stay in IR for polling (I1: no more blind clearing).
    #[test]
    fn disabled_flags_untouched(mut board: Board) {
        let (mut can, tx) = setup(&mut board, Interrupts::TX_COMPLETED, Interrupts::NONE);
        send_one(&mut can, tx, 2);
        on_interrupt(FdCanInstance::FdCan1, InterruptLine::Line0);
        on_interrupt(FdCanInstance::FdCan1, InterruptLine::Line1);
        defmt::assert!(!asserted(IT0));
        defmt::assert_eq!(
            can.take_interrupt_flags(Interrupts::RX_FIFO0_NEW_MESSAGE | Interrupts::TX_COMPLETED),
            Interrupts::RX_FIFO0_NEW_MESSAGE | Interrupts::TX_COMPLETED,
            "RF0N from IR, TC latched by the handler"
        );
    }

    /// A lost frame is still reported when the message-lost interrupt is enabled and handled (Y6).
    #[test]
    fn lost_frame_seen_after_handler(mut board: Board) {
        let (mut can, tx) = setup(
            &mut board,
            Interrupts::RX_FIFO0_MESSAGE_LOST,
            Interrupts::NONE,
        );
        for n in 0..5 {
            send_one(&mut can, tx, n);
        }
        defmt::assert!(asserted(IT0));
        on_interrupt(FdCanInstance::FdCan1, InterruptLine::Line0);
        defmt::assert!(!asserted(IT0));
        defmt::assert!(can.take_rx_fifo_message_lost(RxFifo::Fifo0));
        defmt::assert!(!can.take_rx_fifo_message_lost(RxFifo::Fifo0));
    }

    /// `wait_interrupts` is woken by the real interrupt handler (I3).
    #[test]
    fn wait_interrupts_wakes(mut board: Board) {
        let (mut can, tx) = setup(&mut board, Interrupts::NONE, Interrupts::NONE);
        unsafe {
            NVIC::unmask(IT0);
            NVIC::unmask(IT1);
        }
        for n in 0..3 {
            // Request the frame without waiting, so the future is pending before it arrives.
            defmt::unwrap!(can.write_tx_buffer_pend(tx, classic(IDS[n]), &pattern(n, 8)[..8]));
            let flags = defmt::unwrap!(block_on_woken(
                Duration::from_millis(10),
                can.wait_interrupts(Interrupts::RX_FIFO0_NEW_MESSAGE)
            ));
            defmt::assert_eq!(flags, Interrupts::RX_FIFO0_NEW_MESSAGE);
            let mut buf = [0u8; 8];
            let (h, _) = receive(&mut can, RxFifo::Fifo0, &mut buf);
            defmt::assert_eq!(h.id, IDS[n]);
        }
        defmt::assert_eq!(can.enabled_interrupts(), Interrupts::RX_FIFO0_NEW_MESSAGE);
    }
}
