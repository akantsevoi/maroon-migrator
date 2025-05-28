#[macro_export]
macro_rules! guard_ok {
  ($expr:expr, $err_var:ident, $else_block:block) => {
    match $expr {
      Ok(v) => v,
      Err($err_var) => $else_block,
    }
  };
}
