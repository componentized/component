use crate::{
    componentized::http::client::{self as http},
    exports::componentized::component::path_loader::Guest,
    exports::componentized::component::types::{Component, Error},
};

pub(crate) struct HttpLoader;

impl Guest for HttpLoader {
    #[allow(async_fn_in_trait)]
    async fn load(path: String) -> Result<Component, Error> {
        let response = http::get(path, vec![], None).await?;
        let component = response.body.collect().await;
        Ok(component)
    }
}

impl From<http::ErrorCode> for Error {
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
