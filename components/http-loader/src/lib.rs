#![no_main]

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

wit_bindgen::generate!({
    path: "../wit",
    world: "http-loader",
    merge_structurally_equal_types: true,
    generate_all
});

export!(HttpLoader);
