# Framework packs

The catalog is intentionally limited to the 12 languages implemented by the
language bridge. Each language has its own folder, and each framework has its
own `pack.json`. A pack is a declarative rule set: dependency/source signals
select the pack and `outputs` declare the normalized facts it is allowed to
emit. `rule_sets` declares which shared analyzers are enabled for that pack.

`adapters.json` explicitly maps all 82 qualified pack IDs to the shared
analyzer family that executes them. The runtime rejects a pack without an
adapter entry.

This directory is not a list of names only. The runtime gate requires every
pack to have a stable id, language, category, detection signals, output fact
types, and a checked `fixture.json`. The shared analyzer must never create an
API, handler, service, or component from a name match without source/provider
evidence.

Fixture metadata uses the project's normal manifest shape where possible:
`package.json`, `pyproject.toml`, `pom.xml`, `*.csproj`, `CMakeLists.txt`,
`go.mod`, `Cargo.toml`, `composer.json`, `Gemfile`, or `pubspec.yaml`.
The pack gate checks that the fixture uses the metadata shape for its language;
non-JavaScript fixtures cannot silently pass with a fake `package.json`.

Excluded from this catalog: Kotlin and Swift, which are outside the current
Windows 12-language code-memory contract, and server runtimes such as Tomcat,
Node.js, Tokio, and SwiftNIO, which are runtime metadata rather than framework
flow packs.
