use crate::Error;
use static_cell::StaticCell;

#[macro_export]
macro_rules! configure_pins {
    (tx: $tx_pin:expr, rx: $rx_pin:expr) => {{
        use embassy_stm32::gpio::{AfType, Flex, Level, Output, OutputType, Pull, Speed};
        let af_tx = embassy_stm32::can::TxPin::af_num(&$tx_pin);
        let mut can_tx = Flex::new($tx_pin);
        can_tx.set_as_af_unchecked(af_tx, AfType::output(OutputType::PushPull, Speed::VeryHigh));
        core::mem::forget(can_tx);

        let af_rx = embassy_stm32::can::RxPin::af_num(&$rx_pin);
        let mut can_rx = Flex::new($rx_pin);
        can_rx.set_as_af_unchecked(af_rx, AfType::input(Pull::None));
        core::mem::forget(can_rx);
    }};
}
pub use configure_pins;

pub(crate) struct State {}

impl State {
    fn new() -> Self {
        State {}
    }
}

static STATE_FDCAN1: StaticCell<State> = StaticCell::new();
static STATE_FDCAN2: StaticCell<State> = StaticCell::new();
#[cfg(feature = "h7")]
static STATE_FDCAN3: StaticCell<State> = StaticCell::new();

pub(crate) fn state_fdcan1() -> Result<&'static mut State, Error> {
    STATE_FDCAN1
        .try_init(State::new())
        .ok_or(Error::PeripheralTaken)
}

pub(crate) fn state_fdcan2() -> Result<&'static mut State, Error> {
    STATE_FDCAN2
        .try_init(State::new())
        .ok_or(Error::PeripheralTaken)
}

#[cfg(feature = "h7")]
pub(crate) fn state_fdcan3() -> Result<&'static mut State, Error> {
    STATE_FDCAN3
        .try_init(State::new())
        .ok_or(Error::PeripheralTaken)
}
