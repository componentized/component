use std::{
    collections::{BTreeMap, HashMap},
    fmt::Display,
};

use id_arena::{Arena, ArenaBehavior, DefaultArenaBehavior, Id};

use crate::{
    componentized::component::types::{ErrorCode, Malformed},
    exports::componentized::component::wit::{
        Docs, Enum, EnumCase, Flag, Flags, Function, FunctionKind, Guest, Handle, IncludeName,
        Interface, InterfaceId, List, Map, Package, PackageId, PackageName, Param, PrintOptions,
        PrintedPackages, Record, RecordField, Result as Result_, Stability, Stable, Summary, Tuple,
        Type, TypeDef, TypeDefKind, TypeId, TypeOwner, Unstable, Variant, VariantCase, Version,
        VersionIdentifier, Wit, WitSource, World, WorldId, WorldInclude, WorldItem,
        WorldItemInterface, WorldKey,
    },
};

pub(crate) struct WitTools;

impl Guest for WitTools {
    fn parse(source: WitSource) -> Result<Wit, ErrorCode> {
        match source {
            WitSource::Parsed(wit) => Ok(wit),
            source => {
                let (resolve, package_id, component_world_id) = Self::resolve(source)?;
                Wit::new(&resolve, package_id, component_world_id)
            }
        }
    }

    fn print(source: WitSource, options: Option<PrintOptions>) -> Result<String, ErrorCode> {
        let additional_packages = options
            .and_then(|options| options.packages)
            .unwrap_or(PrintedPackages::All);
        let (resolve, package_id, _) = Self::resolve(source)?;
        let output = wit_component::OutputToString::default();
        let mut printer = wit_component::WitPrinter::new(output);
        let mut nested_package_ids: Vec<Id<wit_parser::Package>> = match additional_packages {
            PrintedPackages::Default => vec![],
            PrintedPackages::All => resolve.packages.iter().map(|(id, _pkg)| id).collect(),
            PrintedPackages::Selected(names) => names
                .iter()
                .map(|name| Self::select_packages(&resolve, name))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .flatten()
                .collect(),
        };
        nested_package_ids.retain(|id| id != &package_id);
        nested_package_ids.sort_by(|a, b| {
            let a = &resolve.packages.get(*a).unwrap().name;
            let b = &resolve.packages.get(*b).unwrap().name;
            a.cmp(b)
        });
        nested_package_ids.dedup();
        printer.print(&resolve, package_id, &nested_package_ids)?;
        Ok(printer.output.to_string())
    }

    fn summarize(source: WitSource) -> Result<Summary, ErrorCode> {
        let wit = Self::parse(source)?;
        let world_id = wit
            .component_world
            .as_ref()
            .ok_or_else(|| ErrorCode::NotComponent(None))?;
        let world = wit.worlds.get(world_id).ok_or_else(|| {
            ErrorCode::Malformed(Malformed {
                name: None,
                message: format!("unknown world id: {world_id}"),
            })
        })?;

        let extract_name = |key: &WorldKey, item: &WorldItem| match key {
            WorldKey::Name(name) => match item {
                WorldItem::Interface(iface) => {
                    iface.external_id.clone().unwrap_or(name.to_string())
                }
                WorldItem::Function(..) => "--unknown--".to_owned(),
                WorldItem::Type { .. } => "--unknown--".to_owned(),
            },
            WorldKey::Interface(interface_id) => {
                let iface = wit.interfaces.get(interface_id).unwrap();
                match (iface.name.clone(), iface.package.clone()) {
                    (Some(name), Some(package_id)) => {
                        let package = wit.packages.get(&package_id).unwrap();
                        let namespace = package.name.namespace.clone();
                        let package_name = package.name.name.clone();
                        let version = package
                            .name
                            .version
                            .clone()
                            .map(|v| format!("@{v}"))
                            .unwrap_or("".to_owned());
                        format!("{namespace}:{package_name}/{name}{version}")
                    }
                    _ => "--unknown--".to_owned(),
                }
            }
        };

        Ok(Summary {
            imports: world
                .imports
                .iter()
                .map(|(key, item)| extract_name(key, item))
                .collect(),
            exports: world
                .exports
                .iter()
                .map(|(key, item)| extract_name(key, item))
                .collect(),
        })
    }
}

impl Wit {
    fn new(
        resolve: &wit_parser::Resolve,
        package_id: wit_parser::PackageId,
        component_world_id: Option<wit_parser::WorldId>,
    ) -> Result<Self, ErrorCode> {
        let wit = Self {
            worlds: resolve.worlds.clone().into_iter().fold(
                BTreeMap::new(),
                |mut worlds, (id, world)| {
                    worlds.insert(Self::world_id(id), Self::world(world));
                    worlds
                },
            ),
            interfaces: resolve.interfaces.clone().into_iter().fold(
                BTreeMap::new(),
                |mut interfaces, (id, interface)| {
                    interfaces.insert(Self::interface_id(id), Self::interface(interface));
                    interfaces
                },
            ),
            types: resolve.types.clone().into_iter().fold(
                BTreeMap::new(),
                |mut types, (id, type_def)| {
                    types.insert(Self::type_id(id), Self::type_def(type_def));
                    types
                },
            ),
            packages: resolve.packages.clone().into_iter().fold(
                BTreeMap::new(),
                |mut packages, (id, package)| {
                    packages.insert(Self::package_id(id), Self::package(package));
                    packages
                },
            ),

            default_package: Self::package_id(package_id),
            component_world: component_world_id.map(Self::world_id),
        };

        if wit.packages.get(&wit.default_package.clone()).is_none() {
            Err(ErrorCode::Malformed(Malformed {
                name: None,
                message: "decoded package must exist".to_string(),
            }))?;
        }
        if let Some(world_id) = wit.component_world.clone() {
            if wit.worlds.get(&world_id).is_none() {
                Err(ErrorCode::Malformed(Malformed {
                    name: None,
                    message: "component world must exist if set".to_string(),
                }))?;
            }
        }

        Ok(wit)
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
                wit_parser::FunctionKind::Getter => FunctionKind::Getter,
                wit_parser::FunctionKind::Setter => FunctionKind::Setter,
                wit_parser::FunctionKind::MethodGetter(id) => {
                    FunctionKind::MethodGetter(Self::type_id(id))
                }
                wit_parser::FunctionKind::MethodSetter(id) => {
                    FunctionKind::MethodSetter(Self::type_id(id))
                }
                wit_parser::FunctionKind::StaticGetter(id) => {
                    FunctionKind::StaticGetter(Self::type_id(id))
                }
                wit_parser::FunctionKind::StaticSetter(id) => {
                    FunctionKind::StaticSetter(Self::type_id(id))
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
        PackageId::from(format!("package:{}", id.index()))
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

impl WitTools {
    /// Resolves a source into its packages, the default package, and the world of a component.
    fn resolve(
        source: WitSource,
    ) -> Result<
        (
            wit_parser::Resolve,
            wit_parser::PackageId,
            Option<wit_parser::WorldId>,
        ),
        ErrorCode,
    > {
        match source {
            WitSource::Wasm(component) => Self::decode(&component),
            WitSource::Wit(text) => {
                let mut resolve = wit_parser::Resolve::default();
                // keep `@unstable` items, rather than dropping those without an enabled feature
                resolve.all_features = true;
                let package_id = resolve
                    .push_str("source.wit", &text)
                    .map_err(Self::malformed)?;
                Ok((resolve, package_id, None))
            }
            WitSource::Parsed(wit) => {
                let component_world = wit.component_world.clone();
                let (resolve, package_id, ids) = wit.into_resolve()?;
                let component_world_id = component_world
                    .map(|world_id| ids.world_id(&world_id))
                    .transpose()?;
                Ok((resolve, package_id, component_world_id))
            }
        }
    }

    /// Decodes a component, or a WIT package encoded as a component. Anything else, e.g. a core
    /// module, is not a component.
    fn decode(
        component: &[u8],
    ) -> Result<
        (
            wit_parser::Resolve,
            wit_parser::PackageId,
            Option<wit_parser::WorldId>,
        ),
        ErrorCode,
    > {
        if !wasmparser::Parser::is_component(component) {
            Err(ErrorCode::NotComponent(None))?;
        }
        let decoded = wit_component::decode(component).map_err(Self::malformed)?;
        let package_id = decoded.package();
        Ok(match decoded {
            wit_component::DecodedWasm::Component(resolve, world_id) => {
                (resolve, package_id, Some(world_id))
            }
            wit_component::DecodedWasm::WitPackage(resolve, _) => (resolve, package_id, None),
        })
    }

    /// A `malformed` error for the single source of a call.
    fn malformed(error: impl Display) -> ErrorCode {
        ErrorCode::Malformed(Malformed {
            name: None,
            message: error.to_string(),
        })
    }

    /// The packages with the name, every version of the package when the name has no version.
    fn select_packages(
        resolve: &wit_parser::Resolve,
        name: &PackageName,
    ) -> Result<Vec<wit_parser::PackageId>, ErrorCode> {
        let version = name.version.clone().map(ResolveIds::version).transpose()?;
        let package_ids: Vec<_> = resolve
            .packages
            .iter()
            .filter(|(_, package)| {
                package.name.namespace == name.namespace
                    && package.name.name == name.name
                    && (version.is_none() || package.name.version == version)
            })
            .map(|(id, _)| id)
            .collect();
        if package_ids.is_empty() {
            Err(ErrorCode::NotFound(Some(name.to_string())))?;
        }
        Ok(package_ids)
    }
}

impl Wit {
    fn into_resolve(
        self,
    ) -> Result<(wit_parser::Resolve, wit_parser::PackageId, ResolveIds), ErrorCode> {
        let mut resolve = wit_parser::Resolve::default();
        let ids = ResolveIds {
            worlds: ResolveIds::assign(self.worlds.keys(), &resolve.worlds),
            interfaces: ResolveIds::assign(self.interfaces.keys(), &resolve.interfaces),
            types: ResolveIds::assign(self.types.keys(), &resolve.types),
            packages: ResolveIds::assign(self.packages.keys(), &resolve.packages),
        };

        for (id, package) in ResolveIds::ordered(self.packages, &ids.packages) {
            let package = ids.package(package)?;
            resolve.package_names.insert(package.name.clone(), id);
            ResolveIds::alloc(&mut resolve.packages, id, package);
        }
        for (id, interface) in ResolveIds::ordered(self.interfaces, &ids.interfaces) {
            ResolveIds::alloc(&mut resolve.interfaces, id, ids.interface(interface)?);
        }
        for (id, type_def) in ResolveIds::ordered(self.types, &ids.types) {
            ResolveIds::alloc(&mut resolve.types, id, ids.type_def(type_def)?);
        }
        for (id, world) in ResolveIds::ordered(self.worlds, &ids.worlds) {
            ResolveIds::alloc(&mut resolve.worlds, id, ids.world(world)?);
        }

        let package_id = ids.package_id(&self.default_package)?;
        Ok((resolve, package_id, ids))
    }
}

/// Maps the string ids used by `Wit` to the arena ids of a `wit_parser::Resolve`.
struct ResolveIds {
    worlds: HashMap<String, wit_parser::WorldId>,
    interfaces: HashMap<String, wit_parser::InterfaceId>,
    types: HashMap<String, wit_parser::TypeId>,
    packages: HashMap<String, wit_parser::PackageId>,
}

impl ResolveIds {
    /// Assigns sequential arena ids, ordered by the numeric suffix of each id
    /// (`type:2` before `type:10`) so the original arena order is preserved.
    fn assign<'a, T>(
        keys: impl Iterator<Item = &'a String>,
        arena: &Arena<T>,
    ) -> HashMap<String, Id<T>> {
        let arena_id = DefaultArenaBehavior::<T>::arena_id(arena.next_id());
        let mut keys: Vec<&String> = keys.collect();
        keys.sort_by_key(|key| {
            let index = key
                .rsplit_once(':')
                .and_then(|(_, index)| index.parse::<u64>().ok());
            (index, key.to_string())
        });
        keys.into_iter()
            .enumerate()
            .map(|(index, key)| {
                (
                    key.clone(),
                    DefaultArenaBehavior::<T>::new_id(arena_id, arena.len() + index),
                )
            })
            .collect()
    }

    fn ordered<V, T>(items: BTreeMap<String, V>, ids: &HashMap<String, Id<T>>) -> Vec<(Id<T>, V)> {
        let mut items: Vec<(Id<T>, V)> = items
            .into_iter()
            .map(|(key, value)| (ids[&key], value))
            .collect();
        items.sort_by_key(|(id, _)| id.index());
        items
    }

    fn alloc<T>(arena: &mut Arena<T>, id: Id<T>, item: T) {
        let allocated = arena.alloc(item);
        debug_assert_eq!(allocated, id);
    }

    fn lookup<T>(ids: &HashMap<String, Id<T>>, kind: &str, id: &str) -> Result<Id<T>, ErrorCode> {
        ids.get(id).copied().ok_or_else(|| {
            ErrorCode::Malformed(Malformed {
                name: None,
                message: format!("unknown {kind} id: {id}"),
            })
        })
    }

    fn world_id(&self, id: &WorldId) -> Result<wit_parser::WorldId, ErrorCode> {
        Self::lookup(&self.worlds, "world", id)
    }

    fn interface_id(&self, id: &InterfaceId) -> Result<wit_parser::InterfaceId, ErrorCode> {
        Self::lookup(&self.interfaces, "interface", id)
    }

    fn type_id(&self, id: &TypeId) -> Result<wit_parser::TypeId, ErrorCode> {
        Self::lookup(&self.types, "type", id)
    }

    fn package_id(&self, id: &PackageId) -> Result<wit_parser::PackageId, ErrorCode> {
        Self::lookup(&self.packages, "package", id)
    }

    fn world(&self, world: World) -> Result<wit_parser::World, ErrorCode> {
        Ok(wit_parser::World {
            name: world.name,
            imports: world
                .imports
                .into_iter()
                .map(|entry| self.world_entry(entry))
                .collect::<Result<_, _>>()?,
            exports: world
                .exports
                .into_iter()
                .map(|entry| self.world_entry(entry))
                .collect::<Result<_, _>>()?,
            package: world.package.map(|id| self.package_id(&id)).transpose()?,
            docs: Self::docs(world.docs),
            stability: Self::stability(world.stability)?,
            includes: world
                .includes
                .into_iter()
                .map(|include| {
                    Ok(wit_parser::WorldInclude {
                        stability: Self::stability(include.stability)?,
                        id: self.world_id(&include.id)?,
                        names: include
                            .names
                            .into_iter()
                            .map(|name| wit_parser::IncludeName {
                                name: name.name,
                                as_: name.as_,
                            })
                            .collect(),
                        span: Default::default(),
                    })
                })
                .collect::<Result<_, ErrorCode>>()?,
            span: Default::default(),
        })
    }

    fn world_entry(
        &self,
        (key, item): (WorldKey, WorldItem),
    ) -> Result<(wit_parser::WorldKey, wit_parser::WorldItem), ErrorCode> {
        Ok((
            match key {
                WorldKey::Name(name) => wit_parser::WorldKey::Name(name),
                WorldKey::Interface(id) => wit_parser::WorldKey::Interface(self.interface_id(&id)?),
            },
            match item {
                WorldItem::Interface(iface) => wit_parser::WorldItem::Interface {
                    id: self.interface_id(&iface.id)?,
                    stability: Self::stability(iface.stability)?,
                    external_id: iface.external_id,
                    docs: Self::docs(iface.docs),
                    span: Default::default(),
                },
                WorldItem::Function(function) => {
                    wit_parser::WorldItem::Function(self.function(function)?)
                }
                WorldItem::Type(id) => wit_parser::WorldItem::Type {
                    id: self.type_id(&id)?,
                    span: Default::default(),
                },
            },
        ))
    }

    fn interface(&self, interface: Interface) -> Result<wit_parser::Interface, ErrorCode> {
        Ok(wit_parser::Interface {
            name: interface.name,
            types: interface
                .types
                .into_iter()
                .map(|(name, id)| Ok((name, self.type_id(&id)?)))
                .collect::<Result<_, ErrorCode>>()?,
            functions: interface
                .functions
                .into_iter()
                .map(|(name, function)| Ok((name, self.function(function)?)))
                .collect::<Result<_, ErrorCode>>()?,
            docs: Self::docs(interface.docs),
            stability: Self::stability(interface.stability)?,
            package: interface
                .package
                .map(|id| self.package_id(&id))
                .transpose()?,
            span: Default::default(),
            clone_of: None,
        })
    }

    fn function(&self, function: Function) -> Result<wit_parser::Function, ErrorCode> {
        Ok(wit_parser::Function {
            name: function.name,
            kind: match function.kind {
                FunctionKind::Freestanding => wit_parser::FunctionKind::Freestanding,
                FunctionKind::AsyncFreestanding => wit_parser::FunctionKind::AsyncFreestanding,
                FunctionKind::Method(id) => wit_parser::FunctionKind::Method(self.type_id(&id)?),
                FunctionKind::AsyncMethod(id) => {
                    wit_parser::FunctionKind::AsyncMethod(self.type_id(&id)?)
                }
                FunctionKind::Static(id) => wit_parser::FunctionKind::Static(self.type_id(&id)?),
                FunctionKind::AsyncStatic(id) => {
                    wit_parser::FunctionKind::AsyncStatic(self.type_id(&id)?)
                }
                FunctionKind::Constructor(id) => {
                    wit_parser::FunctionKind::Constructor(self.type_id(&id)?)
                }
                FunctionKind::Getter => wit_parser::FunctionKind::Getter,
                FunctionKind::Setter => wit_parser::FunctionKind::Setter,
                FunctionKind::MethodGetter(id) => {
                    wit_parser::FunctionKind::MethodGetter(self.type_id(&id)?)
                }
                FunctionKind::MethodSetter(id) => {
                    wit_parser::FunctionKind::MethodSetter(self.type_id(&id)?)
                }
                FunctionKind::StaticGetter(id) => {
                    wit_parser::FunctionKind::StaticGetter(self.type_id(&id)?)
                }
                FunctionKind::StaticSetter(id) => {
                    wit_parser::FunctionKind::StaticSetter(self.type_id(&id)?)
                }
            },
            params: function
                .params
                .into_iter()
                .map(|param| {
                    Ok(wit_parser::Param {
                        name: param.name,
                        ty: self.type_(param.type_)?,
                        span: Default::default(),
                    })
                })
                .collect::<Result<_, ErrorCode>>()?,
            result: function.result.map(|ty| self.type_(ty)).transpose()?,
            docs: Self::docs(function.docs),
            stability: Self::stability(function.stability)?,
            span: Default::default(),
            external_id: function.external_id,
        })
    }

    fn type_(&self, type_: Type) -> Result<wit_parser::Type, ErrorCode> {
        Ok(match type_ {
            Type::Bool => wit_parser::Type::Bool,
            Type::U8 => wit_parser::Type::U8,
            Type::U16 => wit_parser::Type::U16,
            Type::U32 => wit_parser::Type::U32,
            Type::U64 => wit_parser::Type::U64,
            Type::S8 => wit_parser::Type::S8,
            Type::S16 => wit_parser::Type::S16,
            Type::S32 => wit_parser::Type::S32,
            Type::S64 => wit_parser::Type::S64,
            Type::F32 => wit_parser::Type::F32,
            Type::F64 => wit_parser::Type::F64,
            Type::Char => wit_parser::Type::Char,
            Type::String => wit_parser::Type::String,
            Type::ErrorContext => wit_parser::Type::ErrorContext,
            Type::Id(id) => wit_parser::Type::Id(self.type_id(&id)?),
        })
    }

    fn type_def(&self, type_def: TypeDef) -> Result<wit_parser::TypeDef, ErrorCode> {
        Ok(wit_parser::TypeDef {
            name: type_def.name,
            kind: self.type_def_kind(type_def.kind)?,
            owner: match type_def.owner {
                TypeOwner::World(id) => wit_parser::TypeOwner::World(self.world_id(&id)?),
                TypeOwner::Interface(id) => {
                    wit_parser::TypeOwner::Interface(self.interface_id(&id)?)
                }
                TypeOwner::None => wit_parser::TypeOwner::None,
            },
            docs: Self::docs(type_def.docs),
            stability: Self::stability(type_def.stability)?,
            span: Default::default(),
            external_id: type_def.external_id,
        })
    }

    fn type_def_kind(&self, kind: TypeDefKind) -> Result<wit_parser::TypeDefKind, ErrorCode> {
        Ok(match kind {
            TypeDefKind::Record(record) => wit_parser::TypeDefKind::Record(wit_parser::Record {
                fields: record
                    .fields
                    .into_iter()
                    .map(|field| {
                        Ok(wit_parser::Field {
                            name: field.name,
                            ty: self.type_(field.type_)?,
                            docs: Self::docs(field.docs),
                            span: Default::default(),
                        })
                    })
                    .collect::<Result<_, ErrorCode>>()?,
            }),
            TypeDefKind::Resource => wit_parser::TypeDefKind::Resource,
            TypeDefKind::Handle(handle) => wit_parser::TypeDefKind::Handle(match handle {
                Handle::Own(id) => wit_parser::Handle::Own(self.type_id(&id)?),
                Handle::Borrow(id) => wit_parser::Handle::Borrow(self.type_id(&id)?),
            }),
            TypeDefKind::Flags(flags) => wit_parser::TypeDefKind::Flags(wit_parser::Flags {
                flags: flags
                    .flags
                    .into_iter()
                    .map(|flag| wit_parser::Flag {
                        name: flag.name,
                        docs: Self::docs(flag.docs),
                        span: Default::default(),
                    })
                    .collect(),
            }),
            TypeDefKind::Tuple(tuple) => wit_parser::TypeDefKind::Tuple(wit_parser::Tuple {
                types: tuple
                    .types
                    .into_iter()
                    .map(|ty| self.type_(ty))
                    .collect::<Result<_, _>>()?,
            }),
            TypeDefKind::Variant(variant) => {
                wit_parser::TypeDefKind::Variant(wit_parser::Variant {
                    cases: variant
                        .cases
                        .into_iter()
                        .map(|case| {
                            Ok(wit_parser::Case {
                                name: case.name,
                                ty: case.type_.map(|ty| self.type_(ty)).transpose()?,
                                docs: Self::docs(case.docs),
                                span: Default::default(),
                            })
                        })
                        .collect::<Result<_, ErrorCode>>()?,
                })
            }
            TypeDefKind::Enum(enum_) => wit_parser::TypeDefKind::Enum(wit_parser::Enum {
                cases: enum_
                    .cases
                    .into_iter()
                    .map(|case| wit_parser::EnumCase {
                        name: case.name,
                        docs: Self::docs(case.docs),
                        span: Default::default(),
                    })
                    .collect(),
            }),
            TypeDefKind::Option(ty) => wit_parser::TypeDefKind::Option(self.type_(ty)?),
            TypeDefKind::Result(result) => wit_parser::TypeDefKind::Result(wit_parser::Result_ {
                ok: result.ok.map(|ty| self.type_(ty)).transpose()?,
                err: result.err.map(|ty| self.type_(ty)).transpose()?,
            }),
            TypeDefKind::List(list) => match list.fixed_length {
                Some(length) => {
                    wit_parser::TypeDefKind::FixedLengthList(self.type_(list.type_)?, length)
                }
                None => wit_parser::TypeDefKind::List(self.type_(list.type_)?),
            },
            TypeDefKind::Map(map) => {
                wit_parser::TypeDefKind::Map(self.type_(map.key)?, self.type_(map.value)?)
            }
            TypeDefKind::Future(ty) => {
                wit_parser::TypeDefKind::Future(ty.map(|ty| self.type_(ty)).transpose()?)
            }
            TypeDefKind::Stream(ty) => {
                wit_parser::TypeDefKind::Stream(ty.map(|ty| self.type_(ty)).transpose()?)
            }
            TypeDefKind::Type(ty) => wit_parser::TypeDefKind::Type(self.type_(ty)?),
            TypeDefKind::Unknown => wit_parser::TypeDefKind::Unknown,
        })
    }

    fn package(&self, package: Package) -> Result<wit_parser::Package, ErrorCode> {
        Ok(wit_parser::Package {
            name: wit_parser::PackageName {
                namespace: package.name.namespace,
                name: package.name.name,
                version: package.name.version.map(Self::version).transpose()?,
            },
            docs: Self::docs(package.docs),
            interfaces: package
                .interfaces
                .into_iter()
                .map(|(name, id)| Ok((name, self.interface_id(&id)?)))
                .collect::<Result<_, ErrorCode>>()?,
            worlds: package
                .worlds
                .into_iter()
                .map(|(name, id)| Ok((name, self.world_id(&id)?)))
                .collect::<Result<_, ErrorCode>>()?,
        })
    }

    fn docs(docs: Docs) -> wit_parser::Docs {
        wit_parser::Docs {
            contents: docs.contents,
        }
    }

    fn stability(stability: Stability) -> Result<wit_parser::Stability, ErrorCode> {
        Ok(match stability {
            Stability::Unknown => wit_parser::Stability::Unknown,
            Stability::Unstable(Unstable {
                feature,
                deprecated,
            }) => wit_parser::Stability::Unstable {
                feature,
                deprecated: deprecated.map(Self::version).transpose()?,
            },
            Stability::Stable(Stable { since, deprecated }) => wit_parser::Stability::Stable {
                since: Self::version(since)?,
                deprecated: deprecated.map(Self::version).transpose()?,
            },
        })
    }

    fn version(version: Version) -> Result<semver::Version, ErrorCode> {
        semver::Version::parse(&version.to_string()).map_err(|err| {
            ErrorCode::Malformed(Malformed {
                name: None,
                message: format!("invalid version {version}: {err}"),
            })
        })
    }
}

impl From<anyhow::Error> for ErrorCode {
    fn from(value: anyhow::Error) -> Self {
        Self::Other(Some(value.to_string()))
    }
}

impl Display for PackageName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let PackageName {
            namespace,
            name,
            version,
        } = self;
        f.write_fmt(format_args!("{namespace}:{name}"))?;
        if let Some(version) = version {
            f.write_fmt(format_args!("@{version}"))?;
        }
        Ok(())
    }
}

impl Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Version {
            major,
            minor,
            patch,
            prerelease,
            build_metadata,
        } = self;
        let prerelease = prerelease
            .clone()
            .map(|chunks| {
                chunks
                    .into_iter()
                    .map(|chunk| chunk.to_string())
                    .collect::<Vec<String>>()
                    .join(".")
            })
            .map(|s| format!("-{s}"))
            .unwrap_or("".to_string());
        let build_metadata = build_metadata
            .clone()
            .map(|chunks| {
                chunks
                    .into_iter()
                    .map(|chunk| chunk.to_string())
                    .collect::<Vec<String>>()
                    .join(".")
            })
            .map(|s| format!("+{s}"))
            .unwrap_or("".to_string());
        f.write_fmt(format_args!(
            "{major}.{minor}.{patch}{prerelease}{build_metadata}"
        ))
    }
}

impl Display for VersionIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VersionIdentifier::String(vi) => f.write_fmt(format_args!("{vi}")),
            VersionIdentifier::Numeric(vi) => f.write_fmt(format_args!("{vi}")),
        }
    }
}

wit_bindgen::generate!({
    path: "../wit",
    world: "wit-tools",
    merge_structurally_equal_types: true,
    generate_all
});

export!(WitTools);

#[cfg(test)]
mod tests;
