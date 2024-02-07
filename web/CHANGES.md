# unreleased 0.3.0
## Add
- enable Rust nightly feature `error_generic_member_access` when `xitca-web`'s `nightly` feature is enabled. this enables runtime context type interaction like `std::backtrace::BackTrace` for enhanced error handling.

## Remove
- remove `xitca_web::error::{BadRequest, Internal}` types. `xitca_web::error::ErrorStatus` replace their roles where `ErrorStatus::bad_request` and `ErrorStatus::internal` would generate identical error information as `BadRequest` and `Internal` types. this change would simplify runtime error type casting a bit with two less possible error types.

## Change
- change `xitca_web::middleware::eraser::TypeEraser::error`'s trait bound. `From` trait is used for conversion between generic error type and `xitca_web::error::Error`. With this change `Error` does not double boxing itself therefore removing the need of nested type casting when handling typed error.

# 0.2.2
## Add
- `StateRef` can used for extracting `?Sized` type from application state.

## Change
- update `xitca-http` to `0.2.1`.

# 0.2.1
## Add
- `RateLimit` middleware with optional feature `rate-limit`.
- implement `Responder` trait for `serde_json::Value`.
- re-export `http_ws::{ResponseSender, ResponseWeakSender}` types in `xitca_web::handler::websocket` module.

## Change
- `App::with_state` and `App::with_async_state` expect `Self`. Enables more flexible application state construction. Example:
    ```rust
    // delayed state attachment:
    App::new().at("/", ...).enclosed(...).with_state(996);

    // modular application configuration before attaching state:
    use xitca_web::NestApp;

    fn configure(app: NestApp<String>) -> NestApp<String> {
        app.at("/", ...)
    }

    let mut app = App::new();
    app = configure(app);
    app.with_state(String::from("996"));
    ```
- update `xitca-http` to version `0.2.0`.
- update `http-encoding` to version `0.2.0`.
- update `http-ws` to version `0.2.0`.

## Fix
- fix nested App routing. `App::new().at("/foo", App::new().at("/"))` would be successfully matching against `/foo/`
- fix bug where certain tower-http layers causing compile issue.
- fix bug where multiple tower-http layers can't be chained together with `ServiceExt::enclosed`.