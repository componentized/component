#![no_main]

use crate::{
    componentized::oci::client::{self as oci, Digest, Reference},
    exports::componentized::component::{
        path_loader::Guest,
        types::{Component, Error},
    },
};

pub(crate) struct OCILoader;

impl Guest for OCILoader {
    #[allow(async_fn_in_trait)]
    async fn load(path: String) -> Result<Component, Error> {
        let manifest_reference = oci::parse_reference(&path)?;
        let manifest = match oci::get_manifest(manifest_reference.clone()).await? {
            oci::Manifest::OciImageV1(oci_image_manifest_v1) => oci_image_manifest_v1,
            _ => Err(Error::Other(Some("unexpected manifest".to_string())))?,
        };
        match manifest.config.media_type {
            oci::MediaType::ApplicationVndWasmConfigV0(oci::MediaTypeSuffix::Json) => {}
            _ => Err(Error::Other(Some(
                "unexpected config media type".to_string(),
            )))?,
        };
        if manifest.layers.len() != 1 {
            Err(Error::Other(Some("unknown artifact layout".to_string())))?
        }
        let component_descriptor = manifest.layers.first().unwrap();
        match component_descriptor.media_type {
            oci::MediaType::ApplicationWasm => {}
            _ => Err(Error::Other(Some(
                "unknown artifact media type".to_string(),
            )))?,
        }

        let config_reference = manifest_reference.with_digest(&manifest.config.digest);
        let config =
            match oci::get_config(config_reference, Some(manifest.config.media_type.clone()))
                .await?
            {
                oci::Config::WasmV0(wasm_config_v0) => wasm_config_v0,
                _ => Err(Error::Other(Some("unexpected config".to_string())))?,
            };
        if config.component.is_none() {
            Err(Error::Other(Some("not a component".to_string())))?
        }

        let component_reference =
            manifest_reference.with_digest(&manifest.layers.first().unwrap().digest);
        let component = oci::get_blob(component_reference).await?;
        if component_descriptor.size != component.len() as u64 {
            Err(Error::Other(Some(format!(
                "component size mismatch: expected {expected}, actual {actual}",
                expected = component_descriptor.size,
                actual = component.len()
            ))))?
        }

        Ok(component)
    }
}

impl Reference {
    fn with_digest(&self, digest: &Digest) -> Self {
        Self {
            registry: self.registry.clone(),
            repository: self.repository.clone(),
            tag: None,
            digest: Some(digest.clone()),
        }
    }
}

impl From<oci::ErrorCode> for Error {
    fn from(value: oci::ErrorCode) -> Self {
        match value {
            oci::ErrorCode::BlobUnknown(message) => {
                Self::Other(Some(format!("OCI error blob-unknown: {message}")))
            }
            oci::ErrorCode::BlobUploadInvalid(message) => {
                Self::Other(Some(format!("OCI error blob-upload-invalid: {message}")))
            }
            oci::ErrorCode::BlobUploadUnknown(message) => {
                Self::Other(Some(format!("OCI error blob-upload-unknown: {message}")))
            }
            oci::ErrorCode::DigestInvalid(message) => {
                Self::Other(Some(format!("OCI error digest-invalid: {message}")))
            }
            oci::ErrorCode::ManifestBlobUnknown(message) => {
                Self::Other(Some(format!("OCI error manifest-blob-unknown: {message}")))
            }
            oci::ErrorCode::ManifestInvalid(message) => {
                Self::Other(Some(format!("OCI error manifest-invalid: {message}")))
            }
            oci::ErrorCode::ManifestUnknown(message) => {
                Self::Other(Some(format!("OCI error manifest-unknown: {message}")))
            }
            oci::ErrorCode::NameInvalid(message) => {
                Self::Other(Some(format!("OCI error name-invalid: {message}")))
            }
            oci::ErrorCode::NameUnknown(message) => {
                Self::Other(Some(format!("OCI error name-unknown: {message}")))
            }
            oci::ErrorCode::SizeInvalid(message) => {
                Self::Other(Some(format!("OCI error size-invalid: {message}")))
            }
            oci::ErrorCode::Unauthorized(message) => {
                Self::Other(Some(format!("OCI error unauthorized: {message}")))
            }
            oci::ErrorCode::Denied(message) => {
                Self::Other(Some(format!("OCI error denied: {message}")))
            }
            oci::ErrorCode::Unsupported(message) => {
                Self::Other(Some(format!("OCI error unsupported: {message}")))
            }
            oci::ErrorCode::Toomanyrequests(_) => {
                // TODO detect and retry request if within a grace period
                Self::Other(Some(format!("OCI error toomanyrequests")))
            }
            oci::ErrorCode::Other(Some(message)) => {
                Self::Other(Some(format!("OCI error other: {message}")))
            }
            oci::ErrorCode::Other(None) => Self::Other(Some(format!("OCI error other"))),
        }
    }
}

wit_bindgen::generate!({
    path: "../wit",
    world: "oci-loader",
    merge_structurally_equal_types: true,
    generate_all
});

export!(OCILoader);
