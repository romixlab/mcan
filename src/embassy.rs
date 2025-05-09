use crate::Error;
use static_cell::StaticCell;

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
