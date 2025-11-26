pub trait PaginateOptions {
    fn from_zero(size: u64) -> Self;
    fn next_page(page: u64, size: u64) -> Self;
}

pub trait PaginateResponse<T> {
    fn total(&self) -> u64;
    fn items(self) -> Vec<T>;
}
