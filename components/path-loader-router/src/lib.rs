#![no_main]

use crate::exports::componentized::component::{
    path_loader::Guest,
    types::{Component, Error},
};

pub(crate) struct PathLoader;

impl Guest for PathLoader {
    #[allow(async_fn_in_trait)]
    async fn load(path: String) -> Result<Component, Error> {
        if path.starts_with("http:") {
            http_loader::load(path).await
        } else if path.starts_with("https:") {
            http_loader::load(path).await
        } else if path.starts_with("file:///") {
            filesystem_loader::load(path[7..].to_string()).await
        } else if path.starts_with("file://") {
            Err(Error::Other(Some(format!(
                "invalid filesystem-path loader: {path}"
            ))))
        } else if path.starts_with("file:") {
            filesystem_loader::load(path[5..].to_string()).await
        } else if path.starts_with("oci:") {
            oci_loader::load(path[4..].to_string()).await
        } else {
            Err(Error::Other(Some(format!("unknown path loader: {path}"))))
        }
    }
}

wit_bindgen::generate!({
    path: "../wit",
    world: "path-loader",
    merge_structurally_equal_types: true,
    generate_all
});

export!(PathLoader);
