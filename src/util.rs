use crate::fdcan::Error;

#[inline]
pub fn checked_wait<F: Fn() -> bool>(f: F, timeout_iterations: u32) -> Result<(), Error> {
    let mut elapsed = 0;
    while f() {
        elapsed += 1;
        if elapsed >= timeout_iterations {
            return Err(Error::Timeout);
        }
    }
    Ok(())
}
