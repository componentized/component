use crate::{
    componentized::{
        component::types::{ErrorCode, Wasm},
        http::client::{self as http},
    },
    exports::componentized::component::path_loader::Guest,
};

pub(crate) struct HttpLoader;

impl Guest for HttpLoader {
    #[allow(async_fn_in_trait)]
    async fn load(path: String) -> Result<Wasm, ErrorCode> {
        let response = http::get(path.clone(), vec![], None).await?;
        match response.status {
            200..=299 => {}
            404 | 410 => Err(ErrorCode::NotFound(Some(path)))?,
            status => Err(ErrorCode::Other(Some(format!(
                "unexpected HTTP status {status} for {path}"
            ))))?,
        }
        let component = response.body.collect().await;
        Ok(component)
    }
}

impl From<http::ErrorCode> for ErrorCode {
    fn from(value: http::ErrorCode) -> Self {
        match value {
            http::ErrorCode::RedirectLimitExceeded((_, count)) => {
                Self::Other(Some(format!("too many redirects, stopped after {count}")))
            }
            http::ErrorCode::RedirectRequiresBody(_) => Self::Other(Some(
                "redirect requires resending the request body".to_string(),
            )),
            http::ErrorCode::Other(message) => Self::Other(message),
        }
    }
}

wit_bindgen::generate!({
    path: "../wit",
    world: "http-loader",
    merge_structurally_equal_types: true,
    generate_all
});

export!(HttpLoader);
