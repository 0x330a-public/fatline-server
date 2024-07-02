use thiserror::Error;

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("There was a problem with the database")]
    DbError
}
