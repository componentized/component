#![no_main]

use std::collections::BTreeMap;

use crate::{
    componentized::component::types::{Component, Error},
    exports::componentized::component::wit::{
        Case, Docs, Enum, EnumCase, Field, Flag, Flags, Function, FunctionKind, Guest, GuestWit,
        Handle, IncludeName, Interface, InterfaceId, Package, PackageId, PackageName, Param,
        Record, ResultWorkaround as Result_, Stability, Stable, Tuple, Type, TypeDef, TypeDefKind,
        TypeId, TypeOwner, Unstable, Variant, Version, VersionIdentifier, Wit, World, WorldId,
        WorldInclude, WorldItem, WorldItemInterface, WorldItemType, WorldKey,
    },
};

pub(crate) struct ExtractWit;

impl Guest for ExtractWit {
    type Wit = ExtractedWit;

    #[allow(async_fn_in_trait)]
    fn parse(_wit: String) -> Result<Wit, Error> {
        todo!()
    }

    #[allow(async_fn_in_trait)]
    async fn extract(component: &Component) -> Result<(Wit, PackageId), Error> {
        let wasm = component.into_wasm();
        let wit = wit_component::decode(&wasm)?;

        Ok((
            Wit::new(ExtractedWit::new(wit.resolve())),
            ExtractedWit::package_id(wit.package()),
        ))
    }
}

pub(crate) struct ExtractedWit {
    world_ids: Vec<WorldId>,
    worlds: BTreeMap<WorldId, World>,
    interface_ids: Vec<InterfaceId>,
    interfaces: BTreeMap<InterfaceId, Interface>,
    type_ids: Vec<TypeId>,
    types: BTreeMap<TypeId, TypeDef>,
    package_ids: Vec<PackageId>,
    packages: BTreeMap<PackageId, Package>,
}

impl ExtractedWit {
    fn new(resolve: &wit_parser::Resolve) -> Self {
        let worlds = Self::worlds(resolve);
        let interfaces = Self::interfaces(resolve);
        let types = Self::types(resolve);
        let packages = Self::packages(resolve);

        Self {
            world_ids: worlds.clone().into_keys().collect(),
            worlds,
            interface_ids: interfaces.clone().into_keys().collect(),
            interfaces,
            type_ids: types.clone().into_keys().collect(),
            types,
            package_ids: packages.clone().into_keys().collect(),
            packages,
        }
    }

    fn worlds(resolve: &wit_parser::Resolve) -> BTreeMap<WorldId, World> {
        resolve
            .worlds
            .clone()
            .into_iter()
            .fold(BTreeMap::new(), |mut worlds, (id, world)| {
                worlds.insert(Self::world_id(id), Self::world(world));
                worlds
            })
    }

    fn world_id(id: wit_parser::WorldId) -> WorldId {
        WorldId::from(format!("world:{}", id.index()))
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
            includes: Self::world_includes(world.includes),
        }
    }

    fn world_entry(entry: (wit_parser::WorldKey, wit_parser::WorldItem)) -> (WorldKey, WorldItem) {
        (Self::world_key(entry.0), Self::world_item(entry.1))
    }

    fn world_key(key: wit_parser::WorldKey) -> WorldKey {
        match key {
            wit_parser::WorldKey::Name(name) => WorldKey::Name(name),
            wit_parser::WorldKey::Interface(id) => WorldKey::Interface(Self::interface_id(id)),
        }
    }

    fn world_item(item: wit_parser::WorldItem) -> WorldItem {
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
            wit_parser::WorldItem::Function(function) => WorldItem::Function(Function {
                name: function.name,
                kind: Self::function_kind(function.kind),
                params: Self::params(function.params),
                result: function.result.map(Self::type_),
                docs: Self::docs(function.docs),
                stability: Self::stability(function.stability),
                external_id: function.external_id,
            }),
            wit_parser::WorldItem::Type { id, .. } => WorldItem::Type(WorldItemType {
                id: Self::type_id(id),
            }),
        }
    }

    fn world_includes(includes: Vec<wit_parser::WorldInclude>) -> Vec<WorldInclude> {
        includes.into_iter().map(Self::world_include).collect()
    }

    fn world_include(include: wit_parser::WorldInclude) -> WorldInclude {
        WorldInclude {
            stability: Self::stability(include.stability),
            id: Self::world_id(include.id),
            names: Self::include_names(include.names),
        }
    }

    fn include_names(names: Vec<wit_parser::IncludeName>) -> Vec<IncludeName> {
        names.into_iter().map(Self::include_name).collect()
    }

    fn include_name(name: wit_parser::IncludeName) -> IncludeName {
        IncludeName {
            name: name.name,
            as_: name.as_,
        }
    }

    fn interfaces(resolve: &wit_parser::Resolve) -> BTreeMap<InterfaceId, Interface> {
        resolve.interfaces.clone().into_iter().fold(
            BTreeMap::new(),
            |mut interfaces, (id, interface)| {
                interfaces.insert(Self::interface_id(id), Self::interface(interface));
                interfaces
            },
        )
    }

    fn interface_id(id: wit_parser::InterfaceId) -> InterfaceId {
        InterfaceId::from(format!("interface:{}", id.index()))
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
            kind: Self::function_kind(function.kind),
            params: Self::params(function.params),
            result: function.result.map(Self::type_),
            docs: Self::docs(function.docs),
            stability: Self::stability(function.stability),
            external_id: function.external_id,
        }
    }

    fn types(resolve: &wit_parser::Resolve) -> BTreeMap<TypeId, TypeDef> {
        resolve
            .types
            .clone()
            .into_iter()
            .fold(BTreeMap::new(), |mut types, (id, type_def)| {
                types.insert(Self::type_id(id), Self::type_def(type_def));
                types
            })
    }

    fn type_id(id: wit_parser::TypeId) -> TypeId {
        TypeId::from(format!("type:{}", id.index()))
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
            owner: Self::type_owner(type_def.owner),
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
            wit_parser::TypeDefKind::List(list) => TypeDefKind::List((Self::type_(list), None)),
            wit_parser::TypeDefKind::Map(key, value) => {
                TypeDefKind::Map((Self::type_(key), Self::type_(value)))
            }
            wit_parser::TypeDefKind::FixedLengthList(type_, length) => {
                TypeDefKind::List((Self::type_(type_), Some(length)))
            }
            wit_parser::TypeDefKind::Future(future) => TypeDefKind::Future(future.map(Self::type_)),
            wit_parser::TypeDefKind::Stream(stream) => TypeDefKind::Stream(stream.map(Self::type_)),
            wit_parser::TypeDefKind::Type(type_) => TypeDefKind::Type(Self::type_(type_)),
            wit_parser::TypeDefKind::Unknown => TypeDefKind::Unknown,
        }
    }

    fn record(record: wit_parser::Record) -> Record {
        Record {
            fields: Self::fields(record.fields),
        }
    }

    fn fields(fields: Vec<wit_parser::Field>) -> Vec<Field> {
        fields.into_iter().map(Self::field).collect()
    }

    fn field(field: wit_parser::Field) -> Field {
        Field {
            name: field.name,
            type_: Self::type_(field.ty),
            docs: Self::docs(field.docs),
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
            flags: Self::flags_(flags.flags),
        }
    }

    fn flags_(flags: Vec<wit_parser::Flag>) -> Vec<Flag> {
        flags.into_iter().map(Self::flag).collect()
    }

    fn flag(flag: wit_parser::Flag) -> Flag {
        Flag {
            name: flag.name,
            docs: Self::docs(flag.docs),
        }
    }

    fn tuple(tuple: wit_parser::Tuple) -> Tuple {
        Tuple {
            types: tuple.types.into_iter().map(Self::type_).collect(),
        }
    }

    fn variant(variant: wit_parser::Variant) -> Variant {
        Variant {
            cases: Self::cases(variant.cases),
        }
    }

    fn cases(cases: Vec<wit_parser::Case>) -> Vec<Case> {
        cases.into_iter().map(Self::case).collect()
    }

    fn case(case: wit_parser::Case) -> Case {
        Case {
            name: case.name,
            type_: case.ty.map(Self::type_),
            docs: Self::docs(case.docs),
        }
    }

    fn enum_(enum_: wit_parser::Enum) -> Enum {
        Enum {
            cases: Self::enum_cases(enum_.cases),
        }
    }

    fn enum_cases(cases: Vec<wit_parser::EnumCase>) -> Vec<EnumCase> {
        cases.into_iter().map(Self::enum_case).collect()
    }

    fn enum_case(case: wit_parser::EnumCase) -> EnumCase {
        EnumCase {
            name: case.name,
            docs: Self::docs(case.docs),
        }
    }

    fn result(result: wit_parser::Result_) -> Result_ {
        Result_ {
            ok: result.ok.map(Self::type_),
            err: result.err.map(Self::type_),
        }
    }

    fn type_owner(owner: wit_parser::TypeOwner) -> TypeOwner {
        match owner {
            wit_parser::TypeOwner::World(id) => TypeOwner::World(Self::world_id(id)),
            wit_parser::TypeOwner::Interface(id) => TypeOwner::Interface(Self::interface_id(id)),
            wit_parser::TypeOwner::None => TypeOwner::None,
        }
    }

    fn packages(resolve: &wit_parser::Resolve) -> BTreeMap<PackageId, Package> {
        resolve
            .packages
            .clone()
            .into_iter()
            .fold(BTreeMap::new(), |mut packages, (id, package)| {
                packages.insert(Self::package_id(id), Self::package(package));
                packages
            })
    }

    fn package_id(id: wit_parser::PackageId) -> PackageId {
        PackageId::from(format!("package:{}", id.index()))
    }

    fn package(package: wit_parser::Package) -> Package {
        Package {
            name: Self::package_name(package.name),
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

    fn package_name(package_name: wit_parser::PackageName) -> PackageName {
        PackageName {
            namespace: package_name.namespace,
            name: package_name.name,
            version: package_name.version.map(Self::version),
        }
    }

    fn function_kind(kind: wit_parser::FunctionKind) -> FunctionKind {
        match kind {
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
        }
    }

    fn params(params: Vec<wit_parser::Param>) -> Vec<Param> {
        params.into_iter().map(Self::param).collect()
    }

    fn param(param: wit_parser::Param) -> Param {
        Param {
            name: param.name,
            type_: Self::type_(param.ty),
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
    fn world_ids(&self) -> Vec<WorldId> {
        self.world_ids.clone()
    }

    #[allow(async_fn_in_trait)]
    fn world(&self, id: WorldId) -> Option<World> {
        self.worlds.get(&id).cloned()
    }

    #[allow(async_fn_in_trait)]
    fn interface_ids(&self) -> Vec<InterfaceId> {
        self.interface_ids.clone()
    }

    #[allow(async_fn_in_trait)]
    fn interface(&self, id: InterfaceId) -> Option<Interface> {
        self.interfaces.get(&id).cloned()
    }

    #[allow(async_fn_in_trait)]
    fn type_ids(&self) -> Vec<TypeId> {
        self.type_ids.clone()
    }

    #[allow(async_fn_in_trait)]
    fn type_(&self, id: TypeId) -> Option<TypeDef> {
        self.types.get(&id).cloned()
    }

    #[allow(async_fn_in_trait)]
    fn package_ids(&self) -> Vec<PackageId> {
        self.package_ids.clone()
    }

    #[allow(async_fn_in_trait)]
    fn package(&self, id: PackageId) -> Option<Package> {
        self.packages.get(&id).cloned()
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
