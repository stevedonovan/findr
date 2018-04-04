// minimal error handling...
use std::error::Error;
use std::io;

// the one constructable error in stdlib
pub fn io_error(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::Other, msg)
}

pub type BoxResult<T> = Result<T,Box<Error>>;

pub fn err_io<T>(msg: &str) -> BoxResult<T> {
    Err(io_error(msg).into())
}
