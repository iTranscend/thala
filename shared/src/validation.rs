pub const MIN_TASK_EXPIRATION_TIME: u64 = 60;

pub trait Validate {
    type Error;

    fn validate(&self) -> Result<(), Self::Error>;
}
