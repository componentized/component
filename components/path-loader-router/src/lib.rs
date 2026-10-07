use crate::{
    componentized::component::types::{ErrorCode, Malformed, Wasm},
    exports::componentized::component::path_loader::Guest,
};

pub(crate) struct PathLoader;

impl Guest for PathLoader {
    #[allow(async_fn_in_trait)]
    async fn load(path: String) -> Result<Wasm, ErrorCode> {
        if path.starts_with("http:") {
            http_loader::load(path).await
        } else if path.starts_with("https:") {
            http_loader::load(path).await
        } else if path.starts_with("file:///") {
            filesystem_loader::load(path[7..].to_string()).await
        } else if path.starts_with("file://") {
            Err(ErrorCode::Malformed(Malformed {
                name: None,
                message: format!("invalid filesystem-path loader: {path}"),
            }))
        } else if path.starts_with("file:") {
            filesystem_loader::load(path[5..].to_string()).await
        } else if path.starts_with("oci://") {
            oci_loader::load(path[6..].to_string()).await
        } else {
            Err(ErrorCode::Malformed(Malformed {
                name: None,
                message: format!("unknown path loader: {path}"),
            }))
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
