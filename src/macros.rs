#[macro_export]
macro_rules! guard_ok {
    ($expr:expr, $ok_var:ident, $err_var:ident, $else_block:block) => {
        #[allow(unused_variables)]
        let $ok_var = match $expr {
            Ok(v) => v,
            Err($err_var) => $else_block,
        };
    };
}
