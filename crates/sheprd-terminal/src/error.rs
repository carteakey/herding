pub type Result<T, E = Error> = std::result::Result<T, E>;

/// The crate's failures. Emulation itself cannot fail: any bytes are valid
/// input, and sizes arrive already checked as [`crate::Size`].
#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error(transparent)]
    Backend(#[from] sheprd_backend::Error),
    #[error(
        "the search query is longer than {} bytes",
        sheprd_backend::MAX_SEARCH_QUERY_BYTES
    )]
    QueryTooLarge,
    #[error("the search query cannot be searched for")]
    Search(#[source] Box<alacritty_terminal::term::search::BuildError>),
    // keep-sorted end
}
