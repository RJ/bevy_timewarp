/// https://github.com/jaynus/reliable.io/blob/master/rust/src/error.rs
///

#[derive(Debug)]
pub enum TimewarpError {
    Io(std::io::Error),
    FrameTooOld,
}

impl std::fmt::Display for TimewarpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid first item to double")
    }
}

// This is important for other errors to wrap this one.
impl std::error::Error for TimewarpError {
    fn description(&self) -> &str {
        "invalid first item to double"
    }

    fn cause(&self) -> Option<&dyn std::error::Error> {
        None
    }
}

impl From<std::io::Error> for TimewarpError {
    fn from(err: std::io::Error) -> Self {
        TimewarpError::Io(err)
    }
}
