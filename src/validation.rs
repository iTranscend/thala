use crate::{
    error::ValidationError,
    message::{TaskAnnouncement, TaskClaim, TaskResult},
};

const MIN_TASK_EXPIRATION_TIME: u64 = 60;

pub trait Validate {
    type Error;

    fn validate(&self) -> Result<(), Self::Error>;
}

impl Validate for TaskAnnouncement {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        if self.expires < MIN_TASK_EXPIRATION_TIME {
            Err(ValidationError::InvalidExpires)
        } else {
            Ok(())
        }
    }
}

impl Validate for TaskClaim {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        // TODO: validate claimer's capabilities
        todo!()
    }
}

impl Validate for TaskResult {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        // TODO: verify that result is from peer that claimed task.
        todo!()
    }
}
