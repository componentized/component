use crate::{
    componentized::{
        component::types::{ErrorCode, Malformed, Wasm},
        oci::client::{self as oci, Digest, Reference},
        oci::media_types,
    },
    exports::componentized::component::path_loader::Guest,
};

pub(crate) struct OCILoader;

impl Guest for OCILoader {
    #[allow(async_fn_in_trait)]
    async fn load(path: String) -> Result<Wasm, ErrorCode> {
        load_reference(&path)
            .await
            .map_err(|error| name_not_found(error, &path))
    }
}

async fn load_reference(path: &str) -> Result<Wasm, ErrorCode> {
    let manifest_reference = oci::parse_reference(path)?;
    let manifest = match oci::get_manifest(manifest_reference.clone()).await? {
        oci::Manifest::OciImageV1(oci_image_manifest_v1) => oci_image_manifest_v1,
        _ => Err(ErrorCode::Other(Some("unexpected manifest".to_string())))?,
    };
    if manifest.config.media_type != media_types::application_vnd_wasm_config_v0_json() {
        Err(ErrorCode::Other(Some(
            "unexpected config media type".to_string(),
        )))?
    }
    if manifest.layers.len() != 1 {
        Err(ErrorCode::Other(Some(
            "unknown artifact layout".to_string(),
        )))?
    }
    let component_descriptor = manifest.layers.first().unwrap();
    if component_descriptor.media_type != media_types::application_wasm() {
        Err(ErrorCode::Other(Some(
            "unknown artifact media type".to_string(),
        )))?
    }

    let config_reference = manifest_reference.with_digest(&manifest.config.digest);
    let config =
        match oci::get_config(config_reference, Some(manifest.config.media_type.clone())).await? {
            oci::Config::WasmV0(wasm_config_v0) => wasm_config_v0,
            _ => Err(ErrorCode::Other(Some("unexpected config".to_string())))?,
        };
    if config.component.is_none() {
        Err(ErrorCode::NotComponent(None))?
    }

    let component_reference =
        manifest_reference.with_digest(&manifest.layers.first().unwrap().digest);
    let (component, verified) = oci::get_blob(component_reference).await?;
    let component = component.collect().await;
    verified.await?;
    if component_descriptor.size != component.len() as u64 {
        Err(ErrorCode::Malformed(Malformed {
            name: None,
            message: format!(
                "component size mismatch: expected {expected}, actual {actual}",
                expected = component_descriptor.size,
                actual = component.len()
            ),
        }))?
    }

    Ok(component)
}

/// Names the loaded path in a `not-found` error that does not name what was not found.
fn name_not_found(error: ErrorCode, path: &str) -> ErrorCode {
    match error {
        ErrorCode::NotFound(None) => ErrorCode::NotFound(Some(path.to_string())),
        error => error,
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

impl From<oci::ErrorCode> for ErrorCode {
    fn from(value: oci::ErrorCode) -> Self {
        match value {
            oci::ErrorCode::BlobUnknown(_) => Self::NotFound(None),
            oci::ErrorCode::BlobUploadInvalid(message) => {
                Self::Other(Some(format!("OCI error blob-upload-invalid: {message}")))
            }
            oci::ErrorCode::BlobUploadUnknown(message) => {
                Self::Other(Some(format!("OCI error blob-upload-unknown: {message}")))
            }
            oci::ErrorCode::DigestInvalid(message) => {
                Self::Other(Some(format!("OCI error digest-invalid: {message}")))
            }
            oci::ErrorCode::ManifestBlobUnknown(_) => Self::NotFound(None),
            oci::ErrorCode::ManifestInvalid(message) => {
                Self::Other(Some(format!("OCI error manifest-invalid: {message}")))
            }
            oci::ErrorCode::ManifestUnknown(_) => Self::NotFound(None),
            oci::ErrorCode::NameInvalid(message) => {
                Self::Other(Some(format!("OCI error name-invalid: {message}")))
            }
            oci::ErrorCode::NameUnknown(_) => Self::NotFound(None),
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
