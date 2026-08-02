pub(crate) fn implementation_file_score(symbol: &str) -> u8 {
    let location = symbol.split('@').next().unwrap_or(symbol);
    let location = location.rsplit(['#', '/', '.']).next().unwrap_or(location);
    let extension = symbol
        .split('#')
        .next()
        .and_then(|value| value.rsplit_once('.'))
        .map(|(_, extension)| extension.to_ascii_lowercase());
    match extension.as_deref() {
        Some("c") | Some("cc") | Some("cpp") | Some("cxx") | Some("m") | Some("mm") => 2,
        Some("h") | Some("hh") | Some("hpp") | Some("hxx") => 1,
        _ if !location.is_empty() => 0,
        _ => 0,
    }
}

pub(crate) fn symbol_short_name(symbol: &str) -> &str {
    let symbol = symbol.split('@').next().unwrap_or(symbol);
    let property_descriptor = symbol.ends_with(':');
    let symbol = symbol.trim_end_matches(['.', ':', '/']);
    let symbol = symbol.trim_end_matches('#');
    let symbol = symbol.rsplit(['#', '.', ':', '/']).next().unwrap_or(symbol);
    let symbol = symbol.split('(').next().unwrap_or(symbol);
    let symbol = symbol.split_whitespace().last().unwrap_or(symbol);
    if property_descriptor {
        symbol.trim_end_matches(char::is_numeric)
    } else {
        symbol
    }
}

#[cfg(test)]
pub(crate) fn symbol_matches_name(symbol: &str, name: &str) -> bool {
    let name = name
        .rsplit(['#', '.', ':', '/', '\\'])
        .next()
        .unwrap_or(name)
        .trim_matches(['"', '\'', '`']);
    symbol_short_name(symbol) == name
}

pub(crate) fn self_test(root: &Path) -> Result<usize, String> {
    let packs = load_packs(root)?;
    let mut passed = 0usize;
    for pack in &packs {
        if pack
            .fixture
            .expected_relations
            .iter()
            .any(|relation| !pack.outputs.iter().any(|output| output == relation))
        {
            return Err(format!("{} has an invalid fixture relation", pack.id));
        }
        let mut facts = Vec::new();
        for file in &pack.fixture.files {
            if !fixture_is_source_file(pack, &file.path) {
                continue;
            }
            if pack.rules.iter().any(|rule| rule == "HTTP_ROUTE") {
                extract_routes(pack, &file.path, &file.source, &[], None, &mut facts);
            }
            extract_generic_facts(pack, &file.path, &file.source, &[], &mut facts);
        }
        for rule in &pack.fixture.expected_facts {
            if !facts.iter().any(|fact| fact.kind == *rule) {
                return Err(format!("{} did not emit {}", pack.id, rule));
            }
        }
        if facts
            .iter()
            .map(|fact| &fact.id)
            .collect::<HashSet<_>>()
            .len()
            != facts.len()
        {
            return Err(format!("{} emitted duplicate fact IDs", pack.id));
        }
        passed += 1;
    }
    println!("framework-pack-self-test\t{passed}");
    Ok(passed)
}

fn fixture_is_source_file(pack: &FrameworkPack, path: &str) -> bool {
    LANGUAGES
        .iter()
        .find(|language| language.id == pack.language)
        .map(|language| {
            language
                .extensions
                .iter()
                .any(|extension| path.ends_with(extension))
        })
        .unwrap_or(false)
}
