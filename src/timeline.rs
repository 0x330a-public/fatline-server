use fatline_rs::posts::Cast;

pub(crate) struct TimelinePage {
    casts: Vec<Cast>,
    next_page: Option<Vec<u8>>,
}
