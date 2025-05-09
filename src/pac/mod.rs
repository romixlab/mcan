pub mod registers;

// TODO: remove unused code from rcc
#[cfg(feature = "g0")]
pub(crate) mod rcc_g0;

#[cfg(feature = "h7")]
pub(crate) mod rcc_h7;

#[cfg(feature = "g0")]
pub(crate) mod mapping {
    pub(crate) const RCC_REGISTER_BLOCK_ADDR: *mut () = 0x4002_1000 as *mut ();
    pub(crate) const FDCAN1_REGISTER_BLOCK_ADDR: *mut () = 0x4000_6400 as *mut ();
    pub(crate) const FDCAN2_REGISTER_BLOCK_ADDR: *mut () = 0x4000_6800 as *mut ();
    pub(crate) const FDCAN_MSGRAM: *mut u32 = 0x4000_B400 as *mut u32;
    pub(crate) const FDCAN_MSGRAM_LEN_WORDS: usize = 512;
}

#[cfg(feature = "h7")]
pub(crate) mod mapping {
    pub(crate) const RCC_REGISTER_BLOCK_ADDR: *mut () = 0x5802_4400 as *mut ();
    pub(crate) const FDCAN1_REGISTER_BLOCK_ADDR: *mut () = 0x4000_A000 as *mut ();
    pub(crate) const FDCAN2_REGISTER_BLOCK_ADDR: *mut () = 0x4000_A400 as *mut ();
    pub(crate) const FDCAN3_REGISTER_BLOCK_ADDR: *mut () = 0x4000_D400 as *mut ();
    // pub(crate) const FDCAN_CCU_REGISTER_BLOCK_ADDR: *mut () = 0x4000_A800 as *mut ();
    pub(crate) const FDCAN_MSGRAM: *mut u32 = 0x4000_AC00 as *mut u32;
    pub(crate) const FDCAN_MSGRAM_LEN_WORDS: usize = 2560;
}

pub(crate) use mapping::*;
