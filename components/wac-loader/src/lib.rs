use indexmap::IndexMap;
use wac_graph::{
    CompositionGraph, EncodeOptions,
    types::{BorrowedPackageKey, Package, Types},
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

        let socket = package("socket", socket, graph.types_mut())?;
        let socket = graph.register_package(socket)?;

        let mut graph_plugs = Vec::new();
        for (i, plug) in plugs.into_iter().enumerate() {
            let name = format!("plug:{i}");
            let plug = package(&name, plug, graph.types_mut())?;
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

                // the resolution only parses the packages the script uses, each dependency is
                // parsed up front so a malformed one is reported even when it is unused
                let mut types = Types::default();
                let (names, components): (Vec<String>, Vec<Wasm>) = deps.into_iter().unzip();
                let mut dependencies = IndexMap::new();
                for (pkg, wasm) in names.iter().zip(components) {
                    let key = BorrowedPackageKey::from_name_and_version(pkg, None);
                    let package = package(pkg, wasm, &mut types)?;
                    dependencies.insert(key, package.bytes().to_vec());
                }
                let resolution = document.resolve(dependencies)?;
                let component = resolution.encode(EncodeOptions::default())?;

                Ok(component)
            }
        }
    }
}

/// The header of a binary-encoded component, the wasm magic number followed by the component
/// version and layer.
const COMPONENT_HEADER: [u8; 8] = *b"\0asm\x0d\x00\x01\x00";

/// Parses and validates the wasm as a package, otherwise a `not-component` or `malformed` error
/// naming it.
fn package(name: &str, wasm: Wasm, types: &mut Types) -> Result<Package, ErrorCode> {
    if !wasm.starts_with(&COMPONENT_HEADER) {
        return Err(ErrorCode::NotComponent(Some(name.to_string())));
    }
    Package::from_bytes(name, None, wasm, types).map_err(|error| {
        ErrorCode::Malformed(Malformed {
            name: Some(name.to_string()),
            message: format!("{error:#}"),
        })
    })
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
