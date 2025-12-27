use std::fmt;

#[derive(Debug)]
pub enum ValidationError {
    InvalidTaskId,
    _ExpiredTask(u64),
    InvalidExpires,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationError::InvalidTaskId => write!(f, "Invalid task ID"),
            ValidationError::_ExpiredTask(expiration_time) => {
                write!(f, "Task expired at {}", expiration_time)
            }
            ValidationError::InvalidExpires => write!(f, "Invalid expiration time"),
        }
    }
}

impl std::error::Error for ValidationError {}
