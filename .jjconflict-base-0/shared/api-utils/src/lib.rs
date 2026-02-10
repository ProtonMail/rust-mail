pub trait PaginateOptions {
    fn from_zero(size: u64) -> Self;
    fn with_page(self, page: u64) -> Self;
    fn size(&self) -> u64;
}

pub trait PaginateResponse<T> {
    fn total(&self) -> u64;
    fn items(self) -> Vec<T>;
}
