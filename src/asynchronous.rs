use crate::pac::registers::Fdcan;
use crate::pac::registers::regs::Ir;
use crate::pac::{
    FDCAN1_REGISTER_BLOCK_ADDR, FDCAN2_REGISTER_BLOCK_ADDR, FDCAN3_REGISTER_BLOCK_ADDR,
};
use crate::{Error, FdCanInstance, FdCanInterrupt};
use embassy_sync::waitqueue::AtomicWaker;

pub(crate) struct State {
    pub(crate) rx_dedicated_waker: AtomicWaker,
}

impl State {
    const fn new() -> Self {
        State {
            rx_dedicated_waker: AtomicWaker::new(),
        }
    }
}

pub(crate) fn state_fdcan1() -> &'static State {
    static STATE: State = State::new();
    &STATE
}

pub(crate) fn state_fdcan2() -> &'static State {
    static STATE: State = State::new();
    &STATE
}

#[cfg(feature = "h7")]
pub(crate) fn state_fdcan3() -> &'static State {
    static STATE: State = State::new();
    &STATE
}

pub fn on_interrupt(instance: FdCanInstance, irq: FdCanInterrupt) {
    let (state, regs) = match instance {
        FdCanInstance::FdCan1 => (state_fdcan1(), unsafe {
            Fdcan::from_ptr(FDCAN1_REGISTER_BLOCK_ADDR)
        }),
        FdCanInstance::FdCan2 => (state_fdcan2(), unsafe {
            Fdcan::from_ptr(FDCAN2_REGISTER_BLOCK_ADDR)
        }),
        FdCanInstance::FdCan3 => (state_fdcan3(), unsafe {
            Fdcan::from_ptr(FDCAN3_REGISTER_BLOCK_ADDR)
        }),
    };

    let ir = regs.ir().read();
    #[cfg(feature = "defmt")]
    defmt::trace!("ir: {:?}", ir); // TODO: remove

    // RX
    if ir.drx() {
        state.rx_dedicated_waker.wake();
    }

    regs.ir().write_value(Ir(u32::MAX >> 2));
}
