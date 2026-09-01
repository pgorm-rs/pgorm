#[macro_export]
macro_rules! block_on {
    ($($expr:tt)*) => {
        ::tokio::runtime::Runtime::new()
            .unwrap()
            .block_on( $($expr)* )
    };
}
