#![no_main]

use std::{fs, io};

use crate::{
    exports::componentized::component::path_loader::Guest,
    exports::componentized::component::types::{Component, Error},
};

pub(crate) struct FilesystemLoader;

impl Guest for FilesystemLoader {
    #[allow(async_fn_in_trait)]
    async fn load(path: String) -> Result<Component, Error> {
        let component = fs::read(path)?;
        Ok(component)
    }
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Other(Some(value.to_string()))
    }
}

wit_bindgen::generate!({
    path: "../wit",
    world: "filesystem-loader",
    merge_structurally_equal_types: true,
    generate_all
});

export!(FilesystemLoader);
