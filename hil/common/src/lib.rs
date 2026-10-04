//! Frames both boards of a board <-> board HIL test agree on (FEATURES.md Q5a). The sender builds frame `n`
//! from [frame], the receiver checks what arrived with [check]. Plain integers only, so the crate builds for
//! any target and doesn't depend on mcan.
#![no_std]

/// Nominal and data bit rate both boards are configured for (Mbit/s).
pub const NOMINAL_MBPS: u32 = 1;
pub const DATA_MBPS: u32 = 2;

/// How long a receiver waits for the first frame: the sender is flashed after it, so this covers the flashing
/// time of the other board (milliseconds).
pub const FIRST_FRAME_TIMEOUT_MS: u64 = 30_000;
/// Longest gap between two frames of one run (milliseconds).
pub const FRAME_GAP_TIMEOUT_MS: u64 = 200;
/// Frames in the `soak` scenario.
pub const SOAK_FRAMES: usize = 1000;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Frame {
    /// 11 or 29 bit identifier, see `extended`.
    pub id: u32,
    pub extended: bool,
    pub fd: bool,
    pub brs: bool,
    /// Data length in bytes (valid CAN FD length).
    pub len: u8,
}

const fn f(id: u32, extended: bool, fd: bool, brs: bool, len: u8) -> Frame {
    Frame { id, extended, fd, brs, len }
}

/// One pass of every frame class.
pub const FRAMES: [Frame; 16] = [
    f(0x000, false, false, false, 0),
    f(0x123, false, false, false, 1),
    f(0x7FF, false, false, false, 8),
    f(0x0000_0000, true, false, false, 4),
    f(0x1234_5678, true, false, false, 8),
    f(0x1FFF_FFFF, true, false, false, 0),
    f(0x100, false, true, false, 12),
    f(0x101, false, true, false, 24),
    f(0x102, false, true, false, 64),
    f(0x200, false, true, true, 8),
    f(0x201, false, true, true, 12),
    f(0x202, false, true, true, 32),
    f(0x203, false, true, true, 64),
    f(0x1ABC_DE01, true, true, true, 16),
    f(0x1ABC_DE02, true, true, true, 48),
    f(0x1ABC_DE03, true, true, true, 64),
];

/// Frame number `n` of a run (the list repeats).
pub fn frame(n: usize) -> Frame {
    FRAMES[n % FRAMES.len()]
}

/// Payload of frame `n`: distinct per frame number and byte position, so a lost or duplicated frame shows up.
pub fn payload(n: usize, len: usize) -> [u8; 64] {
    core::array::from_fn(|i| if i < len { (n * 31 + i * 7 + 1) as u8 } else { 0 })
}

/// What a receiver saw.
pub struct Received<'a> {
    pub id: u32,
    pub extended: bool,
    pub fd: bool,
    pub brs: bool,
    pub rtr: bool,
    pub len: usize,
    pub data: &'a [u8],
}

/// Checks that `rx` is frame number `n`. Returns what differs.
pub fn check(n: usize, rx: &Received) -> Result<(), &'static str> {
    let want = frame(n);
    if rx.rtr {
        return Err("rtr set");
    }
    if rx.extended != want.extended {
        return Err("id type");
    }
    if rx.id != want.id {
        return Err("id");
    }
    if rx.fd != want.fd {
        return Err("fd flag");
    }
    if rx.brs != want.brs {
        return Err("brs flag");
    }
    if rx.len != want.len as usize || rx.data.len() != rx.len {
        return Err("length");
    }
    if rx.data != &payload(n, rx.len)[..rx.len] {
        return Err("data");
    }
    Ok(())
}
