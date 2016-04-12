//! Provide a general handler for file descriptor reasources via the `OwnedFd` and `FdRef` types

use std::os::unix::io::{IntoRawFd,AsRawFd,FromRawFd,RawFd};
use std::mem::{forget, transmute};
use std::io;
use std::borrow::{Borrow,ToOwned};
use std::ops::{Deref};

extern crate libc;

unsafe fn dup(i: RawFd) -> io::Result<RawFd> {
    let v = libc::dup(i);
    if v < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(v)
    }
}

/**
 * OwnedFd is an RAII wrapper around RawFd: it automatically closes on drop & provides borrow()
 * functionality to support lifetimes & borrow checking around the use of file descriptors
 *
 * Compared to using File, TcpStream, etc as a wrapper to close on drop, OwnedFd:
 *
 *  - allows any type of filedescriptor to be used safely
 *  - has no overhead greater than a RawFd (no buffer, metadata, or other allocations)
 *  - allows use of the borrow system to ensure drop (close) happens only when all users of an
 *    ownedfd have released it.
 */
pub struct OwnedFd {
    inner: RawFd,
}

impl OwnedFd {
    /**
     * Given a raw file descriptor that may be owned by another (ie: another data structure might
     * close it), create a Owned version that we have control over (via dup())
     *
     * For taking ownership, see `FromRawFd::from_raw_fd()`.
     *
     * Unsafety:
     *
     *  - @i _must_ be a valid file descriptor (of any kind)
     */
    pub unsafe fn from_unowned_raw(i : RawFd) -> io::Result<OwnedFd> {
        Ok(OwnedFd { inner: try!(dup(i)) })
    }

    /**
     * Duplicate this OwnedFd, and allow the error to be detected.
     *
     * Clone uses this, but panics on error
     */
    pub fn dup(&self) -> io::Result<OwnedFd> {
        unsafe { OwnedFd::from_unowned_raw(self.as_raw_fd()) }
    }

    /**
     * Given a type that impliments `IntoRawFd` construct an OwnedFd.
     *
     * This is safe because we assume the promises of `IntoRawFd` are followed.
     *
     * NOTE: ideally, we'd impl this as From<T>, but current rust doesn't allow that. Revisit when
     * specialization stabilizes.
     */
    pub fn from<T: IntoRawFd>(i: T) -> Self {
        OwnedFd { inner: i.into_raw_fd() }
    }
}

impl AsRawFd for OwnedFd {
    fn as_raw_fd(&self) -> RawFd {
        self.inner
    }
}


impl IntoRawFd for OwnedFd {
    fn into_raw_fd(self) -> RawFd {
        let v = self.inner;
        forget(self);
        v
    }
}

impl FromRawFd for OwnedFd {
    unsafe fn from_raw_fd(fd: RawFd) -> OwnedFd {
        OwnedFd { inner: fd }
    }
}

impl Drop for OwnedFd {
    fn drop(&mut self) {
        unsafe { libc::close(self.inner) };
    }
}

impl Clone for OwnedFd {
    fn clone(&self) -> Self {
        self.dup().unwrap()
    }
}

/*
 * WARNING: assumes RawFd and (*const _) are the same size! (or at least that RawFd is bounded by
 * isize).
 */
impl Borrow<FdRef> for OwnedFd {
    fn borrow(&self) -> &FdRef {
        unsafe { FdRef::from_unowned_raw(self.as_raw_fd()) }
    }
}

impl Deref for OwnedFd {
    type Target = FdRef;
    fn deref(&self) -> &Self::Target {
        self.borrow()
    }
}

/**
 * A zero-cost (well, very, very, low cost) borrow of an OwnedFd.
 *
 * This cannot be constructed directly, and can only exist as `&FdRef`.
 *
 * As a result, it might be slightly larger than a bare RawFd.
 */
pub struct FdRef {
    #[doc(hidden)]
    __nothing: ()
}

impl FdRef {
    /**
     * Construct a FdRef reference from a RawFd. No ownership is taken.
     *
     * unsafety:
     *
     *  - @i _must_ be a valid fd
     *  - the lifetime 'a must be appropriately bound
     */
    pub unsafe fn from_unowned_raw<'a>(i: RawFd) -> &'a FdRef{
        transmute(i as isize)
    }
}

impl AsRawFd for FdRef {
    fn as_raw_fd(&self) -> RawFd {
        let i : isize = unsafe { transmute(self) };
        i as RawFd
    }
}

impl ToOwned for FdRef {
    type Owned = OwnedFd;
    fn to_owned(&self) -> Self::Owned {
        unsafe { OwnedFd::from_unowned_raw(self.as_raw_fd()).unwrap() }
    }
}

#[cfg(test)]
mod tests {
    extern crate tempfile;
    use super::{OwnedFd,FdRef};
    use std::borrow::Borrow;
    use std::os::unix::io::{AsRawFd};

    #[test]
    fn it_works() {
        let t = tempfile::tempfile().unwrap();
        let fd = OwnedFd::from(t);

        let r : &FdRef = fd.borrow();

        assert!(r.to_owned().as_raw_fd() != fd.as_raw_fd());
    }
}
