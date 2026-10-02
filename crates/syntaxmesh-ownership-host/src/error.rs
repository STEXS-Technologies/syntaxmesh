#[derive(Debug)]
pub enum OwnershipError {
    Io(std::io::Error),
    InvalidTarget(&'static str),
    AlreadyOwned,
}

impl std::fmt::Display for OwnershipError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
            Self::InvalidTarget(message) => formatter.write_str(message),
            Self::AlreadyOwned => {
                formatter.write_str("index target is already owned by another cooperating writer")
            }
        }
    }
}

impl std::error::Error for OwnershipError {}

impl From<std::io::Error> for OwnershipError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
