//! The daemon's approval lifecycle: admission, generation-checked resolution and the
//! wait the blocking hook sits in, plus the shutdown flag the threads share.
//!
//! There is no notification transport here any more. The island is the only surface: it
//! is on screen, it expands on its own and it carries the buttons, so a second card in
//! the notification centre was an echo of something the user had already been shown.
//! Sounds are the away-channel and live in `crate::sound`.

pub mod approval;
pub mod lifecycle;
pub mod shutdown;
