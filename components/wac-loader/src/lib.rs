use indexmap::IndexMap;
use wac_graph::{
    CompositionGraph, EncodeOptions,
    types::{BorrowedPackageKey, Package},
};
use wac_parser::Document;

use crate::{
    componentized::component::types::{ErrorCode, Malformed, Wasm},
    exports::componentized::component::wac_loader::{Dependency, Guest, Plan},
};

pub(crate) struct WacLoader;

impl Guest for WacLoader {
    #[allow(async_fn_in_trait)]
    async fn plug(socket: Wasm, plugs: Vec<Wasm>) -> Result<Wasm, ErrorCode> {
        let mut graph = CompositionGraph::new();

        let socket = Package::from_bytes(
            "socket",
            None,
            component("socket", socket)?,
            graph.types_mut(),
        )?;
        let socket = graph.register_package(socket)?;

        let mut graph_plugs = Vec::new();
        for (i, plug) in plugs.into_iter().enumerate() {
            let name = format!("plug:{i}");
            let plug = component(&name, plug)?;
            let plug = Package::from_bytes(&name, None, plug, graph.types_mut())?;
            let plug = graph.register_package(plug)?;
            graph_plugs.push(plug);
        }

        wac_graph::plug(&mut graph, graph_plugs, socket)?;
        let component = graph.encode(EncodeOptions::default())?;

        Ok(component)
    }

    #[allow(async_fn_in_trait)]
    async fn compose(plan: Plan, deps: Vec<Dependency>) -> Result<Wasm, ErrorCode> {
        match plan {
            Plan::Wac(script) => {
                let document = Document::parse(&script)?;

                let (names, components): (Vec<String>, Vec<Wasm>) = deps.into_iter().unzip();
                let mut dependencies = IndexMap::new();
                for (pkg, wasm) in names.iter().zip(components) {
                    let key = BorrowedPackageKey::from_name_and_version(pkg, None);
                    dependencies.insert(key, component(pkg, wasm)?);
                }
                let resolution = document.resolve(dependencies)?;
                let component = resolution.encode(EncodeOptions::default())?;

                Ok(component)
            }
        }
    }
}

/// The wasm when it is a valid component, otherwise a `not-component` or `malformed` error naming
/// it.
fn component(name: &str, wasm: Wasm) -> Result<Wasm, ErrorCode> {
    if !wasmparser::Parser::is_component(&wasm) {
        return Err(ErrorCode::NotComponent(Some(name.to_string())));
    }
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
        .validate_all(&wasm)
        .map_err(|error| {
            ErrorCode::Malformed(Malformed {
                name: Some(name.to_string()),
                message: error.to_string(),
            })
        })?;
    Ok(wasm)
}

impl From<anyhow::Error> for ErrorCode {
    fn from(err: anyhow::Error) -> Self {
        Self::Other(Some(err.to_string()))
    }
}

impl From<wac_graph::EncodeError> for ErrorCode {
    fn from(err: wac_graph::EncodeError) -> Self {
        Self::Other(Some(err.to_string()))
    }
}

impl From<wac_graph::PlugError> for ErrorCode {
    fn from(err: wac_graph::PlugError) -> Self {
        Self::Other(Some(err.to_string()))
    }
}

impl From<wac_graph::RegisterPackageError> for ErrorCode {
    fn from(err: wac_graph::RegisterPackageError) -> Self {
        Self::Other(Some(err.to_string()))
    }
}

impl From<wac_parser::Error> for ErrorCode {
    fn from(err: wac_parser::Error) -> Self {
        Self::Malformed(Malformed {
            name: None,
            message: format!("malformed wac script: {err}"),
        })
    }
}

impl From<wac_parser::resolution::Error> for ErrorCode {
    fn from(err: wac_parser::resolution::Error) -> Self {
        match err {
            wac_parser::resolution::Error::UnknownPackage { name, .. } => {
                Self::NotFound(Some(name.to_string()))
            }
            err => Self::Other(Some(err.to_string())),
        }
    }
}

wit_bindgen::generate!({
    path: "../wit",
    world: "wac-loader",
    merge_structurally_equal_types: true,
    generate_all
});

export!(WacLoader);

#[cfg(test)]
mod tests;
