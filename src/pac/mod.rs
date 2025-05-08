pub mod registers;

// TODO: remove unused code from rcc
#[cfg(feature = "g0")]
pub(crate) mod rcc_g0;

#[cfg(feature = "g0")]
pub(crate) mod mapping {
    pub(crate) const RCC_REGISTER_BLOCK_ADDR: *mut () = 0x4002_1000 as *mut ();
    pub(crate) const FDCAN1_REGISTER_BLOCK_ADDR: *mut () = 0x4000_6400 as *mut ();
    pub(crate) const FDCAN_MSGRAM: *mut u32 = 0x4000_B400 as *mut u32;
    pub(crate) const FDCAN_MSGRAM_LEN_WORDS: usize = 2048 / 4;
}

pub(crate) use mapping::*;