impl FastApiGraph {
    fn from_sources(sources: &BTreeMap<String, String>) -> Self {
        let mut modules = BTreeMap::new();
        let mut module_by_path = HashMap::new();

        for (path, source) in sources {
            let path = normalize_source_path(path);
            let Some((module, is_package)) = python_module(&path) else {
                continue;
            };
            let parsed = parse_module(module.clone(), is_package, source);
            module_by_path.insert(path, module.clone());
            modules.insert(module, parsed);
        }

        let mut graph = Self {
            modules,
            module_by_path,
            incoming: HashMap::new(),
        };
        graph.build_mount_edges();
        graph
    }

    fn build_mount_edges(&mut self) {
        let mut incoming = HashMap::<RouterKey, Vec<MountEdge>>::new();

        for module in self.modules.values() {
            for include in &module.includes {
                let Some(child) = self.resolve_symbol(&module.module, &include.child) else {
                    continue;
                };
                let parent =
                    if let Some(parent) = self.resolve_symbol(&module.module, &include.parent) {
                        MountParent::Router(parent)
                    } else if module.applications.contains(&include.parent) {
                        MountParent::Root
                    } else {
                        continue;
                    };
                incoming.entry(child).or_default().push(MountEdge {
                    parent,
                    prefix: include.prefix.clone(),
                });
            }
        }
        self.incoming = incoming;
    }

    fn mounted_route_path(
        &self,
        source_path: &str,
        handler_line: u64,
        method: &str,
        local_path: &str,
    ) -> Option<MountedRoutePath> {
        let path = normalize_source_path(source_path);
        let module_name = self.module_for_path(&path)?;
        let module = self.modules.get(module_name)?;
        let (router_symbol, source_local_path) =
            route_router_symbol(module, handler_line, method, local_path)?;
        let router = self.resolve_symbol(module_name, &router_symbol)?;
        let resolution = self.resolve_mount(&router, &mut HashSet::new());
        if resolution.uncertain || !resolution.rooted || resolution.prefixes.len() != 1 {
            return None;
        }
        let prefix = resolution.prefixes.iter().next()?;
        Some(MountedRoutePath {
            local: source_local_path.clone(),
            mounted: join_url_path(prefix, &source_local_path),
        })
    }

    fn module_for_path(&self, requested: &str) -> Option<&String> {
        if let Some(module) = self.module_by_path.get(requested) {
            return Some(module);
        }
        let suffix = format!("/{requested}");
        let mut matches = self
            .module_by_path
            .iter()
            .filter(|(path, _)| requested.ends_with(&format!("/{path}")) || path.ends_with(&suffix))
            .map(|(_, module)| module);
        let found = matches.next()?;
        matches.next().is_none().then_some(found)
    }

    fn resolve_symbol(&self, module_name: &str, expression: &str) -> Option<RouterKey> {
        let symbol = expression.trim();
        if !is_dotted_identifier(symbol) {
            return None;
        }
        let module_name = self.canonical_module(module_name)?;
        let module = self.modules.get(&module_name)?;

        if module.routers.contains_key(symbol) {
            return Some(RouterKey {
                module: module_name,
                symbol: symbol.to_string(),
            });
        }
        let imported = module.imports.get(symbol)?;
        self.resolve_imported(imported, &mut HashSet::new())
    }

    fn resolve_imported(
        &self,
        key: &RouterKey,
        seen: &mut HashSet<RouterKey>,
    ) -> Option<RouterKey> {
        let module_name = self.canonical_module(&key.module)?;
        let normalized = RouterKey {
            module: module_name.clone(),
            symbol: key.symbol.clone(),
        };
        if !seen.insert(normalized.clone()) {
            return None;
        }
        let module = self.modules.get(&module_name)?;
        if module.routers.contains_key(&key.symbol) {
            return Some(normalized);
        }
        let imported = module.imports.get(&key.symbol)?;
        self.resolve_imported(imported, seen)
    }

    fn canonical_module(&self, requested: &str) -> Option<String> {
        if self.modules.contains_key(requested) {
            return Some(requested.to_string());
        }
        let suffix = format!(".{requested}");
        let mut matches = self
            .modules
            .keys()
            .filter(|module| module.ends_with(&suffix));
        let found = matches.next()?.clone();
        matches.next().is_none().then_some(found)
    }

    fn resolve_mount(&self, router: &RouterKey, stack: &mut HashSet<RouterKey>) -> MountResolution {
        if !stack.insert(router.clone()) {
            return MountResolution {
                uncertain: true,
                ..MountResolution::default()
            };
        }

        let own_prefix = self
            .modules
            .get(&router.module)
            .and_then(|module| module.routers.get(&router.symbol));
        let Some(own_prefix) = own_prefix else {
            stack.remove(router);
            return MountResolution {
                uncertain: true,
                ..MountResolution::default()
            };
        };
        let StaticPath::Known(own_prefix) = own_prefix else {
            stack.remove(router);
            return MountResolution {
                uncertain: true,
                ..MountResolution::default()
            };
        };

        let mut result = MountResolution::default();
        let incoming = self.incoming.get(router);
        if incoming.is_none_or(Vec::is_empty) {
            result.prefixes.insert(normalize_url_prefix(own_prefix));
        } else if let Some(incoming) = incoming {
            for edge in incoming {
                let StaticPath::Known(include_prefix) = &edge.prefix else {
                    result.uncertain = true;
                    continue;
                };
                match &edge.parent {
                    MountParent::Root => {
                        result.rooted = true;
                        result.prefixes.insert(join_url_path(
                            &normalize_url_prefix(include_prefix),
                            own_prefix,
                        ));
                    }
                    MountParent::Router(parent) => {
                        let parent_resolution = self.resolve_mount(parent, stack);
                        result.uncertain |= parent_resolution.uncertain;
                        result.rooted |= parent_resolution.rooted;
                        for parent_prefix in parent_resolution.prefixes {
                            let mounted = join_url_path(&parent_prefix, include_prefix);
                            result.prefixes.insert(join_url_path(&mounted, own_prefix));
                        }
                    }
                }
            }
        }

        stack.remove(router);
        result
    }
}

