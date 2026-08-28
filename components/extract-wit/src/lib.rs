#![no_main]

use std::collections::BTreeMap;

use crate::{
    componentized::component::types::{Component, Error},
    exports::componentized::component::wit::{
        Docs, Enum, EnumCase, Flag, Flags, Function, FunctionKind, Guest, GuestWit, Handle,
        IncludeName, Interface, InterfaceId, List, Map, Package, PackageId, PackageName, Param,
        Record, RecordField, Result as Result_, Stability, Stable, Tuple, Type, TypeDef,
        TypeDefKind, TypeId, TypeOwner, Unstable, Variant, VariantCase, Version, VersionIdentifier,
        Wit, World, WorldId, WorldInclude, WorldItem, WorldItemInterface, WorldKey,
    },
};

pub(crate) struct ExtractWit;

impl Guest for ExtractWit {
    type Wit = ExtractedWit;

    #[allow(async_fn_in_trait)]
    async fn extract(component: &Component) -> Result<(Wit, Package), Error> {
        let wasm = component.into_wasm();
        let decoded = wit_component::decode(&wasm)?;

        let wit = ExtractedWit::new(decoded.resolve());
        let package = wit
            .package(ExtractedWit::package_id(decoded.package()))
            .expect("decoded package must exist");

        Ok((Wit::new(wit), package))
    }
}

pub(crate) struct ExtractedWit {
    worlds: BTreeMap<u32, World>,
    interfaces: BTreeMap<u32, Interface>,
    types: BTreeMap<u32, TypeDef>,
    packages: BTreeMap<u32, Package>,
}

impl ExtractedWit {
    fn new(resolve: &wit_parser::Resolve) -> Self {
        Self {
            worlds: resolve.worlds.clone().into_iter().fold(
                BTreeMap::new(),
                |mut worlds, (id, world)| {
                    worlds.insert(Self::world_id(id).world_id, Self::world(world));
                    worlds
                },
            ),
            interfaces: resolve.interfaces.clone().into_iter().fold(
                BTreeMap::new(),
                |mut interfaces, (id, interface)| {
                    interfaces.insert(
                        Self::interface_id(id).interface_id,
                        Self::interface(interface),
                    );
                    interfaces
                },
            ),
            types: resolve.types.clone().into_iter().fold(
                BTreeMap::new(),
                |mut types, (id, type_def)| {
                    types.insert(Self::type_id(id).type_id, Self::type_def(type_def));
                    types
                },
            ),
            packages: resolve.packages.clone().into_iter().fold(
                BTreeMap::new(),
                |mut packages, (id, package)| {
                    packages.insert(Self::package_id(id).package_id, Self::package(package));
                    packages
                },
            ),
        }
    }

    fn world_id(id: wit_parser::WorldId) -> WorldId {
        WorldId {
            world_id: u32::try_from(id.index()).expect("id too large"),
        }
    }

    fn world(world: wit_parser::World) -> World {
        World {
            name: world.name,
            imports: world
                .imports
                .into_iter()
                .map(|import| Self::world_entry(import))
                .collect(),
            exports: world
                .exports
                .into_iter()
                .map(|export| Self::world_entry(export))
                .collect(),
            package: world.package.map(Self::package_id),
            docs: Self::docs(world.docs),
            stability: Self::stability(world.stability),
            includes: world
                .includes
                .into_iter()
                .map(|include| WorldInclude {
                    stability: Self::stability(include.stability),
                    id: Self::world_id(include.id),
                    names: include
                        .names
                        .into_iter()
                        .map(|name| IncludeName {
                            name: name.name,
                            as_: name.as_,
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    fn world_entry(
        (key, item): (wit_parser::WorldKey, wit_parser::WorldItem),
    ) -> (WorldKey, WorldItem) {
        (
            match key {
                wit_parser::WorldKey::Name(name) => WorldKey::Name(name),
                wit_parser::WorldKey::Interface(id) => WorldKey::Interface(Self::interface_id(id)),
            },
            match item {
                wit_parser::WorldItem::Interface {
                    id,
                    stability,
                    external_id,
                    docs,
                    ..
                } => WorldItem::Interface(WorldItemInterface {
                    id: Self::interface_id(id),
                    stability: Self::stability(stability),
                    external_id: external_id,
                    docs: Self::docs(docs),
                }),
                wit_parser::WorldItem::Function(function) => {
                    WorldItem::Function(Self::function(function))
                }
                wit_parser::WorldItem::Type { id, .. } => WorldItem::Type(Self::type_id(id)),
            },
        )
    }

    fn interface_id(id: wit_parser::InterfaceId) -> InterfaceId {
        InterfaceId {
            interface_id: u32::try_from(id.index()).expect("id too large"),
        }
    }

    fn interface(interface: wit_parser::Interface) -> Interface {
        Interface {
            name: interface.name,
            types: interface
                .types
                .into_iter()
                .map(|(name, id)| (name, Self::type_id(id)))
                .collect(),
            functions: interface
                .functions
                .into_iter()
                .map(|(name, function)| (name, Self::function(function)))
                .collect(),
            docs: Self::docs(interface.docs),
            stability: Self::stability(interface.stability),
            package: interface.package.map(Self::package_id),
        }
    }

    fn function(function: wit_parser::Function) -> Function {
        Function {
            name: function.name,
            kind: match function.kind {
                wit_parser::FunctionKind::Freestanding => FunctionKind::Freestanding,
                wit_parser::FunctionKind::AsyncFreestanding => FunctionKind::AsyncFreestanding,
                wit_parser::FunctionKind::Method(id) => FunctionKind::Method(Self::type_id(id)),
                wit_parser::FunctionKind::AsyncMethod(id) => {
                    FunctionKind::AsyncMethod(Self::type_id(id))
                }
                wit_parser::FunctionKind::Static(id) => FunctionKind::Static(Self::type_id(id)),
                wit_parser::FunctionKind::AsyncStatic(id) => {
                    FunctionKind::AsyncStatic(Self::type_id(id))
                }
                wit_parser::FunctionKind::Constructor(id) => {
                    FunctionKind::Constructor(Self::type_id(id))
                }
            },
            params: function
                .params
                .into_iter()
                .map(|param| Param {
                    name: param.name,
                    type_: Self::type_(param.ty),
                })
                .collect(),
            result: function.result.map(Self::type_),
            docs: Self::docs(function.docs),
            stability: Self::stability(function.stability),
            external_id: function.external_id,
        }
    }

    fn type_id(id: wit_parser::TypeId) -> TypeId {
        TypeId {
            type_id: u32::try_from(id.index()).expect("id too large"),
        }
    }

    fn type_(type_: wit_parser::Type) -> Type {
        match type_ {
            wit_parser::Type::Bool => Type::Bool,
            wit_parser::Type::U8 => Type::U8,
            wit_parser::Type::U16 => Type::U16,
            wit_parser::Type::U32 => Type::U32,
            wit_parser::Type::U64 => Type::U64,
            wit_parser::Type::S8 => Type::S8,
            wit_parser::Type::S16 => Type::S16,
            wit_parser::Type::S32 => Type::S32,
            wit_parser::Type::S64 => Type::S64,
            wit_parser::Type::F32 => Type::F32,
            wit_parser::Type::F64 => Type::F64,
            wit_parser::Type::Char => Type::Char,
            wit_parser::Type::String => Type::String,
            wit_parser::Type::ErrorContext => Type::ErrorContext,
            wit_parser::Type::Id(id) => Type::Id(Self::type_id(id)),
        }
    }

    fn type_def(type_def: wit_parser::TypeDef) -> TypeDef {
        TypeDef {
            name: type_def.name,
            kind: Self::type_def_kind(type_def.kind),
            owner: match type_def.owner {
                wit_parser::TypeOwner::World(id) => TypeOwner::World(Self::world_id(id)),
                wit_parser::TypeOwner::Interface(id) => {
                    TypeOwner::Interface(Self::interface_id(id))
                }
                wit_parser::TypeOwner::None => TypeOwner::None,
            },
            docs: Self::docs(type_def.docs),
            stability: Self::stability(type_def.stability),
            external_id: type_def.external_id,
        }
    }

    fn type_def_kind(kind: wit_parser::TypeDefKind) -> TypeDefKind {
        match kind {
            wit_parser::TypeDefKind::Record(record) => TypeDefKind::Record(Self::record(record)),
            wit_parser::TypeDefKind::Resource => TypeDefKind::Resource,
            wit_parser::TypeDefKind::Handle(handle) => TypeDefKind::Handle(Self::handle(handle)),
            wit_parser::TypeDefKind::Flags(flags) => TypeDefKind::Flags(Self::flags(flags)),
            wit_parser::TypeDefKind::Tuple(tuple) => TypeDefKind::Tuple(Self::tuple(tuple)),
            wit_parser::TypeDefKind::Variant(variant) => {
                TypeDefKind::Variant(Self::variant(variant))
            }
            wit_parser::TypeDefKind::Enum(enum_) => TypeDefKind::Enum(Self::enum_(enum_)),
            wit_parser::TypeDefKind::Option(option) => TypeDefKind::Option(Self::type_(option)),
            wit_parser::TypeDefKind::Result(result) => TypeDefKind::Result(Self::result(result)),
            wit_parser::TypeDefKind::List(list) => TypeDefKind::List(Self::list(list)),
            wit_parser::TypeDefKind::FixedLengthList(type_, length) => {
                TypeDefKind::List(Self::list_fixed_length(type_, length))
            }
            wit_parser::TypeDefKind::Map(key, value) => TypeDefKind::Map(Self::map(key, value)),

            wit_parser::TypeDefKind::Future(future) => TypeDefKind::Future(future.map(Self::type_)),
            wit_parser::TypeDefKind::Stream(stream) => TypeDefKind::Stream(stream.map(Self::type_)),
            wit_parser::TypeDefKind::Type(type_) => TypeDefKind::Type(Self::type_(type_)),
            wit_parser::TypeDefKind::Unknown => TypeDefKind::Unknown,
        }
    }

    fn record(record: wit_parser::Record) -> Record {
        Record {
            fields: record
                .fields
                .into_iter()
                .map(|field| RecordField {
                    name: field.name,
                    type_: Self::type_(field.ty),
                    docs: Self::docs(field.docs),
                })
                .collect(),
        }
    }

    fn handle(handle: wit_parser::Handle) -> Handle {
        match handle {
            wit_parser::Handle::Own(id) => Handle::Own(Self::type_id(id)),
            wit_parser::Handle::Borrow(id) => Handle::Borrow(Self::type_id(id)),
        }
    }

    fn flags(flags: wit_parser::Flags) -> Flags {
        Flags {
            flags: flags
                .flags
                .into_iter()
                .map(|flag| Flag {
                    name: flag.name,
                    docs: Self::docs(flag.docs),
                })
                .collect(),
        }
    }

    fn tuple(tuple: wit_parser::Tuple) -> Tuple {
        Tuple {
            types: tuple.types.into_iter().map(Self::type_).collect(),
        }
    }

    fn variant(variant: wit_parser::Variant) -> Variant {
        Variant {
            cases: variant
                .cases
                .into_iter()
                .map(|case| VariantCase {
                    name: case.name,
                    type_: case.ty.map(Self::type_),
                    docs: Self::docs(case.docs),
                })
                .collect(),
        }
    }

    fn enum_(enum_: wit_parser::Enum) -> Enum {
        Enum {
            cases: enum_
                .cases
                .into_iter()
                .map(|case| EnumCase {
                    name: case.name,
                    docs: Self::docs(case.docs),
                })
                .collect(),
        }
    }

    fn result(result: wit_parser::Result_) -> Result_ {
        Result_ {
            ok: result.ok.map(Self::type_),
            err: result.err.map(Self::type_),
        }
    }

    fn list(type_: wit_parser::Type) -> List {
        List {
            type_: Self::type_(type_),
            fixed_length: None,
        }
    }

    fn list_fixed_length(type_: wit_parser::Type, fixed_length: u32) -> List {
        List {
            type_: Self::type_(type_),
            fixed_length: Some(fixed_length),
        }
    }

    fn map(key: wit_parser::Type, value: wit_parser::Type) -> Map {
        Map {
            key: Self::type_(key),
            value: Self::type_(value),
        }
    }

    fn package_id(id: wit_parser::PackageId) -> PackageId {
        PackageId {
            package_id: u32::try_from(id.index()).expect("id too large"),
        }
    }

    fn package(package: wit_parser::Package) -> Package {
        Package {
            name: PackageName {
                namespace: package.name.namespace,
                name: package.name.name,
                version: package.name.version.map(Self::version),
            },
            docs: Self::docs(package.docs),
            interfaces: package
                .interfaces
                .into_iter()
                .map(|(name, id)| (name, Self::interface_id(id)))
                .collect(),
            worlds: package
                .worlds
                .into_iter()
                .map(|(name, id)| (name, Self::world_id(id)))
                .collect(),
        }
    }

    fn docs(docs: wit_parser::Docs) -> Docs {
        Docs {
            contents: docs.contents,
        }
    }

    fn stability(stability: wit_parser::Stability) -> Stability {
        match stability {
            wit_parser::Stability::Unknown => Stability::Unknown,
            wit_parser::Stability::Unstable {
                feature,
                deprecated,
            } => Stability::Unstable(Unstable {
                feature,
                deprecated: deprecated.map(Self::version),
            }),
            wit_parser::Stability::Stable { since, deprecated } => Stability::Stable(Stable {
                deprecated: deprecated.map(Self::version),
                since: Self::version(since),
            }),
        }
    }

    fn version(version: semver::Version) -> Version {
        Version {
            major: version.major,
            minor: version.minor,
            patch: version.patch,
            prerelease: Self::version_identifiers(version.pre.as_str()),
            build_metadata: Self::version_identifiers(version.build.as_str()),
        }
    }

    fn version_identifiers(identifiers: &str) -> Option<Vec<VersionIdentifier>> {
        match identifiers.is_empty() {
            true => None,
            false => Some(
                identifiers
                    .split('.')
                    .map(|identifier| match identifier.parse::<u64>() {
                        Ok(val) => VersionIdentifier::Numeric(val),
                        Err(_) => VersionIdentifier::String(identifier.into()),
                    })
                    .collect(),
            ),
        }
    }
}

impl GuestWit for ExtractedWit {
    #[allow(async_fn_in_trait)]
    fn world(&self, id: WorldId) -> Option<World> {
        self.worlds.get(&id.world_id).cloned()
    }

    #[allow(async_fn_in_trait)]
    fn interface(&self, id: InterfaceId) -> Option<Interface> {
        self.interfaces.get(&id.interface_id).cloned()
    }

    #[allow(async_fn_in_trait)]
    fn type_(&self, id: TypeId) -> Option<TypeDef> {
        self.types.get(&id.type_id).cloned()
    }

    #[allow(async_fn_in_trait)]
    fn package(&self, id: PackageId) -> Option<Package> {
        self.packages.get(&id.package_id).cloned()
    }
}

impl From<anyhow::Error> for Error {
    fn from(value: anyhow::Error) -> Self {
        Self::Other(Some(value.to_string()))
    }
}

wit_bindgen::generate!({
    path: "../wit",
    world: "extract-wit",
    merge_structurally_equal_types: true,
    generate_all
});

export!(ExtractWit);
